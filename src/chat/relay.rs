use crate::chat::{IncomingChatMessage, kick, twitch, youtube};
use crate::config::ChatSettings;
use anyhow::Result;
use reqwest::Client;
use std::collections::{HashMap, VecDeque};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

static RATE_LIMITS: LazyLock<Mutex<HashMap<String, VecDeque<Instant>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRule {
    pub tenant_id: String,
    pub source: String,
    pub destination: String,
    pub prefix: String,
}

async fn allow_delivery(tenant_id: &str, destination: &str) -> bool {
    let mut limits = RATE_LIMITS.lock().await;
    let window = limits
        .entry(format!("{tenant_id}:{destination}"))
        .or_default();
    let now = Instant::now();
    while window
        .front()
        .is_some_and(|timestamp| now.duration_since(*timestamp) >= Duration::from_secs(10))
    {
        window.pop_front();
    }
    if window.len() >= 5 {
        return false;
    }
    window.push_back(now);
    true
}

/// Applies a rule and dispatches to the selected authenticated bot adapter.
pub async fn dispatch(
    rule: &RelayRule,
    message: &IncomingChatMessage,
    settings: &ChatSettings,
    client: &Client,
) -> Result<()> {
    if message.source == rule.destination || message.source != rule.source {
        return Ok(());
    }
    if !allow_delivery(&rule.tenant_id, &rule.destination).await {
        anyhow::bail!("chat relay rate limit exceeded");
    }
    let text = format!(
        "{}{}: {}",
        rule.prefix,
        message.author,
        message.text.replace(['\r', '\n'], " ")
    );
    match rule.destination.as_str() {
        "twitch" => {
            twitch::send_message(
                settings.twitch_channel.as_deref().unwrap_or_default(),
                &text,
            )
            .await
        }
        "kick" => {
            kick::send_message(
                client,
                settings.kick_channel.as_deref().unwrap_or_default(),
                &text,
            )
            .await
        }
        "youtube" => {
            youtube::send_message(
                client,
                settings.youtube_live_chat_id.as_deref().unwrap_or_default(),
                &text,
            )
            .await
        }
        other => anyhow::bail!("no bot adapter configured for {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::allow_delivery;

    #[tokio::test]
    async fn limits_burst_delivery_per_destination() {
        let destination = "test-rate-limit-destination";
        for _ in 0..5 {
            assert!(allow_delivery("tenant-a", destination).await);
        }
        assert!(!allow_delivery("tenant-a", destination).await);
        assert!(allow_delivery("tenant-b", destination).await);
    }
}
