use crate::chat::{
    ChatInbox,
    youtube::{YouTubeChatConfig, YouTubeChatTarget, YouTubeIngestStatus},
};
use crate::config::{AppConfig, ConfigStore, TargetConfig};
use crate::metrics::Metrics;
use reqwest::Client;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::{Mutex, RwLock, watch};
use tokio::task::JoinHandle;

pub struct ProxyState {
    pub metrics: Arc<Metrics>,
    pub config: Arc<RwLock<AppConfig>>,
    pub http_client: Client,
    pub active_relays: Mutex<HashMap<String, Vec<RelayProcess>>>,
    staged_stream: Mutex<Option<StagedStream>>,
    next_stream_session_id: AtomicU64,
    preview_dir: PathBuf,
    pub chat_inbox: Mutex<ChatInbox>,
    chat_revision: watch::Sender<u64>,
    pub youtube_status: RwLock<Option<YouTubeIngestStatus>>,
    twitch_task: Mutex<Option<JoinHandle<()>>>,
    youtube_task: Mutex<Option<JoinHandle<()>>>,
    pub listen_port: u16,
    pub config_store: ConfigStore,
}

struct StagedStream {
    session_id: u64,
    stream_key: String,
    preview_process: Child,
    preview_failed: bool,
    published: bool,
}

pub struct RelayProcess {
    running: Arc<AtomicBool>,
    cancel: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl RelayProcess {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct StreamStatus {
    pub active: bool,
    pub preview_ready: bool,
    pub preview_failed: bool,
    pub published: bool,
    pub session_id: Option<u64>,
}

impl ProxyState {
    pub async fn new(
        metrics: Arc<Metrics>,
        config: AppConfig,
        http_client: Client,
        listen_port: u16,
        config_store: ConfigStore,
    ) -> anyhow::Result<Self> {
        let chat_inbox = ChatInbox::open(config_store.path(), config.chat.queue_capacity).await?;
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let preview_dir =
            std::env::temp_dir().join(format!("rtmp-manager-hls-{}-{unique}", std::process::id()));
        create_preview_dir(&preview_dir)?;
        Ok(Self {
            metrics,
            config: Arc::new(RwLock::new(config)),
            http_client,
            active_relays: Mutex::new(HashMap::new()),
            staged_stream: Mutex::new(None),
            next_stream_session_id: AtomicU64::new(1),
            preview_dir,
            chat_inbox: Mutex::new(chat_inbox),
            chat_revision: watch::channel(0).0,
            youtube_status: RwLock::new(None),
            twitch_task: Mutex::new(None),
            youtube_task: Mutex::new(None),
            listen_port,
            config_store,
        })
    }

    pub async fn stage_stream(&self, stream_key: String) -> anyhow::Result<()> {
        self.end_current_stream().await;
        create_preview_dir(&self.preview_dir)?;

        let source_url = format!("rtmp://127.0.0.1:{}/live/{}", self.listen_port, stream_key);
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
            // The source URL contains the private ingest stream key.
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        *self.staged_stream.lock().await = Some(StagedStream {
            session_id: self.next_stream_session_id.fetch_add(1, Ordering::Relaxed),
            stream_key,
            preview_process,
            preview_failed: false,
            published: false,
        });
        tracing::info!("Stream staged with local HLS preview");
        Ok(())
    }

    pub async fn publish_staged_stream(&self) -> anyhow::Result<()> {
        let (session_id, stream_key) = {
            let mut staged = self.staged_stream.lock().await;
            let stream = staged
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("No stream is currently staged"))?;
            if stream.published {
                anyhow::bail!("The staged stream is already published");
            }
            stream.published = true;
            (stream.session_id, stream.stream_key.clone())
        };

        let config = self.config.read().await;
        let active_targets = config
            .targets
            .iter()
            .filter(|target| target.enabled)
            .cloned()
            .collect::<Vec<_>>();
        let notification_targets = active_targets
            .iter()
            .map(crate::notifications::NotificationTarget::from)
            .collect::<Vec<_>>();
        let dispatcher = crate::notifications::NotificationDispatcher::new(
            &config.notifications,
            self.http_client.clone(),
        );
        drop(config);

        let source_url = format!("rtmp://127.0.0.1:{}/live/{}", self.listen_port, stream_key);
        let mut relays = Vec::new();
        for target in active_targets {
            relays.push(spawn_relay_supervisor(
                Arc::clone(&self.metrics),
                source_url.clone(),
                target,
            ));
        }
        let staged = self.staged_stream.lock().await;
        let still_published = staged
            .as_ref()
            .is_some_and(|stream| stream.session_id == session_id && stream.published);
        if !still_published {
            drop(staged);
            for relay in relays {
                let _ = relay.cancel.send(true);
                let _ = relay.task.await;
            }
            anyhow::bail!("The staged stream changed while publishing was starting");
        }
        self.active_relays.lock().await.insert(stream_key, relays);
        drop(staged);
        tokio::spawn(async move {
            dispatcher.dispatch(&notification_targets).await;
        });
        tracing::info!("Staged stream published to enabled targets");
        Ok(())
    }

    pub async fn stop_publishing(&self) -> anyhow::Result<()> {
        let stream_key = {
            let mut staged = self.staged_stream.lock().await;
            let stream = staged
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("No stream is currently staged"))?;
            stream.published = false;
            stream.stream_key.clone()
        };
        self.stop_relays(&stream_key).await;
        tracing::info!("External publishing stopped; stream remains staged");
        Ok(())
    }

