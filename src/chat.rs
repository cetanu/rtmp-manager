use crate::config::ChatSettings;
use crate::metrics::Metrics;
use crate::util::{non_empty, now_unix_ms};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use toasty::Executor;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

pub mod kick;
pub mod twitch;
pub mod types;
pub mod util;
pub mod x;
pub mod youtube;

pub use types::{
    ChatMessagePart, ChatState, EnqueueOutcome, IncomingChatMessage, PomodoroState, Source,
    YouTubeChatConfig, YouTubeChatSink, YouTubeChatTarget, YouTubeIngestState, YouTubeIngestStatus,
};

const INBOX_PREVIEW_LIMIT: usize = 10;
const SEEN_ID_RETENTION_MULTIPLIER: usize = 4;
const ACTOR_COMMAND_CAPACITY: usize = 64;
const STATUS_DETAIL_LIMIT: usize = 240;
pub const POMODORO_MIN_DURATION_SECS: u64 = 60;
pub const POMODORO_MAX_DURATION_SECS: u64 = 45 * 60;
pub const POMODORO_DEFAULT_MESSAGE: &str = "Focus mode — chat paused";
pub const POMODORO_MAX_MESSAGE_CHARS: usize = 280;

impl IncomingChatMessage {
    pub fn normalized(mut self) -> Result<Self> {
        self.external_id = self.external_id.trim().to_string();
        self.author = self.author.trim().to_string();
        self.text = self.text.trim().to_string();
        self.parts.retain(|part| match part {
            ChatMessagePart::Text(text) => !text.is_empty(),
            ChatMessagePart::Emoji { alt, url } => !alt.is_empty() && !url.is_empty(),
        });
        self.avatar_url = non_empty(self.avatar_url);
        self.sent_at = non_empty(self.sent_at);

        self.validate()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;

        Ok(self)
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TestChatMessageRequest {
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

const SAMPLE_TEST_MESSAGES: &[(Source, &str, &str)] = &[
    (
        Source::Twitch,
        "Gabriel",
        "Look up from the relentless noise of your daily life and listen to the quiet intuition guiding you, because the answers you seek have already been spoken.",
    ),
    (
        Source::YouTube,
        "Michael",
        "Stand firm against the fear that seeks to paralyze this world, for you have the inherent strength and courage to protect what is right.",
    ),
    (
        Source::Kick,
        "Raphael",
        "Forgive yourself for the heavy burdens you were never meant to carry alone, and allow your exhausted mind and body the necessary grace to truly heal.",
    ),
    (
        Source::X,
        "Uriel",
        "Seek the light of truth in times of manufactured chaos, remembering that real wisdom is found in calm discernment rather than the loudest voices.",
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: u64,
    pub source: Source,
    pub external_id: String,
    pub author: String,
    pub text: String,
    pub parts: Vec<ChatMessagePart>,
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
    emoji_data: Option<String>,
    avatar_url: Option<String>,
    sent_at: Option<String>,
    received_at_unix_ms: u64,
}

#[derive(Debug, toasty::Model)]
#[table = "chat_seen"]
#[index(source, external_id)]
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

/// Durable focus-timer row so an active pomodoro survives process restarts.
/// Single-row table (`id = 1`); absent when no pomodoro is active.
#[derive(Debug, toasty::Model)]
#[table = "chat_pomodoro"]
struct StoredPomodoro {
    #[key]
    id: u64,
    message: String,
    started_at_unix_ms: u64,
    ends_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatInboxSnapshot {
    pub messages: Vec<ChatMessage>,
    pub queued: usize,
    pub dropped: u64,
    #[serde(default)]
    pub pomodoro: Option<PomodoroState>,
}

pub fn validate_pomodoro(message: &str, duration_secs: u64) -> Result<(String, u64)> {
    anyhow::ensure!(
        (POMODORO_MIN_DURATION_SECS..=POMODORO_MAX_DURATION_SECS).contains(&duration_secs),
        "Pomodoro duration must be between {} and {} minutes",
        POMODORO_MIN_DURATION_SECS / 60,
        POMODORO_MAX_DURATION_SECS / 60,
    );
    let trimmed = message.trim();
    let message = if trimmed.is_empty() {
        POMODORO_DEFAULT_MESSAGE.to_string()
    } else {
        trimmed.to_string()
    };
    anyhow::ensure!(
        message.chars().count() <= POMODORO_MAX_MESSAGE_CHARS,
        "Pomodoro message must be at most {POMODORO_MAX_MESSAGE_CHARS} characters",
    );
    Ok((message, duration_secs))
}

/// SQLite-backed persistent chat inbox with bounded queue capacity and deduplication.
pub struct ChatInbox {
    capacity: usize,
    database: toasty::Db,
}

impl ChatInbox {
    pub async fn open(path: &Path, capacity: usize) -> Result<Self> {
        anyhow::ensure!(capacity > 0, "chat queue capacity must be positive");
        // Full model registry: this file is shared with the config store
        // (see `crate::db::connect`).
        let database = crate::db::connect(path).await?;
        Self::from_database(database, capacity).await
    }

    async fn from_database(database: toasty::Db, capacity: usize) -> Result<Self> {
        let mut database = database.clone();
        // Embedded migrations run on every open so redeploys against an
        // existing SQLite file pick up schema changes instead of crashing.
        crate::db::run_migrations(&database)
            .await
            .context("Failed to migrate chat inbox database")?;
        let state = StoredChatState::filter(StoredChatState::fields().id().eq(1_u64))
            .first()
            .exec(&mut database)
            .await
            .context("Failed to load chat state after migrations")?;
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
            StoredChatSeen::fields()
                .source()
                .eq(incoming.source.as_str())
                .and(
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
            source: incoming.source.to_string(),
            external_id: incoming.external_id.clone(),
        })
        .exec(&mut transaction)
        .await?;
        trim_seen(
            &mut transaction,
            self.capacity.saturating_mul(SEEN_ID_RETENTION_MULTIPLIER),
        )
        .await?;

        let message_count = count_messages(&mut transaction).await?;
        if message_count >= self.capacity {
            increment_dropped(&mut transaction, 1).await?;
            if self.capacity == 1 {
                transaction.commit().await?;
                return Ok(EnqueueOutcome::Dropped);
            }
            let mut messages = ordered_messages(&mut transaction, 2).await?;
            remove_oldest_waiting(&mut messages, &mut transaction).await?;
        }

        let emoji_data = (!incoming.parts.is_empty())
            .then(|| serde_json::to_string(&incoming.parts))
            .transpose()?;
        toasty::create!(StoredChatMessage {
            source: incoming.source.to_string(),
            external_id: incoming.external_id,
            author: incoming.author,
            text: incoming.text,
            emoji_data,
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
        let Some(message) = ordered_messages(&mut database, 1).await?.into_iter().next() else {
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
        let messages = ordered_messages(&mut database, INBOX_PREVIEW_LIMIT).await?;
        let queued = count_messages(&mut database).await?;
        let messages = messages
            .iter()
            .map(chat_message_from_model)
            .collect::<Result<_>>()?;
        let dropped = load_state(&mut database).await?.dropped;

        Ok(ChatInboxSnapshot {
            messages,
            queued,
            dropped,
            pomodoro: None,
        })
    }

    pub async fn resize(&mut self, capacity: usize) -> Result<()> {
        anyhow::ensure!(capacity > 0, "chat queue capacity must be positive");
        self.capacity = capacity;
        self.trim_to_capacity().await
    }

    /// Loads the persisted pomodoro, deleting it if already expired.
    /// Used once at startup to rehydrate focus mode after a restart.
    pub async fn load_persisted_pomodoro(&self) -> Result<Option<PomodoroState>> {
        let mut database = self.database.clone();
        let Some(row) = StoredPomodoro::filter(StoredPomodoro::fields().id().eq(1_u64))
            .first()
            .exec(&mut database)
            .await?
        else {
            return Ok(None);
        };
        let pomodoro = PomodoroState {
            message: row.message.clone(),
            started_at_unix_ms: row.started_at_unix_ms,
            ends_at_unix_ms: row.ends_at_unix_ms,
        };
        if pomodoro.is_expired(now_unix_ms()) {
            row.delete().exec(&mut database).await?;
            return Ok(None);
        }
        Ok(Some(pomodoro))
    }

    pub async fn save_persisted_pomodoro(&self, pomodoro: &PomodoroState) -> Result<()> {
        let mut database = self.database.clone();
        if let Some(mut row) = StoredPomodoro::filter(StoredPomodoro::fields().id().eq(1_u64))
            .first()
            .exec(&mut database)
            .await?
        {
            row.update()
                .message(pomodoro.message.clone())
                .started_at_unix_ms(pomodoro.started_at_unix_ms)
                .ends_at_unix_ms(pomodoro.ends_at_unix_ms)
                .exec(&mut database)
                .await?;
        } else {
            toasty::create!(StoredPomodoro {
                id: 1,
                message: pomodoro.message.clone(),
                started_at_unix_ms: pomodoro.started_at_unix_ms,
                ends_at_unix_ms: pomodoro.ends_at_unix_ms,
            })
            .exec(&mut database)
            .await?;
        }
        Ok(())
    }

    pub async fn clear_persisted_pomodoro(&self) -> Result<()> {
        let mut database = self.database.clone();
        if let Some(row) = StoredPomodoro::filter(StoredPomodoro::fields().id().eq(1_u64))
            .first()
            .exec(&mut database)
            .await?
        {
            row.delete().exec(&mut database).await?;
        }
        Ok(())
    }

    async fn trim_to_capacity(&mut self) -> Result<()> {
        let mut database = self.database.clone();
        let mut transaction = database.transaction().await?;
        let message_count = count_messages(&mut transaction).await?;
        let excess = message_count.saturating_sub(self.capacity);
        delete_oldest_waiting_messages(&mut transaction, excess).await?;
        if excess > 0 {
            increment_dropped(&mut transaction, excess).await?;
        }
        trim_seen(
            &mut transaction,
            self.capacity.saturating_mul(SEEN_ID_RETENTION_MULTIPLIER),
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}

fn chat_message_from_model(message: &StoredChatMessage) -> Result<ChatMessage> {
    let parts = message
        .emoji_data
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .with_context(|| format!("Stored chat message {} has invalid emoji data", message.id))?
        .unwrap_or_else(|| vec![ChatMessagePart::Text(message.text.clone())]);
    Ok(ChatMessage {
        id: message.id,
        source: message
            .source
            .parse()
            .with_context(|| format!("Stored chat message {} has an invalid source", message.id))?,
        external_id: message.external_id.clone(),
        author: message.author.clone(),
        text: message.text.clone(),
        parts,
        avatar_url: message.avatar_url.clone(),
        sent_at: message.sent_at.clone(),
        received_at_unix_ms: message.received_at_unix_ms,
    })
}

/// The first message is currently on-air. Eviction therefore starts at the
/// oldest waiting message and never removes the on-air item.
async fn remove_oldest_waiting(
    messages: &mut Vec<StoredChatMessage>,
    executor: &mut dyn Executor,
) -> Result<()> {
    anyhow::ensure!(messages.len() > 1, "Cannot evict a waiting chat message");
    messages.remove(1).delete().exec(executor).await?;
    Ok(())
}

async fn ordered_messages(
    executor: &mut dyn Executor,
    limit: usize,
) -> Result<Vec<StoredChatMessage>> {
    Ok(StoredChatMessage::all()
        .order_by(StoredChatMessage::fields().id().asc())
        .limit(limit)
        .exec(executor)
        .await?)
}

async fn count_messages(executor: &mut dyn Executor) -> Result<usize> {
    let count = StoredChatMessage::all().count().exec(executor).await?;
    usize::try_from(count).context("Chat message count exceeds platform limits")
}

async fn delete_oldest_waiting_messages(executor: &mut dyn Executor, amount: usize) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }

    let current = ordered_messages(executor, 1)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Cannot evict from an empty chat inbox"))?;
    let boundary = StoredChatMessage::all()
        .order_by(StoredChatMessage::fields().id().asc())
        .limit(1)
        .offset(amount + 1)
        .exec(executor)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Chat inbox eviction boundary is missing"))?;

    toasty::sql::statement("DELETE FROM chat_messages WHERE id > ?1 AND id < ?2")
        .bind(current.id)
        .bind(boundary.id)
        .exec(executor)
        .await?;
    Ok(())
}

async fn increment_dropped(executor: &mut dyn Executor, amount: usize) -> Result<()> {
    let mut state = load_state(executor).await?;
    let dropped = state.dropped.saturating_add(amount as u64);
    state.update().dropped(dropped).exec(executor).await?;
    Ok(())
}

async fn trim_seen(executor: &mut dyn Executor, capacity: usize) -> Result<()> {
    let Some(boundary) = StoredChatSeen::all()
        .order_by(StoredChatSeen::fields().id().asc())
        .limit(1)
        .offset(capacity)
        .exec(executor)
        .await?
        .into_iter()
        .next()
    else {
        return Ok(());
    };

    toasty::sql::statement("DELETE FROM chat_seen WHERE id < ?1")
        .bind(boundary.id)
        .exec(executor)
        .await?;
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
        respond_to: oneshot::Sender<Result<()>>,
    },
    UpdateYouTubeStatus {
        state: YouTubeIngestState,
        detail: String,
        last_success_at_unix_ms: Option<u64>,
        newly_received: Option<u64>,
    },
    StartPomodoro {
        message: String,
        duration_secs: u64,
        respond_to: oneshot::Sender<Result<ChatInboxSnapshot>>,
    },
    StopPomodoro {
        respond_to: oneshot::Sender<Result<ChatInboxSnapshot>>,
    },
    ExpirePomodoro,
}

/// Actor owning chat queue storage and background platform workers exclusively without mutexes.
struct ChatActor {
    inbox: ChatInbox,
    twitch_task: Option<JoinHandle<()>>,
    youtube_task: Option<JoinHandle<()>>,
    pomodoro_timer: Option<JoinHandle<()>>,
    state: ChatState,
    state_tx: watch::Sender<ChatState>,
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
                        handle.metrics.add_chat_messages_received(1);
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
                    let response = self.inbox.snapshot().await.map(|mut snapshot| {
                        snapshot.pomodoro = self.active_pomodoro();
                        snapshot
                    });
                    let _ = respond_to.send(response);
                }
                ChatCommand::ApplyConfig { config, respond_to } => {
                    let res = self.handle_apply_config(&config, &handle).await;
                    let _ = respond_to.send(res);
                }
                ChatCommand::SetYouTubePolling { config, respond_to } => {
                    self.configure_youtube(&config, &handle).await;
                    let _ = respond_to.send(Ok(()));
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
                        .state
                        .youtube_status
                        .get_or_insert_with(YouTubeIngestStatus::default);
                    status.state = state;
                    status.detail = detail.chars().take(STATUS_DETAIL_LIMIT).collect();
                    if let Some(last_success_at_unix_ms) = last_success_at_unix_ms {
                        status.last_success_at_unix_ms = Some(last_success_at_unix_ms);
                    }
                    if let Some(newly_received) = newly_received {
                        status.messages_received =
                            status.messages_received.saturating_add(newly_received);
                    }
                    self.state_tx.send_replace(self.state.clone());
                }
                ChatCommand::StartPomodoro {
                    message,
                    duration_secs,
                    respond_to,
                } => {
                    let response = self
                        .start_pomodoro(&handle, message, duration_secs)
                        .await;
                    let _ = respond_to.send(response);
                }
                ChatCommand::StopPomodoro { respond_to } => {
                    let response = self.stop_pomodoro().await;
                    let _ = respond_to.send(response);
                }
                ChatCommand::ExpirePomodoro => {
                    if self.active_pomodoro().is_some()
                        && let Err(error) = self.clear_pomodoro().await
                    {
                        tracing::warn!("Failed to clear expired pomodoro: {error:#}");
                    }
                }
            }
        }
    }

    fn notify_changed(&mut self) {
        self.state.revision = self.state.revision.wrapping_add(1);
        self.state_tx.send_replace(self.state.clone());
    }

    fn active_pomodoro(&self) -> Option<PomodoroState> {
        let pomodoro = self.state.pomodoro.clone()?;
        if pomodoro.is_expired(now_unix_ms()) {
            None
        } else {
            Some(pomodoro)
        }
    }

    async fn clear_pomodoro(&mut self) -> Result<()> {
        if let Some(timer) = self.pomodoro_timer.take() {
            timer.abort();
        }
        self.inbox.clear_persisted_pomodoro().await?;
        if self.state.pomodoro.take().is_some() {
            self.notify_changed();
        }
        Ok(())
    }

    async fn snapshot_with_pomodoro(&self) -> Result<ChatInboxSnapshot> {
        let mut snapshot = self.inbox.snapshot().await?;
        snapshot.pomodoro = self.active_pomodoro();
        Ok(snapshot)
    }

    async fn start_pomodoro(
        &mut self,
        handle: &ChatHandle,
        message: String,
        duration_secs: u64,
    ) -> Result<ChatInboxSnapshot> {
        let (message, duration_secs) = validate_pomodoro(&message, duration_secs)?;
        let now = now_unix_ms();
        let pomodoro = PomodoroState {
            message,
            started_at_unix_ms: now,
            ends_at_unix_ms: now.saturating_add(duration_secs.saturating_mul(1000)),
        };
        // Durable before visible: a restart between state-set and save would
        // otherwise silently drop focus mode.
        self.inbox.save_persisted_pomodoro(&pomodoro).await?;
        if let Some(timer) = self.pomodoro_timer.take() {
            timer.abort();
        }
        self.state.pomodoro = Some(pomodoro);
        self.notify_changed();

        let sender = handle.sender.clone();
        let timer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(duration_secs)).await;
            let _ = sender.send(ChatCommand::ExpirePomodoro).await;
        });
        self.pomodoro_timer = Some(timer);

