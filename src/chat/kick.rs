use crate::chat::util::WebhookEvent;
use crate::chat::{IncomingChatMessage, Source};
use crate::config::ChatSettings;
use anyhow::{Context, Result, bail};
use aws_lc_rs::signature::{ParsedPublicKey, RSA_PKCS1_2048_8192_SHA256};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Client, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::sync::OnceLock;

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

#[derive(Deserialize)]
struct KickAccessToken {
    access_token: String,
}

#[derive(Deserialize)]
struct KickApiResponse<T> {
    data: T,
}

#[derive(Deserialize)]
struct KickApiError {
    message: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct KickSubscription {
    id: String,
    broadcaster_user_id: u64,
    event: String,
    version: u64,
}

#[derive(Deserialize)]
struct KickChannel {
    broadcaster_user_id: u64,
}

#[derive(Serialize)]
struct CreateSubscriptions {
    broadcaster_user_id: u64,
    method: &'static str,
    events: [KickEvent; 1],
}

#[derive(Serialize)]
struct KickEvent {
    name: &'static str,
    version: u64,
}

#[derive(Deserialize)]
struct CreatedSubscription {
    error: Option<String>,
    subscription_id: Option<String>,
}

const KICK_TOKEN_URL: &str = "https://id.kick.com/oauth/token";
const KICK_CHANNELS_URL: &str = "https://api.kick.com/public/v1/channels";
const KICK_SUBSCRIPTIONS_URL: &str = "https://api.kick.com/public/v1/events/subscriptions";

pub fn verify_webhook(
    message_id: &str,
    timestamp: &str,
    body: &[u8],
    signature: &str,
) -> Result<()> {
    let signature = STANDARD
        .decode(signature)
        .context("Kick signature is not valid base64")?;
    let public_key = kick_public_key()?;
    let payload = [message_id.as_bytes(), timestamp.as_bytes(), body].join(&b'.');
    public_key
        .verify_sig(&payload, &signature)
        .map_err(|_| anyhow::anyhow!("Kick webhook signature verification failed"))
}

fn kick_public_key() -> Result<&'static ParsedPublicKey> {
    static PUBLIC_KEY: OnceLock<Result<ParsedPublicKey, String>> = OnceLock::new();

    match PUBLIC_KEY.get_or_init(|| {
        let public_key = STANDARD
            .decode(KICK_PUBLIC_KEY)
            .map_err(|error| format!("Kick public key is invalid base64: {error}"))?;
        ParsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, public_key)
            .map_err(|error| format!("Kick public key is invalid: {error}"))
    }) {
        Ok(public_key) => Ok(public_key),
        Err(error) => Err(anyhow::anyhow!(error.clone())),
    }
}

pub fn parse_chat_event(body: &[u8]) -> Result<IncomingChatMessage> {
    let event: KickChatEvent = serde_json::from_slice(body).context("Invalid Kick chat event")?;
    if event.message_id.trim().is_empty() || event.content.trim().is_empty() {
        bail!("Kick chat event is missing a message ID or content");
    }
    Ok(IncomingChatMessage {
        source: Source::Kick,
        external_id: event.message_id,
        author: event.sender.username,
        text: event.content,
        avatar_url: event.sender.profile_picture,
        sent_at: event.created_at,
    })
}

pub fn process_event(config: &ChatSettings, event: &WebhookEvent) -> Result<IncomingChatMessage> {
    if !config.kick_webhook_enabled {
        bail!("Kick webhook ingestion is disabled");
    }
    let message_id = event
        .header("kick-event-message-id")
        .context("Kick webhook is missing its message ID")?;
    let timestamp = event
        .header("kick-event-message-timestamp")
        .context("Kick webhook is missing its timestamp")?;
    let signature = event
        .header("kick-event-signature")
        .context("Kick webhook is missing its signature")?;
    verify_webhook(message_id, timestamp, &event.body, signature)?;
    if event.header("kick-event-type") != Some("chat.message.sent")
        || event.header("kick-event-version") != Some("1")
    {
        bail!("Unsupported Kick webhook event type or version");
    }
    parse_chat_event(&event.body)
}

pub async fn set_chat_subscription(
    http_client: &Client,
    config: &ChatSettings,
    enabled: bool,
) -> Result<()> {
    let client_id = required_setting(&config.kick_client_id, "Kick client ID")?;
    let client_secret = required_setting(&config.kick_client_secret, "Kick client secret")?;
    let channel = required_setting(&config.kick_channel, "Kick channel")?;
    let access_token = app_access_token(http_client, client_id, client_secret).await?;
    let broadcaster_user_id =
        resolve_broadcaster_user_id(http_client, &access_token, channel).await?;
    let subscriptions = list_subscriptions(http_client, &access_token, broadcaster_user_id).await?;
    let chat_subscriptions: Vec<_> = subscriptions
        .into_iter()
        .filter(|subscription| {
            subscription.broadcaster_user_id == broadcaster_user_id
                && subscription.event == "chat.message.sent"
                && subscription.version == 1
        })
        .collect();

    if enabled {
        if chat_subscriptions.is_empty() {
            create_subscription(http_client, &access_token, broadcaster_user_id).await?;
        }
    } else if !chat_subscriptions.is_empty() {
        delete_subscriptions(http_client, &access_token, &chat_subscriptions).await?;
    }
    Ok(())
}