    pub async fn end_stream(&self, stream_key: &str) {
        let is_current = self
            .staged_stream
            .lock()
            .await
            .as_ref()
            .is_some_and(|stream| stream.stream_key == stream_key);
        if is_current {
            self.end_current_stream().await;
        } else {
            self.stop_relays(stream_key).await;
        }
    }

    async fn end_current_stream(&self) {
        let stream = self.staged_stream.lock().await.take();
        if let Some(mut stream) = stream {
            self.stop_relays(&stream.stream_key).await;
            let _ = stream.preview_process.kill().await;
        }
        if let Err(error) = tokio::fs::remove_dir_all(&self.preview_dir).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "Failed to remove HLS preview files");
        }
    }

    async fn stop_relays(&self, stream_key: &str) {
        let relays = self.active_relays.lock().await.remove(stream_key);
        if let Some(relays) = relays {
            for relay in relays {
                let _ = relay.cancel.send(true);
                let _ = relay.task.await;
            }
        }
    }

    pub async fn stream_status(&self) -> StreamStatus {
        let mut staged = self.staged_stream.lock().await;
        if let Some(stream) = staged.as_mut() {
            match stream.preview_process.try_wait() {
                Ok(Some(status)) if !stream.preview_failed => {
                    tracing::error!(%status, "HLS preview process stopped unexpectedly");
                    stream.preview_failed = true;
                }
                Err(error) if !stream.preview_failed => {
                    tracing::error!(%error, "Failed to inspect HLS preview process");
                    stream.preview_failed = true;
                }
                _ => {}
            }
        }
        let preview_failed = staged.as_ref().is_some_and(|stream| stream.preview_failed);

        StreamStatus {
            active: staged.is_some(),
            preview_ready: staged.is_some()
                && !preview_failed
                && self.preview_dir.join("index.m3u8").is_file(),
            preview_failed,
            published: staged.as_ref().is_some_and(|stream| stream.published),
            session_id: staged.as_ref().map(|stream| stream.session_id),
        }
    }

    pub fn preview_file(&self, name: &str) -> Option<PathBuf> {
        valid_preview_file_name(name).then(|| self.preview_dir.join(name))
    }

    pub fn notify_chat_changed(&self) {
        self.chat_revision
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub fn subscribe_chat_changes(&self) -> watch::Receiver<u64> {
        self.chat_revision.subscribe()
    }

    pub async fn apply_chat_config(self: &Arc<Self>) -> anyhow::Result<()> {
        let chat = self.config.read().await.chat.clone();
        self.chat_inbox
            .lock()
            .await
            .resize(chat.queue_capacity)
            .await?;

        if let Some(task) = self.twitch_task.lock().await.take() {
            task.abort();
        }
        if let Some(task) = self.youtube_task.lock().await.take() {
            task.abort();
        }
        *self.youtube_status.write().await = None;

        if let Some(channel) = chat
            .twitch_channel
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().trim_start_matches('#').to_ascii_lowercase())
        {
            let state = Arc::clone(self);
            let task = tokio::spawn(crate::chat::twitch::run(state, channel.clone()));
            *self.twitch_task.lock().await = Some(task);
            tracing::info!(channel, "Twitch anonymous IRC ingest configured");
        }

        let Some(api_key) = chat
            .youtube_api_key
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(());
        };
        let target = chat
            .youtube_live_chat_id
            .filter(|value| !value.trim().is_empty())
            .map(YouTubeChatTarget::LiveChat)
            .or_else(|| {
                chat.youtube_video_id
                    .filter(|value| !value.trim().is_empty())
                    .map(YouTubeChatTarget::Video)
            })
            .or_else(|| {
                chat.youtube_channel_id
                    .filter(|value| !value.trim().is_empty())
                    .map(YouTubeChatTarget::Channel)
            });
        let Some(target) = target else {
            return Ok(());
        };
        if !chat.youtube_polling_enabled {
            *self.youtube_status.write().await = Some(YouTubeIngestStatus {
                state: "off".into(),
                detail: "Polling is off. Turn it on when the YouTube stream is live.".into(),
                ..YouTubeIngestStatus::default()
            });
            self.notify_chat_changed();
            tracing::info!("YouTube live chat polling is off");
            return Ok(());
        }

        let state = Arc::clone(self);
        let task = tokio::spawn(crate::chat::youtube::run(
            state,
            YouTubeChatConfig {
                api_key,
                target,
                min_poll_interval: Duration::from_secs(chat.youtube_min_poll_interval_secs),
                adaptive_polling: chat.youtube_adaptive_polling,
            },
        ));
        *self.youtube_task.lock().await = Some(task);
        tracing::info!("YouTube live chat ingest configured");
        Ok(())
    }
}

