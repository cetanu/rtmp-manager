use super::state::AppHandle;
use crate::util::secure_token_matches;
use rtmp_rs::media::flv::FlvTag;
use rtmp_rs::protocol::message::{ConnectParams, PublishParams};
use rtmp_rs::session::SessionContext;
use rtmp_rs::session::context::StreamContext;
use rtmp_rs::{AuthResult, RtmpHandler};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};

pub struct ProxyHandler {
    pub app: AppHandle,
    sessions: Arc<parking_lot::Mutex<HashMap<u64, String>>>,
}

impl ProxyHandler {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            sessions: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }
}

impl RtmpHandler for ProxyHandler {
    async fn on_media_tag(&self, ctx: &StreamContext, tag: &FlvTag) -> bool {
        if ctx.is_publishing {
            self.app.metrics.add_ingest_bytes(tag.size() as u64);
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
        // Fail closed on unexpected app names instead of accepting everything.
        if params.app != "live" {
            error!(session_id = %ctx.session_id, app = %params.app, "Rejected RTMP connect with unexpected app");
            return AuthResult::Reject("Invalid app".into());
        }
        AuthResult::Accept
    }

    async fn on_publish(&self, ctx: &SessionContext, params: &PublishParams) -> AuthResult {
        let stream_key = params.stream_key.clone();
        let expected_stream_key = self.app.config.get().server.ingest_stream_key.clone();

        if !secure_token_matches(&expected_stream_key, &stream_key) {
            error!(session_id = %ctx.session_id, "Rejected RTMP publish with invalid stream key");
            return AuthResult::Reject("Invalid stream key".into());
        }
        info!(
            session_id = %ctx.session_id,
            "Stream published from client"
        );

        match self.app.stream.stage_stream(stream_key.clone()).await {
            Ok(()) => {
                self.sessions.lock().insert(ctx.session_id, stream_key);
                AuthResult::Accept
            }
            Err(error) => {
                error!(%error, "Failed to stage stream preview");
                AuthResult::Reject("Failed to start staged preview".into())
            }
        }
    }

    async fn on_unpublish(&self, ctx: &StreamContext) {
        info!("Stream stopped publishing");
        {
            self.sessions.lock().remove(&ctx.session.session_id);
        }
        self.app.stream.end_stream(&ctx.stream_key).await;
    }

    async fn on_disconnect(&self, ctx: &SessionContext) {
        info!(session_id = %ctx.session_id, "Client disconnected");
        // Abrupt disconnects may skip on_unpublish: tear down by session map.
        let stream_key = { self.sessions.lock().remove(&ctx.session_id) };
        if let Some(stream_key) = stream_key {
            self.app.stream.end_stream(&stream_key).await;
        }
    }
}
