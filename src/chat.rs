use crate::config::ChatSettings;
use crate::util::{non_empty, now_unix_ms};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::path::Path;
use std::time::Duration;
use toasty::Executor;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

pub mod twitch;
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

#[derive(Debug, toasty::Model)]
#[table = "chat_messages"]
struct StoredChatMessage {
    #[key]
    #[auto]
    id: u64,
    source: String,
    external_id: String,
    author: String,
    text: String,
    avatar_url: Option<String>,
    sent_at: Option<String>,
    received_at_unix_ms: u64,
}

#[derive(Debug, toasty::Model)]
#[table = "chat_seen"]
struct StoredChatSeen {
    #[key]
    #[auto]
    id: u64,
    source: String,
    external_id: String,
}

#[derive(Debug, toasty::Model)]
#[table = "chat_state"]
struct StoredChatState {
    #[key]
    id: u64,
    dropped: u64,
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

/// SQLite-backed persistent chat inbox with bounded queue capacity and deduplication.
pub struct ChatInbox {
    capacity: usize,
    database: toasty::Db,
}

impl ChatInbox {
    pub async fn open(path: &Path, capacity: usize) -> Result<Self> {
        assert!(capacity > 0, "chat queue capacity must be positive");
        let database = toasty::Db::builder()
            .models(toasty::models!(
                StoredChatMessage,
                StoredChatSeen,
                StoredChatState
            ))
            .connect(&format!("sqlite:{}", path.display()))
            .await
            .with_context(|| format!("Failed to open chat inbox database '{}'", path.display()))?;
        Self::from_database(database, capacity).await
    }

    async fn from_database(database: toasty::Db, capacity: usize) -> Result<Self> {
        let mut database = database.clone();
        let state = StoredChatState::filter(StoredChatState::fields().id().eq(1_u64))
            .first()
            .exec(&mut database)
            .await;
        let state = if state.is_err() {
            database.push_schema().await?;
            StoredChatState::filter(StoredChatState::fields().id().eq(1_u64))
                .first()
                .exec(&mut database)
                .await?
        } else {
            state?
        };
        if state.is_none() {
            toasty::create!(StoredChatState { id: 1, dropped: 0 })
                .exec(&mut database)
                .await?;
        }

        let mut inbox = Self { capacity, database };
        inbox.trim_to_capacity().await?;
        Ok(inbox)
    }

    pub async fn enqueue(&mut self, incoming: IncomingChatMessage) -> Result<EnqueueOutcome> {
        let incoming = incoming.normalized()?;
        let mut database = self.database.clone();
        let mut transaction = database.transaction().await?;

        let seen = StoredChatSeen::filter(
            StoredChatSeen::fields().source().eq(&incoming.source).and(
                StoredChatSeen::fields()
                    .external_id()
                    .eq(&incoming.external_id),
            ),
        )
        .first()
        .exec(&mut transaction)
        .await?;
        if seen.is_some() {
            return Ok(EnqueueOutcome::Duplicate);
        }
        toasty::create!(StoredChatSeen {
            source: incoming.source.clone(),
            external_id: incoming.external_id.clone(),
        })
        .exec(&mut transaction)
        .await?;
        trim_seen(&mut transaction, self.capacity.saturating_mul(4)).await?;

        let mut messages = ordered_messages(&mut transaction).await?;
        let message_count = messages.len();
        if message_count >= self.capacity {
            increment_dropped(&mut transaction, 1).await?;
            if self.capacity == 1 {
                transaction.commit().await?;
                return Ok(EnqueueOutcome::Dropped);
            }
            messages.remove(1).delete().exec(&mut transaction).await?;
        }

        toasty::create!(StoredChatMessage {
            source: incoming.source,
            external_id: incoming.external_id,
            author: incoming.author,
            text: incoming.text,
            avatar_url: incoming.avatar_url,
            sent_at: incoming.sent_at,
            received_at_unix_ms: now_unix_ms(),
        })
        .exec(&mut transaction)
        .await?;
        transaction.commit().await?;
        Ok(EnqueueOutcome::Accepted)
    }

    pub async fn acknowledge(&mut self, expected_id: u64) -> Result<bool> {
        let mut database = self.database.clone();
        let Some(message) = ordered_messages(&mut database).await?.into_iter().next() else {
            return Ok(false);
        };
        if message.id != expected_id {
            return Ok(false);
        }
        message.delete().exec(&mut database).await?;
        Ok(true)
    }

