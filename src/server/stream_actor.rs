use crate::billing::UsageRepository;
use crate::config::TargetConfig;
use crate::metrics::Metrics;
use crate::notifications::{NotificationDispatcher, NotificationTarget};
use crate::server::preview::{
    StreamState, StreamStatus, create_preview_dir, valid_preview_file_name,
};
use crate::server::relay::{
    LocalRelayExecutor, RedisRelayExecutor, RelayExecutor, RelayProcess, cancel_relays,
    run_direct_test,
};
use crate::tenant::{Tenant, TenantId, TenantRepository};
use crate::util::stream_key_digest;
use anyhow::{Context, Result, bail};
use reqwest::Client;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot, watch};

struct StagedStream {
    tenant_id: TenantId,
    stream_key: String,
    preview_dir: PathBuf,
    preview_process: Child,
    preview_failed: bool,
    published: bool,
    started_at_unix: i64,
}

pub enum StreamCommand {
    StageStream {
        tenant: Box<Tenant>,
        stream_key: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    PublishStagedStream {
        tenant_id: TenantId,
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopPublishing {
        tenant_id: TenantId,
        respond_to: oneshot::Sender<Result<()>>,
    },
    EndStreamIfUnpublished {
        stream_key: String,
    },
    MarkUnpublished {
        stream_key: String,
    },
    EmergencyStop,
    EmergencyStopTenant {
        tenant_id: TenantId,
    },
    RunTestStream {
        duration_secs: u64,
        targets: Vec<TargetConfig>,
    },
}

/// Actor exclusively managing HLS preview FFmpeg processes, RTMP target relays, and lifecycle state.
pub struct StreamActor {
    preview_dir: PathBuf,
    staged: HashMap<String, StagedStream>,
    active_relays: HashMap<String, Vec<RelayProcess>>,
    listen_port: u16,
    http_client: Client,
    tenants: TenantRepository,
    usage: UsageRepository,
    relay_executor: Arc<dyn RelayExecutor>,
    status_tx: watch::Sender<Arc<HashMap<TenantId, StreamStatus>>>,
}

impl StreamActor {
    pub async fn run(mut self, mut receiver: mpsc::Receiver<StreamCommand>) {
        let mut status_check_interval = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                _ = status_check_interval.tick() => {
                    self.check_preview_process_health();
                }
                cmd = receiver.recv() => {
                    let Some(cmd) = cmd else {
                        break;
                    };
                    match cmd {
                        StreamCommand::StageStream { tenant, stream_key, respond_to } => {
                            let res = self.handle_stage_stream(*tenant, stream_key).await;
                            let _ = respond_to.send(res);
                        }
                        StreamCommand::PublishStagedStream { tenant_id, respond_to } => {
                            let res = self.handle_publish_staged(&tenant_id).await;
                            let _ = respond_to.send(res);
                        }
                        StreamCommand::StopPublishing { tenant_id, respond_to } => {
                            let res = self.handle_stop_publishing(&tenant_id).await;
                            let _ = respond_to.send(res);
                        }
                        StreamCommand::EndStreamIfUnpublished { stream_key } => {
                            if self.staged.get(&stream_key).is_some_and(|stream| !stream.published) {
                                self.handle_end_stream(&stream_key).await;
                            }
                        }
                        StreamCommand::MarkUnpublished { stream_key } => {
                            self.handle_mark_unpublished(&stream_key).await;
                        }
                        StreamCommand::EmergencyStop => self.handle_emergency_stop().await,
                        StreamCommand::EmergencyStopTenant { tenant_id } => self.handle_emergency_stop_tenant(&tenant_id).await,
                        StreamCommand::RunTestStream { duration_secs, targets } => {
                            run_direct_test(duration_secs, targets);
                        }
                    }
                }
            }
        }

        self.cleanup().await;
    }

    fn check_preview_process_health(&mut self) {
        for stream in self
            .staged
            .values_mut()
            .filter(|stream| !stream.preview_failed)
        {
            match stream.preview_process.try_wait() {
                Ok(Some(status)) => {
                    tracing::error!(%status, "HLS preview process stopped unexpectedly");
                    stream.preview_failed = true;
                }
                Err(error) => {
                    tracing::error!(%error, "Failed to inspect HLS preview process");
                    stream.preview_failed = true;
                }
                Ok(None) => {}
            }
        }
        self.update_status();
    }

