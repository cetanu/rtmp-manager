use super::state::AppHandle;
use crate::util::{redact_secrets, secure_token_matches};
use anyhow::{Context, Result};
use futures_util::StreamExt;
use srt_tokio::access::{RejectReason, ServerRejectReason};
use srt_tokio::{SrtListener, SrtSocket};
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info, warn};

/// Extracts the stream key from a supported SRT stream ID.
///
/// Supports Haivision access control syntax:
/// - `#!::r=live,m=publish,u=<key>`
/// - `#!::u=<key>,r=live`
/// - `#!::r=live/<key>,m=publish`
///
/// As well as raw key inputs and URL path styles:
/// - `<key>`
/// - `live/<key>`
/// - `key=<key>`
/// - `u=<key>`
pub fn parse_srt_stream_id(stream_id: &str) -> Option<&str> {
    let trimmed = stream_id.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(haivision) = trimmed.strip_prefix("#!::") {
        let mut u_val = None;
        let mut r_val = None;
        for part in haivision.split(',') {
            let part = part.trim();
            if let Some((k, v)) = part.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                if k.eq_ignore_ascii_case("u") {
                    u_val = Some(v);
                } else if k.eq_ignore_ascii_case("r") {
                    r_val = Some(v);
                }
            }
        }
        return u_val
            .or_else(|| r_val.and_then(|resource| resource.strip_prefix("live/")))
            .filter(|key| !key.is_empty());
    }

    trimmed
        .strip_prefix("key=")
        .or_else(|| trimmed.strip_prefix("u="))
        .or_else(|| trimmed.strip_prefix("live/"))
        .or(Some(trimmed))
        .filter(|key| !key.is_empty())
}

/// Validates whether an incoming SRT `streamid` matches the expected stream key.
pub fn validate_srt_stream_id(stream_id: &str, expected_stream_key: &str) -> bool {
    if expected_stream_key.is_empty() {
        return false;
    }
    parse_srt_stream_id(stream_id)
        .is_some_and(|candidate| secure_token_matches(expected_stream_key, candidate))
}

/// Runs the SRT ingest listener on the given UDP bind address.
pub async fn run_srt_server(
    bind_addr: SocketAddr,
    internal_rtmp_addr: SocketAddr,
    app: AppHandle,
) -> Result<()> {
    info!(bind = %bind_addr, "Listening for SRT stream ingest on {bind_addr}");
    let builder = SrtListener::builder().latency(Duration::from_millis(200));
    let (_listener, mut incoming) = builder
        .bind(bind_addr)
        .await
        .with_context(|| format!("Failed to bind SRT ingest listener on {bind_addr}"))?;

    while let Some(request) = incoming.incoming().next().await {
        let client_ip = request.remote();
        let stream_id = request.stream_id().map(|s| s.as_str()).unwrap_or_default();
        let expected_stream_key = app.config.get().server.ingest_stream_key.clone();

        // Never log the raw stream_id: it carries the secret stream key.
        info!(
            client_ip = %client_ip,
            has_stream_id = !stream_id.is_empty(),
            "Client connecting to SRT Ingest"
        );

        if !validate_srt_stream_id(stream_id, &expected_stream_key) {
            warn!(
                client_ip = %client_ip,
                "Rejected SRT publish with invalid stream key"
            );
            let _ = request
                .reject(RejectReason::Server(ServerRejectReason::Unauthorized))
                .await;
            continue;
        }

        match request.accept(None).await {
            Ok(socket) => {
                info!(client_ip = %client_ip, "SRT stream accepted from client");
                // Bound concurrent bridges with the same limiter as RTMP relays.
                let slot = match super::relay::FFMPEG_SLOTS.clone().acquire_owned().await {
                    Ok(slot) => slot,
                    Err(error) => {
                        warn!(%error, client_ip = %client_ip, "Rejecting SRT ingest: server overloaded");
                        continue;
                    }
                };
                let stream_key = expected_stream_key.clone();
                let metrics = app.metrics.clone();
                tokio::spawn(async move {
                    let _slot = slot;
                    handle_srt_session(socket, internal_rtmp_addr, stream_key, client_ip, metrics).await;
                });
            }
            Err(error) => {
                error!(%error, client_ip = %client_ip, "Failed to accept SRT connection");
            }
        }
    }

    warn!("SRT ingest listener stream ended; SRT ingest is no longer accepting connections");
    Ok(())
}