    pub async fn snapshot(&self) -> Result<ChatInboxSnapshot> {
        let mut database = self.database.clone();
        let messages = ordered_messages(&mut database).await?;
        let queued = messages.len();
        let messages = messages
            .iter()
            .take(10)
            .map(chat_message_from_model)
            .collect::<Result<_>>()?;
        let dropped = load_state(&mut database).await?.dropped;

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
        let mut database = self.database.clone();
        let mut transaction = database.transaction().await?;
        let mut messages = ordered_messages(&mut transaction).await?;
        let excess = messages.len().saturating_sub(self.capacity);
        for _ in 0..excess {
            messages.remove(1).delete().exec(&mut transaction).await?;
        }
        if excess > 0 {
            increment_dropped(&mut transaction, excess).await?;
        }
        trim_seen(&mut transaction, self.capacity.saturating_mul(4)).await?;
        transaction.commit().await?;
        Ok(())
    }
}

fn chat_message_from_model(message: &StoredChatMessage) -> Result<ChatMessage> {
    Ok(ChatMessage {
        id: message.id,
        source: message.source.clone(),
        external_id: message.external_id.clone(),
        author: message.author.clone(),
        text: message.text.clone(),
        avatar_url: message.avatar_url.clone(),
        sent_at: message.sent_at.clone(),
        received_at_unix_ms: message.received_at_unix_ms,
    })
}

async fn ordered_messages(executor: &mut dyn Executor) -> Result<Vec<StoredChatMessage>> {
    Ok(StoredChatMessage::all()
        .order_by(StoredChatMessage::fields().id().asc())
        .exec(executor)
        .await?)
}

async fn increment_dropped(executor: &mut dyn Executor, amount: usize) -> Result<()> {
    let mut state = load_state(executor).await?;
    let dropped = state.dropped.saturating_add(amount as u64);
    state.update().dropped(dropped).exec(executor).await?;
    Ok(())
}

async fn trim_seen(executor: &mut dyn Executor, capacity: usize) -> Result<()> {
    let mut seen = StoredChatSeen::all()
        .order_by(StoredChatSeen::fields().id().asc())
        .exec(executor)
        .await?;
    while seen.len() > capacity {
        seen.remove(0).delete().exec(executor).await?;
    }
    Ok(())
}

async fn load_state(executor: &mut dyn Executor) -> Result<StoredChatState> {
    StoredChatState::filter(StoredChatState::fields().id().eq(1_u64))
        .first()
        .exec(executor)
        .await?
        .ok_or_else(|| anyhow::anyhow!("chat state row is missing"))
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
    SetXPolling {
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
    x_task: Option<JoinHandle<()>>,
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
                ChatCommand::SetXPolling { config, respond_to } => {
                    self.configure_x(&config, &handle);
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

        self.configure_x(chat, handle);
        self.configure_youtube(chat, handle);
        Ok(())
    }

    fn configure_x(&mut self, chat: &ChatSettings, handle: &ChatHandle) {
        if let Some(task) = self.x_task.take() {
            task.abort();
        }

        if chat.x_polling_enabled {
            let media_key = chat
                .x_media_key
                .clone()
                .filter(|value| !value.trim().is_empty());
            match media_key {
                Some(media_key) => {
                    let handle_clone = handle.clone();
                    let task = tokio::spawn(x::run(
                        self.http_client.clone(),
                        handle_clone,
                        x::XChatConfig { media_key },
                    ));
                    self.x_task = Some(task);
                    tracing::info!("X live chat ingest configured");
                }
                None => {
                    tracing::warn!("X live chat polling is enabled but no media key is configured");
                }
            }
        } else {
            tracing::info!("X live chat polling is off");
        }
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
    pub async fn spawn(path: &Path, capacity: usize, http_client: Client) -> Result<Self> {
        let inbox = ChatInbox::open(path, capacity).await?;
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
            x_task: None,
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

    pub async fn set_x_polling(&self, config: ChatSettings) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ChatCommand::SetXPolling {
                config,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Chat actor stopped"))?;
        rx.await
            .context("Chat actor dropped set_x_polling response")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DATABASE_ID: AtomicU64 = AtomicU64::new(1);

    async fn inbox(capacity: usize) -> ChatInbox {
        let path = database_path();
        ChatInbox::open(&path, capacity).await.unwrap()
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
        let mut inbox = ChatInbox::open(&path, 12).await.unwrap();
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
            let mut inbox = ChatInbox::open(&path, 3).await.unwrap();
            inbox
                .enqueue(message("youtube", "persisted", "still here"))
                .await
                .unwrap();
        }
        {
            let inbox = ChatInbox::open(&path, 3).await.unwrap();
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
            let mut inbox = ChatInbox::open(&path, 4).await.unwrap();
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
            let mut inbox = ChatInbox::open(&path, 2).await.unwrap();
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
        let _ = crate::config::ConfigStore::open(&path).await.unwrap();
        let mut inbox = ChatInbox::open(&path, 2).await.unwrap();
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
        let handle = ChatHandle::spawn(&path, 5, Client::new()).await.unwrap();
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
    async fn polling_controls_only_restart_the_selected_worker() {
        let actor_path = database_path();
        let handle_path = database_path();
        let inbox = ChatInbox::open(&actor_path, 5).await.unwrap();
        let (revision_tx, _) = watch::channel(0);
        let (youtube_status_tx, _) = watch::channel(None);
        let handle = ChatHandle::spawn(&handle_path, 5, Client::new())
            .await
            .unwrap();
        let mut actor = ChatActor {
            inbox,
            twitch_task: None,
            youtube_task: None,
            x_task: None,
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
        let x_task = tokio::spawn(std::future::pending());
        let x_abort = x_task.abort_handle();
        actor.x_task = Some(x_task);

        actor.configure_x(&ChatSettings::default(), &handle);
        tokio::task::yield_now().await;
        assert!(!twitch_abort.is_finished());
        assert!(!youtube_abort.is_finished());
        assert!(x_abort.is_finished());

        let x_task = tokio::spawn(std::future::pending());
        let x_abort = x_task.abort_handle();
        actor.x_task = Some(x_task);
        actor.configure_youtube(&ChatSettings::default(), &handle);
        tokio::task::yield_now().await;
        assert!(!twitch_abort.is_finished());
        assert!(youtube_abort.is_finished());
        assert!(!x_abort.is_finished());

        drop(actor);
        std::fs::remove_file(actor_path).unwrap();
        std::fs::remove_file(handle_path).unwrap();
    }
}