    fn compute_statuses(&self) -> HashMap<TenantId, StreamStatus> {
        self.staged
            .values()
            .map(|stream| {
                let state = if stream.published {
                    StreamState::Live
                } else if stream.preview_failed {
                    StreamState::PreviewFailed
                } else if stream.preview_dir.join("index.m3u8").is_file() {
                    StreamState::PreviewReady
                } else {
                    StreamState::Preparing
                };
                (stream.tenant_id.clone(), StreamStatus { state })
            })
            .collect()
    }

    fn update_status(&mut self) {
        let statuses = Arc::new(self.compute_statuses());
        if *self.status_tx.borrow() != statuses {
            self.status_tx.send_replace(statuses);
        }
    }

    async fn handle_stage_stream(&mut self, tenant: Tenant, stream_key: String) -> Result<()> {
        if !tenant_has_capacity(
            &tenant,
            self.staged.values().map(|stream| &stream.tenant_id),
        ) {
            bail!("Tenant already has the maximum number of active streams");
        }
        if !self
            .usage
            .begin_stream(
                tenant.id.as_str(),
                &stream_key,
                crate::util::now_unix_secs() as i64,
            )
            .await?
        {
            bail!("Tenant monthly stream quota has been exhausted");
        }
        if self.staged.contains_key(&stream_key) {
            bail!("This ingest stream is already active");
        }
        let preview_dir = self.preview_dir.join(stream_key_digest(tenant.id.as_str()));
        create_preview_dir(&preview_dir)?;

        let source_url = format!("rtmp://127.0.0.1:{}/live/{stream_key}", self.listen_port);
        let playlist = preview_dir.join("index.m3u8");
        let segments = preview_dir.join("segment_%06d.ts");
        let playlist = playlist.to_string_lossy().into_owned();
        let segments = segments.to_string_lossy().into_owned();

        let preview_process = tokio::process::Command::new("ffmpeg")
            .args([
                "-loglevel",
                "warning",
                "-i",
                &source_url,
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-tune",
                "zerolatency",
                "-g",
                "60",
                "-keyint_min",
                "60",
                "-sc_threshold",
                "0",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-f",
                "hls",
                "-hls_time",
                "2",
                "-hls_list_size",
                "6",
                "-hls_flags",
                "delete_segments+append_list+omit_endlist+independent_segments",
                "-hls_segment_filename",
                &segments,
                &playlist,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        self.staged.insert(
            stream_key.clone(),
            StagedStream {
                tenant_id: tenant.id,
                stream_key,
                preview_dir,
                preview_process,
                preview_failed: false,
                published: false,
                started_at_unix: crate::util::now_unix_secs() as i64,
            },
        );

        self.update_status();
        tracing::info!("Stream staged with local HLS preview");
        Ok(())
    }

    async fn handle_publish_staged(&mut self, tenant_id: &TenantId) -> Result<()> {
        let stream = self
            .staged
            .values_mut()
            .find(|stream| &stream.tenant_id == tenant_id)
            .ok_or_else(|| anyhow::anyhow!("No stream is currently staged for this tenant"))?;
        if stream.published {
            bail!("The staged stream is already published");
        }
        let stream_key = stream.stream_key.clone();

        let tenant = self
            .tenants
            .find(tenant_id)
            .await?
            .filter(|tenant| tenant.active)
            .ok_or_else(|| anyhow::anyhow!("Tenant is inactive"))?;
        stream.published = true;
        let active_targets = tenant
            .targets
            .iter()
            .filter(|target| target.enabled)
            .cloned()
            .collect::<Vec<_>>();
        let notification_targets = active_targets
            .iter()
            .map(NotificationTarget::from)
            .collect::<Vec<_>>();
        let dispatcher =
            NotificationDispatcher::new(&tenant.notifications, self.http_client.clone());

        let source_url = format!("rtmp://127.0.0.1:{}/live/{stream_key}", self.listen_port);
        let mut relays = Vec::new();
        for target in active_targets {
            relays.push(self.relay_executor.spawn(
                tenant_id.as_str().to_owned(),
                source_url.clone(),
                target,
            ));
        }

        self.active_relays.insert(stream_key, relays);
        self.update_status();

        tokio::spawn(async move {
            dispatcher.dispatch(&notification_targets).await;
        });

        tracing::info!(tenant_id = %tenant_id.as_str(), "Staged stream published to enabled targets");
        Ok(())
    }

    async fn handle_stop_publishing(&mut self, tenant_id: &TenantId) -> Result<()> {
        let stream = self
            .staged
            .values_mut()
            .find(|stream| &stream.tenant_id == tenant_id)
            .ok_or_else(|| anyhow::anyhow!("No stream is currently staged for this tenant"))?;
        stream.published = false;
        let stream_key = stream.stream_key.clone();

        if let Some(relays) = self.active_relays.remove(&stream_key) {
            cancel_relays(relays).await;
        }

        self.update_status();
        tracing::info!("External publishing stopped; stream remains staged");
        Ok(())
    }

    async fn handle_mark_unpublished(&mut self, stream_key: &str) {
        if let Some(stream) = self.staged.get_mut(stream_key) {
            stream.published = false;
            self.update_status();
        }
    }

    async fn handle_end_stream(&mut self, stream_key: &str) {
        if let Some(mut stream) = self.staged.remove(stream_key) {
            let _ = self
                .usage
                .record_seconds(
                    stream.tenant_id.as_str(),
                    &stream.stream_key,
                    stream.started_at_unix,
                    crate::util::now_unix_secs() as i64,
                )
                .await;
            let _ = stream.preview_process.kill().await;
            if let Err(error) = tokio::fs::remove_dir_all(&stream.preview_dir).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%error, "Failed to remove HLS preview files");
            }
        }
        if let Some(relays) = self.active_relays.remove(stream_key) {
            cancel_relays(relays).await;
        }
        self.update_status();
    }

