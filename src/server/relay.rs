use crate::config::{EncodingMode, TargetConfig};
use crate::metrics::Metrics;
use crate::util::redact_secrets;
use std::collections::HashMap;
#[cfg(unix)]
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::watch;
use tokio::task::JoinHandle;

static RELAY_JOB_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn ffmpeg_command(args: &[String]) -> tokio::process::Command {
    let cpu = std::env::var("RELAY_MAX_CPU_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0);
    let memory = std::env::var("RELAY_MAX_MEMORY_MB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0);
    #[cfg(unix)]
    if (cpu.is_some() || memory.is_some())
        && Command::new("prlimit").arg("--version").output().is_ok()
    {
        let mut command = tokio::process::Command::new("prlimit");
        if let Some(cpu) = cpu {
            command.arg(format!("--cpu={cpu}"));
        }
        if let Some(memory) = memory {
            command.arg(format!("--as={}", memory * 1024 * 1024));
        }
        command.arg("--").arg("ffmpeg").args(args);
        return command;
    }
    let mut command = tokio::process::Command::new("ffmpeg");
    command.args(args);
    command
}

pub struct RelayProcess {
    pub cancel: watch::Sender<bool>,
    pub task: JoinHandle<()>,
}

pub trait RelayExecutor: Send + Sync {
    fn spawn(&self, tenant_id: String, source_url: String, target: TargetConfig) -> RelayProcess;
    fn spawn_standby(&self, tenant_id: String, target: TargetConfig) -> RelayProcess;
}

#[derive(Clone)]
pub struct LocalRelayExecutor {
    metrics: Arc<Metrics>,
}

/// Publishes relay intents to Redis Streams for execution by separate media workers.
#[derive(Clone)]
pub struct RedisRelayExecutor {
    url: String,
}

impl RedisRelayExecutor {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

#[derive(serde::Deserialize)]
struct RelayIntent {
    kind: String,
    job_id: String,
    tenant_id: String,
    source_url: Option<String>,
    target: Option<TargetConfig>,
}

fn decode_relay_intent(item: &redis::streams::StreamId) -> anyhow::Result<RelayIntent> {
    let payload = item
        .map
        .get("payload")
        .and_then(|value| redis::from_redis_value::<String>(value.clone()).ok())
        .ok_or_else(|| anyhow::anyhow!("relay intent has no payload"))?;
    Ok(serde_json::from_str(&payload)?)
}

async fn apply_relay_intent(intent: RelayIntent, active: &mut HashMap<String, RelayProcess>) {
    if intent.kind == "stop" {
        if let Some(process) = active.remove(&intent.job_id) {
            let _ = process.cancel.send(true);
            let _ = process.task.await;
        }
    } else if intent.kind == "start" {
        let Some(source_url) = intent.source_url else {
            return;
        };
        let Some(target) = intent.target else { return };
        let process = spawn_relay(
            Arc::new(Metrics::default()),
            intent.tenant_id,
            source_url,
            target,
        );
        active.insert(intent.job_id, process);
    } else if intent.kind == "standby" {
        let Some(target) = intent.target else { return };
        let process = spawn_standby_relay(Arc::new(Metrics::default()), intent.tenant_id, target);
        active.insert(intent.job_id, process);
    }
}

pub async fn run_redis_worker(url: &str) -> anyhow::Result<()> {
    let client = redis::Client::open(url)?;
    let mut connection = client.get_multiplexed_async_connection().await?;
    let stream = "rtmp-manager:relay-intents";
    let group = "rtmp-manager-workers";
    let _ = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(stream)
        .arg(group)
        .arg("0")
        .arg("MKSTREAM")
        .query_async::<()>(&mut connection)
        .await;
    let consumer = format!("worker-{}", std::process::id());
    let mut active: HashMap<String, RelayProcess> = HashMap::new();
    loop {
        let reclaimed: redis::streams::StreamAutoClaimReply = redis::cmd("XAUTOCLAIM")
            .arg(stream)
            .arg(group)
            .arg(&consumer)
            .arg(30_000)
            .arg("0-0")
            .arg("COUNT")
            .arg(16)
            .query_async(&mut connection)
            .await?;
        for item in reclaimed.claimed {
            if let Ok(intent) = decode_relay_intent(&item) {
                apply_relay_intent(intent, &mut active).await;
            }
            let _: () = redis::cmd("XACK")
                .arg(stream)
                .arg(group)
                .arg(item.id)
                .query_async(&mut connection)
                .await?;
        }
        let reply: redis::streams::StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(group)
            .arg(&consumer)
            .arg("BLOCK")
            .arg(5000)
            .arg("COUNT")
            .arg(16)
            .arg("STREAMS")
            .arg(stream)
            .arg(">")
            .query_async(&mut connection)
            .await?;
        for stream in reply.keys {
            for item in stream.ids {
                if let Ok(intent) = decode_relay_intent(&item) {
                    apply_relay_intent(intent, &mut active).await;
                }
                let _: () = redis::cmd("XACK")
                    .arg("rtmp-manager:relay-intents")
                    .arg(group)
                    .arg(item.id)
                    .query_async(&mut connection)
                    .await?;
            }
        }
    }
}

impl RelayExecutor for RedisRelayExecutor {
    fn spawn(&self, tenant_id: String, source_url: String, target: TargetConfig) -> RelayProcess {
        let (cancel, mut cancel_rx) = watch::channel(false);
        let url = self.url.clone();
        let task = tokio::spawn(async move {
            let client = match redis::Client::open(url) {
                Ok(client) => client,
                Err(error) => {
                    tracing::error!(%error, "Invalid relay broker URL");
                    return;
                }
            };
            let mut connection = match client.get_multiplexed_async_connection().await {
                Ok(connection) => connection,
                Err(error) => {
                    tracing::error!(%error, "Relay broker connection failed");
                    return;
                }
            };
            let job_id = format!(
                "{}-{}-{}",
                std::process::id(),
                crate::util::now_unix_ms(),
                RELAY_JOB_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            );
            let payload = match serde_json::to_string(&serde_json::json!({
                "kind": "start", "job_id": job_id, "tenant_id": tenant_id, "source_url": source_url, "target": target
            })) {
                Ok(payload) => payload,
                Err(error) => {
                    tracing::error!(%error, "Failed to serialize relay intent");
                    return;
                }
            };
            if let Err(error) = redis::cmd("XADD")
                .arg("rtmp-manager:relay-intents")
                .arg("*")
                .arg("payload")
                .arg(payload)
                .query_async::<String>(&mut connection)
                .await
            {
                tracing::error!(%error, "Failed to publish relay intent");
                return;
            }
            let _ = cancel_rx.changed().await;
            let stop = serde_json::json!({ "kind": "stop", "job_id": job_id }).to_string();
            let _ = redis::cmd("XADD")
                .arg("rtmp-manager:relay-intents")
                .arg("*")
                .arg("payload")
                .arg(stop)
                .query_async::<String>(&mut connection)
                .await;
        });
        RelayProcess { cancel, task }
    }

    fn spawn_standby(&self, tenant_id: String, target: TargetConfig) -> RelayProcess {
        let (cancel, mut cancel_rx) = watch::channel(false);
        let url = self.url.clone();
        let task = tokio::spawn(async move {
            let Ok(client) = redis::Client::open(url) else {
                return;
            };
            let Ok(mut connection) = client.get_multiplexed_async_connection().await else {
                return;
            };
            let job_id = format!(
                "{}-{}-{}",
                std::process::id(),
                crate::util::now_unix_ms(),
                RELAY_JOB_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            );
            let payload = serde_json::json!({
                "kind": "standby", "job_id": job_id, "tenant_id": tenant_id, "target": target
            })
            .to_string();
            let _ = redis::cmd("XADD")
                .arg("rtmp-manager:relay-intents")
                .arg("*")
                .arg("payload")
                .arg(payload)
                .query_async::<String>(&mut connection)
                .await;
            let _ = cancel_rx.changed().await;
            let stop = serde_json::json!({ "kind": "stop", "job_id": job_id }).to_string();
            let _ = redis::cmd("XADD")
                .arg("rtmp-manager:relay-intents")
                .arg("*")
                .arg("payload")
                .arg(stop)
                .query_async::<String>(&mut connection)
                .await;
        });
        RelayProcess { cancel, task }
    }
}

impl LocalRelayExecutor {
    pub fn new(metrics: Arc<Metrics>) -> Self {
        Self { metrics }
    }
}

impl RelayExecutor for LocalRelayExecutor {
    fn spawn(&self, tenant_id: String, source_url: String, target: TargetConfig) -> RelayProcess {
        spawn_relay(Arc::clone(&self.metrics), tenant_id, source_url, target)
    }

    fn spawn_standby(&self, tenant_id: String, target: TargetConfig) -> RelayProcess {
        spawn_standby_relay(Arc::clone(&self.metrics), tenant_id, target)
    }
}

pub fn spawn_relay(
    metrics: Arc<Metrics>,
    tenant_id: String,
    source_url: String,
    target: TargetConfig,
) -> RelayProcess {
    let (cancel, cancel_rx) = watch::channel(false);
    let task = tokio::spawn(supervise_relay(
        metrics, tenant_id, source_url, target, cancel_rx,
    ));
    RelayProcess { cancel, task }
}

fn spawn_standby_relay(
    metrics: Arc<Metrics>,
    tenant_id: String,
    target: TargetConfig,
) -> RelayProcess {
    let (cancel, mut cancel_rx) = watch::channel(false);
    let destination = target_destination(&target);
    let target_name = target.name.clone();
    let task = tokio::spawn(async move {
        let bitrate = metrics.register_target(tenant_id.clone(), target_name.clone());
        let args = standby_ffmpeg_args(&target, &destination);
        let mut child = match ffmpeg_command(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                tracing::error!(tenant_id = %tenant_id, name = %target_name, %error, "Failed to start standby relay");
                metrics.unregister_target(&tenant_id, &target_name);
                return;
            }
        };
        tokio::select! {
            _ = cancel_rx.changed() => {
                let _ = tokio::time::timeout(Duration::from_secs(5), child.kill()).await;
            }
            _ = child.wait() => {}
        }
        bitrate.update_from_ffmpeg(0);
        metrics.unregister_target(&tenant_id, &target_name);
    });
    RelayProcess { cancel, task }
}

fn target_encoder(target: &TargetConfig) -> &'static str {
    match target.encoding.hardware_encoder.as_deref() {
        Some("nvenc") => "h264_nvenc",
        Some("vaapi") => "h264_vaapi",
        Some("qsv") => "h264_qsv",
        Some("videotoolbox") => "h264_videotoolbox",
        _ => "libx264",
    }
}

fn standby_ffmpeg_args(target: &TargetConfig, destination: &str) -> Vec<String> {
    let width = target.encoding.width.unwrap_or(1280);
    let height = target.encoding.height.unwrap_or(720);
    let mut args = vec![
        "-loglevel".into(),
        "warning".into(),
        "-re".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("color=c=black:s={width}x{height}:r=30"),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "anullsrc=channel_layout=stereo:sample_rate=48000".into(),
        "-c:v".into(),
        target_encoder(target).into(),
        "-preset".into(),
        "veryfast".into(),
        "-tune".into(),
        "zerolatency".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "128k".into(),
        "-f".into(),
        "flv".into(),
        destination.into(),
    ];
    if let Some(bitrate) = target.encoding.max_video_bitrate_kbps {
        let output_index = args.len() - 1;
        args.splice(
            output_index..output_index,
            ["-b:v".into(), format!("{bitrate}k")],
        );
    }
    args
}

const RELAY_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn cancel_relays(relays: Vec<RelayProcess>) {
    for relay in relays {
        let _ = relay.cancel.send(true);
        let _ = relay.task.await;
    }
}

pub fn target_destination(target: &TargetConfig) -> String {
    if target.stream_key.is_empty() {
        target.url.clone()
    } else if target.url.ends_with('/') {
        format!("{}{}", target.url, target.stream_key)
    } else {
        format!("{}/{}", target.url, target.stream_key)
    }
}

pub fn run_direct_test(duration_secs: u64, targets: Vec<TargetConfig>) {
    tokio::spawn(async move {
        tracing::info!(
            duration_secs,
            target_count = targets.len(),
            "Starting direct test stream to enabled targets"
        );
        let mut tasks = tokio::task::JoinSet::new();
        for target in targets {
            tasks.spawn(async move {
                let destination = target_destination(&target);
                let video_source =
                    format!("testsrc=duration={duration_secs}:size=1280x720:rate=30");
                let audio_source = format!("sine=frequency=1000:duration={duration_secs}");
                let output = tokio::process::Command::new("ffmpeg")
                    .args([
                        "-hide_banner",
                        "-loglevel",
                        "warning",
                        "-re",
                        "-f",
                        "lavfi",
                        "-i",
                        &video_source,
                        "-f",
                        "lavfi",
                        "-i",
                        &audio_source,
                        "-c:v",
                        "libx264",
                        "-preset",
                        "veryfast",
                        "-pix_fmt",
                        "yuv420p",
                        "-c:a",
                        "aac",
                        "-b:a",
                        "128k",
                        "-f",
                        "flv",
                        &destination,
                    ])
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .output()
                    .await;
                match output {
                    Ok(output) if output.status.success() => {
                        tracing::info!(
                            name = %target.name,
                            "Direct target test completed successfully"
                        );
                    }
                    Ok(output) => {
                        let detail = safe_ffmpeg_failure(
                            &String::from_utf8_lossy(&output.stderr),
                            &[target.stream_key.clone(), destination],
                        );
                        tracing::error!(
                            name = %target.name,
                            status = %output.status,
                            %detail,
                            "Direct target test failed"
                        );
                    }
                    Err(error) => {
                        tracing::error!(
                            name = %target.name,
                            %error,
                            "Direct target test FFmpeg failed to start"
                        );
                    }
                }
            });
        }
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                tracing::error!(%error, "Direct target test task failed");
            }
        }
    });
}

