use crate::chat::IncomingChatMessage;
use crate::server::state::WebhookEvent;
use anyhow::{Context, Result, bail};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
struct EventSubEnvelope {
    subscription: Subscription,
    event: Option<ChatEvent>,
    challenge: Option<String>,
}
#[derive(Debug, Deserialize)]
struct Subscription {
    #[serde(rename = "type")]
    kind: String,
}
#[derive(Debug, Deserialize)]
struct ChatEvent {
    #[serde(default)]
    message_id: String,
    #[serde(default)]
    chatter_user_name: String,
    #[serde(default)]
    message: Message,
}
#[derive(Debug, Default, Deserialize)]
struct Message {
    #[serde(default)]
    text: String,
}

pub fn verify(event: &WebhookEvent, secret: &str) -> Result<()> {
    let message_id = event
        .header("twitch-eventsub-message-id")
        .context("Missing Twitch EventSub message ID")?;
    let timestamp = event
        .header("twitch-eventsub-message-timestamp")
        .context("Missing Twitch EventSub timestamp")?;
    let signature = event
        .header("twitch-eventsub-message-signature")
        .context("Missing Twitch EventSub signature")?;
    let provided = signature
        .strip_prefix("sha256=")
        .context("Invalid Twitch EventSub signature")?;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts all key sizes");
    mac.update(message_id.as_bytes());
    mac.update(timestamp.as_bytes());
    mac.update(&event.body);
    let expected = hex::encode(mac.finalize().into_bytes());
    if subtle::ConstantTimeEq::ct_eq(expected.as_bytes(), provided.as_bytes()).unwrap_u8() == 0 {
        bail!("Twitch EventSub signature verification failed");
    }
    Ok(())
}

pub fn parse(
    event: &WebhookEvent,
    secret: &str,
) -> Result<(Option<String>, Option<IncomingChatMessage>)> {
    verify(event, secret)?;
    let envelope: EventSubEnvelope =
        serde_json::from_slice(&event.body).context("Invalid Twitch EventSub JSON")?;
    if envelope.subscription.kind == "channel.chat.message" {
        let Some(message) = envelope.event else {
            return Ok((None, None));
        };
        return Ok((
            None,
            Some(IncomingChatMessage {
                source: "twitch".into(),
                external_id: message.message_id,
                author: message.chatter_user_name,
                text: message.message.text,
                avatar_url: None,
                sent_at: None,
            }),
        ));
    }
    Ok((envelope.challenge, None))
}
