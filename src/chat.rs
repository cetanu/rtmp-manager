use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use toasty::Executor;

pub mod twitch;
pub mod youtube;

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
    fn normalized(mut self) -> Result<Self> {
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

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
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
    pub current: Option<ChatMessage>,
    pub waiting: usize,
    pub dropped: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    Accepted,
    Duplicate,
    Dropped,
}

#[derive(Clone)]
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
        let current = messages.first().map(chat_message_from_model).transpose()?;
        let dropped = load_state(&mut database).await?.dropped;

        Ok(ChatInboxSnapshot {
            current,
            waiting: messages.len().saturating_sub(1),
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

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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
            "rtmp-proxy-chat-test-{}-{}.sqlite3",
            std::process::id(),
            TEST_DATABASE_ID.fetch_add(1, Ordering::Relaxed)
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
        assert_eq!(first.current.unwrap().text, "first");
        assert_eq!(first.waiting, 1);
        assert!(inbox.acknowledge(1).await.unwrap());

        let second = inbox.snapshot().await.unwrap();
        assert_eq!(second.current.unwrap().text, "second");
        assert_eq!(second.waiting, 0);
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
        assert_eq!(
            inbox.snapshot().await.unwrap().current.unwrap().text,
            "second"
        );
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
        assert!(inbox.snapshot().await.unwrap().current.is_none());
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

        assert_eq!(
            inbox.snapshot().await.unwrap().current.unwrap().text,
            "current"
        );
        assert_eq!(inbox.snapshot().await.unwrap().waiting, 2);
        assert_eq!(inbox.snapshot().await.unwrap().dropped, 1);
        assert!(inbox.acknowledge(1).await.unwrap());
        assert_eq!(
            inbox.snapshot().await.unwrap().current.unwrap().text,
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
                inbox.snapshot().await.unwrap().current.unwrap().text,
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
            assert_eq!(snapshot.current.unwrap().text, "current");
            assert_eq!(snapshot.waiting, 1);
            assert_eq!(snapshot.dropped, 2);
            assert!(inbox.acknowledge(1).await.unwrap());
            assert_eq!(
                inbox.snapshot().await.unwrap().current.unwrap().text,
                "newest"
            );
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
        assert_eq!(
            inbox.snapshot().await.unwrap().current.unwrap().text,
            "works"
        );
        std::fs::remove_file(path).unwrap();
    }
}
