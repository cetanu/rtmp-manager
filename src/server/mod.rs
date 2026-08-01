pub mod handler;
pub mod state;

use anyhow::{Context, Result};
use handler::ProxyHandler;
use rtmp_rs::{RtmpServer, ServerConfig};
use state::ProxyState;
use std::net::SocketAddr;
use std::sync::Arc;

pub async fn run_rtmp_server(bind_addr: SocketAddr, state: Arc<ProxyState>) -> Result<()> {
    let server_config = ServerConfig::default().bind(bind_addr);
    let handler = ProxyHandler { state };
    let server = RtmpServer::new(server_config, handler);

    server.run().await.context("RTMP server failed")?;
    Ok(())
}
