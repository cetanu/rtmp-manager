use crate::server::state::AppHandle;
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
        let tenant = match stream_key {
            Some(stream_key) => match app.tenants.authenticate(stream_key).await {
                Ok(tenant) => tenant,
                Err(error) => {
                    tracing::error!(%remote, %error, "Failed to authenticate SRT publish");
                    None
                }
            },
            None => None,
        };
        let Some((stream_key, tenant)) = stream_key.zip(tenant) else {
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
            if let Err(error) =
                relay_mpeg_ts(socket, &stream_key, tenant.id.as_str(), session_app).await
            {
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
    tenant_id: &str,
    app: AppHandle,
) -> Result<()> {
    let destination = format!(
        "rtmp://127.0.0.1:{}/live/{stream_key}",
        app.config.get().server.listen.port()
    );
    let args = vec![
        "-hide_banner".to_owned(),
        "-loglevel".to_owned(),
        "warning".to_owned(),
        "-f".to_owned(),
        "mpegts".to_owned(),
        "-i".to_owned(),
        "pipe:0".to_owned(),
        "-c".to_owned(),
        "copy".to_owned(),
        "-f".to_owned(),
        "flv".to_owned(),
        destination,
    ];
    let mut child = crate::server::relay::ffmpeg_command(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to start SRT MPEG-TS relay")?;
    let mut stdin = child.stdin.take().context("FFmpeg stdin was unavailable")?;
    publish_staged_stream(&app, tenant_id).await;
    tracing::info!(%tenant_id, "Authenticated SRT stream connected");

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

async fn publish_staged_stream(app: &AppHandle, tenant_id: &str) {
    let Ok(tenant_id) = crate::tenant::TenantId::new(tenant_id) else {
        return;
    };
    for _ in 0..20 {
        match app.stream.publish_staged_stream(tenant_id.clone()).await {
            Ok(()) => {
                tracing::info!(tenant_id = %tenant_id.as_str(), "SRT stream published to configured targets");
                return;
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
        }
    }
    tracing::warn!(tenant_id = %tenant_id.as_str(), "SRT stream could not be published after staging retries");
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
