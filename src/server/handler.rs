use super::state::ProxyState;
use crate::util::secure_token_matches;
use rtmp_rs::media::flv::FlvTag;
use rtmp_rs::protocol::message::{ConnectParams, PublishParams};
use rtmp_rs::session::SessionContext;
use rtmp_rs::session::context::StreamContext;
use rtmp_rs::{AuthResult, RtmpHandler};
use std::sync::Arc;
use tracing::{error, info};

pub struct ProxyHandler {
    pub state: Arc<ProxyState>,
}

impl RtmpHandler for ProxyHandler {
    async fn on_media_tag(&self, ctx: &StreamContext, tag: &FlvTag) -> bool {
        if ctx.is_publishing {
            self.state.metrics.add_ingest_bytes(tag.size() as u64);
        }
        true
    }

    async fn on_connect(&self, ctx: &SessionContext, params: &ConnectParams) -> AuthResult {
        info!(
            session_id = %ctx.session_id,
            client_ip = %ctx.peer_addr,
            app = %params.app,
            "Client connected to RTMP Ingest"
        );
        AuthResult::Accept
    }

    async fn on_publish(&self, ctx: &SessionContext, params: &PublishParams) -> AuthResult {
        let stream_key = params.stream_key.clone();
        let expected_stream_key = self
            .state
            .config
            .read()
            .await
            .server
            .ingest_stream_key
            .clone();
        if !secure_token_matches(&expected_stream_key, &stream_key) {
            error!(session_id = %ctx.session_id, "Rejected RTMP publish with invalid stream key");
            return AuthResult::Reject("Invalid stream key".into());
        }
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_requires_an_exact_nonempty_stream_key() {
        assert!(secure_token_matches("private-key", "private-key"));
        assert!(!secure_token_matches("private-key", "wrong-key"));
        assert!(!secure_token_matches("private-key", "private-key-extra"));
        assert!(!secure_token_matches("", ""));
    }
}
