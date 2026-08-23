pub mod handler;
pub mod preview;
pub mod relay;
pub mod state;
pub mod stream_actor;

use anyhow::{Context, Result};
use handler::ProxyHandler;
use rtmp_rs::{RtmpServer, ServerConfig};
use state::AppHandle;
use std::net::SocketAddr;

pub async fn run_rtmp_server(bind_addr: SocketAddr, app: AppHandle) -> Result<()> {
    let server_config = ServerConfig::default().bind(bind_addr);
    let handler = ProxyHandler { app };
    let server = RtmpServer::new(server_config, handler);

    server.run().await.context("RTMP server failed")?;
    Ok(())
}