async fn supervise_relay(
    metrics: Arc<Metrics>,
    tenant_id: String,
    source_url: String,
    target: TargetConfig,
    mut cancel: watch::Receiver<bool>,
) {
    let relay_span = tracing::info_span!(
        "relay.worker",
        otel.name = "relay.worker",
        tenant_id = %tenant_id,
        target = %target.name,
    );
    let _span_guard = relay_span.enter();
    let bitrate = metrics.register_target(tenant_id.clone(), target.name.clone());
    let destination = target_destination(&target);
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
        let args = relay_ffmpeg_args(&source_url, &destination, &target);
        let child = ffmpeg_command(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn();

        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                tracing::error!(tenant_id = %tenant_id, name = %target.name, attempt, %error, "Failed to start target relay FFmpeg");
                if wait_for_retry(&mut cancel, retry_seconds).await {
                    break;
                }
                retry_seconds = (retry_seconds * 2).min(30);
                continue;
            }
        };
        tracing::info!(tenant_id = %tenant_id, name = %target.name, attempt, "Stream target relay process started");

        let last_progress = Arc::new(AtomicU64::new(crate::util::now_unix_ms()));
        let stdout_task = child.stdout.take().map(|stdout| {
            let bitrate = Arc::clone(&bitrate);
            let last_progress = Arc::clone(&last_progress);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    last_progress.store(crate::util::now_unix_ms(), Ordering::Relaxed);
                    if let Some(value) = line.strip_prefix("bitrate=")
                        && let Some(bps) = parse_ffmpeg_bitrate(value)
                    {
                        bitrate.update_from_ffmpeg(bps);
                    }
                    if let Some(value) = line.strip_prefix("drop_frames=")
                        && let Ok(frames) = value.trim().parse::<u64>()
                    {
                        bitrate.update_dropped_frames(frames);
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

        let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
        let exit = loop {
            tokio::select! {
                changed = cancel.changed() => {
                    if tokio::time::timeout(Duration::from_secs(5), child.kill()).await.is_err() {
                        let _ = child.start_kill();
                    }
                    let _ = changed;
                    break None;
                }
                result = child.wait() => break Some(result),
                _ = heartbeat.tick() => {
                    let elapsed = crate::util::now_unix_ms()
                        .saturating_sub(last_progress.load(Ordering::Relaxed));
                    if elapsed >= RELAY_HEARTBEAT_TIMEOUT.as_millis() as u64 {
                        tracing::error!(tenant_id = %tenant_id, name = %target.name, elapsed_ms = elapsed, "Relay heartbeat timed out; terminating FFmpeg");
                        let _ = child.start_kill();
                        break Some(child.wait().await);
                    }
                }
            }
        };
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
        bitrate.record_reconnection();
        match exit {
            Ok(status) => {
                tracing::error!(tenant_id = %tenant_id, name = %target.name, %status, "Stream target relay disconnected")
            }
            Err(error) => {
                tracing::error!(tenant_id = %tenant_id, name = %target.name, %error, "Failed while waiting for target relay")
            }
        }
        if started_at.elapsed() >= Duration::from_secs(30) {
            retry_seconds = 1;
        }
        tracing::warn!(tenant_id = %tenant_id, name = %target.name, retry_seconds, "Target relay will reconnect");
        if wait_for_retry(&mut cancel, retry_seconds).await {
            break;
        }
        retry_seconds = (retry_seconds * 2).min(30);
    }

    metrics.unregister_target(&tenant_id, &target.name);
    tracing::info!(tenant_id = %tenant_id, name = %target.name, "Stream target relay supervisor stopped");
}

fn relay_ffmpeg_args(source_url: &str, destination: &str, target: &TargetConfig) -> Vec<String> {
    let mut args = vec![
        "-loglevel".to_owned(),
        "warning".to_owned(),
        "-stats_period".to_owned(),
        "1".to_owned(),
        "-progress".to_owned(),
        "pipe:1".to_owned(),
        "-i".to_owned(),
        source_url.to_owned(),
    ];
    if target.encoding.mode == EncodingMode::Passthrough {
        args.extend(["-c", "copy"].into_iter().map(str::to_owned));
    } else {
        let encoder = target_encoder(target);
        args.extend(
            [
                "-c:v", encoder, "-preset", "veryfast", "-c:a", "aac", "-b:a", "128k", "-ar",
                "48000", "-ac", "2",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        if let Some(bitrate) = target.encoding.max_video_bitrate_kbps {
            args.extend(["-b:v".to_owned(), format!("{bitrate}k")]);
        }
        if let (Some(width), Some(height)) = (target.encoding.width, target.encoding.height) {
            args.extend([
                "-vf".to_owned(),
                format!(
                    "scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height}"
                ),
            ]);
        }
    }
    args.extend(["-f".to_owned(), "flv".to_owned(), destination.to_owned()]);
    args
}

async fn wait_for_retry(cancel: &mut watch::Receiver<bool>, seconds: u64) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(seconds)) => false,
        _ = cancel.changed() => true,
    }
}

