use anyhow::{Context, Result};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tracing::info;

#[derive(Default)]
pub struct Metrics {
    pub total_connections: AtomicU64,
    pub active_connections: AtomicU64,
}

#[derive(Serialize)]
struct HealthResponse<'a> {
    status: &'a str,
    active_connections: u64,
    total_connections: u64,
}

pub async fn run_health_server(addr: SocketAddr, metrics: Arc<Metrics>) -> Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind health check listener to {}", addr))?;

    info!("Health check HTTP server listening on {}", addr);

    loop {
        let (mut stream, _) = listener.accept().await?;
        let active = metrics.active_connections.load(Ordering::Relaxed);
        let total = metrics.total_connections.load(Ordering::Relaxed);

        let health = HealthResponse {
            status: "OK",
            active_connections: active,
            total_connections: total,
        };

        let mut body = serde_json::to_string(&health).unwrap_or_default();
        body.push('\n');

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );

        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;
    }
}
