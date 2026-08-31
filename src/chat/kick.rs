use crate::chat::ChatHandle;
use crate::chat::IncomingChatMessage;
use crate::config::AppConfig;
use crate::server::state::WebhookEvent;
use anyhow::{Context, Result, bail};
use aws_lc_rs::signature::{ParsedPublicKey, RSA_PKCS1_2048_8192_SHA256};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

const KICK_PUBLIC_KEY: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAq/+l1WnlRrGSolDMA+A86rAhMbQGmQ2SapVcGM3zq8ANXjnhDWocMqfWcTd95btDydITa10kDvHzw9WQOqp2MZI7ZyrfzJuz5nhTPCiJwTwnEtWft7nV14BYRDHvlfqPUaZ+1KR4OCaO/wWIk/rQL/TjY0M70gse8rlBkbo2a8rKhu69RQTRsoaf4DVhDPEeSeI5jVrRDGAMGL3cGuyY6CLKGdjVEM78g3JfYOvDU/RvfqD7L89TZ3iN94jrmWdGz34JNlEI5hqK8dd7C5EFBEbZ5jgB8s8ReQV8H+MkuffjdAj3ajDDX3DOJMIut1lBrUVD1AaSrGCKHooWoL2etwIDAQAB";

#[derive(Debug, Deserialize)]
struct KickChatEvent {
    message_id: String,
    content: String,
    sender: KickSender,
    created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KickSender {
    username: String,
    profile_picture: Option<String>,
}

pub fn verify_webhook(
    message_id: &str,
    timestamp: &str,
    body: &[u8],
    signature: &str,
) -> Result<()> {
    let signature = STANDARD
        .decode(signature)
        .context("Kick signature is not valid base64")?;
    let public_key = STANDARD
        .decode(KICK_PUBLIC_KEY)
        .context("Kick public key is invalid base64")?;
    let public_key = ParsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, public_key)
        .context("Kick public key is invalid")?;
    let payload = [message_id.as_bytes(), timestamp.as_bytes(), body].join(&b'.');
    public_key
        .verify_sig(&payload, &signature)
        .map_err(|_| anyhow::anyhow!("Kick webhook signature verification failed"))
}

pub fn parse_chat_event(body: &[u8]) -> Result<IncomingChatMessage> {
    let event: KickChatEvent = serde_json::from_slice(body).context("Invalid Kick chat event")?;
    if event.message_id.trim().is_empty() || event.content.trim().is_empty() {
        bail!("Kick chat event is missing a message ID or content");
    }
    Ok(IncomingChatMessage {
        source: "kick".into(),
        external_id: event.message_id,
        author: event.sender.username,
        text: event.content,
        avatar_url: event.sender.profile_picture,
        sent_at: event.created_at,
    })
}

pub async fn run(
    mut webhooks: broadcast::Receiver<WebhookEvent>,
    config: watch::Receiver<Arc<AppConfig>>,
    chat: ChatHandle,
) {
    loop {
        match webhooks.recv().await {
            Ok(event) => {
                if !config.borrow().chat.kick_webhook_enabled {
                    continue;
                }
                if event.header("kick-event-type") != Some("chat.message.sent")
                    || event.header("kick-event-version") != Some("1")
                {
                    continue;
                }
                let Some(message_id) = event.header("kick-event-message-id") else {
                    tracing::warn!("Ignoring Kick webhook without a message ID");
                    continue;
                };
                let Some(timestamp) = event.header("kick-event-message-timestamp") else {
                    tracing::warn!("Ignoring Kick webhook without a timestamp");
                    continue;
                };
                let Some(signature) = event.header("kick-event-signature") else {
                    tracing::warn!("Ignoring Kick webhook without a signature");
                    continue;
                };
                if let Err(error) = verify_webhook(message_id, timestamp, &event.body, signature) {
                    tracing::warn!("Rejected Kick webhook: {error:#}");
                    continue;
                }
                let message = match parse_chat_event(&event.body) {
                    Ok(message) => message,
                    Err(error) => {
                        tracing::warn!("Discarding Kick chat event: {error:#}");
                        continue;
                    }
                };
                if let Err(error) = chat.enqueue(message).await {
                    tracing::warn!("Discarding Kick chat event: {error:#}");
                }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "Kick webhook subscriber lagged behind");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chat_event() {
        let message = parse_chat_event(
            br#"{"message_id":"01ABC","content":"Hello Kick","sender":{"username":"viewer","profile_picture":"https://example.test/avatar.jpg"},"created_at":"2026-08-30T15:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(message.source, "kick");
        assert_eq!(message.external_id, "01ABC");
        assert_eq!(message.author, "viewer");
        assert_eq!(message.text, "Hello Kick");
    }

    #[test]
    fn parses_public_key_before_rejecting_invalid_signature() {
        let signature = STANDARD.encode([0_u8; 256]);
        let error = verify_webhook("message", "timestamp", b"{}", &signature).unwrap_err();
        assert!(error.to_string().contains("signature verification failed"));
    }
}