/// Bridges an active SRT MPEG-TS stream into the local RTMP multiplexer.
async fn handle_srt_session(
    mut socket: SrtSocket,
    internal_rtmp_addr: SocketAddr,
    stream_key: String,
    client_ip: SocketAddr,
    metrics: std::sync::Arc<crate::metrics::Metrics>,
) {
    info!(client_ip = %client_ip, "Starting SRT ingest session bridge");
    // Note: the stream key appears in the ffmpeg argv (visible via `ps`).
    // This is a known limitation of the internal RTMP bridge; the key is
    // redacted from all logs via `redact_secrets`.
    let rtmp_target = format!("rtmp://{internal_rtmp_addr}/live/{stream_key}");
    let mut bridge_child = match tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-fflags",
            "+genpts",
            "-i",
            "pipe:0",
            "-c",
            "copy",
            "-f",
            "flv",
            &rtmp_target,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            error!(%error, "Failed to spawn internal FFmpeg SRT-to-RTMP remux bridge");
            return;
        }
    };

    let mut stdin = match require_piped_stdin(bridge_child.stdin.take()) {
        Ok(stdin) => stdin,
        Err(error) => {
            error!(%error, "SRT bridge FFmpeg did not provide a piped stdin");
            return;
        }
    };
    let secrets = [rtmp_target.clone(), stream_key.clone()];

    let stderr_task = bridge_child.stderr.take().map(|stderr| {
        let secrets = secrets.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let detail = redact_secrets(&line, &secrets);
                        let trimmed = detail.trim();
                        if !trimmed.is_empty() {
                            tracing::warn!(detail = %trimmed, "SRT bridge FFmpeg diagnostic");
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!(%error, "SRT bridge FFmpeg stderr read error");
                        break;
                    }
                }
            }
        })
    });

    let mut total_bytes = 0_u64;
    while let Some(packet) = socket.next().await {
        match packet {
            Ok((_instant, bytes)) => {
                total_bytes = total_bytes.saturating_add(bytes.len() as u64);
                metrics.add_ingest_bytes(bytes.len() as u64);

                // Bound stdin backpressure so a stalled ffmpeg can't stall SRT forever.
                if let Err(error) =
                    tokio::time::timeout(Duration::from_secs(5), stdin.write_all(&bytes)).await
                {
                    tracing::warn!(%error, "SRT bridge stdin write timed out or closed");
                    break;
                }
            }
            Err(error) => {
                tracing::warn!(%error, "SRT socket read error");
                break;
            }
        }
    }

    drop(stdin);
    info!(
        client_ip = %client_ip,
        total_bytes,
        "SRT client stream stopped publishing"
    );

    // Graceful shutdown first, then kill, both bounded so the task can't leak.
    let _ = tokio::time::timeout(Duration::from_secs(3), bridge_child.wait()).await;
    let _ = bridge_child.start_kill();
    match tokio::time::timeout(Duration::from_secs(5), bridge_child.wait()).await {
        Ok(_) => {}
        Err(_) => {
            warn!("SRT bridge FFmpeg did not exit after kill; dropping (kill_on_drop)");
        }
    }

    if let Some(task) = stderr_task {
        // Bound stderr drain so a hung pipe can't leak the bridge task.
        match tokio::time::timeout(Duration::from_secs(5), task).await {
            Ok(Ok(())) => {}
            Ok(Err(join_error)) => {
                warn!(%join_error, "SRT bridge stderr task failed");
            }
            Err(_) => {
                warn!("SRT bridge stderr drain timed out");
            }
        }
    }

    info!(client_ip = %client_ip, "SRT ingest session ended");
}

