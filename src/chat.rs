use crate::config::ChatSettings;
use crate::util::{non_empty, now_unix_ms};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use sqlx::{Any, Row, Transaction};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use tokio::task::JoinHandle;

pub mod kick;
pub mod relay;
pub mod twitch;
pub mod twitch_eventsub;
pub mod x;
pub mod youtube;

pub use youtube::{YouTubeChatConfig, YouTubeChatTarget, YouTubeIngestStatus};

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct IncomingChatMessage {
    #[validate(min_length = 1)]
    #[validate(max_length = 32)]
    #[validate(pattern = r"^[A-Za-z0-9_-]+$")]
    pub source: String,
    #[validate(min_length = 1)]
    #[validate(max_length = 256)]
    pub external_id: String,
    #[validate(min_length = 1)]
    #[validate(max_length = 200)]
    pub author: String,
    #[validate(min_length = 1)]
    #[validate(max_length = 5000)]
    pub text: String,
    #[serde(default)]
    #[validate(max_length = 2048)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    #[validate(max_length = 100)]
    pub sent_at: Option<String>,
}

impl IncomingChatMessage {
    pub fn normalized(mut self) -> Result<Self> {
        self.source = self.source.trim().to_ascii_lowercase();
        self.external_id = self.external_id.trim().to_string();
        self.author = self.author.trim().to_string();
        self.text = self.text.trim().to_string();
        self.avatar_url = non_empty(self.avatar_url);
        self.sent_at = non_empty(self.sent_at);

        self.validate()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;

        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub id: u64,
    pub source: String,
    pub external_id: String,
    pub author: String,
    pub text: String,
    pub avatar_url: Option<String>,
    pub sent_at: Option<String>,
    pub received_at_unix_ms: u64,
}

#[derive(Debug)]
struct StoredChatMessage {
    id: i64,
    source: String,
    external_id: String,
    author: String,
    text: String,
    avatar_url: Option<String>,
    sent_at: Option<String>,
    received_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatInboxSnapshot {
    pub messages: Vec<ChatMessage>,
    pub queued: usize,
    pub dropped: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    Accepted,
    Duplicate,
    Dropped,
}

/// Database-backed persistent chat inbox with bounded queue capacity and deduplication.
pub struct ChatInbox {
    tenant_id: crate::tenant::TenantId,
    capacity: usize,
    database: crate::database::Database,
}

impl ChatInbox {
    pub async fn open(
        database: crate::database::Database,
        tenant_id: crate::tenant::TenantId,
        capacity: usize,
    ) -> Result<Self> {
        assert!(capacity > 0, "chat queue capacity must be positive");
        sqlx::query(
            "INSERT INTO chat_state (tenant_id, dropped) VALUES ($1, $2) \
             ON CONFLICT (tenant_id) DO NOTHING",
        )
        .bind(tenant_id.as_str())
        .bind(0_i64)
        .execute(database.pool())
        .await?;
        let mut inbox = Self {
            tenant_id,
            capacity,
            database,
        };
        inbox.trim_to_capacity().await?;
        Ok(inbox)
    }