    async fn handle_emergency_stop(&mut self) {
        for (_, mut stream) in self.staged.drain() {
            let _ = stream.preview_process.kill().await;
        }
        for (_, relays) in self.active_relays.drain() {
            cancel_relays(relays).await;
        }
        self.update_status();
        tracing::warn!("Emergency stop terminated all active streams");
    }

    async fn handle_emergency_stop_tenant(&mut self, tenant_id: &TenantId) {
        let keys: Vec<String> = self
            .staged
            .iter()
            .filter(|(_, stream)| &stream.tenant_id == tenant_id)
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys {
            self.handle_end_stream(&key).await;
        }
        tracing::warn!(tenant_id = %tenant_id.as_str(), "Tenant emergency stop terminated active streams");
    }

    async fn cleanup(&mut self) {
        for (_, mut stream) in self.staged.drain() {
            let _ = stream.preview_process.kill().await;
        }
        for (_, relays) in self.active_relays.drain() {
            cancel_relays(relays).await;
        }
        let _ = tokio::fs::remove_dir_all(&self.preview_dir).await;
    }
}

fn tenant_has_capacity<'a>(
    tenant: &Tenant,
    active_tenants: impl Iterator<Item = &'a TenantId>,
) -> bool {
    active_tenants
        .filter(|active| *active == &tenant.id)
        .count()
        < tenant.max_concurrent_streams
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NotificationSettings;

    fn tenant(id: &str, max_concurrent_streams: usize) -> Tenant {
        Tenant {
            id: TenantId::new(id).unwrap(),
            name: id.to_owned(),
            active: true,
            max_concurrent_streams,
            notifications: NotificationSettings::default(),
            chat: crate::config::ChatSettings::default(),
            overlay: crate::config::OverlaySettings::default(),
            targets: Vec::new(),
        }
    }

    #[test]
    fn concurrency_limits_are_isolated_per_tenant() {
        let alpha = tenant("alpha", 1);
        let beta = tenant("beta", 1);
        let active = [alpha.id.clone()];
        assert!(!tenant_has_capacity(&alpha, active.iter()));
        assert!(tenant_has_capacity(&beta, active.iter()));
    }
}

