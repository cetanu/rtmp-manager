use crate::config::{AppConfig, TargetConfig};
use crate::metrics::Metrics;
use crate::notifications::{NotificationDispatcher, NotificationTarget};
use crate::server::preview::{
    StreamState, StreamStatus, create_preview_dir, valid_preview_file_name,
};
use crate::server::relay::{RelayProcess, cancel_relays, run_direct_test, spawn_relay};
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
    stream_key: String,
    preview_process: Child,
    preview_failed: bool,
    published: bool,
}

pub enum StreamCommand {
    StageStream {
        stream_key: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    PublishStagedStream {
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopPublishing {
        respond_to: oneshot::Sender<Result<()>>,
    },
    EndStream {
        stream_key: String,
        respond_to: Option<oneshot::Sender<()>>,
    },
    RunTestStream {
        duration_secs: u64,
        targets: Vec<TargetConfig>,
    },
}

/// Actor exclusively managing HLS preview FFmpeg processes, RTMP target relays, and lifecycle state.
pub struct StreamActor {
    preview_dir: PathBuf,
    staged: Option<StagedStream>,
    active_relays: HashMap<String, Vec<RelayProcess>>,
    listen_port: u16,
    metrics: Arc<Metrics>,
    http_client: Client,
    config_rx: watch::Receiver<Arc<AppConfig>>,
    status_tx: watch::Sender<StreamStatus>,
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
                        StreamCommand::StageStream { stream_key, respond_to } => {
                            let res = self.handle_stage_stream(stream_key).await;
                            let _ = respond_to.send(res);
                        }
                        StreamCommand::PublishStagedStream { respond_to } => {
                            let res = self.handle_publish_staged().await;
                            let _ = respond_to.send(res);
                        }
                        StreamCommand::StopPublishing { respond_to } => {
                            let res = self.handle_stop_publishing().await;
                            let _ = respond_to.send(res);
                        }
                        StreamCommand::EndStream { stream_key, respond_to } => {
                            self.handle_end_stream(&stream_key).await;
                            if let Some(respond_to) = respond_to {
                                let _ = respond_to.send(());
                            }
                        }
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
        if let Some(stream) = self.staged.as_mut()
            && !stream.preview_failed
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
                _ => {}
            }
        }
        self.update_status();
    }

    fn compute_status(&self) -> StreamStatus {
        let state = match self.staged.as_ref() {
            None => StreamState::Offline,
            Some(stream) if stream.published => StreamState::Live,
            Some(stream) if stream.preview_failed => StreamState::PreviewFailed,
            Some(_) if self.preview_dir.join("index.m3u8").is_file() => StreamState::PreviewReady,
            Some(_) => StreamState::Preparing,
        };
        StreamStatus { state }
    }

    fn update_status(&mut self) {
        let status = self.compute_status();
        if *self.status_tx.borrow() != status {
            self.status_tx.send_replace(status);
        }
    }

    async fn handle_stage_stream(&mut self, stream_key: String) -> Result<()> {
        self.end_current_stream().await;
        create_preview_dir(&self.preview_dir)?;

        let source_url = format!("rtmp://127.0.0.1:{}/live/{stream_key}", self.listen_port);
        let playlist = self.preview_dir.join("index.m3u8");
        let segments = self.preview_dir.join("segment_%06d.ts");
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

        self.staged = Some(StagedStream {
            stream_key,
            preview_process,
            preview_failed: false,
            published: false,
        });

        self.update_status();
        tracing::info!("Stream staged with local HLS preview");
        Ok(())
    }

    async fn handle_publish_staged(&mut self) -> Result<()> {
        let stream = self
            .staged
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No stream is currently staged"))?;
        if stream.published {
            bail!("The staged stream is already published");
        }
        stream.published = true;
        let stream_key = stream.stream_key.clone();

        let config = self.config_rx.borrow().clone();
        let active_targets = config
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
            NotificationDispatcher::new(&config.notifications, self.http_client.clone());

        let source_url = format!("rtmp://127.0.0.1:{}/live/{stream_key}", self.listen_port);
        let mut relays = Vec::new();
        for target in active_targets {
            relays.push(spawn_relay(
                Arc::clone(&self.metrics),
                source_url.clone(),
                target,
            ));
        }

        self.active_relays.insert(stream_key, relays);
        self.update_status();

        tokio::spawn(async move {
            dispatcher.dispatch(&notification_targets).await;
        });

        tracing::info!("Staged stream published to enabled targets");
        Ok(())
    }

    async fn handle_stop_publishing(&mut self) -> Result<()> {
        let stream = self
            .staged
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No stream is currently staged"))?;
        stream.published = false;
        let stream_key = stream.stream_key.clone();

        if let Some(relays) = self.active_relays.remove(&stream_key) {
            cancel_relays(relays).await;
        }

        self.update_status();
        tracing::info!("External publishing stopped; stream remains staged");
        Ok(())
    }

    async fn handle_end_stream(&mut self, stream_key: &str) {
        let is_current = self
            .staged
            .as_ref()
            .is_some_and(|stream| stream.stream_key == stream_key);
        if is_current {
            self.end_current_stream().await;
        } else if let Some(relays) = self.active_relays.remove(stream_key) {
            cancel_relays(relays).await;
        }
    }

    async fn end_current_stream(&mut self) {
        if let Some(mut stream) = self.staged.take() {
            let _ = stream.preview_process.kill().await;
            if let Some(relays) = self.active_relays.remove(&stream.stream_key) {
                cancel_relays(relays).await;
            }
        }
        if let Err(error) = tokio::fs::remove_dir_all(&self.preview_dir).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "Failed to remove HLS preview files");
        }
        self.update_status();
    }

    async fn cleanup(&mut self) {
        if let Some(mut stream) = self.staged.take() {
            let _ = stream.preview_process.kill().await;
        }
        for (_, relays) in self.active_relays.drain() {
            cancel_relays(relays).await;
        }
        let _ = tokio::fs::remove_dir_all(&self.preview_dir).await;
    }
}

