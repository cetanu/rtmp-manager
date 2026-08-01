use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub mod twitch;
pub mod youtube;

#[derive(Debug, Clone, Deserialize)]
pub struct IncomingChatMessage {
    pub source: String,
    pub external_id: String,
    pub author: String,
    pub text: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
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

        if self.source.is_empty()
            || self.source.len() > 32
            || !self.source.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            bail!("source must be 1-32 ASCII letters, numbers, hyphens, or underscores");
        }
        if self.external_id.is_empty() || self.external_id.len() > 256 {
            bail!("external_id must be 1-256 characters");
        }
        if self.author.is_empty() || self.author.len() > 200 {
            bail!("author must be 1-200 characters");
        }
        if self.text.is_empty() || self.text.len() > 5000 {
            bail!("text must be 1-5000 characters");
        }
        if self.avatar_url.as_ref().is_some_and(|url| url.len() > 2048) {
            bail!("avatar_url must be at most 2048 characters");
        }
        if self
            .sent_at
            .as_ref()
            .is_some_and(|timestamp| timestamp.len() > 100)
        {
            bail!("sent_at must be at most 100 characters");
        }

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

#[derive(Debug)]
pub struct ChatInbox {
    capacity: usize,
    connection: Connection,
}

impl ChatInbox {
    pub fn open(path: &Path, capacity: usize) -> Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("Failed to open chat inbox database '{}'", path.display()))?;
        Self::from_connection(connection, capacity)
    }

    fn from_connection(connection: Connection, capacity: usize) -> Result<Self> {
        assert!(capacity > 0, "chat queue capacity must be positive");
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source TEXT NOT NULL,
                external_id TEXT NOT NULL,
                author TEXT NOT NULL,
                text TEXT NOT NULL,
                avatar_url TEXT,
                sent_at TEXT,
                received_at_unix_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chat_seen (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source TEXT NOT NULL,
                external_id TEXT NOT NULL,
                UNIQUE(source, external_id)
            );
            CREATE TABLE IF NOT EXISTS chat_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                dropped INTEGER NOT NULL DEFAULT 0
            );
            INSERT OR IGNORE INTO chat_state (id, dropped) VALUES (1, 0);
            ",
        )?;

        let mut inbox = Self {
            capacity,
            connection,
        };
        inbox.trim_to_capacity()?;
        Ok(inbox)
    }

    pub fn enqueue(&mut self, incoming: IncomingChatMessage) -> Result<EnqueueOutcome> {
        let incoming = incoming.normalized()?;
        let transaction = self.connection.transaction()?;

        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO chat_seen (source, external_id) VALUES (?1, ?2)",
            params![incoming.source, incoming.external_id],
        )?;
        if inserted == 0 {
            return Ok(EnqueueOutcome::Duplicate);
        }
        trim_seen(&transaction, self.capacity.saturating_mul(4))?;

        let message_count = message_count(&transaction)?;
        if message_count >= self.capacity {
            increment_dropped(&transaction, 1)?;
            if self.capacity == 1 {
                transaction.commit()?;
                return Ok(EnqueueOutcome::Dropped);
            }
            transaction.execute(
                "DELETE FROM chat_messages
                 WHERE id = (
                    SELECT id FROM chat_messages ORDER BY id LIMIT 1 OFFSET 1
                 )",
                [],
            )?;
        }

        transaction.execute(
            "INSERT INTO chat_messages (
                source, external_id, author, text, avatar_url, sent_at,
                received_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                incoming.source,
                incoming.external_id,
                incoming.author,
                incoming.text,
                incoming.avatar_url,
                incoming.sent_at,
                now_unix_ms() as i64,
            ],
        )?;
        transaction.commit()?;
        Ok(EnqueueOutcome::Accepted)
    }

    pub fn acknowledge(&mut self, expected_id: u64) -> Result<bool> {
        let affected = self.connection.execute(
            "DELETE FROM chat_messages
             WHERE id = ?1
               AND id = (SELECT MIN(id) FROM chat_messages)",
            [expected_id as i64],
        )?;
        Ok(affected == 1)
    }

    pub fn snapshot(&self) -> Result<ChatInboxSnapshot> {
        let current = self
            .connection
            .query_row(
                "SELECT id, source, external_id, author, text, avatar_url, sent_at,
                        received_at_unix_ms
                 FROM chat_messages
                 ORDER BY id
                 LIMIT 1",
                [],
                chat_message_from_row,
            )
            .optional()?;
        let count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM chat_messages", [], |row| row.get(0))?;
        let dropped: i64 = self.connection.query_row(
            "SELECT dropped FROM chat_state WHERE id = 1",
            [],
            |row| row.get(0),
        )?;

        Ok(ChatInboxSnapshot {
            current,
            waiting: (count as usize).saturating_sub(1),
            dropped: dropped as u64,
        })
    }

    pub fn resize(&mut self, capacity: usize) -> Result<()> {
        assert!(capacity > 0, "chat queue capacity must be positive");
        self.capacity = capacity;
        self.trim_to_capacity()
    }

    fn trim_to_capacity(&mut self) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let count = message_count(&transaction)?;
        let excess = count.saturating_sub(self.capacity);
        for _ in 0..excess {
            transaction.execute(
                "DELETE FROM chat_messages
                 WHERE id = (
                    SELECT id FROM chat_messages ORDER BY id LIMIT 1 OFFSET ?1
                 )",
                [1_i64],
            )?;
        }
        if excess > 0 {
            increment_dropped(&transaction, excess)?;
        }
        trim_seen(&transaction, self.capacity.saturating_mul(4))?;
        transaction.commit()?;
        Ok(())
    }
}