pub fn parse_ffmpeg_bitrate(value: &str) -> Option<u64> {
    let value = value.trim();
    let number = value.strip_suffix("kbits/s")?.trim().parse::<f64>().ok()?;
    number
        .is_finite()
        .then_some((number.max(0.0) * 1_000.0) as u64)
}

pub fn safe_ffmpeg_failure(stderr: &str, secrets: &[String]) -> String {
    let detail = stderr.to_ascii_lowercase();
    for (needle, message) in [
        ("connection refused", "Connection refused by target"),
        ("connection timed out", "Connection to target timed out"),
        ("network is unreachable", "Target network is unreachable"),
        (
            "name or service not known",
            "Target hostname could not be resolved",
        ),
        ("authentication", "Target rejected authentication"),
        ("broken pipe", "Target closed the connection"),
        ("server error", "Target returned a server error"),
        ("unknown encoder", "Required FFmpeg encoder is unavailable"),
    ] {
        if detail.contains(needle) {
            return message.to_owned();
        }
    }
    let sanitized = redact_secrets(stderr, secrets)
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .collect::<String>();
    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        return "FFmpeg exited without diagnostic output".to_owned();
    }
    let desired_start = sanitized.len().saturating_sub(2_000);
    let start = sanitized
        .char_indices()
        .find(|(index, _)| *index >= desired_start)
        .map_or(0, |(index, _)| index);
    sanitized[start..].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ffmpeg_progress_bitrate() {
        assert_eq!(parse_ffmpeg_bitrate(" 2450.5kbits/s"), Some(2_450_500));
        assert_eq!(parse_ffmpeg_bitrate("N/A"), None);
    }

    #[test]
    fn cpu_profile_normalizes_audio_and_scales_video() {
        let target = TargetConfig {
            name: "Twitch".into(),
            url: "rtmp://example.test/app".into(),
            stream_key: "key".into(),
            public_url: None,
            enabled: true,
            encoding: crate::config::EncodingProfile {
                mode: crate::config::EncodingMode::Cpu,
                max_video_bitrate_kbps: Some(6_000),
                width: Some(1920),
                height: Some(1080),
                hardware_encoder: None,
            },
        };
        let args = relay_ffmpeg_args("rtmp://127.0.0.1/live/key", "rtmp://target/key", &target);
        assert!(args.windows(2).any(|pair| pair == ["-c:a", "aac"]));
        assert!(args.windows(2).any(|pair| pair == ["-ac", "2"]));
        assert!(args.windows(2).any(|pair| pair == ["-ar", "48000"]));
        assert!(args.windows(2).any(|pair| {
            pair == [
                "-vf",
                "scale=1920:1080:force_original_aspect_ratio=increase,crop=1920:1080",
            ]
        }));
        assert!(args.windows(2).any(|pair| pair == ["-b:v", "6000k"]));
    }

    #[test]
    fn standby_profile_generates_black_silent_video() {
        let target = TargetConfig {
            name: "Vertical".into(),
            url: "rtmp://example.test/app".into(),
            stream_key: "key".into(),
            public_url: None,
            enabled: true,
            encoding: crate::config::EncodingProfile {
                max_video_bitrate_kbps: Some(3000),
                width: Some(1080),
                height: Some(1920),
                ..Default::default()
            },
        };
        let args = standby_ffmpeg_args(&target, "rtmp://target/key");
        assert!(
            args.iter()
                .any(|arg| arg == "color=c=black:s=1080x1920:r=30")
        );
        assert!(
            args.iter()
                .any(|arg| arg == "anullsrc=channel_layout=stereo:sample_rate=48000")
        );
        assert!(args.windows(2).any(|pair| pair == ["-b:v", "3000k"]));
    }

    #[test]
    fn ffmpeg_failure_summary_does_not_echo_diagnostics() {
        let stderr = "rtmp://example.test/app/private-key: Connection refused";
        let secrets = vec!["private-key".to_owned()];
        assert_eq!(
            safe_ffmpeg_failure(stderr, &secrets),
            "Connection refused by target"
        );
        assert!(!safe_ffmpeg_failure(stderr, &secrets).contains("private-key"));
    }

    #[test]
    fn unknown_ffmpeg_failure_keeps_safe_diagnostic_text() {
        let stderr =
            "rtmp://example.test/app/private-key: Invalid data found when processing input";
        let detail = safe_ffmpeg_failure(stderr, &["private-key".to_owned()]);
        assert_eq!(
            detail,
            "[RTMP_URL_REDACTED] Invalid data found when processing input"
        );
    }
}