async fn resolve_broadcaster_user_id(
    http_client: &Client,
    access_token: &str,
    channel: &str,
) -> Result<u64> {
    let response: KickApiResponse<Vec<KickChannel>> = send_json(
        http_client
            .get(KICK_CHANNELS_URL)
            .bearer_auth(access_token)
            .query(&[("slug", channel)]),
        format!("Failed to look up Kick channel '{channel}'"),
        "look up the Kick channel",
        "Kick returned an invalid channel lookup response",
    )
    .await?;
    response
        .data
        .into_iter()
        .next()
        .map(|channel| channel.broadcaster_user_id)
        .with_context(|| format!("Kick channel '{channel}' was not found"))
}

fn required_setting<'a>(value: &'a Option<String>, name: &str) -> Result<&'a str> {
    value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("{name} is not configured"))
}

async fn app_access_token(
    http_client: &Client,
    client_id: &str,
    client_secret: &str,
) -> Result<String> {
    let token: KickAccessToken = send_json(
        http_client.post(KICK_TOKEN_URL).form(&[
            ("grant_type", "client_credentials"),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ]),
        "Failed to request a Kick app access token",
        "request a Kick app access token",
        "Kick returned an invalid app access token response",
    )
    .await?;
    if token.access_token.is_empty() {
        bail!("Kick returned an empty app access token");
    }
    Ok(token.access_token)
}

async fn list_subscriptions(
    http_client: &Client,
    access_token: &str,
    broadcaster_user_id: u64,
) -> Result<Vec<KickSubscription>> {
    let response: KickApiResponse<Vec<KickSubscription>> = send_json(
        http_client
            .get(KICK_SUBSCRIPTIONS_URL)
            .bearer_auth(access_token)
            .query(&[("broadcaster_user_id", broadcaster_user_id)]),
        "Failed to list Kick webhook subscriptions",
        "list Kick webhook subscriptions",
        "Kick returned an invalid webhook subscription list",
    )
    .await?;
    Ok(response.data)
}

async fn create_subscription(
    http_client: &Client,
    access_token: &str,
    broadcaster_user_id: u64,
) -> Result<()> {
    let request = CreateSubscriptions {
        broadcaster_user_id,
        method: "webhook",
        events: [KickEvent {
            name: "chat.message.sent",
            version: 1,
        }],
    };
    let response: KickApiResponse<Vec<CreatedSubscription>> = send_json(
        http_client
            .post(KICK_SUBSCRIPTIONS_URL)
            .bearer_auth(access_token)
            .json(&request),
        "Failed to create the Kick chat webhook subscription",
        "create the Kick chat webhook subscription",
        "Kick returned an invalid webhook subscription response",
    )
    .await?;
    let created = response
        .data
        .into_iter()
        .next()
        .context("Kick did not return a chat webhook subscription result")?;
    if let Some(error) = created.error.filter(|error| !error.is_empty()) {
        bail!("Kick rejected the chat webhook subscription: {error}");
    }
    if created.subscription_id.is_none() {
        bail!("Kick did not create the chat webhook subscription");
    }
    Ok(())
}

async fn delete_subscriptions(
    http_client: &Client,
    access_token: &str,
    subscriptions: &[KickSubscription],
) -> Result<()> {
    let query: Vec<_> = subscriptions
        .iter()
        .map(|subscription| ("id", subscription.id.as_str()))
        .collect();
    send_request(
        http_client
            .delete(KICK_SUBSCRIPTIONS_URL)
            .bearer_auth(access_token)
            .query(&query),
        "Failed to delete the Kick chat webhook subscription",
        "delete the Kick chat webhook subscription",
    )
    .await?;
    Ok(())
}

async fn send_json<T: DeserializeOwned>(
    request: RequestBuilder,
    send_context: impl Display + Send + Sync + 'static,
    action: &str,
    json_context: &'static str,
) -> Result<T> {
    send_request(request, send_context, action)
        .await?
        .json()
        .await
        .context(json_context)
}

async fn send_request(
    request: RequestBuilder,
    send_context: impl Display + Send + Sync + 'static,
    action: &str,
) -> Result<Response> {
    let response = request.send().await.context(send_context)?;
    require_success(response, action).await
}

async fn require_success(response: Response, action: &str) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let detail = response
        .json::<KickApiError>()
        .await
        .ok()
        .and_then(|error| error.message.or(error.error))
        .filter(|message| !message.is_empty());
    match detail {
        Some(detail) => bail!("Failed to {action}: Kick returned {status}: {detail}"),
        None => bail!("Failed to {action}: Kick returned {status}"),
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
        assert_eq!(message.source, Source::Kick);
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

    #[test]
    fn serializes_the_official_chat_subscription_request() {
        let request = CreateSubscriptions {
            broadcaster_user_id: 123,
            method: "webhook",
            events: [KickEvent {
                name: "chat.message.sent",
                version: 1,
            }],
        };

        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "broadcaster_user_id": 123,
                "method": "webhook",
                "events": [{"name": "chat.message.sent", "version": 1}]
            })
        );
    }

    #[test]
    fn parses_broadcaster_id_from_channel_lookup() {
        let response: KickApiResponse<Vec<KickChannel>> = serde_json::from_value(
            serde_json::json!({"data": [{"broadcaster_user_id": 123456789}]}),
        )
        .unwrap();

        assert_eq!(response.data[0].broadcaster_user_id, 123456789);
    }
}
