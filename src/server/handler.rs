use super::state::AppHandle;
use rtmp_rs::media::flv::FlvTag;
use rtmp_rs::protocol::message::{ConnectParams, PublishParams};
use rtmp_rs::session::SessionContext;
use rtmp_rs::session::context::StreamContext;
use rtmp_rs::{AuthResult, RtmpHandler};
use tracing::{error, info};

pub struct ProxyHandler {
    pub app: AppHandle,
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
        AuthResult::Accept
    }

    async fn on_publish(&self, ctx: &SessionContext, params: &PublishParams) -> AuthResult {
        let stream_key = params.stream_key.clone();
        let tenant = match self.app.tenants.authenticate(&stream_key).await {
            Ok(Some(tenant)) => tenant,
            Ok(None) => {
                error!(session_id = %ctx.session_id, "Rejected RTMP publish with invalid stream key");
                return AuthResult::Reject("Invalid stream key".into());
            }
            Err(error) => {
                error!(session_id = %ctx.session_id, %error, "Failed to authenticate RTMP publish");
                return AuthResult::Reject("Stream authentication unavailable".into());
            }
        };
        if !tenant.active {
            error!(session_id = %ctx.session_id, "Rejected RTMP publish with invalid stream key");
            return AuthResult::Reject("Invalid stream key".into());
        }
        info!(
            session_id = %ctx.session_id,
            tenant_id = %tenant.id.as_str(),
            "Stream published from client"
        );

        match self.app.stream.stage_stream(tenant, stream_key).await {
            Ok(()) => AuthResult::Accept,
            Err(error) => {
                error!(%error, "Failed to stage stream preview");
                AuthResult::Reject("Failed to start staged preview".into())
            }
        }
    }

    async fn on_unpublish(&self, ctx: &StreamContext) {
        info!("Stream stopped publishing");
        self.app.stream.grace_disconnect(&ctx.stream_key).await;
    }

    async fn on_disconnect(&self, ctx: &SessionContext) {
        info!(session_id = %ctx.session_id, "Client disconnected");
    }
}
