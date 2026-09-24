use crate::config::{AppConfig, MAX_TARGET_COUNT, MAX_TEST_STREAM_DURATION_SECS, TargetConfig};
use crate::metrics::Metrics;
use crate::notifications::{NotificationDispatcher, NotificationTarget};
use crate::server::preview::{
    StreamState, StreamStatus, create_preview_dir, valid_preview_file_name,
};
use crate::server::relay::{RelayProcess, cancel_relays, run_direct_test, spawn_relay};
use crate::util::redact_secrets;
use anyhow::{Context, Result, bail};
use reqwest::Client;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot, watch};

struct StagedStream {
    stream_key: String,
    preview_process: Child,
    preview_failed: bool,
    published: bool,
    consecutive_restart_failures: u8,
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
    preview_playlist_ready: bool,
}

impl StreamActor {
    pub async fn run(mut self, mut receiver: mpsc::Receiver<StreamCommand>) {
        let mut status_check = Box::pin(tokio::time::sleep(Duration::from_secs(5)));

        loop {
            tokio::select! {
                _ = &mut status_check => {
                    self.check_preview_process_health().await;
                    status_check.as_mut().reset(
                        tokio::time::Instant::now() + self.preview_health_check_interval(),
                    );
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
                    }
                    // Don't reset the health timer here: a flooded command
                    // channel must not starve preview health checks.
                }
            }
        }

        self.cleanup().await;
    }

    async fn check_preview_process_health(&mut self) {
        let process_exited = {
            let Some(stream) = self.staged.as_mut() else {
                return;
            };
            match stream.preview_process.try_wait() {
                Ok(Some(status)) => {
                    tracing::error!(%status, "HLS preview process stopped; restarting it");
                    true
                }
                Err(error) => {
                    tracing::error!(%error, "Failed to inspect HLS preview process");
                    true
                }
                Ok(None) => false,
            }
        };

        if process_exited {
            let Some(stream_key) = self.staged.as_ref().map(|stream| stream.stream_key.clone())
            else {
                return;
            };
            let failures = self.staged.as_ref().map(|s| s.consecutive_restart_failures).unwrap_or(0);
            // Circuit-breaker: don't spawn-bomb ffmpeg forever on permanent errors.
            if failures >= 5 {
                if let Some(stream) = self.staged.as_mut() {
                    stream.preview_failed = true;
                }
                tracing::error!("HLS preview restart failed 5 times; giving up until next stage");
                self.update_status();
                return;
            }
            if let Some(stream) = self.staged.as_mut() {
                stream.preview_failed = true;
            }
            self.clear_preview_files().await;
            self.preview_playlist_ready = false;
            // Reap the exited child before spawning a replacement.
            if let Some(stream) = self.staged.as_mut() {
                let _ = tokio::time::timeout(Duration::from_secs(2), stream.preview_process.wait()).await;
            }
            match spawn_preview_process(self.listen_port, &stream_key, &self.preview_dir) {
                Ok(preview_process) => {
                    if let Some(stream) = self.staged.as_mut() {
                        stream.preview_process = preview_process;
                        stream.preview_failed = false;
                        stream.consecutive_restart_failures = 0;
                    }
                    tracing::info!("Restarted HLS preview process");
                }
                Err(error) => {
                    if let Some(stream) = self.staged.as_mut() {
                        stream.consecutive_restart_failures = failures.saturating_add(1);
                    }
                    tracing::error!(%error, "Failed to restart HLS preview process");
                }
            }
        } else if self.staged.is_some() {
            // Refresh cached playlist existence without blocking `compute_status`.
            self.preview_playlist_ready = tokio::fs::try_exists(self.preview_dir.join("index.m3u8"))
                .await
                .unwrap_or(false);
        }
        self.update_status();
    }

    async fn clear_preview_files(&self) {
        let Ok(mut entries) = tokio::fs::read_dir(&self.preview_dir).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            if name.to_str().is_some_and(valid_preview_file_name)
                && let Err(error) = tokio::fs::remove_file(entry.path()).await
            {
                tracing::warn!(%error, "Failed to remove stale HLS preview file");
            }
        }
    }

    fn preview_health_check_interval(&self) -> Duration {
        if self.staged.is_some() {
            Duration::from_millis(500)
        } else {
            Duration::from_secs(5)
        }
    }

    fn compute_status(&self) -> StreamStatus {
        let state = match self.staged.as_ref() {
            None => StreamState::Offline,
            Some(stream) if stream.preview_failed => StreamState::PreviewFailed,
            Some(_) if !self.preview_playlist_ready => StreamState::Preparing,
            Some(stream) if stream.published => StreamState::Live,
            Some(_) => StreamState::PreviewReady,
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
        if self.staged.is_some() {
            bail!("Another stream is already active");
        }
        create_preview_dir(&self.preview_dir).await?;

        let preview_process =
            spawn_preview_process(self.listen_port, &stream_key, &self.preview_dir)?;

        self.staged = Some(StagedStream {
            stream_key,
            preview_process,
            preview_failed: false,
            published: false,
            consecutive_restart_failures: 0,
        });
        // Best-effort initial playlist check; health loop refreshes it.
        self.preview_playlist_ready = tokio::fs::try_exists(self.preview_dir.join("index.m3u8"))
            .await
            .unwrap_or(false);

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

        // Bound notification dispatch so a slow webhook can't leak a detached task
        // that fires `went-live` after the stream stopped.
        tokio::spawn(async move {
            match tokio::time::timeout(Duration::from_secs(10), dispatcher.dispatch(&notification_targets)).await {
                Ok(()) => {}
                Err(_) => tracing::warn!("Going-live notification dispatch timed out"),
            }
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
            let _ = tokio::time::timeout(Duration::from_secs(5), stream.preview_process.wait()).await;
            if let Some(relays) = self.active_relays.remove(&stream.stream_key) {
                cancel_relays(relays).await;
            }
        }
        self.preview_playlist_ready = false;
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
            let _ = tokio::time::timeout(Duration::from_secs(5), stream.preview_process.wait()).await;
        }
        for (_, relays) in self.active_relays.drain() {
            cancel_relays(relays).await;
        }
        let _ = tokio::fs::remove_dir_all(&self.preview_dir).await;
    }
}