fn chat_message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatMessage> {
    Ok(ChatMessage {
        id: row.get::<_, i64>(0)? as u64,
        source: row.get(1)?,
        external_id: row.get(2)?,
        author: row.get(3)?,
        text: row.get(4)?,
        avatar_url: row.get(5)?,
        sent_at: row.get(6)?,
        received_at_unix_ms: row.get::<_, i64>(7)? as u64,
    })
}

fn message_count(transaction: &Transaction<'_>) -> rusqlite::Result<usize> {
    let count: i64 =
        transaction.query_row("SELECT COUNT(*) FROM chat_messages", [], |row| row.get(0))?;
    Ok(count as usize)
}

fn increment_dropped(transaction: &Transaction<'_>, amount: usize) -> rusqlite::Result<()> {
    transaction.execute(
        "UPDATE chat_state SET dropped = dropped + ?1 WHERE id = 1",
        [amount as i64],
    )?;
    Ok(())
}

fn trim_seen(transaction: &Transaction<'_>, capacity: usize) -> rusqlite::Result<()> {
    transaction.execute(
        "DELETE FROM chat_seen
         WHERE id IN (
            SELECT id FROM chat_seen ORDER BY id
            LIMIT MAX(0, (SELECT COUNT(*) FROM chat_seen) - ?1)
         )",
        [capacity as i64],
    )?;
    Ok(())
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

    fn inbox(capacity: usize) -> ChatInbox {
        ChatInbox::from_connection(Connection::open_in_memory().unwrap(), capacity).unwrap()
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

    #[test]
    fn acknowledge_advances_one_message_at_a_time() {
        let mut inbox = inbox(3);
        inbox.enqueue(message("twitch", "1", "first")).unwrap();
        inbox.enqueue(message("youtube", "2", "second")).unwrap();

        let first = inbox.snapshot().unwrap();
        assert_eq!(first.current.unwrap().text, "first");
        assert_eq!(first.waiting, 1);
        assert!(inbox.acknowledge(1).unwrap());

        let second = inbox.snapshot().unwrap();
        assert_eq!(second.current.unwrap().text, "second");
        assert_eq!(second.waiting, 0);
    }

    #[test]
    fn stale_acknowledgement_cannot_clear_a_newer_message() {
        let mut inbox = inbox(3);
        inbox.enqueue(message("twitch", "1", "first")).unwrap();
        inbox.enqueue(message("twitch", "2", "second")).unwrap();
        assert!(inbox.acknowledge(1).unwrap());
        assert!(!inbox.acknowledge(1).unwrap());
        assert_eq!(inbox.snapshot().unwrap().current.unwrap().text, "second");
    }

    #[test]
    fn duplicate_platform_message_is_ignored_after_acknowledgement() {
        let mut inbox = inbox(3);
        assert_eq!(
            inbox.enqueue(message("twitch", "same", "first")).unwrap(),
            EnqueueOutcome::Accepted
        );
        assert!(inbox.acknowledge(1).unwrap());
        assert_eq!(
            inbox
                .enqueue(message("twitch", "same", "duplicate"))
                .unwrap(),
            EnqueueOutcome::Duplicate
        );
        assert!(inbox.snapshot().unwrap().current.is_none());
    }

    #[test]
    fn full_queue_keeps_current_and_most_recent_waiting_messages() {
        let mut inbox = inbox(3);
        inbox.enqueue(message("twitch", "1", "current")).unwrap();
        inbox
            .enqueue(message("twitch", "2", "old waiting"))
            .unwrap();
        inbox
            .enqueue(message("youtube", "3", "newer waiting"))
            .unwrap();
        inbox.enqueue(message("x", "4", "newest waiting")).unwrap();

        assert_eq!(inbox.snapshot().unwrap().current.unwrap().text, "current");
        assert_eq!(inbox.snapshot().unwrap().waiting, 2);
        assert_eq!(inbox.snapshot().unwrap().dropped, 1);
        assert!(inbox.acknowledge(1).unwrap());
        assert_eq!(
            inbox.snapshot().unwrap().current.unwrap().text,
            "newer waiting"
        );
    }

    #[test]
    fn sqlite_queue_survives_reopening() {
        let path = database_path();
        {
            let mut inbox = ChatInbox::open(&path, 3).unwrap();
            inbox
                .enqueue(message("youtube", "persisted", "still here"))
                .unwrap();
        }
        {
            let inbox = ChatInbox::open(&path, 3).unwrap();
            assert_eq!(
                inbox.snapshot().unwrap().current.unwrap().text,
                "still here"
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn reducing_capacity_preserves_current_and_newest_waiting_messages() {
        let path = database_path();
        {
            let mut inbox = ChatInbox::open(&path, 4).unwrap();
            inbox.enqueue(message("twitch", "1", "current")).unwrap();
            inbox.enqueue(message("twitch", "2", "oldest")).unwrap();
            inbox.enqueue(message("twitch", "3", "newer")).unwrap();
            inbox.enqueue(message("twitch", "4", "newest")).unwrap();
        }
        {
            let mut inbox = ChatInbox::open(&path, 2).unwrap();
            let snapshot = inbox.snapshot().unwrap();
            assert_eq!(snapshot.current.unwrap().text, "current");
            assert_eq!(snapshot.waiting, 1);
            assert_eq!(snapshot.dropped, 2);
            assert!(inbox.acknowledge(1).unwrap());
            assert_eq!(inbox.snapshot().unwrap().current.unwrap().text, "newest");
        }
        std::fs::remove_file(path).unwrap();
    }
}
