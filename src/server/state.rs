use crate::chat::ChatHandle;
use crate::config::ConfigHandle;
use crate::metrics::Metrics;
use crate::server::stream_actor::StreamHandle;
use anyhow::Result;
use reqwest::Client;
use std::sync::Arc;

pub use crate::server::preview::{StreamState, StreamStatus};

/// Unified application handle containing cloneable subsystem handles and lock-free channels.
#[derive(Clone)]
pub struct AppHandle {
    pub stream: StreamHandle,
    pub chat: ChatHandle,
    pub config: ConfigHandle,
    pub metrics: Arc<Metrics>,
    pub http_client: Client,
}

impl AppHandle {
    pub async fn new(
        metrics: Arc<Metrics>,
        config_handle: ConfigHandle,
        http_client: Client,
        listen_port: u16,
    ) -> Result<Self> {
        let initial_config = config_handle.get();
        let chat = ChatHandle::spawn(
            config_handle.path(),
            initial_config.chat.queue_capacity,
            http_client.clone(),
        )
        .await?;

        let stream = StreamHandle::spawn(
            listen_port,
            Arc::clone(&metrics),
            http_client.clone(),
            config_handle.subscribe(),
        )
        .await?;

        let handle = Self {
            stream,
            chat,
            config: config_handle,
            metrics,
            http_client,
        };

        handle.apply_chat_config().await?;

        Ok(handle)
    }

    /// Re-applies the latest chat configuration to the background chat ingestion workers.
    pub async fn apply_chat_config(&self) -> Result<()> {
        let chat_settings = self.config.get().chat.clone();
        self.chat.apply_config(chat_settings).await
    }
}