        self.snapshot_with_pomodoro().await
    }

    async fn stop_pomodoro(&mut self) -> Result<ChatInboxSnapshot> {
        self.clear_pomodoro().await?;
        self.snapshot_with_pomodoro().await
    }

    async fn handle_apply_config(
        &mut self,
        chat: &ChatSettings,
        handle: &ChatHandle,
    ) -> Result<()> {
        self.inbox.resize(chat.queue_capacity).await?;

        if let Some(task) = self.twitch_task.take() {
            task.abort();
            let _ = task.await;
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

        self.configure_youtube(chat, handle).await;
        Ok(())
    }

    async fn configure_youtube(&mut self, chat: &ChatSettings, handle: &ChatHandle) {
        if let Some(task) = self.youtube_task.take() {
            task.abort();
            let _ = task.await;
        }
        self.state.youtube_status = None;
        self.state_tx.send_replace(self.state.clone());

        let target = resolve_youtube_target(chat);

        let Some(target) = target else {
            return;
        };

        if !chat.youtube_polling_enabled {
            let status = YouTubeIngestStatus {
                state: YouTubeIngestState::Off,
                detail: "Polling is off. Turn it on when the YouTube stream is live.".into(),
                ..YouTubeIngestStatus::default()
            };
            self.state.youtube_status = Some(status);
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

fn resolve_youtube_target(chat: &ChatSettings) -> Option<YouTubeChatTarget> {
    chat.youtube_live_chat_id
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
        })
}

/// Lightweight, cloneable handle to the ChatActor for lock-free reads and async command dispatch.
#[derive(Clone)]
pub struct ChatHandle {
    sender: mpsc::Sender<ChatCommand>,
    state_rx: watch::Receiver<ChatState>,
    metrics: Arc<Metrics>,
    test_message_sequence: Arc<AtomicU64>,
}

impl ChatHandle {
    pub async fn spawn(
        path: &Path,
        capacity: usize,
        http_client: Client,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        let inbox = ChatInbox::open(path, capacity).await?;
        let (state_tx, state_rx) = watch::channel(ChatState::default());
        let (sender, receiver) = mpsc::channel(ACTOR_COMMAND_CAPACITY);

        let handle = Self {
            sender,
            state_rx,
            metrics,
            test_message_sequence: Arc::new(AtomicU64::new(1)),
        };

        // Rehydrate focus mode persisted before a restart, if it has not
        // expired while the process was down, and re-arm its expiry timer.
        let mut state = ChatState::default();
        let mut pomodoro_timer = None;
        if let Some(pomodoro) = inbox.load_persisted_pomodoro().await? {
            let remaining_ms = pomodoro.ends_at_unix_ms.saturating_sub(now_unix_ms());
            if remaining_ms > 0 {
                state.pomodoro = Some(pomodoro);
                let timer_sender = handle.sender.clone();
                pomodoro_timer = Some(tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(remaining_ms)).await;
                    let _ = timer_sender.send(ChatCommand::ExpirePomodoro).await;
                }));
                tracing::info!("Restored active pomodoro focus mode after restart");
            }
        }
        state_tx.send_replace(state.clone());

        let actor = ChatActor {
            inbox,
            twitch_task: None,
            youtube_task: None,
            pomodoro_timer,
            state,
            state_tx,
            http_client,
        };

        let handle_for_actor = handle.clone();
        tokio::spawn(async move {
            actor.run(receiver, handle_for_actor).await;
        });

        Ok(handle)
    }

    pub async fn enqueue(&self, message: IncomingChatMessage) -> Result<EnqueueOutcome> {
        self.call(
            |respond_to| ChatCommand::Enqueue {
                message,
                respond_to,
            },
            "Chat actor dropped enqueue response",
        )
        .await
    }

    pub async fn enqueue_test(
        &self,
        request: Option<TestChatMessageRequest>,
    ) -> Result<EnqueueOutcome> {
        let seq = self.test_message_sequence.fetch_add(1, Ordering::Relaxed);
        let sample = SAMPLE_TEST_MESSAGES[(seq as usize) % SAMPLE_TEST_MESSAGES.len()];
        let req = request.unwrap_or_default();

        let source = req.source.unwrap_or(sample.0);

        let author = req
            .author
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| sample.1.to_string());

        let text = req
            .text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| sample.2.to_string());

        let message = IncomingChatMessage {
            source,
            external_id: format!("test-{}-{}", now_unix_ms(), seq),
            author,
            text,
            parts: Vec::new(),
            avatar_url: req.avatar_url,
            sent_at: None,
        };

        self.enqueue(message).await
    }

    pub async fn acknowledge(&self, expected_id: u64) -> Result<bool> {
        self.call(
            |respond_to| ChatCommand::Acknowledge {
                expected_id,
                respond_to,
            },
            "Chat actor dropped acknowledge response",
        )
        .await
    }

    pub async fn snapshot(&self) -> Result<ChatInboxSnapshot> {
        self.call(
            |respond_to| ChatCommand::Snapshot { respond_to },
            "Chat actor dropped snapshot response",
        )
        .await
    }

    pub async fn apply_config(&self, config: ChatSettings) -> Result<()> {
        self.call(
            |respond_to| ChatCommand::ApplyConfig { config, respond_to },
            "Chat actor dropped apply_config response",
        )
        .await
    }

    pub async fn set_youtube_polling(&self, config: ChatSettings) -> Result<()> {
        self.call(
            |respond_to| ChatCommand::SetYouTubePolling { config, respond_to },
            "Chat actor dropped set_youtube_polling response",
        )
        .await
    }

    pub async fn start_pomodoro(
        &self,
        message: String,
        duration_secs: u64,
    ) -> Result<ChatInboxSnapshot> {
        self.call(
            |respond_to| ChatCommand::StartPomodoro {
                message,
                duration_secs,
                respond_to,
            },
            "Chat actor dropped start_pomodoro response",
        )
        .await
    }

    pub async fn stop_pomodoro(&self) -> Result<ChatInboxSnapshot> {
        self.call(
            |respond_to| ChatCommand::StopPomodoro { respond_to },
            "Chat actor dropped stop_pomodoro response",
        )
        .await
    }

    async fn call<T>(
        &self,
        command: impl FnOnce(oneshot::Sender<Result<T>>) -> ChatCommand,
        response_context: &'static str,
    ) -> Result<T> {
        let (respond_to, response) = oneshot::channel();
        self.sender
            .send(command(respond_to))
            .await
            .map_err(|_| anyhow::anyhow!("Chat actor stopped"))?;
        response.await.context(response_context)?
    }

    pub fn update_youtube_status(
        &self,
        state: YouTubeIngestState,
        detail: impl Into<String>,
        last_success_at_unix_ms: Option<u64>,
        newly_received: Option<u64>,
    ) {
        let _ = self.sender.try_send(ChatCommand::UpdateYouTubeStatus {
            state,
            detail: detail.into(),
            last_success_at_unix_ms,
            newly_received,
        });
    }

    pub fn subscribe_changes(&self) -> watch::Receiver<ChatState> {
        self.state_rx.clone()
    }
}

