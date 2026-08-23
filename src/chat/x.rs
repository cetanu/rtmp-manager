use crate::chat::{ChatHandle, IncomingChatMessage};
use anyhow::{Context, Result, bail};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const PUBLIC_BEARER_TOKEN: &str = "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%3D1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA";
const GUEST_ACTIVATE_URL: &str = "https://api.twitter.com/1.1/guest/activate.json";
const BROADCAST_STATUS_URL: &str = "https://twitter.com/i/api/1.1/live_video_stream/status/";
const ACCESS_CHAT_URL: &str = "https://proxsee.pscp.tv/api/v2/accessChatPublic";
const HISTORY_PATH: &str = "/chatapi/v1/history";
const POLL_INTERVAL: Duration = Duration::from_secs(3);
const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(2);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct XChatConfig {
    pub media_key: String,
}

#[derive(Debug, Deserialize)]
struct GuestTokenResponse {
    guest_token: String,
}

#[derive(Debug, Deserialize)]
struct BroadcastStatusResponse {
    #[serde(rename = "chatToken")]
    chat_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct AccessChatRequest<'a> {
    chat_token: &'a str,
}

#[derive(Debug, Deserialize)]
struct AccessChatResponse {
    access_token: String,
    endpoint: Option<String>,
}

#[derive(Debug, Clone)]
struct ChatSession {
    access_token: String,
    endpoint_url: String,
}

#[derive(Debug, Serialize)]
struct HistoryRequest<'a> {
    access_token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct HistoryResponse {
    #[serde(default)]
    messages: Vec<HistoryMessage>,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HistoryMessage {
    payload: String,
}

#[derive(Debug, Deserialize)]
struct InnerPayload {
    #[serde(default)]
    kind: u8,
    #[serde(default)]
    body: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    user_name: Option<String>,
    #[serde(default)]
    screen_name: Option<String>,
}

impl InnerPayload {
    fn author(&self) -> String {
        self.username
            .as_deref()
            .or(self.user_name.as_deref())
            .or(self.screen_name.as_deref())
            .filter(|author| !author.trim().is_empty())
            .unwrap_or("X viewer")
            .to_owned()
    }
}

pub async fn run(client: Client, chat: ChatHandle, config: XChatConfig) {
    let mut session = None;
    let mut cursor = Some(String::new());
    let mut retry_delay = INITIAL_RETRY_DELAY;

    loop {
        if session.is_none() {
            match bootstrap_chat(&client, &config.media_key).await {
                Ok(new_session) => {
                    tracing::info!("X live chat session bootstrapped");
                    session = Some(new_session);
                    cursor = Some(String::new());
                    retry_delay = INITIAL_RETRY_DELAY;
                }
                Err(error) => {
                    tracing::warn!(
                        "X live chat bootstrap failed: {error:#}. Retrying in {} seconds",
                        retry_delay.as_secs()
                    );
                    tokio::time::sleep(retry_delay).await;
                    retry_delay = (retry_delay * 2).min(MAX_RETRY_DELAY);
                    continue;
                }
            }
        }

        let current_session = session.as_ref().expect("X session was just initialized");
        match fetch_history(&client, current_session, cursor.as_deref()).await {
            Ok(history) => {
                retry_delay = INITIAL_RETRY_DELAY;
                let received =
                    enqueue_messages(&chat, history.messages, history.cursor.as_deref()).await;
                cursor = history.cursor;
                tracing::debug!(received, "X live chat poll completed");
                tokio::time::sleep(POLL_INTERVAL).await;
            }
            Err(PollFailure::Retry(error)) => {
                tracing::warn!(
                    "X live chat poll failed: {error:#}. Retrying in {} seconds",
                    retry_delay.as_secs()
                );
                tokio::time::sleep(retry_delay).await;
                retry_delay = (retry_delay * 2).min(MAX_RETRY_DELAY);
            }
            Err(PollFailure::Rebootstrap(error)) => {
                tracing::warn!("X live chat session expired: {error:#}. Re-authenticating");
                session = None;
                cursor = Some(String::new());
                retry_delay = INITIAL_RETRY_DELAY;
            }
        }
    }
}

async fn bootstrap_chat(client: &Client, media_key: &str) -> Result<ChatSession> {
    let guest_response = client
        .post(GUEST_ACTIVATE_URL)
        .header("Authorization", PUBLIC_BEARER_TOKEN)
        .send()
        .await
        .context("Failed to activate an X guest session")?;
    let guest: GuestTokenResponse = decode_response(guest_response, "X guest activation").await?;

    let status_url = format!("{BROADCAST_STATUS_URL}{media_key}");
    let status_response = client
        .get(status_url)
        .header("Authorization", PUBLIC_BEARER_TOKEN)
        .header("x-guest-token", guest.guest_token)
        .send()
        .await
        .context("Failed to query the X broadcast status")?;
    let status: BroadcastStatusResponse =
        decode_response(status_response, "X broadcast status").await?;
    let chat_token = status
        .chat_token
        .filter(|token| !token.trim().is_empty())
        .context("The X broadcast does not expose a chat token")?;

    let access_response = client
        .post(ACCESS_CHAT_URL)
        .json(&AccessChatRequest {
            chat_token: &chat_token,
        })
        .send()
        .await
        .context("Failed to exchange the X chat token")?;
    let access: AccessChatResponse =
        decode_response(access_response, "Periscope chat access").await?;
    let endpoint_url = access
        .endpoint
        .filter(|endpoint| !endpoint.trim().is_empty())
        .unwrap_or_else(|| "https://proxsee.pscp.tv".into());

    Ok(ChatSession {
        access_token: access.access_token,
        endpoint_url,
    })
}

async fn decode_response<T: for<'de> Deserialize<'de>>(
    response: Response,
    operation: &str,
) -> Result<T> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!(
            "{operation} returned {}: {}",
            status,
            body.chars().take(500).collect::<String>()
        );
    }
    response
        .json()
        .await
        .with_context(|| format!("Failed to decode the {operation} response"))
}

