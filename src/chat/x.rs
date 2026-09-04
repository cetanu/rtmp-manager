use crate::chat::IncomingChatMessage;
use crate::config::ChatSettings;
use crate::server::state::WebhookEvent;
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

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

pub fn process_event(
    config: &ChatSettings,
    event: &WebhookEvent,
) -> Result<Option<IncomingChatMessage>> {
    if !config.x_webhook_enabled {
        bail!("X webhook ingestion is disabled");
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const BODY: &[u8] = br#"{"data":{"event_uuid":"7365873565827875221","filter":{"user_id":"2055263440998973400"},"event_type":"broadcast.chat","tag":"broadcast chat","payload":{"broadcast_id":"1mGPaZaAdNqJN","message_id":"2090000000000000004","message":"hello from chat","is_subscriber":false,"author":{"data":{"id":"1234567890123456789","username":"ExampleUser","name":"Example User","created_at":"2024-08-31T23:23:51.000Z","description":"X","protected":false,"verified":true,"verified_type":"blue","is_identity_verified":false,"profile_image_url":"https://example.test/avatar.jpg","profile_banner_url":"https://example.test/banner.jpg","url":"https://t.co/xxxxxxxx","entities":{"url":{"urls":[]},"description":{"mentions":[]}},"public_metrics":{"followers_count":2774,"following_count":268,"tweet_count":705,"listed_count":42,"like_count":983,"media_count":93},"affiliation":{"url":"https://twitter.com/X","badge_url":"https://example.test/badge.jpg","description":"X"}}}}}}"#;

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
        assert_eq!(message.external_id, "2090000000000000004");
        assert_eq!(message.author, "ExampleUser");
        assert_eq!(message.text, "hello from chat");
        assert_eq!(
            message.avatar_url.as_deref(),
            Some("https://example.test/avatar.jpg")
        );
    }

    #[test]
    fn disabled_webhook_rejects_events() {
        assert!(
            process_event(&ChatSettings::default(), &signed_event("secret"))
                .unwrap_err()
                .to_string()
                .contains("disabled")
        );
    }
}