fn spawn_preview_process(listen_port: u16, stream_key: &str, preview_dir: &Path) -> Result<Child> {
    let source_url = format!("rtmp://127.0.0.1:{listen_port}/live/{stream_key}");
    let playlist = preview_dir.join("index.m3u8");
    let segments = preview_dir.join("segment_%06d.ts");
    let mut preview_process = tokio::process::Command::new("ffmpeg")
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
            &segments.to_string_lossy(),
            &playlist.to_string_lossy(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to start HLS preview FFmpeg")?;

    if let Some(stderr) = preview_process.stderr.take() {
        let stream_key = stream_key.to_owned();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        tracing::warn!(
                            message = %redact_secrets(&line, std::slice::from_ref(&stream_key)),
                            "HLS preview FFmpeg"
                        );
                    }
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!(%error, "HLS preview FFmpeg stderr read error");
                        break;
                    }
                }
            }
        });
    }

    Ok(preview_process)
}

/// Lightweight, cloneable handle to the StreamActor for lock-free status reads and async operations.
#[derive(Clone)]
pub struct StreamHandle {
    sender: mpsc::Sender<StreamCommand>,
    status_rx: watch::Receiver<StreamStatus>,
    preview_dir: PathBuf,
    metrics: Arc<Metrics>,
    test_stream_running: Arc<AtomicBool>,
}

impl StreamHandle {
    pub async fn spawn(
        listen_port: u16,
        metrics: Arc<Metrics>,
        http_client: Client,
        config_rx: watch::Receiver<Arc<AppConfig>>,
    ) -> Result<Self> {
        // `SystemTime` can fail on clock skew; fall back to a pid + random suffix
        // instead of failing server startup.
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_else(|_| {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                std::process::id().hash(&mut hasher);
                std::thread::current().id().hash(&mut hasher);
                hasher.finish() as u128
            });
        let preview_dir =
            std::env::temp_dir().join(format!("rtmp-manager-hls-{}-{unique}", std::process::id()));
        create_preview_dir(&preview_dir).await?;

        let (status_tx, status_rx) = watch::channel(StreamStatus {
            state: StreamState::Offline,
        });
        let (sender, receiver) = mpsc::channel(64);
        let test_stream_running = Arc::new(AtomicBool::new(false));

        let handle = Self {
            sender,
            status_rx,
            preview_dir: preview_dir.clone(),
            metrics: Arc::clone(&metrics),
            test_stream_running: Arc::clone(&test_stream_running),
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
            preview_playlist_ready: false,
        };

        tokio::spawn(async move {
            actor.run(receiver).await;
            tracing::warn!("Stream actor exited");
        });

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
        tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Stream actor stage timed out")?
            .context("Stream actor dropped stage response")?
    }

    pub async fn publish_staged_stream(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StreamCommand::PublishStagedStream { respond_to: tx })
            .await
            .map_err(|_| anyhow::anyhow!("Stream actor stopped"))?;
        tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Stream actor publish timed out")?
            .context("Stream actor dropped publish response")?
    }

    pub async fn stop_publishing(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StreamCommand::StopPublishing { respond_to: tx })
            .await
            .map_err(|_| anyhow::anyhow!("Stream actor stopped"))?;
        tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Stream actor stop timed out")?
            .context("Stream actor dropped stop response")?
    }

    pub async fn end_stream(&self, stream_key: &str) {
        // Best-effort with timeout: never block RTMP disconnect on a full actor queue.
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            self.sender.send(StreamCommand::EndStream {
                stream_key: stream_key.to_string(),
                respond_to: None,
            }),
        )
        .await
        .map_err(|_| tracing::warn!("Timed out queueing EndStream for actor"));
    }

    pub fn run_test_stream(&self, duration_secs: u64, targets: Vec<TargetConfig>) -> Result<()> {
        anyhow::ensure!(
            (1..=MAX_TEST_STREAM_DURATION_SECS).contains(&duration_secs),
            "Test stream duration must be between 1 and {MAX_TEST_STREAM_DURATION_SECS} seconds"
        );
        anyhow::ensure!(
            !targets.is_empty() && targets.len() <= MAX_TARGET_COUNT,
            "Test stream must have between 1 and {MAX_TARGET_COUNT} targets"
        );
        if run_direct_test(
            Arc::clone(&self.metrics),
            Arc::clone(&self.test_stream_running),
            duration_secs,
            targets,
        ) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("A test stream is already in progress"))
        }
    }

    /// Returns whether a direct test stream is currently in progress.
    pub fn is_test_stream_running(&self) -> bool {
        self.test_stream_running.load(Ordering::SeqCst)
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