    pub async fn enqueue(&mut self, incoming: IncomingChatMessage) -> Result<EnqueueOutcome> {
        let incoming = incoming.normalized()?;
        let mut transaction = self.database.pool().begin().await?;

        let seen: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM chat_seen \
                 WHERE tenant_id = $1 AND source = $2 AND external_id = $3",
        )
        .bind(self.tenant_id.as_str())
        .bind(&incoming.source)
        .bind(&incoming.external_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if seen.is_some() {
            return Ok(EnqueueOutcome::Duplicate);
        }
        let seen_id = next_seen_id(&mut transaction).await?;
        sqlx::query(
            "INSERT INTO chat_seen (id, tenant_id, source, external_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(seen_id)
        .bind(self.tenant_id.as_str())
        .bind(&incoming.source)
        .bind(&incoming.external_id)
        .execute(&mut *transaction)
        .await?;
        trim_seen(
            &mut transaction,
            &self.tenant_id,
            self.capacity.saturating_mul(4),
        )
        .await?;

        let mut messages = ordered_messages(&mut transaction, &self.tenant_id).await?;
        let message_count = messages.len();
        if message_count >= self.capacity {
            increment_dropped(&mut transaction, &self.tenant_id, 1).await?;
            if self.capacity == 1 {
                transaction.commit().await?;
                return Ok(EnqueueOutcome::Dropped);
            }
            delete_message(&mut transaction, &self.tenant_id, messages.remove(1).id).await?;
        }

        let message_id = next_message_id(&mut transaction).await?;
        sqlx::query(
            "INSERT INTO chat_messages \
             (id, tenant_id, source, external_id, author, text, avatar_url, sent_at, received_at_unix_ms) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(message_id)
        .bind(self.tenant_id.as_str())
        .bind(incoming.source)
        .bind(incoming.external_id)
        .bind(incoming.author)
        .bind(incoming.text)
        .bind(incoming.avatar_url)
        .bind(incoming.sent_at)
        .bind(now_unix_ms() as i64)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(EnqueueOutcome::Accepted)
    }

    pub async fn acknowledge(&mut self, expected_id: u64) -> Result<bool> {
        let mut transaction = self.database.pool().begin().await?;
        let Some(message) = ordered_messages(&mut transaction, &self.tenant_id)
            .await?
            .into_iter()
            .next()
        else {
            return Ok(false);
        };
        if message.id != expected_id as i64 {
            return Ok(false);
        }
        delete_message(&mut transaction, &self.tenant_id, message.id).await?;
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn snapshot(&self) -> Result<ChatInboxSnapshot> {
        let mut transaction = self.database.pool().begin().await?;
        let messages = ordered_messages(&mut transaction, &self.tenant_id).await?;
        let queued = messages.len();
        let messages = messages
            .iter()
            .take(10)
            .map(chat_message_from_model)
            .collect::<Result<_>>()?;
        let dropped = load_dropped(&mut transaction, &self.tenant_id).await?;

        Ok(ChatInboxSnapshot {
            messages,
            queued,
            dropped,
        })
    }

    pub async fn resize(&mut self, capacity: usize) -> Result<()> {
        assert!(capacity > 0, "chat queue capacity must be positive");
        self.capacity = capacity;
        self.trim_to_capacity().await
    }

    async fn trim_to_capacity(&mut self) -> Result<()> {
        let mut transaction = self.database.pool().begin().await?;
        let mut messages = ordered_messages(&mut transaction, &self.tenant_id).await?;
        let excess = messages.len().saturating_sub(self.capacity);
        for _ in 0..excess {
            delete_message(&mut transaction, &self.tenant_id, messages.remove(1).id).await?;
        }
        if excess > 0 {
            increment_dropped(&mut transaction, &self.tenant_id, excess).await?;
        }
        trim_seen(
            &mut transaction,
            &self.tenant_id,
            self.capacity.saturating_mul(4),
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}

fn chat_message_from_model(message: &StoredChatMessage) -> Result<ChatMessage> {
    Ok(ChatMessage {
        id: message
            .id
            .try_into()
            .context("Stored chat ID is negative")?,
        source: message.source.clone(),
        external_id: message.external_id.clone(),
        author: message.author.clone(),
        text: message.text.clone(),
        avatar_url: message.avatar_url.clone(),
        sent_at: message.sent_at.clone(),
        received_at_unix_ms: message
            .received_at_unix_ms
            .try_into()
            .context("Stored chat timestamp is negative")?,
    })
}

async fn ordered_messages(
    transaction: &mut Transaction<'_, Any>,
    tenant_id: &crate::tenant::TenantId,
) -> Result<Vec<StoredChatMessage>> {
    let rows = sqlx::query(
        "SELECT id, source, external_id, author, text, avatar_url, sent_at, received_at_unix_ms \
         FROM chat_messages WHERE tenant_id = $1 ORDER BY id ASC",
    )
    .bind(tenant_id.as_str())
    .fetch_all(&mut **transaction)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(StoredChatMessage {
                id: row.try_get("id")?,
                source: row.try_get("source")?,
                external_id: row.try_get("external_id")?,
                author: row.try_get("author")?,
                text: row.try_get("text")?,
                avatar_url: row.try_get("avatar_url")?,
                sent_at: row.try_get("sent_at")?,
                received_at_unix_ms: row.try_get("received_at_unix_ms")?,
            })
        })
        .collect()
}

async fn increment_dropped(
    transaction: &mut Transaction<'_, Any>,
    tenant_id: &crate::tenant::TenantId,
    amount: usize,
) -> Result<()> {
    sqlx::query("UPDATE chat_state SET dropped = dropped + $1 WHERE tenant_id = $2")
        .bind(amount as i64)
        .bind(tenant_id.as_str())
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn trim_seen(
    transaction: &mut Transaction<'_, Any>,
    tenant_id: &crate::tenant::TenantId,
    capacity: usize,
) -> Result<()> {
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM chat_seen WHERE tenant_id = $1 ORDER BY id ASC")
            .bind(tenant_id.as_str())
            .fetch_all(&mut **transaction)
            .await?;
    let excess = ids.len().saturating_sub(capacity);
    for id in ids.into_iter().take(excess) {
        sqlx::query("DELETE FROM chat_seen WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id.as_str())
            .bind(id)
            .execute(&mut **transaction)
            .await?;
    }
    Ok(())
}

async fn load_dropped(
    transaction: &mut Transaction<'_, Any>,
    tenant_id: &crate::tenant::TenantId,
) -> Result<u64> {
    let dropped: i64 = sqlx::query_scalar("SELECT dropped FROM chat_state WHERE tenant_id = $1")
        .bind(tenant_id.as_str())
        .fetch_one(&mut **transaction)
        .await?;
    dropped
        .try_into()
        .context("Stored dropped count is negative")
}

async fn next_seen_id(transaction: &mut Transaction<'_, Any>) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) + 1 FROM chat_seen")
            .fetch_one(&mut **transaction)
            .await?,
    )
}

async fn next_message_id(transaction: &mut Transaction<'_, Any>) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) + 1 FROM chat_messages")
            .fetch_one(&mut **transaction)
            .await?,
    )
}

async fn delete_message(
    transaction: &mut Transaction<'_, Any>,
    tenant_id: &crate::tenant::TenantId,
    id: i64,
) -> Result<()> {
    sqlx::query("DELETE FROM chat_messages WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id.as_str())
        .bind(id)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

enum ChatCommand {
    Enqueue {
        message: IncomingChatMessage,
        respond_to: oneshot::Sender<Result<EnqueueOutcome>>,
    },
    Acknowledge {
        expected_id: u64,
        respond_to: oneshot::Sender<Result<bool>>,
    },
    Snapshot {
        respond_to: oneshot::Sender<Result<ChatInboxSnapshot>>,
    },
    ApplyConfig {
        config: ChatSettings,
        respond_to: oneshot::Sender<Result<()>>,
    },
    SetYouTubePolling {
        config: ChatSettings,
        respond_to: oneshot::Sender<()>,
    },
    UpdateYouTubeStatus {
        state: String,
        detail: String,
        last_success_at_unix_ms: Option<u64>,
        newly_received: Option<u64>,
    },
}

/// Actor owning chat queue storage and background platform workers exclusively without mutexes.
struct ChatActor {
    inbox: ChatInbox,
    twitch_task: Option<JoinHandle<()>>,
    youtube_task: Option<JoinHandle<()>>,
    youtube_status: Option<YouTubeIngestStatus>,
    revision: u64,
    revision_tx: watch::Sender<u64>,
    youtube_status_tx: watch::Sender<Option<YouTubeIngestStatus>>,
    http_client: Client,
}

impl ChatActor {
    async fn run(mut self, mut receiver: mpsc::Receiver<ChatCommand>, handle: ChatHandle) {
        while let Some(command) = receiver.recv().await {
            match command {
                ChatCommand::Enqueue {
                    message,
                    respond_to,
                } => {
                    let outcome = self.inbox.enqueue(message).await;
                    if let Ok(outcome) = &outcome
                        && matches!(outcome, EnqueueOutcome::Accepted | EnqueueOutcome::Dropped)
                    {
                        self.notify_changed();
                    }
                    let _ = respond_to.send(outcome);
                }
                ChatCommand::Acknowledge {
                    expected_id,
                    respond_to,
                } => {
                    let acknowledged = self.inbox.acknowledge(expected_id).await;
                    if let Ok(true) = acknowledged {
                        self.notify_changed();
                    }
                    let _ = respond_to.send(acknowledged);
                }
                ChatCommand::Snapshot { respond_to } => {
                    let _ = respond_to.send(self.inbox.snapshot().await);
                }
                ChatCommand::ApplyConfig { config, respond_to } => {
                    let res = self.handle_apply_config(&config, &handle).await;
                    let _ = respond_to.send(res);
                }
                ChatCommand::SetYouTubePolling { config, respond_to } => {
                    self.configure_youtube(&config, &handle);
                    let _ = respond_to.send(());
                }
                ChatCommand::UpdateYouTubeStatus {
                    state,
                    detail,
                    last_success_at_unix_ms,
                    newly_received,
                } => {
                    if self.youtube_task.is_none() {
                        continue;
                    }
                    let status = self
                        .youtube_status
                        .get_or_insert_with(YouTubeIngestStatus::default);
                    status.state = state;
                    status.detail = detail.chars().take(240).collect();
                    if let Some(last_success_at_unix_ms) = last_success_at_unix_ms {
                        status.last_success_at_unix_ms = Some(last_success_at_unix_ms);
                    }
                    if let Some(newly_received) = newly_received {
                        status.messages_received =
                            status.messages_received.saturating_add(newly_received);
                    }
                    self.youtube_status_tx
                        .send_replace(self.youtube_status.clone());
                }
            }
        }
    }

    fn notify_changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.revision_tx.send_replace(self.revision);
    }

    async fn handle_apply_config(
        &mut self,
        chat: &ChatSettings,
        handle: &ChatHandle,
    ) -> Result<()> {
        self.inbox.resize(chat.queue_capacity).await?;

        if let Some(task) = self.twitch_task.take() {
            task.abort();
        }

        if let Some(channel) = chat
            .twitch_channel
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().trim_start_matches('#').to_ascii_lowercase())
        {
            let handle_clone = handle.clone();
            let task = tokio::spawn(twitch::run(handle_clone, channel.clone()));
            self.twitch_task = Some(task);
            tracing::info!(channel, "Twitch anonymous IRC ingest configured");
        }

        self.configure_youtube(chat, handle);
        Ok(())
    }

    fn configure_youtube(&mut self, chat: &ChatSettings, handle: &ChatHandle) {
        if let Some(task) = self.youtube_task.take() {
            task.abort();
        }
        self.youtube_status = None;
        self.youtube_status_tx.send_replace(None);

        let target = chat
            .youtube_live_chat_id
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .map(YouTubeChatTarget::LiveChat)
            .or_else(|| {
                chat.youtube_video_id
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
                    .map(YouTubeChatTarget::Video)
            })
            .or_else(|| {
                chat.youtube_channel_id
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
                    .map(YouTubeChatTarget::Channel)
            });

        let Some(target) = target else {
            return;
        };

        if !chat.youtube_polling_enabled {
            let status = YouTubeIngestStatus {
                state: "off".into(),
                detail: "Polling is off. Turn it on when the YouTube stream is live.".into(),
                ..YouTubeIngestStatus::default()
            };
            self.youtube_status = Some(status.clone());
            self.youtube_status_tx.send_replace(Some(status));
            self.notify_changed();
            tracing::info!("YouTube live chat polling is off");
            return;
        }

        let handle_clone = handle.clone();
        let task = tokio::spawn(youtube::run(
            self.http_client.clone(),
            handle_clone,
            YouTubeChatConfig {
                target,
                min_poll_interval: Duration::from_secs(chat.youtube_min_poll_interval_secs),
                adaptive_polling: chat.youtube_adaptive_polling,
            },
        ));
        self.youtube_task = Some(task);
        tracing::info!("YouTube live chat ingest configured");
    }
}

/// Lightweight, cloneable handle to the ChatActor for lock-free reads and async command dispatch.
#[derive(Clone)]
pub struct ChatHandle {
    sender: mpsc::Sender<ChatCommand>,
    revision_rx: watch::Receiver<u64>,
    #[allow(dead_code)]
    youtube_status_rx: watch::Receiver<Option<YouTubeIngestStatus>>,
}

impl ChatHandle {
    pub async fn spawn(
        database: crate::database::Database,
        tenant_id: crate::tenant::TenantId,
        capacity: usize,
        http_client: Client,
    ) -> Result<Self> {
        let inbox = ChatInbox::open(database, tenant_id, capacity).await?;
        let (revision_tx, revision_rx) = watch::channel(0);
        let (youtube_status_tx, youtube_status_rx) = watch::channel(None);
        let (sender, receiver) = mpsc::channel(64);

        let handle = Self {
            sender,
            revision_rx,
            youtube_status_rx,
        };

        let actor = ChatActor {
            inbox,
            twitch_task: None,
            youtube_task: None,
            youtube_status: None,
            revision: 0,
            revision_tx,
            youtube_status_tx,
            http_client,
        };

        let handle_for_actor = handle.clone();
        tokio::spawn(async move {
            actor.run(receiver, handle_for_actor).await;
        });

        Ok(handle)
    }

    pub async fn enqueue(&self, message: IncomingChatMessage) -> Result<EnqueueOutcome> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ChatCommand::Enqueue {
                message,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Chat actor stopped"))?;
        rx.await.context("Chat actor dropped enqueue response")?
    }

    pub async fn acknowledge(&self, expected_id: u64) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ChatCommand::Acknowledge {
                expected_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Chat actor stopped"))?;
        rx.await
            .context("Chat actor dropped acknowledge response")?
    }

    pub async fn snapshot(&self) -> Result<ChatInboxSnapshot> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ChatCommand::Snapshot { respond_to: tx })
            .await
            .map_err(|_| anyhow::anyhow!("Chat actor stopped"))?;
        rx.await.context("Chat actor dropped snapshot response")?
    }

    pub async fn apply_config(&self, config: ChatSettings) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ChatCommand::ApplyConfig {
                config,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Chat actor stopped"))?;
        rx.await
            .context("Chat actor dropped apply_config response")?
    }

    pub async fn set_youtube_polling(&self, config: ChatSettings) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ChatCommand::SetYouTubePolling {
                config,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Chat actor stopped"))?;
        rx.await
            .context("Chat actor dropped set_youtube_polling response")
    }

    pub fn update_youtube_status(
        &self,
        state: &str,
        detail: impl Into<String>,
        last_success_at_unix_ms: Option<u64>,
        newly_received: Option<u64>,
    ) {
        let _ = self.sender.try_send(ChatCommand::UpdateYouTubeStatus {
            state: state.to_string(),
            detail: detail.into(),
            last_success_at_unix_ms,
            newly_received,
        });
    }

    pub fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.revision_rx.clone()
    }

