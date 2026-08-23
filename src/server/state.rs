use crate::chat::ChatService;
use crate::config::{AppConfig, ConfigStore, TargetConfig};
use crate::metrics::Metrics;
use crate::notifications::{NotificationDispatcher, NotificationTarget};
use crate::server::preview::HlsPreviewManager;
use crate::server::relay::RelayManager;
use anyhow::Result;
use reqwest::Client;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub use crate::server::preview::{StreamState, StreamStatus};

/// Shared application state coordinating RTMP ingest, HLS preview, relays, metrics, chat, and config.
pub struct ProxyState {
    pub metrics: Arc<Metrics>,
    pub config: Arc<RwLock<AppConfig>>,
    pub http_client: Client,
    pub listen_port: u16,
    pub config_store: ConfigStore,
    pub preview: Arc<HlsPreviewManager>,
    pub relay: Arc<RelayManager>,
    pub chat: Arc<ChatService>,
}

impl ProxyState {
    pub async fn new(
        metrics: Arc<Metrics>,
        config: AppConfig,
        http_client: Client,
        listen_port: u16,
        config_store: ConfigStore,
    ) -> Result<Self> {
        let chat = ChatService::open(config_store.path(), config.chat.queue_capacity).await?;
        let preview = Arc::new(HlsPreviewManager::new()?);
        let relay = Arc::new(RelayManager::new());

        Ok(Self {
            metrics,
            config: Arc::new(RwLock::new(config)),
            http_client,
            listen_port,
            config_store,
            preview,
            relay,
            chat,
        })
    }

    /// Stages a new RTMP ingest stream for previewing before going live.
    pub async fn stage_stream(&self, stream_key: String) -> Result<()> {
        if let Some(old_key) = self.preview.end_current_stream().await {
            self.relay.stop_relays(&old_key).await;
        }
        self.preview.stage_stream(self.listen_port, stream_key).await?;
        Ok(())
    }

    /// Publishes the currently staged preview to all enabled targets.
    pub async fn publish_staged_stream(&self) -> Result<()> {
        let (session_id, stream_key) = self.preview.begin_publishing().await?;

        let config = self.config.read().await;
        let active_targets = config
            .targets
            .iter()
            .filter(|target| target.enabled)
            .cloned()
            .collect::<Vec<_>>();
        let notification_targets = active_targets
            .iter()
            .map(NotificationTarget::from)
            .collect::<Vec<_>>();
        let dispatcher =
            NotificationDispatcher::new(&config.notifications, self.http_client.clone());
        drop(config);

        let source_url = format!("rtmp://127.0.0.1:{}/live/{stream_key}", self.listen_port);
        let relays = self
            .relay
            .spawn_relays(&self.metrics, &source_url, &active_targets);

        if !self.preview.is_session_published(session_id).await {
            RelayManager::cancel_relays(relays).await;
            anyhow::bail!("The staged stream changed while publishing was starting");
        }

        self.relay.store_relays(stream_key, relays).await;
        tokio::spawn(async move {
            dispatcher.dispatch(&notification_targets).await;
        });

        tracing::info!("Staged stream published to enabled targets");
        Ok(())
    }

    /// Stops publishing to external targets while keeping local preview active.
    pub async fn stop_publishing(&self) -> Result<()> {
        let stream_key = self.preview.stop_publishing().await?;
        self.relay.stop_relays(&stream_key).await;
        tracing::info!("External publishing stopped; stream remains staged");
        Ok(())
    }

    /// Ends an RTMP stream session by stream key.
    pub async fn end_stream(&self, stream_key: &str) {
        let _ = self.preview.end_if_current(stream_key).await;
        self.relay.stop_relays(stream_key).await;
    }

    /// Inspects the current stream status.
    pub async fn stream_status(&self) -> StreamStatus {
        self.preview.status().await
    }

    /// Looks up an allowlisted preview file path for HTTP delivery.
    pub fn preview_file(&self, name: &str) -> Option<PathBuf> {
        self.preview.preview_file(name)
    }

    /// Executes a direct test stream without requiring active live ingest.
    pub fn run_test_stream(&self, duration_secs: u64, targets: Vec<TargetConfig>) {
        RelayManager::run_direct_test(duration_secs, targets);
    }

    /// Re-applies chat configuration to the background chat ingestion workers.
    pub async fn apply_chat_config(self: &Arc<Self>) -> Result<()> {
        let chat_config = self.config.read().await.chat.clone();
        self.chat.apply_config(&self.http_client, &chat_config).await
    }
}
