use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Twitch,
    YouTube,
    Kick,
    X,
}

impl Source {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Twitch => "twitch",
            Self::YouTube => "youtube",
            Self::Kick => "kick",
            Self::X => "x",
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for Source {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "twitch" => Ok(Self::Twitch),
            "youtube" => Ok(Self::YouTube),
            "kick" => Ok(Self::Kick),
            "x" => Ok(Self::X),
            value => anyhow::bail!("Unsupported chat source '{value}'"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct IncomingChatMessage {
    pub source: Source,
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
    pub parts: Vec<ChatMessagePart>,
    #[serde(default)]
    #[validate(max_length = 2048)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    #[validate(max_length = 100)]
    pub sent_at: Option<String>,
}

/// The ordered content of a chat message. Platform-specific images are kept
/// separate from the plain text so messages remain readable if an image fails
/// to load or a platform does not provide emote metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatMessagePart {
    Text(String),
    Emoji { alt: String, url: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    Accepted,
    Duplicate,
    Dropped,
}

pub trait YouTubeChatSink: Send + Sync {
    fn enqueue(
        &self,
        message: IncomingChatMessage,
    ) -> Pin<Box<dyn Future<Output = Result<EnqueueOutcome>> + Send + '_>>;

    fn update_youtube_status(
        &self,
        state: YouTubeIngestState,
        detail: String,
        last_success_at_unix_ms: Option<u64>,
        newly_received: Option<u64>,
    );
}

#[derive(Debug, Clone)]
pub struct YouTubeChatConfig {
    pub target: YouTubeChatTarget,
    pub min_poll_interval: Duration,
    pub adaptive_polling: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YouTubeChatTarget {
    LiveChat(String),
    Video(String),
    Channel(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum YouTubeIngestState {
    #[default]
    Off,
    Resolving,
    Connected,
    Polling,
    Error,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct YouTubeIngestStatus {
    pub state: YouTubeIngestState,
    pub detail: String,
    pub last_success_at_unix_ms: Option<u64>,
    pub messages_received: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub revision: u64,
    pub youtube_status: Option<YouTubeIngestStatus>,
    pub pomodoro: Option<PomodoroState>,
    pub poll: Option<PollState>,
}

/// Focus timer shown on the OBS overlay instead of chat messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroState {
    pub message: String,
    pub started_at_unix_ms: u64,
    pub ends_at_unix_ms: u64,
}

impl PomodoroState {
    pub fn remaining_secs(&self, now_unix_ms: u64) -> u64 {
        self.ends_at_unix_ms.saturating_sub(now_unix_ms) / 1000
    }

    pub fn is_expired(&self, now_unix_ms: u64) -> bool {
        now_unix_ms >= self.ends_at_unix_ms
    }

    /// Remaining time as `MM:SS` for countdown displays.
    pub fn remaining_mm_ss(&self, now_unix_ms: u64) -> String {
        let secs = self.remaining_secs(now_unix_ms);
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

/// A poll shown on the chat inbox and OBS overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollState {
    pub question: String,
    pub options: Vec<PollOption>,
    pub started_at_unix_ms: u64,
    pub stopped_at_unix_ms: Option<u64>,
    pub results_ends_at_unix_ms: Option<u64>,
}

impl PollState {
    pub fn is_active(&self) -> bool {
        self.stopped_at_unix_ms.is_none()
    }

    pub fn is_showing_results(&self, now_unix_ms: u64) -> bool {
        self.stopped_at_unix_ms.is_some()
            && self
                .results_ends_at_unix_ms
                .is_some_and(|ends_at| now_unix_ms < ends_at)
    }

    pub fn total_votes(&self) -> u64 {
        self.options.iter().map(|option| option.votes).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollOption {
    pub number: u8,
    pub label: String,
    pub votes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pomodoro_countdown_formats_mm_ss_and_clamps_at_zero() {
        let state = PomodoroState {
            message: "focus".into(),
            started_at_unix_ms: 1_000,
            ends_at_unix_ms: 1_000 + (25 * 60 + 7) * 1000,
        };
        assert_eq!(state.remaining_mm_ss(1_000), "25:07");
        assert_eq!(state.remaining_mm_ss(1_000 + 7 * 1000), "25:00");
        assert_eq!(state.remaining_mm_ss(1_000 + (24 * 60 + 7) * 1000), "01:00");
        assert_eq!(state.remaining_mm_ss(state.ends_at_unix_ms), "00:00");
        assert_eq!(
            state.remaining_mm_ss(state.ends_at_unix_ms + 60_000),
            "00:00"
        );
    }
}