    #[allow(dead_code)]
    pub fn youtube_status(&self) -> Option<YouTubeIngestStatus> {
        self.youtube_status_rx.borrow().clone()
    }
}

/// Lazily owns one isolated chat actor per tenant.
#[derive(Clone)]
pub struct ChatHub {
    database: crate::database::Database,
    http_client: Client,
    handles: Arc<Mutex<HashMap<crate::tenant::TenantId, ChatHandle>>>,
}

impl ChatHub {
    pub fn new(database: crate::database::Database, http_client: Client) -> Self {
        Self {
            database,
            http_client,
            handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn tenant(
        &self,
        tenant_id: &crate::tenant::TenantId,
        capacity: usize,
    ) -> Result<ChatHandle> {
        let mut handles = self.handles.lock().await;
        if let Some(handle) = handles.get(tenant_id) {
            return Ok(handle.clone());
        }
        let handle = ChatHandle::spawn(
            self.database.clone(),
            tenant_id.clone(),
            capacity,
            self.http_client.clone(),
        )
        .await?;
        handles.insert(tenant_id.clone(), handle.clone());
        Ok(handle)
    }

    pub async fn apply_config(
        &self,
        tenant_id: &crate::tenant::TenantId,
        config: ChatSettings,
    ) -> Result<()> {
        self.tenant(tenant_id, config.queue_capacity)
            .await?
            .apply_config(config)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DATABASE_ID: AtomicU64 = AtomicU64::new(1);

    async fn inbox(capacity: usize) -> ChatInbox {
        let path = database_path();
        ChatInbox::open(
            database(&path).await,
            crate::tenant::TenantId::default_tenant(),
            capacity,
        )
        .await
        .unwrap()
    }

    async fn database(path: &std::path::Path) -> crate::database::Database {
        crate::database::Database::connect(&crate::database::sqlite_url(path))
            .await
            .unwrap()
    }

    fn message(source: &str, external_id: &str, text: &str) -> IncomingChatMessage {
        IncomingChatMessage {
            source: source.into(),
            external_id: external_id.into(),
            author: "Viewer".into(),
            text: text.into(),
            avatar_url: None,
            sent_at: None,
        }
    }

    fn database_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "rtmp-proxy-chat-test-{}-{}-{}.sqlite3",
            std::process::id(),
            TEST_DATABASE_ID.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ))
    }

