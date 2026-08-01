use crate::chat::{
    youtube::{YouTubeChatConfig, YouTubeChatTarget, YouTubeIngestStatus},
    ChatInbox,
};
use crate::config::{AppConfig, ConfigStore};
use crate::metrics::Metrics;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Child;
use tokio::sync::{watch, Mutex, RwLock};
use tokio::task::JoinHandle;

pub struct ProxyState {
    pub metrics: Arc<Metrics>,
    pub config: Arc<RwLock<AppConfig>>,
    pub http_client: Client,
    pub active_relays: Mutex<HashMap<String, Vec<Child>>>,
    pub chat_inbox: Mutex<ChatInbox>,
    chat_revision: watch::Sender<u64>,
    pub youtube_status: RwLock<Option<YouTubeIngestStatus>>,
    twitch_task: Mutex<Option<JoinHandle<()>>>,
    youtube_task: Mutex<Option<JoinHandle<()>>>,
    pub listen_port: u16,
    pub config_store: ConfigStore,
}

impl ProxyState {
    pub fn new(
        metrics: Arc<Metrics>,
        config: AppConfig,
        http_client: Client,
        listen_port: u16,
        config_store: ConfigStore,
    ) -> anyhow::Result<Self> {
        let chat_inbox = ChatInbox::open(config_store.path(), config.chat.queue_capacity)?;
        Ok(Self {
            metrics,
            config: Arc::new(RwLock::new(config)),
            http_client,
            active_relays: Mutex::new(HashMap::new()),
            chat_inbox: Mutex::new(chat_inbox),
            chat_revision: watch::channel(0).0,
            youtube_status: RwLock::new(None),
            twitch_task: Mutex::new(None),
            youtube_task: Mutex::new(None),
            listen_port,
            config_store,
        })
    }

    pub fn notify_chat_changed(&self) {
        self.chat_revision
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub fn subscribe_chat_changes(&self) -> watch::Receiver<u64> {
        self.chat_revision.subscribe()
    }

    pub async fn apply_chat_config(self: &Arc<Self>) -> anyhow::Result<()> {
        let chat = self.config.read().await.chat.clone();
        self.chat_inbox.lock().await.resize(chat.queue_capacity)?;

        if let Some(task) = self.twitch_task.lock().await.take() {
            task.abort();
        }
        if let Some(task) = self.youtube_task.lock().await.take() {
            task.abort();
        }
        *self.youtube_status.write().await = None;

        if let Some(channel) = chat
            .twitch_channel
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().trim_start_matches('#').to_ascii_lowercase())
        {
            let state = Arc::clone(self);
            let task = tokio::spawn(crate::chat::twitch::run(state, channel.clone()));
            *self.twitch_task.lock().await = Some(task);
            tracing::info!(channel, "Twitch anonymous IRC ingest configured");
        }

        let Some(api_key) = chat
            .youtube_api_key
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(());
        };
        let target = chat
            .youtube_live_chat_id
            .filter(|value| !value.trim().is_empty())
            .map(YouTubeChatTarget::LiveChat)
            .or_else(|| {
                chat.youtube_video_id
                    .filter(|value| !value.trim().is_empty())
                    .map(YouTubeChatTarget::Video)
            })
            .or_else(|| {
                chat.youtube_channel_id
                    .filter(|value| !value.trim().is_empty())
                    .map(YouTubeChatTarget::Channel)
            });
        let Some(target) = target else {
            return Ok(());
        };

        let state = Arc::clone(self);
        let task = tokio::spawn(crate::chat::youtube::run(
            state,
            YouTubeChatConfig {
                api_key,
                target,
                min_poll_interval: Duration::from_secs(chat.youtube_min_poll_interval_secs),
                adaptive_polling: chat.youtube_adaptive_polling,
            },
        ));
        *self.youtube_task.lock().await = Some(task);
        tracing::info!("YouTube live chat ingest configured");
        Ok(())
    }
}