impl YouTubeChatSink for ChatHandle {
    fn enqueue(
        &self,
        message: IncomingChatMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<EnqueueOutcome>> + Send + '_>>
    {
        Box::pin(self.enqueue(message))
    }

    fn update_youtube_status(
        &self,
        state: YouTubeIngestState,
        detail: String,
        last_success_at_unix_ms: Option<u64>,
        newly_received: Option<u64>,
    ) {
        self.update_youtube_status(state, detail, last_success_at_unix_ms, newly_received);
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
            source: source.parse().unwrap(),
            external_id: external_id.into(),
            author: "Viewer".into(),
            text: text.into(),
            parts: Vec::new(),
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
    async fn sqlite_queue_persists_youtube_emoji_parts() {
        let path = database_path();
        {
            let mut inbox = ChatInbox::open(&path, 3).await.unwrap();
            let mut message = message("youtube", "emoji", "Hello customEmoji");
            message.parts = vec![
                ChatMessagePart::Text("Hello ".into()),
                ChatMessagePart::Emoji {
                    alt: "customEmoji".into(),
                    url: "https://example.com/custom-emoji.png".into(),
                },
            ];
            inbox.enqueue(message).await.unwrap();
        }
        {
            let inbox = ChatInbox::open(&path, 3).await.unwrap();
            assert_eq!(
                inbox.snapshot().await.unwrap().messages[0].parts,
                vec![
                    ChatMessagePart::Text("Hello ".into()),
                    ChatMessagePart::Emoji {
                        alt: "customEmoji".into(),
                        url: "https://example.com/custom-emoji.png".into(),
                    },
                ]
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
        let handle = ChatHandle::spawn(&path, 5, Client::new(), Arc::new(Metrics::default()))
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

        handle.update_youtube_status(
            YouTubeIngestState::Polling,
            "stale update",
            Some(12345),
            Some(2),
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(rev_rx.borrow().youtube_status.is_none());

        let settings = ChatSettings {
            youtube_api_key: Some("api-key".into()),
            youtube_live_chat_id: Some("live-chat-id".into()),
            youtube_polling_enabled: false,
            ..ChatSettings::default()
        };
        handle.set_youtube_polling(settings).await.unwrap();
        let status = rev_rx.borrow().youtube_status.clone().unwrap();
        assert_eq!(status.state, YouTubeIngestState::Off);

        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn youtube_control_does_not_restart_twitch() {
        let actor_path = database_path();
        let handle_path = database_path();
        let inbox = ChatInbox::open(&actor_path, 5).await.unwrap();
        let (state_tx, _) = watch::channel(ChatState::default());
        let handle =
            ChatHandle::spawn(&handle_path, 5, Client::new(), Arc::new(Metrics::default()))
                .await
                .unwrap();
        let mut actor = ChatActor {
            inbox,
            twitch_task: None,
            youtube_task: None,
            pomodoro_timer: None,
            state: ChatState::default(),
            state_tx,
            http_client: Client::new(),
        };

        let twitch_task = tokio::spawn(std::future::pending());
        let twitch_abort = twitch_task.abort_handle();
        actor.twitch_task = Some(twitch_task);
        let youtube_task = tokio::spawn(std::future::pending());
        let youtube_abort = youtube_task.abort_handle();
        actor.youtube_task = Some(youtube_task);
        actor
            .configure_youtube(&ChatSettings::default(), &handle)
            .await;
        tokio::task::yield_now().await;
        assert!(!twitch_abort.is_finished());
        assert!(youtube_abort.is_finished());

        drop(actor);
        std::fs::remove_file(actor_path).unwrap();
        std::fs::remove_file(handle_path).unwrap();
    }

    #[tokio::test]
    async fn enqueue_test_generates_valid_messages_and_can_be_acknowledged() {
        let path = database_path();
        let handle = ChatHandle::spawn(&path, 5, Client::new(), Arc::new(Metrics::default()))
            .await
            .unwrap();

        let outcome = handle.enqueue_test(None).await.unwrap();
        assert_eq!(outcome, EnqueueOutcome::Accepted);

        let snapshot = handle.snapshot().await.unwrap();
        assert_eq!(snapshot.messages.len(), 1);
        let first_id = snapshot.messages[0].id;
        assert!(handle.acknowledge(first_id).await.unwrap());

        let snapshot_after = handle.snapshot().await.unwrap();
        assert_eq!(snapshot_after.messages.len(), 0);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn pomodoro_validation_rejects_bad_durations_and_long_messages() {
        assert!(validate_pomodoro("focus", 30).is_err());
        assert!(validate_pomodoro("focus", POMODORO_MAX_DURATION_SECS + 1).is_err());
        assert!(validate_pomodoro("x".repeat(POMODORO_MAX_MESSAGE_CHARS + 1).as_str(), 600).is_err());
        let (message, secs) = validate_pomodoro("  deep work  ", 25 * 60).unwrap();
        assert_eq!((message.as_str(), secs), ("deep work", 1500));
        let (defaulted, _) = validate_pomodoro("   ", 25 * 60).unwrap();
        assert_eq!(defaulted, POMODORO_DEFAULT_MESSAGE);
    }

    #[tokio::test]
    async fn pomodoro_start_and_stop_flow_through_handle() {
        let path = database_path();
        let handle = ChatHandle::spawn(&path, 5, Client::new(), Arc::new(Metrics::default()))
            .await
            .unwrap();

        assert!(handle.snapshot().await.unwrap().pomodoro.is_none());
        let snapshot = handle
            .start_pomodoro("deep work".into(), 25 * 60)
            .await
            .unwrap();
        let pomodoro = snapshot.pomodoro.clone().unwrap();
        assert_eq!(pomodoro.message, "deep work");
        assert!(pomodoro.ends_at_unix_ms > pomodoro.started_at_unix_ms);

        let via_snapshot = handle.snapshot().await.unwrap();
        assert_eq!(via_snapshot.pomodoro, snapshot.pomodoro);

        let stopped = handle.stop_pomodoro().await.unwrap();
        assert!(stopped.pomodoro.is_none());
        assert!(handle.snapshot().await.unwrap().pomodoro.is_none());

        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn pomodoro_survives_respawn_until_stopped() {
        let path = database_path();
        let handle = ChatHandle::spawn(&path, 5, Client::new(), Arc::new(Metrics::default()))
            .await
            .unwrap();
        let started = handle
            .start_pomodoro("restart-proof".into(), 25 * 60)
            .await
            .unwrap();
        assert!(started.pomodoro.is_some());

        // Simulate a process restart: a fresh handle on the same database
        // rehydrates the still-active pomodoro.
        let restarted = ChatHandle::spawn(&path, 5, Client::new(), Arc::new(Metrics::default()))
            .await
            .unwrap();
        let pomodoro = restarted.snapshot().await.unwrap().pomodoro.unwrap();
        assert_eq!(pomodoro.message, "restart-proof");
        assert_eq!(
            pomodoro.ends_at_unix_ms,
            started.pomodoro.as_ref().unwrap().ends_at_unix_ms
        );

        restarted.stop_pomodoro().await.unwrap();
        let after_stop = ChatHandle::spawn(&path, 5, Client::new(), Arc::new(Metrics::default()))
            .await
            .unwrap();
        assert!(after_stop.snapshot().await.unwrap().pomodoro.is_none());

        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn expired_pomodoro_is_purged_on_startup() {
        let path = database_path();
        let inbox = ChatInbox::open(&path, 5).await.unwrap();
        inbox
            .save_persisted_pomodoro(&PomodoroState {
                message: "stale".into(),
                started_at_unix_ms: 1,
                ends_at_unix_ms: 2,
            })
            .await
            .unwrap();
        drop(inbox);

        let handle = ChatHandle::spawn(&path, 5, Client::new(), Arc::new(Metrics::default()))
            .await
            .unwrap();
        assert!(handle.snapshot().await.unwrap().pomodoro.is_none());
        let reopened = ChatInbox::open(&path, 5).await.unwrap();
        assert!(reopened.load_persisted_pomodoro().await.unwrap().is_none());

        std::fs::remove_file(path).unwrap();
    }
}