    #[tokio::test]
    async fn tenant_inboxes_do_not_share_messages_or_deduplication() {
        let path = database_path();
        let database = database(&path).await;
        let mut alpha = ChatInbox::open(
            database.clone(),
            crate::tenant::TenantId::new("alpha").unwrap(),
            10,
        )
        .await
        .unwrap();
        let mut beta = ChatInbox::open(database, crate::tenant::TenantId::new("beta").unwrap(), 10)
            .await
            .unwrap();

        assert_eq!(
            alpha
                .enqueue(message("twitch", "same-id", "alpha message"))
                .await
                .unwrap(),
            EnqueueOutcome::Accepted
        );
        assert_eq!(
            beta.enqueue(message("twitch", "same-id", "beta message"))
                .await
                .unwrap(),
            EnqueueOutcome::Accepted
        );
        assert_eq!(
            alpha.snapshot().await.unwrap().messages[0].text,
            "alpha message"
        );
        assert_eq!(
            beta.snapshot().await.unwrap().messages[0].text,
            "beta message"
        );
    }

    #[tokio::test]
    async fn acknowledge_advances_one_message_at_a_time() {
        let mut inbox = inbox(3).await;
        inbox
            .enqueue(message("twitch", "1", "first"))
            .await
            .unwrap();
        inbox
            .enqueue(message("youtube", "2", "second"))
            .await
            .unwrap();

        let first = inbox.snapshot().await.unwrap();
        assert_eq!(first.messages[0].text, "first");
        assert_eq!(first.queued, 2);
        assert!(inbox.acknowledge(1).await.unwrap());

        let second = inbox.snapshot().await.unwrap();
        assert_eq!(second.messages[0].text, "second");
        assert_eq!(second.queued, 1);
    }