impl Drop for ProxyState {
    fn drop(&mut self) {
        if let Some(stream) = self.staged_stream.get_mut().as_mut() {
            let _ = stream.preview_process.start_kill();
        }
        for relays in self.active_relays.get_mut().values_mut() {
            for relay in relays {
                let _ = relay.cancel.send(true);
                relay.task.abort();
            }
        }
        let _ = std::fs::remove_dir_all(&self.preview_dir);
    }
}

fn spawn_relay_supervisor(
    metrics: Arc<Metrics>,
    source_url: String,
    target: TargetConfig,
) -> RelayProcess {
    let running = Arc::new(AtomicBool::new(false));
    let task_running = Arc::clone(&running);
    let (cancel, cancel_rx) = watch::channel(false);
    let task = tokio::spawn(supervise_relay(
        metrics,
        source_url,
        target,
        task_running,
        cancel_rx,
    ));
    RelayProcess {
        running,
        cancel,
        task,
    }
}

async fn supervise_relay(
    metrics: Arc<Metrics>,
    source_url: String,
    target: TargetConfig,
    running: Arc<AtomicBool>,
    mut cancel: watch::Receiver<bool>,
) {
    let bitrate = metrics.register_target(target.name.clone());
    let destination = if target.stream_key.is_empty() {
        target.url.clone()
    } else if target.url.ends_with('/') {
        format!("{}{}", target.url, target.stream_key)
    } else {
        format!("{}/{}", target.url, target.stream_key)
    };
    let secrets = [
        source_url.clone(),
        destination.clone(),
        target.stream_key.clone(),
    ];
    let mut retry_seconds = 1_u64;
    let mut attempt = 0_u64;

    loop {
        if *cancel.borrow() {
            break;
        }
        attempt += 1;
        let started_at = tokio::time::Instant::now();
        let child = tokio::process::Command::new("ffmpeg")
            .args([
                "-loglevel",
                "warning",
                "-stats_period",
                "1",
                "-progress",
                "pipe:1",
                "-i",
                &source_url,
                "-c",
                "copy",
                "-f",
                "flv",
                &destination,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn();

        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                tracing::error!(name = %target.name, attempt, %error, "Failed to start target relay FFmpeg");
                if wait_for_retry(&mut cancel, retry_seconds).await {
                    break;
                }
                retry_seconds = (retry_seconds * 2).min(30);
                continue;
            }
        };
        running.store(true, Ordering::Relaxed);
        tracing::info!(name = %target.name, attempt, "Stream target relay process started");

        let stdout_task = child.stdout.take().map(|stdout| {
            let bitrate = Arc::clone(&bitrate);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(value) = line.strip_prefix("bitrate=")
                        && let Some(bps) = parse_ffmpeg_bitrate(value)
                    {
                        bitrate.update_from_ffmpeg(bps);
                    }
                }
            })
        });
        let stderr_task = child.stderr.take().map(|stderr| {
            let target_name = target.name.clone();
            let secrets = secrets.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let detail = redact_secrets(&line, &secrets);
                    if !detail.trim().is_empty() {
                        tracing::warn!(name = %target_name, %detail, "Relay FFmpeg diagnostic");
                    }
                }
            })
        });

        let exit = tokio::select! {
            changed = cancel.changed() => {
                let _ = child.kill().await;
                let _ = changed;
                None
            }
            result = child.wait() => Some(result),
        };
        running.store(false, Ordering::Relaxed);
        bitrate.update_from_ffmpeg(0);
        if let Some(task) = stdout_task {
            let _ = task.await;
        }
        if let Some(task) = stderr_task {
            let _ = task.await;
        }

        let Some(exit) = exit else {
            break;
        };
        match exit {
            Ok(status) => {
                tracing::error!(name = %target.name, %status, "Stream target relay disconnected")
            }
            Err(error) => {
                tracing::error!(name = %target.name, %error, "Failed while waiting for target relay")
            }
        }
        if started_at.elapsed() >= Duration::from_secs(30) {
            retry_seconds = 1;
        }
        tracing::warn!(name = %target.name, retry_seconds, "Target relay will reconnect");
        if wait_for_retry(&mut cancel, retry_seconds).await {
            break;
        }
        retry_seconds = (retry_seconds * 2).min(30);
    }

    running.store(false, Ordering::Relaxed);
    metrics.unregister_target(&target.name);
    tracing::info!(name = %target.name, "Stream target relay supervisor stopped");
}

