use crate::chat::ChatHandle;
use crate::config::ConfigHandle;
use crate::metrics::Metrics;
use crate::server::stream_actor::StreamHandle;
use anyhow::Result;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct WebhookEvent {
    pub headers: HashMap<String, String>,
    pub body: topcoat::router::Bytes,
}

impl WebhookEvent {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

pub use crate::server::preview::{StreamState, StreamStatus};

/// Unified application handle containing cloneable subsystem handles and lock-free channels.
#[derive(Clone)]
pub struct AppHandle {
    pub stream: StreamHandle,
    pub chat: ChatHandle,
    pub config: ConfigHandle,
    pub metrics: Arc<Metrics>,
    pub http_client: Client,
    pub webhooks: broadcast::Sender<WebhookEvent>,
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
        let (webhooks, _) = broadcast::channel(64);

        let handle = Self {
            stream,
            chat,
            config: config_handle,
            metrics,
            http_client,
            webhooks,
        };

        tokio::spawn(crate::chat::kick::run(
            handle.webhooks.subscribe(),
            handle.config.subscribe(),
            handle.chat.clone(),
        ));
        tokio::spawn(crate::chat::x::run(
            handle.webhooks.subscribe(),
            handle.config.subscribe(),
            handle.chat.clone(),
        ));

        handle.apply_chat_config().await?;

        Ok(handle)
    }

    /// Re-applies the latest chat configuration to the background chat ingestion workers.
    pub async fn apply_chat_config(&self) -> Result<()> {
        let chat_settings = self.config.get().chat.clone();
        self.chat.apply_config(chat_settings).await
    }

    pub async fn set_youtube_polling(&self, enabled: bool) -> Result<()> {
        let (config, changed, _) = self.config.set_youtube_polling(enabled).await?;
        if changed {
            self.chat.set_youtube_polling(config.chat.clone()).await?;
        }
        Ok(())
    }

    pub async fn set_x_webhook(&self, enabled: bool) -> Result<()> {
        let (_, changed, _) = self.config.set_x_webhook(enabled).await?;
        if changed {
            tracing::info!(enabled, "X webhook ingestion changed");
        }
        Ok(())
    }

    pub async fn set_kick_webhook(&self, enabled: bool) -> Result<()> {
        let (_, changed, _) = self.config.set_kick_webhook(enabled).await?;
        if changed {
            tracing::info!(enabled, "Kick webhook ingestion changed");
        }
        Ok(())
    }
}
