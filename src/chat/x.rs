use crate::chat::{ChatHandle, IncomingChatMessage};
use crate::config::{AppConfig, ChatSettings};
use crate::server::state::WebhookEvent;
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
struct XEvent {
    data: XEventData,
}

#[derive(Debug, Deserialize)]
struct XEventData {
    event_uuid: String,
    event_type: String,
    payload: XChatPayload,
}

#[derive(Debug, Deserialize)]
struct XChatPayload {
    message_id: String,
    message: String,
    author: XAuthor,
}

#[derive(Debug, Deserialize)]
struct XAuthor {
    data: XAuthorData,
}

#[derive(Debug, Deserialize)]
struct XAuthorData {
    username: String,
    profile_image_url: Option<String>,
}

pub fn response_token(crc_token: &str, consumer_secret: &str) -> Result<String> {
    let mut mac = HmacSha256::new_from_slice(consumer_secret.as_bytes())
        .context("X API secret key is invalid")?;
    mac.update(crc_token.as_bytes());
    Ok(format!(
        "sha256={}",
        STANDARD.encode(mac.finalize().into_bytes())
    ))
}

pub fn verify_webhook(body: &[u8], signature: &str, consumer_secret: &str) -> Result<()> {
    let encoded = signature
        .strip_prefix("sha256=")
        .context("X webhook signature must start with sha256=")?;
    let signature = STANDARD
        .decode(encoded)
        .context("X webhook signature is not valid base64")?;
    let mut mac = HmacSha256::new_from_slice(consumer_secret.as_bytes())
        .context("X API secret key is invalid")?;
    mac.update(body);
    mac.verify_slice(&signature)
        .map_err(|_| anyhow::anyhow!("X webhook signature verification failed"))
}

pub fn parse_chat_event(body: &[u8]) -> Result<Option<IncomingChatMessage>> {
    let event: XEvent = serde_json::from_slice(body).context("Invalid X activity event")?;
    if event.data.event_type != "broadcast.chat" {
        return Ok(None);
    }
    if event.data.event_uuid.trim().is_empty()
        || event.data.payload.message_id.trim().is_empty()
        || event.data.payload.message.trim().is_empty()
    {
        bail!("X broadcast.chat event is missing an event UUID, message ID, or message");
    }
    Ok(Some(IncomingChatMessage {
        source: "x".into(),
        external_id: event.data.payload.message_id,
        author: event.data.payload.author.data.username,
        text: event.data.payload.message,
        avatar_url: event.data.payload.author.data.profile_image_url,
        sent_at: None,
    }))
}

fn process_event(
    config: &ChatSettings,
    event: &WebhookEvent,
) -> Result<Option<IncomingChatMessage>> {
    if !config.x_webhook_enabled {
        return Ok(None);
    }
    let secret = config
        .x_api_secret
        .as_deref()
        .filter(|secret| !secret.is_empty())
        .context("X webhook is enabled without an API secret key")?;
    let signature = event
        .header("x-twitter-webhooks-signature")
        .context("X webhook is missing its signature")?;
    verify_webhook(&event.body, signature, secret)?;
    parse_chat_event(&event.body)
}

pub async fn run(
    mut webhooks: broadcast::Receiver<WebhookEvent>,
    config: watch::Receiver<Arc<AppConfig>>,
    chat: ChatHandle,
) {
    loop {
        match webhooks.recv().await {
            Ok(event) => {
                let settings = config.borrow().chat.clone();
                match process_event(&settings, &event) {
                    Ok(Some(message)) => {
                        if let Err(error) = chat.enqueue(message).await {
                            tracing::warn!("Discarding X chat event: {error:#}");
                        }
                    }
                    Ok(None) => {}
                    Err(error) => tracing::warn!("Rejected X webhook: {error:#}"),
                }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "X webhook subscriber lagged behind");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const BODY: &[u8] = br#"{"data":{"event_uuid":"event-123","event_type":"broadcast.chat","payload":{"message_id":"message-456","message":"Hello X","author":{"data":{"username":"viewer","profile_image_url":"https://example.test/avatar.jpg"}}}}}"#;

    fn signed_event(secret: &str) -> WebhookEvent {
        let mut headers = HashMap::new();
        headers.insert(
            "x-twitter-webhooks-signature".into(),
            response_token(std::str::from_utf8(BODY).unwrap(), secret).unwrap(),
        );
        WebhookEvent {
            headers,
            body: BODY.to_vec().into(),
        }
    }

    #[test]
    fn calculates_crc_response() {
        assert_eq!(
            response_token("hello", "secret").unwrap(),
            "sha256=iKqz7ejTrflNJquQ07r9SiCDBww7zOnAFO4EpEOEfAs="
        );
    }

    #[test]
    fn accepts_valid_signature_and_rejects_invalid_signature() {
        let event = signed_event("secret");
        let signature = event.header("x-twitter-webhooks-signature").unwrap();
        assert!(verify_webhook(&event.body, signature, "secret").is_ok());
        assert!(verify_webhook(&event.body, signature, "wrong").is_err());
    }

    #[test]
    fn parses_broadcast_chat_event() {
        let message = parse_chat_event(BODY).unwrap().unwrap();
        assert_eq!(message.source, "x");
        assert_eq!(message.external_id, "message-456");
        assert_eq!(message.author, "viewer");
        assert_eq!(message.text, "Hello X");
        assert_eq!(
            message.avatar_url.as_deref(),
            Some("https://example.test/avatar.jpg")
        );
    }

    #[test]
    fn disabled_webhook_ignores_events() {
        assert!(
            process_event(&ChatSettings::default(), &signed_event("secret"))
                .unwrap()
                .is_none()
        );
    }
}
