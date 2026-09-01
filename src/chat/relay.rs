use crate::chat::{IncomingChatMessage, kick, twitch, youtube};
use crate::config::ChatSettings;
use anyhow::Result;
use reqwest::Client;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRule {
    pub source: String,
    pub destination: String,
    pub prefix: String,
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