    #[tokio::test]
    async fn snapshot_shows_the_first_ten_messages() {
        let path = database_path();
        let mut inbox = ChatInbox::open(
            database(&path).await,
            crate::tenant::TenantId::default_tenant(),
            12,
        )
        .await
        .unwrap();
        for id in 1..=11 {
            inbox
                .enqueue(message("twitch", &id.to_string(), &format!("message {id}")))
                .await
                .unwrap();
        }

        let snapshot = inbox.snapshot().await.unwrap();
        assert_eq!(snapshot.queued, 11);
        assert_eq!(snapshot.messages.len(), 10);
        assert_eq!(snapshot.messages[0].text, "message 1");
        assert_eq!(snapshot.messages[9].text, "message 10");
        assert!(inbox.acknowledge(snapshot.messages[0].id).await.unwrap());
        assert_eq!(
            inbox.snapshot().await.unwrap().messages[0].text,
            "message 2"
        );
        drop(inbox);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn stale_acknowledgement_cannot_clear_a_newer_message() {
        let mut inbox = inbox(3).await;
        inbox
            .enqueue(message("twitch", "1", "first"))
            .await
            .unwrap();
        inbox
            .enqueue(message("twitch", "2", "second"))
            .await
            .unwrap();
        assert!(inbox.acknowledge(1).await.unwrap());
        assert!(!inbox.acknowledge(1).await.unwrap());
        assert_eq!(inbox.snapshot().await.unwrap().messages[0].text, "second");
    }

    #[tokio::test]
    async fn duplicate_platform_message_is_ignored_after_acknowledgement() {
        let mut inbox = inbox(3).await;
        assert_eq!(
            inbox
                .enqueue(message("twitch", "same", "first"))
                .await
                .unwrap(),
            EnqueueOutcome::Accepted
        );
        assert!(inbox.acknowledge(1).await.unwrap());
        assert_eq!(
            inbox
                .enqueue(message("twitch", "same", "duplicate"))
                .await
                .unwrap(),
            EnqueueOutcome::Duplicate
        );
        assert!(inbox.snapshot().await.unwrap().messages.is_empty());
    }

    #[tokio::test]
    async fn full_queue_keeps_current_and_most_recent_waiting_messages() {
        let mut inbox = inbox(3).await;
        inbox
            .enqueue(message("twitch", "1", "current"))
            .await
            .unwrap();
        inbox
            .enqueue(message("twitch", "2", "old waiting"))
            .await
            .unwrap();
        inbox
            .enqueue(message("youtube", "3", "newer waiting"))
            .await
            .unwrap();
        inbox
            .enqueue(message("x", "4", "newest waiting"))
            .await
            .unwrap();

        assert_eq!(inbox.snapshot().await.unwrap().messages[0].text, "current");
        assert_eq!(inbox.snapshot().await.unwrap().queued, 3);
        assert_eq!(inbox.snapshot().await.unwrap().dropped, 1);
        assert!(inbox.acknowledge(1).await.unwrap());
        assert_eq!(
            inbox.snapshot().await.unwrap().messages[0].text,
            "newer waiting"
        );
    }

    #[tokio::test]
    async fn sqlite_queue_survives_reopening() {
        let path = database_path();
        {
            let mut inbox = ChatInbox::open(
                database(&path).await,
                crate::tenant::TenantId::default_tenant(),
                3,
            )
            .await
            .unwrap();
            inbox
                .enqueue(message("youtube", "persisted", "still here"))
                .await
                .unwrap();
        }
        {
            let inbox = ChatInbox::open(
                database(&path).await,
                crate::tenant::TenantId::default_tenant(),
                3,
            )
            .await
            .unwrap();
            assert_eq!(
                inbox.snapshot().await.unwrap().messages[0].text,
                "still here"
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn reducing_capacity_preserves_current_and_newest_waiting_messages() {
        let path = database_path();
        {
            let mut inbox = ChatInbox::open(
                database(&path).await,
                crate::tenant::TenantId::default_tenant(),
                4,
            )
            .await
            .unwrap();
            inbox
                .enqueue(message("twitch", "1", "current"))
                .await
                .unwrap();
            inbox
                .enqueue(message("twitch", "2", "oldest"))
                .await
                .unwrap();
            inbox
                .enqueue(message("twitch", "3", "newer"))
                .await
                .unwrap();
            inbox
                .enqueue(message("twitch", "4", "newest"))
                .await
                .unwrap();
        }
        {
            let mut inbox = ChatInbox::open(
                database(&path).await,
                crate::tenant::TenantId::default_tenant(),
                2,
            )
            .await
            .unwrap();
            let snapshot = inbox.snapshot().await.unwrap();
            assert_eq!(snapshot.messages[0].text, "current");
            assert_eq!(snapshot.queued, 2);
            assert_eq!(snapshot.dropped, 2);
            assert!(inbox.acknowledge(1).await.unwrap());
            assert_eq!(inbox.snapshot().await.unwrap().messages[0].text, "newest");
        }
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn opens_after_the_configuration_store_on_the_same_database() {
        let path = database_path();
        let database = database(&path).await;
        let _ = crate::config::ConfigStore::open(database.clone())
            .await
            .unwrap();
        let mut inbox = ChatInbox::open(database, crate::tenant::TenantId::default_tenant(), 2)
            .await
            .unwrap();
        assert_eq!(
            inbox
                .enqueue(message("twitch", "shared-db", "works"))
                .await
                .unwrap(),
            EnqueueOutcome::Accepted
        );
        assert_eq!(inbox.snapshot().await.unwrap().messages[0].text, "works");
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn chat_handle_actor_processes_commands() {
        let path = database_path();
        let handle = ChatHandle::spawn(
            database(&path).await,
            crate::tenant::TenantId::default_tenant(),
            5,
            Client::new(),
        )
        .await
        .unwrap();
        let mut rev_rx = handle.subscribe_changes();

        let outcome = handle
            .enqueue(message("twitch", "actor-1", "hello actor"))
            .await
            .unwrap();
        assert_eq!(outcome, EnqueueOutcome::Accepted);

        rev_rx.changed().await.unwrap();
        let snapshot = handle.snapshot().await.unwrap();
        assert_eq!(snapshot.messages.len(), 1);
        assert_eq!(snapshot.messages[0].text, "hello actor");

        let ack = handle.acknowledge(snapshot.messages[0].id).await.unwrap();
        assert!(ack);
        let snapshot2 = handle.snapshot().await.unwrap();
        assert_eq!(snapshot2.messages.len(), 0);

        handle.update_youtube_status("polling", "stale update", Some(12345), Some(2));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(handle.youtube_status().is_none());

        let settings = ChatSettings {
            youtube_api_key: Some("api-key".into()),
            youtube_live_chat_id: Some("live-chat-id".into()),
            youtube_polling_enabled: false,
            ..ChatSettings::default()
        };
        handle.set_youtube_polling(settings).await.unwrap();
        let status = handle.youtube_status().unwrap();
        assert_eq!(status.state, "off");

        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn youtube_control_does_not_restart_twitch() {
        let actor_path = database_path();
        let handle_path = database_path();
        let inbox = ChatInbox::open(
            database(&actor_path).await,
            crate::tenant::TenantId::default_tenant(),
            5,
        )
        .await
        .unwrap();
        let (revision_tx, _) = watch::channel(0);
        let (youtube_status_tx, _) = watch::channel(None);
        let handle = ChatHandle::spawn(
            database(&handle_path).await,
            crate::tenant::TenantId::default_tenant(),
            5,
            Client::new(),
        )
        .await
        .unwrap();
        let mut actor = ChatActor {
            inbox,
            twitch_task: None,
            youtube_task: None,
            youtube_status: None,
            revision: 0,
            revision_tx,
            youtube_status_tx,
            http_client: Client::new(),
        };

        let twitch_task = tokio::spawn(std::future::pending());
        let twitch_abort = twitch_task.abort_handle();
        actor.twitch_task = Some(twitch_task);
        let youtube_task = tokio::spawn(std::future::pending());
        let youtube_abort = youtube_task.abort_handle();
        actor.youtube_task = Some(youtube_task);
        actor.configure_youtube(&ChatSettings::default(), &handle);
        tokio::task::yield_now().await;
        assert!(!twitch_abort.is_finished());
        assert!(youtube_abort.is_finished());

        drop(actor);
        std::fs::remove_file(actor_path).unwrap();
        std::fs::remove_file(handle_path).unwrap();
    }
}
