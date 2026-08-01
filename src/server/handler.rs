use super::state::ProxyState;
use rtmp_rs::protocol::message::{ConnectParams, PublishParams};
use rtmp_rs::session::context::StreamContext;
use rtmp_rs::session::SessionContext;
use rtmp_rs::{AuthResult, RtmpHandler};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::{error, info};

pub struct ProxyHandler {
    pub state: Arc<ProxyState>,
}

impl RtmpHandler for ProxyHandler {
    async fn on_connect(&self, ctx: &SessionContext, params: &ConnectParams) -> AuthResult {
        info!(
            session_id = %ctx.session_id,
            client_ip = %ctx.peer_addr,
            app = %params.app,
            "Client connected to RTMP Ingest"
        );
        self.state
            .metrics
            .total_connections
            .fetch_add(1, Ordering::Relaxed);
        self.state
            .metrics
            .active_connections
            .fetch_add(1, Ordering::Relaxed);
        AuthResult::Accept
    }

    async fn on_publish(&self, ctx: &SessionContext, params: &PublishParams) -> AuthResult {
        let stream_key = params.stream_key.clone();
        info!(
            session_id = %ctx.session_id,
            "Stream published from client"
        );

        match self.state.stage_stream(stream_key).await {
            Ok(()) => AuthResult::Accept,
            Err(error) => {
                error!(%error, "Failed to stage stream preview");
                AuthResult::Reject("Failed to start staged preview".into())
            }
        }
    }

    async fn on_unpublish(&self, ctx: &StreamContext) {
        info!("Stream stopped publishing");

        self.state.end_stream(&ctx.stream_key).await;
    }

    async fn on_disconnect(&self, ctx: &SessionContext) {
        info!(session_id = %ctx.session_id, "Client disconnected");
        self.state
            .metrics
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);
    }
}
