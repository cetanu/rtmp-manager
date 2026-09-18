use crate::config::TargetConfig;
use crate::metrics::Metrics;
use crate::util::redact_secrets;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub struct RelayProcess {
    pub cancel: watch::Sender<bool>,
    pub task: JoinHandle<()>,
}

pub fn spawn_relay(
    metrics: Arc<Metrics>,
    source_url: String,
    target: TargetConfig,
) -> RelayProcess {
    let (cancel, cancel_rx) = watch::channel(false);
    let task = tokio::spawn(supervise_relay(metrics, source_url, target, cancel_rx));
    RelayProcess { cancel, task }
}

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

pub fn run_direct_test(
    metrics: Arc<Metrics>,
    running: Arc<AtomicBool>,
    duration_secs: u64,
    targets: Vec<TargetConfig>,
) {
    if running.swap(true, Ordering::SeqCst) {
        tracing::warn!("Direct test stream is already in progress; ignoring duplicate request");
        return;
    }

    tokio::spawn(async move {
        struct RunningGuard(Arc<AtomicBool>);
        impl Drop for RunningGuard {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _guard = RunningGuard(running);

        tracing::info!(
            duration_secs,
            target_count = targets.len(),
            "Starting direct test stream to enabled targets"
        );
        let mut tasks = tokio::task::JoinSet::new();
        for target in targets {
            let metrics = Arc::clone(&metrics);
            tasks.spawn(async move {
                let bitrate = metrics.register_target(target.name.clone());
                let destination = target_destination(&target);
                let secrets = [destination.clone(), target.stream_key.clone()];
                let video_source =
                    format!("testsrc=duration={duration_secs}:size=1280x720:rate=30");
                let audio_source = format!("sine=frequency=1000:duration={duration_secs}");
                let child = tokio::process::Command::new("ffmpeg")
                    .args([
                        "-hide_banner",
                        "-loglevel",
                        "warning",
                        "-stats_period",
                        "1",
                        "-progress",
                        "pipe:1",
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
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true)
                    .spawn();

                let mut child = match child {
                    Ok(child) => child,
                    Err(error) => {
                        bitrate.update_from_ffmpeg(0);
                        metrics.unregister_target(&target.name);
                        tracing::error!(
                            name = %target.name,
                            %error,
                            "Direct target test FFmpeg failed to start"
                        );
                        return;
                    }
                };

                tracing::info!(
                    name = %target.name,
                    duration_secs,
                    "Direct target test stream process started"
                );

                let stdout_task = child.stdout.take().map(|stdout| {
                    let bitrate = Arc::clone(&bitrate);
                    let target_name = target.name.clone();
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(stdout).lines();
                        let mut current_bitrate_bps = 0_u64;
                        let mut current_total_bytes = 0_u64;
                        let mut current_speed = String::new();
                        let mut last_progress_log = tokio::time::Instant::now();

                        while let Ok(Some(line)) = lines.next_line().await {
                            if let Some(value) = line.strip_prefix("bitrate=") {
                                if let Some(bps) = parse_ffmpeg_bitrate(value) {
                                    current_bitrate_bps = bps;
                                    bitrate.update_from_ffmpeg(bps);
                                }
                            } else if let Some(value) = line.strip_prefix("total_size=") {
                                if let Some(bytes) = parse_ffmpeg_total_size(value) {
                                    current_total_bytes = bytes;
                                }
                            } else if let Some(value) = line.strip_prefix("speed=") {
                                current_speed = value.trim().to_string();
                            } else if let Some(value) = line.strip_prefix("progress=") {
                                let progress = value.trim();
                                if progress == "continue"
                                    && last_progress_log.elapsed() >= Duration::from_secs(3)
                                {
                                    tracing::info!(
                                        name = %target_name,
                                        bitrate = %format_bitrate_display(current_bitrate_bps),
                                        sent = %format_bytes_display(current_total_bytes),
                                        speed = %current_speed,
                                        "Direct test stream progress"
                                    );
                                    last_progress_log = tokio::time::Instant::now();
                                }
                            }
                        }
                        current_total_bytes
                    })
                });

                let stderr_task = child.stderr.take().map(|stderr| {
                    let target_name = target.name.clone();
                    let secrets = secrets.clone();
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(stderr).lines();
                        let mut captured_stderr = Vec::new();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let detail = redact_secrets(&line, &secrets);
                            let trimmed = detail.trim();
                            if !trimmed.is_empty() {
                                tracing::warn!(
                                    name = %target_name,
                                    detail = %trimmed,
                                    "Direct test FFmpeg diagnostic"
                                );
                                captured_stderr.push(trimmed.to_string());
                            }
                        }
                        captured_stderr.join("\n")
                    })
                });

                let exit_status = child.wait().await;
                bitrate.update_from_ffmpeg(0);
                metrics.unregister_target(&target.name);

                let total_bytes = if let Some(task) = stdout_task {
                    task.await.unwrap_or_default()
                } else {
                    0
                };

                let captured_stderr = if let Some(task) = stderr_task {
                    task.await.unwrap_or_default()
                } else {
                    String::new()
                };

                match exit_status {
                    Ok(status) if status.success() => {
                        if total_bytes > 0 {
                            tracing::info!(
                                name = %target.name,
                                total_sent = %format_bytes_display(total_bytes),
                                "Direct target test completed successfully"
                            );
                        } else {
                            tracing::info!(
                                name = %target.name,
                                "Direct target test completed successfully"
                            );
                        }
                    }
                    Ok(status) => {
                        let detail = safe_ffmpeg_failure(
                            &captured_stderr,
                            &[target.stream_key.clone(), destination],
                        );
                        tracing::error!(
                            name = %target.name,
                            %status,
                            %detail,
                            "Direct target test failed"
                        );
                    }
                    Err(error) => {
                        tracing::error!(
                            name = %target.name,
                            %error,
                            "Failed while waiting for direct target test FFmpeg"
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
        tracing::info!("Direct test stream completed for all targets");
    });
}

async fn supervise_relay(
    metrics: Arc<Metrics>,
    source_url: String,
    target: TargetConfig,
    mut cancel: watch::Receiver<bool>,
) {
    let bitrate = metrics.register_target(target.name.clone());
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

    metrics.unregister_target(&target.name);
    tracing::info!(name = %target.name, "Stream target relay supervisor stopped");
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

pub fn parse_ffmpeg_total_size(value: &str) -> Option<u64> {
    let value = value.trim();
    if value == "N/A" {
        return None;
    }
    value.parse::<u64>().ok()
}

pub fn format_bytes_display(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

pub fn format_bitrate_display(bits_per_second: u64) -> String {
    if bits_per_second >= 1_000_000 {
        format!("{:.2} Mbps", bits_per_second as f64 / 1_000_000.0)
    } else if bits_per_second >= 1_000 {
        format!("{:.0} Kbps", bits_per_second as f64 / 1_000.0)
    } else {
        format!("{bits_per_second} bps")
    }
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
    fn parses_ffmpeg_total_size() {
        assert_eq!(parse_ffmpeg_total_size("46015"), Some(46015));
        assert_eq!(parse_ffmpeg_total_size("  1024  "), Some(1024));
        assert_eq!(parse_ffmpeg_total_size("N/A"), None);
    }

    #[test]
    fn formats_bytes_display() {
        assert_eq!(format_bytes_display(500), "500 B");
        assert_eq!(format_bytes_display(1500), "1.5 KB");
        assert_eq!(format_bytes_display(2_500_000), "2.50 MB");
        assert_eq!(format_bytes_display(3_200_000_000), "3.20 GB");
    }

    #[test]
    fn formats_bitrate_display() {
        assert_eq!(format_bitrate_display(500), "500 bps");
        assert_eq!(format_bitrate_display(500_000), "500 Kbps");
        assert_eq!(format_bitrate_display(2_500_000), "2.50 Mbps");
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

    #[test]
    fn metrics_target_lifecycle_updates_and_unregisters() {
        let metrics = Metrics::default();
        let target_name = "Twitch".to_string();
        let bitrate = metrics.register_target(target_name.clone());

        assert_eq!(metrics.current_target_bitrates().len(), 1);
        assert_eq!(metrics.current_target_bitrates()[0].name, "Twitch");
        assert_eq!(metrics.current_target_bitrates()[0].outbound_bps, 0);

        bitrate.update_from_ffmpeg(2_500_000);
        assert_eq!(metrics.current_target_bitrates()[0].outbound_bps, 2_500_000);

        bitrate.update_from_ffmpeg(0);
        metrics.unregister_target(&target_name);
        assert!(metrics.current_target_bitrates().is_empty());
    }

    #[test]
    fn direct_test_prevents_concurrent_runs() {
        let running = Arc::new(AtomicBool::new(false));

        assert!(!running.swap(true, Ordering::SeqCst));
        assert!(running.swap(true, Ordering::SeqCst));

        running.store(false, Ordering::SeqCst);
        assert!(!running.load(Ordering::SeqCst));
    }
}