/// Lightweight, cloneable handle to the StreamActor for lock-free status reads and async operations.
#[derive(Clone)]
pub struct StreamHandle {
    sender: mpsc::Sender<StreamCommand>,
    status_rx: watch::Receiver<Arc<HashMap<TenantId, StreamStatus>>>,
    preview_dir: PathBuf,
}

impl StreamHandle {
    pub async fn spawn(
        listen_port: u16,
        metrics: Arc<Metrics>,
        http_client: Client,
        tenants: TenantRepository,
        usage: UsageRepository,
    ) -> Result<Self> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let preview_dir =
            std::env::temp_dir().join(format!("rtmp-manager-hls-{}-{unique}", std::process::id()));
        create_preview_dir(&preview_dir)?;

        let (status_tx, status_rx) = watch::channel(Arc::new(HashMap::new()));
        let (sender, receiver) = mpsc::channel(64);

        let handle = Self {
            sender,
            status_rx,
            preview_dir: preview_dir.clone(),
        };

        let relay_executor: Arc<dyn RelayExecutor> = match std::env::var("RELAY_BROKER_URL") {
            Ok(url) if !url.trim().is_empty() => Arc::new(RedisRelayExecutor::new(url)),
            _ => Arc::new(LocalRelayExecutor::new(Arc::clone(&metrics))),
        };
        let actor = StreamActor {
            preview_dir,
            staged: HashMap::new(),
            active_relays: HashMap::new(),
            listen_port,
            http_client,
            tenants,
            usage,
            relay_executor,
            status_tx,
        };

        tokio::spawn(actor.run(receiver));

        Ok(handle)
    }

    pub async fn stage_stream(&self, tenant: Tenant, stream_key: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StreamCommand::StageStream {
                tenant: Box::new(tenant),
                stream_key,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Stream actor stopped"))?;
        rx.await.context("Stream actor dropped stage response")?
    }

    pub async fn publish_staged_stream(&self, tenant_id: TenantId) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StreamCommand::PublishStagedStream {
                tenant_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Stream actor stopped"))?;
        rx.await.context("Stream actor dropped publish response")?
    }

    pub async fn stop_publishing(&self, tenant_id: TenantId) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StreamCommand::StopPublishing {
                tenant_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Stream actor stopped"))?;
        rx.await.context("Stream actor dropped stop response")?
    }

    pub async fn grace_disconnect(&self, stream_key: &str) {
        let key = stream_key.to_owned();
        let sender = self.sender.clone();
        let _ = sender
            .send(StreamCommand::MarkUnpublished {
                stream_key: key.clone(),
            })
            .await;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let _ = sender
                .send(StreamCommand::EndStreamIfUnpublished { stream_key: key })
                .await;
        });
    }

    pub async fn emergency_stop(&self) {
        let _ = self.sender.send(StreamCommand::EmergencyStop).await;
    }

    pub async fn emergency_stop_tenant(&self, tenant_id: TenantId) {
        let _ = self
            .sender
            .send(StreamCommand::EmergencyStopTenant { tenant_id })
            .await;
    }

    pub fn run_test_stream(&self, duration_secs: u64, targets: Vec<TargetConfig>) {
        let _ = self.sender.try_send(StreamCommand::RunTestStream {
            duration_secs,
            targets,
        });
    }

    /// Returns the current stream status instantly with zero locks.
    pub fn status(&self, tenant_id: &TenantId) -> StreamStatus {
        self.status_rx
            .borrow()
            .get(tenant_id)
            .copied()
            .unwrap_or(StreamStatus {
                state: StreamState::Offline,
            })
    }

    /// Subscribes to stream status change events.
    pub fn subscribe_status(&self) -> watch::Receiver<Arc<HashMap<TenantId, StreamStatus>>> {
        self.status_rx.clone()
    }

    /// Returns an allowlisted preview file path for HTTP serving.
    pub fn preview_file(&self, tenant_id: &TenantId, name: &str) -> Option<PathBuf> {
        valid_preview_file_name(name).then(|| {
            self.preview_dir
                .join(stream_key_digest(tenant_id.as_str()))
                .join(name)
        })
    }
}
