use crate::chat::{ChatHandle, IncomingChatMessage};
use crate::util::now_unix_ms;
use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const YOUTUBE_CHAT_MESSAGES_URL: &str = "https://www.googleapis.com/youtube/v3/liveChat/messages";
const YOUTUBE_VIDEOS_URL: &str = "https://www.googleapis.com/youtube/v3/videos";
const YOUTUBE_SEARCH_URL: &str = "https://www.googleapis.com/youtube/v3/search";

#[derive(Debug, Clone)]
pub struct YouTubeChatConfig {
    pub api_key: String,
    pub target: YouTubeChatTarget,
    pub min_poll_interval: Duration,
    pub adaptive_polling: bool,
}

#[derive(Debug, Clone)]
pub enum YouTubeChatTarget {
    LiveChat(String),
    Video(String),
    Channel(String),
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct YouTubeIngestStatus {
    pub state: String,
    pub detail: String,
    pub last_success_at_unix_ms: Option<u64>,
    pub messages_received: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveChatResponse {
    #[serde(default)]
    next_page_token: Option<String>,
    #[serde(default = "default_polling_interval")]
    polling_interval_millis: u64,
    #[serde(default)]
    items: Vec<LiveChatItem>,
}

fn default_polling_interval() -> u64 {
    5000
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveChatItem {
    id: String,
    snippet: LiveChatSnippet,
    author_details: LiveChatAuthor,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveChatSnippet {
    #[serde(default)]
    display_message: String,
    #[serde(default)]
    has_display_content: bool,
    #[serde(default)]
    published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveChatAuthor {
    display_name: String,
    #[serde(default)]
    profile_image_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VideosResponse {
    #[serde(default)]
    items: Vec<VideoItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoItem {
    live_streaming_details: Option<LiveStreamingDetails>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveStreamingDetails {
    active_live_chat_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<SearchItem>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    id: SearchItemId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchItemId {
    video_id: String,
}

pub async fn run(client: Client, chat: ChatHandle, config: YouTubeChatConfig) {
    let (mut resolution_delay, maximum_resolution_delay) = match &config.target {
        YouTubeChatTarget::Channel(_) => {
            (Duration::from_secs(30 * 60), Duration::from_secs(30 * 60))
        }
        YouTubeChatTarget::LiveChat(_) | YouTubeChatTarget::Video(_) => {
            (Duration::from_secs(5), Duration::from_secs(60))
        }
    };
    let live_chat_id = loop {
        chat.update_youtube_status(
            "resolving",
            "Resolving the active YouTube live chat",
            None,
            None,
        );
        match resolve_live_chat_id(&client, &config).await {
            Ok(live_chat_id) => break live_chat_id,
            Err(error) => {
                let detail = format!(
                    "Could not resolve an active YouTube chat: {error:#}. Retrying in {} seconds",
                    resolution_delay.as_secs()
                );
                tracing::warn!("{detail}");
                chat.update_youtube_status("error", detail, None, None);
                tokio::time::sleep(resolution_delay).await;
                resolution_delay = (resolution_delay * 2).min(maximum_resolution_delay);
            }
        }
    };

    let mut page_token = None;
    let mut retry_delay = Duration::from_secs(2);
    let mut idle_polls = 0_u32;

    loop {
        match fetch_page(
            &client,
            &config.api_key,
            &live_chat_id,
            page_token.as_deref(),
        )
        .await
        {
            Ok(page) => {
                retry_delay = Duration::from_secs(2);
                page_token = page.next_page_token.clone();
                let mut accepted = 0_u64;

                for item in page.items {
                    if !item.snippet.has_display_content
                        || item.snippet.display_message.trim().is_empty()
                    {
                        continue;
                    }

                    let message = IncomingChatMessage {
                        source: "youtube".into(),
                        external_id: item.id,
                        author: item.author_details.display_name,
                        text: item.snippet.display_message,
                        avatar_url: item.author_details.profile_image_url,
                        sent_at: item.snippet.published_at,
                    };
                    match chat.enqueue(message).await {
                        Ok(crate::chat::EnqueueOutcome::Accepted) => accepted += 1,
                        Ok(
                            crate::chat::EnqueueOutcome::Duplicate
                            | crate::chat::EnqueueOutcome::Dropped,
                        ) => {}
                        Err(error) => {
                            tracing::warn!("Discarding invalid YouTube chat message: {error}")
                        }
                    }
                }

                idle_polls = if accepted == 0 {
                    idle_polls.saturating_add(1)
                } else {
                    0
                };
                let api_interval =
                    Duration::from_millis(page.polling_interval_millis.clamp(1000, 60_000));
                let base_interval = api_interval.max(config.min_poll_interval);
                let adaptive_delay = if config.adaptive_polling && idle_polls >= 3 {
                    Duration::from_secs(u64::from((idle_polls - 2).min(5)) * 2)
                } else {
                    Duration::ZERO
                };
                let poll_interval = base_interval + adaptive_delay;
                let detail = if accepted == 0 {
                    format!(
                        "Connected; no new messages. Next poll in {} seconds",
                        poll_interval.as_secs_f32()
                    )
                } else {
                    format!(
                        "Received {accepted} message(s). Next poll in {} seconds",
                        poll_interval.as_secs_f32()
                    )
                };
                chat.update_youtube_status(
                    "polling",
                    detail,
                    Some(now_unix_ms()),
                    Some(accepted),
                );
                tokio::time::sleep(poll_interval).await;
            }
            Err(PollFailure::Retry(error)) => {
                let detail = format!(
                    "YouTube chat poll failed: {error:#}. Retrying in {} seconds",
                    retry_delay.as_secs()
                );
                tracing::warn!("{detail}");
                chat.update_youtube_status("error", detail, None, None);
                tokio::time::sleep(retry_delay).await;
                retry_delay = (retry_delay * 2).min(Duration::from_secs(60));
            }
            Err(PollFailure::Stop(error)) => {
                let detail = format!("YouTube chat ingest stopped: {error:#}");
                tracing::error!("{detail}");
                chat.update_youtube_status("stopped", detail, None, None);
                return;
            }
        }
    }
}

async fn resolve_live_chat_id(client: &Client, config: &YouTubeChatConfig) -> Result<String> {
    match &config.target {
        YouTubeChatTarget::LiveChat(live_chat_id) => Ok(live_chat_id.clone()),
        YouTubeChatTarget::Video(video_id) => {
            live_chat_id_from_video(client, &config.api_key, video_id).await
        }
        YouTubeChatTarget::Channel(channel_id) => {
            let video_id = active_video_from_channel(client, &config.api_key, channel_id).await?;
            live_chat_id_from_video(client, &config.api_key, &video_id).await
        }
    }
}

async fn live_chat_id_from_video(
    client: &Client,
    api_key: &str,
    video_id: &str,
) -> Result<String> {
    let response = client
        .get(YOUTUBE_VIDEOS_URL)
        .query(&[
            ("id", video_id),
            ("part", "liveStreamingDetails"),
            ("key", api_key),
        ])
        .send()
        .await
        .context("Failed to query the YouTube videos API")?;
    let response: VideosResponse = decode_youtube_response(response).await?;
    response
        .items
        .first()
        .and_then(|item| item.live_streaming_details.as_ref())
        .and_then(|details| details.active_live_chat_id.clone())
        .context("The selected YouTube video does not have an active live chat")
}

async fn active_video_from_channel(
    client: &Client,
    api_key: &str,
    channel_id: &str,
) -> Result<String> {
    let response = client
        .get(YOUTUBE_SEARCH_URL)
        .query(&[
            ("part", "snippet"),
            ("channelId", channel_id),
            ("eventType", "live"),
            ("type", "video"),
            ("maxResults", "1"),
            ("key", api_key),
        ])
        .send()
        .await
        .context("Failed to query the YouTube search API")?;
    let response: SearchResponse = decode_youtube_response(response).await?;
    response
        .items
        .first()
        .map(|item| item.id.video_id.clone())
        .context("The selected YouTube channel has no active live stream")
}

async fn fetch_page(
    client: &Client,
    api_key: &str,
    live_chat_id: &str,
    page_token: Option<&str>,
) -> std::result::Result<LiveChatResponse, PollFailure> {
    let mut request = client.get(YOUTUBE_CHAT_MESSAGES_URL).query(&[
        ("liveChatId", live_chat_id),
        ("part", "id,snippet,authorDetails"),
        ("maxResults", "200"),
        ("key", api_key),
    ]);
    if let Some(page_token) = page_token {
        request = request.query(&[("pageToken", page_token)]);
    }

    let response = request
        .send()
        .await
        .context("Failed to call the YouTube live chat API")
        .map_err(PollFailure::Retry)?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let error = anyhow::anyhow!(
            "YouTube API returned {}: {}",
            status,
            body.chars().take(500).collect::<String>()
        );
        return if matches!(status.as_u16(), 403 | 429) {
            Err(PollFailure::Stop(error))
        } else {
            Err(PollFailure::Retry(error))
        };
    }
    response
        .json()
        .await
        .context("Failed to decode the YouTube API response")
        .map_err(PollFailure::Retry)
}

#[derive(Debug)]
enum PollFailure {
    Retry(anyhow::Error),
    Stop(anyhow::Error),
}

async fn decode_youtube_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!(
            "YouTube API returned {}: {}",
            status,
            body.chars().take(500).collect::<String>()
        );
    }
    response
        .json()
        .await
        .context("Failed to decode the YouTube API response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_displayable_youtube_chat_messages() {
        let response: LiveChatResponse = serde_json::from_str(
            r#"{
                "nextPageToken": "next",
                "pollingIntervalMillis": 2500,
                "items": [{
                    "id": "message-id",
                    "snippet": {
                        "displayMessage": "Hello chat",
                        "hasDisplayContent": true,
                        "publishedAt": "2026-07-26T07:30:00Z"
                    },
                    "authorDetails": {
                        "displayName": "Viewer",
                        "profileImageUrl": "https://example.test/avatar.png"
                    }
                }]
            }"#,
        )
        .unwrap();

        assert_eq!(response.next_page_token.as_deref(), Some("next"));
        assert_eq!(response.polling_interval_millis, 2500);
        assert_eq!(response.items[0].id, "message-id");
        assert_eq!(response.items[0].snippet.display_message, "Hello chat");
        assert_eq!(response.items[0].author_details.display_name, "Viewer");
    }

    #[test]
    fn decodes_video_and_channel_resolution_responses() {
        let videos: VideosResponse = serde_json::from_str(
            r#"{"items":[{"liveStreamingDetails":{"activeLiveChatId":"chat-id"}}]}"#,
        )
        .unwrap();
        assert_eq!(
            videos.items[0]
                .live_streaming_details
                .as_ref()
                .unwrap()
                .active_live_chat_id
                .as_deref(),
            Some("chat-id")
        );

        let search: SearchResponse =
            serde_json::from_str(r#"{"items":[{"id":{"videoId":"video-id"}}]}"#).unwrap();
        assert_eq!(search.items[0].id.video_id, "video-id");
    }
}
