use crate::accounts::AccountRepository;
use crate::chat::{ChatHandle, ChatHub};
use crate::config::{AppConfig, ConfigForm, ConfigHandle};
use crate::metrics::Metrics;
use crate::server::stream_actor::StreamHandle;
use crate::tenant::TenantRepository;
use anyhow::Result;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;

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
    pub usage: crate::billing::UsageRepository,
    pub accounts: AccountRepository,
    pub stream: StreamHandle,
    pub chat: ChatHub,
    pub config: ConfigHandle,
    pub tenants: TenantRepository,
    pub metrics: Arc<Metrics>,
    pub http_client: Client,
}

impl AppHandle {
    pub async fn new(
        metrics: Arc<Metrics>,
        database: crate::database::Database,
        config_handle: ConfigHandle,
        http_client: Client,
        listen_port: u16,
    ) -> Result<Self> {
        let accounts = AccountRepository::new(database.clone());
        let usage = crate::billing::UsageRepository::new(database.clone());
        let tenants = TenantRepository::new(database.clone());
        let chat = ChatHub::new(database.clone(), http_client.clone());

        let stream = StreamHandle::spawn(
            listen_port,
            Arc::clone(&metrics),
            http_client.clone(),
            tenants.clone(),
            crate::billing::UsageRepository::new(database.clone()),
        )
        .await?;
        let handle = Self {
            usage,
            accounts,
            stream,
            chat,
            config: config_handle,
            tenants,
            metrics,
            http_client,
        };

        handle.apply_chat_config().await?;
        let kick_settings = handle.config.get().chat.clone();
        if kick_settings.kick_webhook_enabled {
            let http_client = handle.http_client.clone();
            tokio::spawn(async move {
                match crate::chat::kick::set_chat_subscription(&http_client, &kick_settings, true)
                    .await
                {
                    Ok(()) => tracing::info!("Kick chat webhook subscription is active"),
                    Err(error) => {
                        tracing::warn!(
                            "Failed to restore Kick chat webhook subscription: {error:#}"
                        )
                    }
                }
            });
        }

        Ok(handle)
    }

    /// Re-applies the latest chat configuration to the background chat ingestion workers.
    pub async fn apply_chat_config(&self) -> Result<()> {
        let chat_settings = self.config.get().chat.clone();
        self.chat
            .apply_config(&crate::tenant::TenantId::default_tenant(), chat_settings)
            .await
    }

    pub async fn tenant_chat(&self, tenant_id: &crate::tenant::TenantId) -> Result<ChatHandle> {
        let tenant = self
            .tenants
            .find(tenant_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Tenant does not exist"))?;
        self.chat
            .tenant(tenant_id, tenant.chat.queue_capacity)
            .await
    }

    pub async fn tenant_config(&self, tenant_id: &crate::tenant::TenantId) -> Result<AppConfig> {
        let tenant = self
            .tenants
            .find(tenant_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Tenant does not exist"))?;
        let mut config = self.config.get().as_ref().clone();
        config.server.ingest_stream_key.clear();
        config.notifications = tenant.notifications;
        config.targets = tenant.targets;
        config.chat = tenant.chat;
        config.overlay = tenant.overlay;
        Ok(config)
    }

    pub async fn save_tenant_form(
        &self,
        tenant_id: &crate::tenant::TenantId,
        form: ConfigForm,
    ) -> Result<(AppConfig, bool)> {
        let current = self.tenant_config(tenant_id).await?;
        let updated = current.merge_form(form)?;
        updated.validate()?;
        let chat_changed = current.chat != updated.chat;
        self.tenants
            .update_configuration(
                tenant_id,
                &updated.notifications,
                &updated.chat,
                &updated.overlay,
                &updated.targets,
            )
            .await?;
        Ok((updated, chat_changed))
    }

    pub async fn set_youtube_polling(
        &self,
        tenant_id: &crate::tenant::TenantId,
        enabled: bool,
    ) -> Result<()> {
        let mut chat = self.tenant_config(tenant_id).await?.chat;
        if chat.youtube_polling_enabled == enabled {
            return Ok(());
        }
        chat.youtube_polling_enabled = enabled;
        self.save_chat(tenant_id, chat.clone()).await?;
        self.tenant_chat(tenant_id)
            .await?
            .set_youtube_polling(chat)
            .await?;
        Ok(())
    }

    pub async fn set_x_webhook(
        &self,
        tenant_id: &crate::tenant::TenantId,
        enabled: bool,
    ) -> Result<()> {
        let mut chat = self.tenant_config(tenant_id).await?.chat;
        chat.x_webhook_enabled = enabled;
        self.save_chat(tenant_id, chat).await?;
        tracing::info!(tenant_id = %tenant_id.as_str(), enabled, "X webhook ingestion changed");
        Ok(())
    }

    pub async fn set_kick_webhook(
        &self,
        tenant_id: &crate::tenant::TenantId,
        enabled: bool,
    ) -> Result<()> {
        let mut chat = self.tenant_config(tenant_id).await?.chat;
        if chat.kick_webhook_enabled == enabled {
            return Ok(());
        }
        crate::chat::kick::set_chat_subscription(&self.http_client, &chat, enabled).await?;
        chat.kick_webhook_enabled = enabled;
        self.save_chat(tenant_id, chat).await?;
        tracing::info!(tenant_id = %tenant_id.as_str(), enabled, "Kick webhook ingestion changed");
        Ok(())
    }

    async fn save_chat(
        &self,
        tenant_id: &crate::tenant::TenantId,
        chat: crate::config::ChatSettings,
    ) -> Result<()> {
        let tenant = self
            .tenants
            .find(tenant_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Tenant does not exist"))?;
        self.tenants
            .update_configuration(
                tenant_id,
                &tenant.notifications,
                &chat,
                &tenant.overlay,
                &tenant.targets,
            )
            .await
    }
}