/// Lightweight, cloneable handle to the StreamActor for lock-free status reads and async operations.
#[derive(Clone)]
pub struct StreamHandle {
    sender: mpsc::Sender<StreamCommand>,
    status_rx: watch::Receiver<StreamStatus>,
    preview_dir: PathBuf,
}

impl StreamHandle {
    pub async fn spawn(
        listen_port: u16,
        metrics: Arc<Metrics>,
        http_client: Client,
        config_rx: watch::Receiver<Arc<AppConfig>>,
    ) -> Result<Self> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let preview_dir =
            std::env::temp_dir().join(format!("rtmp-manager-hls-{}-{unique}", std::process::id()));
        create_preview_dir(&preview_dir)?;

        let (status_tx, status_rx) = watch::channel(StreamStatus {
            state: StreamState::Offline,
        });
        let (sender, receiver) = mpsc::channel(64);

        let handle = Self {
            sender,
            status_rx,
            preview_dir: preview_dir.clone(),
        };

        let actor = StreamActor {
            preview_dir,
            staged: None,
            active_relays: HashMap::new(),
            listen_port,
            metrics,
            http_client,
            config_rx,
            status_tx,
        };

        tokio::spawn(actor.run(receiver));

        Ok(handle)
    }

    pub async fn stage_stream(&self, stream_key: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StreamCommand::StageStream {
                stream_key,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Stream actor stopped"))?;
        rx.await.context("Stream actor dropped stage response")?
    }

    pub async fn publish_staged_stream(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StreamCommand::PublishStagedStream { respond_to: tx })
            .await
            .map_err(|_| anyhow::anyhow!("Stream actor stopped"))?;
        rx.await.context("Stream actor dropped publish response")?
    }

    pub async fn stop_publishing(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StreamCommand::StopPublishing { respond_to: tx })
            .await
            .map_err(|_| anyhow::anyhow!("Stream actor stopped"))?;
        rx.await.context("Stream actor dropped stop response")?
    }

    pub async fn end_stream(&self, stream_key: &str) {
        let _ = self.sender.send(StreamCommand::EndStream {
            stream_key: stream_key.to_string(),
            respond_to: None,
        }).await;
    }

    pub fn run_test_stream(&self, duration_secs: u64, targets: Vec<TargetConfig>) {
        let _ = self.sender.try_send(StreamCommand::RunTestStream {
            duration_secs,
            targets,
        });
    }

    /// Returns the current stream status instantly with zero locks.
    pub fn status(&self) -> StreamStatus {
        *self.status_rx.borrow()
    }

    /// Subscribes to stream status change events.
    pub fn subscribe_status(&self) -> watch::Receiver<StreamStatus> {
        self.status_rx.clone()
    }

    /// Returns an allowlisted preview file path for HTTP serving.
    pub fn preview_file(&self, name: &str) -> Option<PathBuf> {
        valid_preview_file_name(name).then(|| self.preview_dir.join(name))
    }
}