fn require_piped_stdin<T>(stdin: Option<T>) -> Result<T> {
    stdin.context("FFmpeg stdin was not piped")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_haivision_stream_id() {
        assert!(validate_srt_stream_id(
            "#!::r=live,m=publish,u=my-secret-key",
            "my-secret-key"
        ));
        assert!(validate_srt_stream_id(
            "#!::u=my-secret-key,r=live,m=publish",
            "my-secret-key"
        ));
        assert!(validate_srt_stream_id(
            "#!::r=live/my-secret-key,m=publish",
            "my-secret-key"
        ));
        assert!(validate_srt_stream_id(
            "#!:: r=live , m=publish , u=my-secret-key ",
            "my-secret-key"
        ));
        assert!(!validate_srt_stream_id(
            "#!::r=live,m=publish,u=wrong-key",
            "my-secret-key"
        ));
    }

    #[test]
    fn parses_plain_and_path_stream_ids() {
        assert!(validate_srt_stream_id("my-secret-key", "my-secret-key"));
        assert!(validate_srt_stream_id(
            "live/my-secret-key",
            "my-secret-key"
        ));
        assert!(validate_srt_stream_id("key=my-secret-key", "my-secret-key"));
        assert!(validate_srt_stream_id("u=my-secret-key", "my-secret-key"));
        assert!(!validate_srt_stream_id("wrong-key", "my-secret-key"));
        assert!(!validate_srt_stream_id(
            "monkey=my-secret-key",
            "my-secret-key"
        ));
        assert!(!validate_srt_stream_id(
            "prefix:u=my-secret-key",
            "my-secret-key"
        ));
        assert!(!validate_srt_stream_id("", "my-secret-key"));
        assert!(!validate_srt_stream_id("my-secret-key", ""));
    }

    #[tokio::test]
    async fn srt_listener_accepts_valid_and_rejects_invalid_stream_id() {
        let udp = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let local_addr = udp.local_addr().unwrap();
        let (_listener, mut incoming) = SrtListener::builder()
            .socket(udp)
            .bind(local_addr)
            .await
            .unwrap();

        let server_task = tokio::spawn(async move {
            while let Some(request) = incoming.incoming().next().await {
                let stream_id = request.stream_id().map(|s| s.as_str()).unwrap_or_default();
                if validate_srt_stream_id(stream_id, "correct-key") {
                    let _socket = request.accept(None).await.unwrap();
                } else {
                    let _ = request
                        .reject(RejectReason::Server(ServerRejectReason::Unauthorized))
                        .await;
                }
            }
        });

        let rejected = SrtSocket::builder()
            .call(local_addr, Some("#!::r=live,m=publish,u=bad-key"))
            .await;
        assert!(rejected.is_err());

        let accepted = SrtSocket::builder()
            .call(local_addr, Some("#!::r=live,m=publish,u=correct-key"))
            .await;
        assert!(accepted.is_ok());

        server_task.abort();
    }

    #[tokio::test]
    async fn srt_stream_transmits_data() {
        use bytes::Bytes;
        use std::time::Instant;

        let udp = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let local_addr = udp.local_addr().unwrap();
        let (_listener, mut incoming) = SrtListener::builder()
            .socket(udp)
            .bind(local_addr)
            .await
            .unwrap();

        let server_task = tokio::spawn(async move {
            let mut total_bytes = 0_u64;
            if let Some(request) = incoming.incoming().next().await {
                let mut socket = request.accept(None).await.unwrap();
                while let Some(Ok((_instant, bytes))) = socket.next().await {
                    total_bytes += bytes.len() as u64;
                }
            }
            total_bytes
        });

        let mut client = SrtSocket::builder()
            .call(local_addr, Some("correct-key"))
            .await
            .unwrap();

        let payload = Bytes::from_static(b"test-mpegts-packet-data");
        let payload_len = payload.len() as u64;
        client
            .try_send(Instant::now(), payload)
            .expect("send packet");
        let _ = client.close_and_finish().await;

        assert_eq!(server_task.await.unwrap(), payload_len);
    }
}
