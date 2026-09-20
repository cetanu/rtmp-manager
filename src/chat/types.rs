use serde::Serialize;
use std::time::Duration;

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

impl YouTubeIngestState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Resolving => "resolving",
            Self::Connected => "connected",
            Self::Polling => "polling",
            Self::Error => "error",
        }
    }
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
}