async fn fetch_history(
    client: &Client,
    session: &ChatSession,
    cursor: Option<&str>,
) -> std::result::Result<HistoryResponse, PollFailure> {
    let url = format!(
        "{}{}",
        session.endpoint_url.trim_end_matches('/'),
        HISTORY_PATH
    );
    let request = HistoryRequest {
        access_token: &session.access_token,
        cursor,
    };
    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .context("Failed to call the X live chat history API")
        .map_err(PollFailure::Retry)?;
    let status = response.status();
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return Err(PollFailure::Rebootstrap(anyhow::anyhow!(
            "X live chat history API returned {status}"
        )));
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(PollFailure::Retry(anyhow::anyhow!(
            "X live chat history API returned {}: {}",
            status,
            body.chars().take(500).collect::<String>()
        )));
    }

    response
        .json()
        .await
        .context("Failed to decode the X live chat API response")
        .map_err(PollFailure::Retry)
}

async fn enqueue_messages(
    chat: &ChatHandle,
    messages: Vec<HistoryMessage>,
    cursor: Option<&str>,
) -> u64 {
    let mut accepted = 0;
    for (index, message) in messages.into_iter().enumerate() {
        let Ok(payload) = serde_json::from_str::<InnerPayload>(&message.payload) else {
            tracing::debug!("Ignoring X live chat message with an invalid payload");
            continue;
        };
        if payload.kind != 1 || payload.body.trim().is_empty() {
            continue;
        }

        let author = payload.author();
        let external_id = payload
            .id
            .clone()
            .unwrap_or_else(|| format!("history-{}-{index}", cursor.unwrap_or("initial")));
        let message = IncomingChatMessage {
            source: "x".into(),
            external_id,
            author,
            text: payload.body,
            avatar_url: None,
            sent_at: None,
        };
        match chat.enqueue(message).await {
            Ok(crate::chat::EnqueueOutcome::Accepted) => accepted += 1,
            Ok(crate::chat::EnqueueOutcome::Duplicate | crate::chat::EnqueueOutcome::Dropped) => {}
            Err(error) => tracing::warn!("Discarding invalid X live chat message: {error}"),
        }
    }
    accepted
}

#[derive(Debug)]
enum PollFailure {
    Retry(anyhow::Error),
    Rebootstrap(anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_bootstrap_responses() {
        let guest: GuestTokenResponse =
            serde_json::from_str(r#"{"guest_token":"guest-token"}"#).unwrap();
        let status: BroadcastStatusResponse =
            serde_json::from_str(r#"{"chatToken":"chat-token"}"#).unwrap();
        let access: AccessChatResponse = serde_json::from_str(
            r#"{"access_token":"access-token","endpoint":"https://chat.example"}"#,
        )
        .unwrap();

        assert_eq!(guest.guest_token, "guest-token");
        assert_eq!(status.chat_token.as_deref(), Some("chat-token"));
        assert_eq!(access.access_token, "access-token");
        assert_eq!(access.endpoint.as_deref(), Some("https://chat.example"));
    }

    #[test]
    fn decodes_history_and_stringified_chat_payload() {
        let response: HistoryResponse = serde_json::from_str(
            r#"{
                "cursor": "next-cursor",
                "messages": [{
                    "payload": "{\"kind\":1,\"id\":\"message-id\",\"username\":\"viewer\",\"body\":\"Hello chat\"}"
                }]
            }"#,
        )
        .unwrap();

        let payload: InnerPayload = serde_json::from_str(&response.messages[0].payload).unwrap();
        assert_eq!(response.cursor.as_deref(), Some("next-cursor"));
        assert_eq!(payload.kind, 1);
        assert_eq!(payload.id.as_deref(), Some("message-id"));
        assert_eq!(payload.author(), "viewer");
        assert_eq!(payload.body, "Hello chat");
    }
}