async fn wait_for_retry(cancel: &mut watch::Receiver<bool>, seconds: u64) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(seconds)) => false,
        _ = cancel.changed() => true,
    }
}

fn parse_ffmpeg_bitrate(value: &str) -> Option<u64> {
    let value = value.trim();
    let number = value.strip_suffix("kbits/s")?.trim().parse::<f64>().ok()?;
    number
        .is_finite()
        .then_some((number.max(0.0) * 1_000.0) as u64)
}

pub(crate) fn redact_secrets(text: &str, secrets: &[String]) -> String {
    let redacted = secrets
        .iter()
        .filter(|secret| !secret.is_empty())
        .fold(text.to_owned(), |redacted, secret| {
            redacted.replace(secret, "[REDACTED]")
        });
    redacted
        .split_whitespace()
        .map(|part| {
            if part.contains("rtmp://") || part.contains("rtmps://") {
                "[RTMP_URL_REDACTED]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn valid_preview_file_name(name: &str) -> bool {
    name == "index.m3u8"
        || name
            .strip_prefix("segment_")
            .and_then(|name| name.strip_suffix(".ts"))
            .is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
}

fn create_preview_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_ffmpeg_bitrate, redact_secrets, valid_preview_file_name};

    #[test]
    fn preview_files_are_strictly_allowlisted() {
        assert!(valid_preview_file_name("index.m3u8"));
        assert!(valid_preview_file_name("segment_000123.ts"));
        assert!(!valid_preview_file_name("../config.sqlite3"));
        assert!(!valid_preview_file_name("segment_.ts"));
        assert!(!valid_preview_file_name("segment_1.m3u8"));
    }

    #[test]
    fn parses_ffmpeg_progress_bitrate() {
        assert_eq!(parse_ffmpeg_bitrate(" 2450.5kbits/s"), Some(2_450_500));
        assert_eq!(parse_ffmpeg_bitrate("N/A"), None);
    }

    #[test]
    fn ffmpeg_diagnostics_redact_every_stream_key() {
        let secrets = ["local-key".to_owned(), "twitch-key".to_owned()];
        let line = "rtmp://localhost/live/local-key -> rtmp://twitch/app/twitch-key";
        let redacted = redact_secrets(line, &secrets);
        assert_eq!(redacted, "[RTMP_URL_REDACTED] -> [RTMP_URL_REDACTED]");
    }
}
