use crate::chat::{IncomingChatMessage, twitch};
use anyhow::Result;

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
    twitch_channel: &str,
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
        "twitch" => twitch::send_message(twitch_channel, &text).await,
        other => anyhow::bail!("no bot adapter configured for {other}"),
    }
}
