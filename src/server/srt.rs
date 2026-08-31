use crate::server::state::AppHandle;
use crate::util::secure_token_matches;
use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use srt_tokio::{
    SrtListener,
    access::{RejectReason, ServerRejectReason},
};
use std::{net::SocketAddr, process::Stdio};
use tokio::io::AsyncWriteExt;

pub async fn run_srt_server(bind_addr: SocketAddr, app: AppHandle) -> Result<()> {
    let (_listener, mut incoming) = SrtListener::builder()
        .bind(bind_addr)
        .await
        .with_context(|| format!("Failed to bind SRT listener on {bind_addr}"))?;
    tracing::info!(%bind_addr, "SRT ingest listening");

    while let Some(request) = incoming.incoming().next().await {
        let remote = request.remote();
        let stream_id = request.stream_id().map(ToString::to_string);
        let stream_key = stream_id.as_deref().and_then(parse_publish_stream_id);
        let expected = app.config.get().server.ingest_stream_key.clone();
        let Some(stream_key) = stream_key.filter(|key| secure_token_matches(&expected, key)) else {
            tracing::warn!(%remote, "Rejected SRT publish with invalid stream ID");
            request
                .reject(RejectReason::Server(ServerRejectReason::Unauthorized))
                .await?;
            continue;
        };

        let socket = request.accept(None).await?;
        let session_app = app.clone();
        let stream_key = stream_key.to_owned();
        tokio::spawn(async move {
            if let Err(error) = relay_mpeg_ts(socket, &stream_key, session_app).await {
                tracing::error!(%remote, %error, "SRT ingest session failed");
            }
        });
    }
    Ok(())
}

fn parse_publish_stream_id(stream_id: &str) -> Option<&str> {
    let fields = stream_id.strip_prefix("#!::")?;
    let mut resource = None;
    let mut mode = None;
    let mut user = None;
    for field in fields.split(',') {
        let (name, value) = field.split_once('=')?;
        match name {
            "r" => resource = Some(value),
            "m" => mode = Some(value),
            "u" => user = Some(value),
            _ => {}
        }
    }
    (resource == Some("live") && mode == Some("publish"))
        .then_some(user)
        .flatten()
        .filter(|key| !key.is_empty())
}

async fn relay_mpeg_ts(
    mut socket: srt_tokio::SrtSocket,
    stream_key: &str,
    app: AppHandle,
) -> Result<()> {
    let destination = format!(
        "rtmp://127.0.0.1:{}/live/{stream_key}",
        app.config.get().server.listen.port()
    );
    let mut child = tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-c",
            "copy",
            "-f",
            "flv",
            &destination,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to start SRT MPEG-TS relay")?;
    let mut stdin = child.stdin.take().context("FFmpeg stdin was unavailable")?;
    tracing::info!("Authenticated SRT stream connected");

    while let Some(packet) = socket.next().await {
        let (_, bytes) = packet.context("Failed to receive SRT packet")?;
        app.metrics.add_ingest_bytes(bytes.len() as u64);
        if stdin.write_all(&bytes).await.is_err() {
            break;
        }
    }
    drop(stdin);
    let status = child.wait().await.context("Failed waiting for SRT relay")?;
    if !status.success() {
        bail!("SRT MPEG-TS relay exited with {status}");
    }
    tracing::info!("SRT stream disconnected");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_publish_stream_ids_for_live_resource() {
        assert_eq!(
            parse_publish_stream_id("#!::r=live,m=publish,u=private-key"),
            Some("private-key")
        );
        assert_eq!(
            parse_publish_stream_id("#!::r=live,m=request,u=private-key"),
            None
        );
        assert_eq!(parse_publish_stream_id("private-key"), None);
        assert_eq!(parse_publish_stream_id("#!::r=live,m=publish,u="), None);
    }
}
