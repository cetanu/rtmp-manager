use crate::chat::{ChatHandle, IncomingChatMessage};
use crate::util::now_unix_ms;
use anyhow::{Context, Result, bail};
use regex::Regex;
use reqwest::Client;
use reqwest::header::{ACCEPT_LANGUAGE, USER_AGENT};
use serde::Serialize;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::Semaphore;

const DEFAULT_INNERTUBE_API_KEY: &str = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";
const INNERTUBE_LIVE_CHAT_URL: &str = "https://www.youtube.com/youtubei/v1/live_chat/get_live_chat";
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const BROWSER_ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";
static REQUEST_LIMITER: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(8));

static API_KEY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""INNERTUBE_API_KEY":"([^"]+)""#).unwrap());
static YT_INITIAL_DATA_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:window\["ytInitialData"\]|var ytInitialData)\s*=\s*(\{.+?\});</script>"#)
        .unwrap()
});
static CANONICAL_URL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<link rel="canonical" href="([^"]+)""#).unwrap());
static VIDEO_ID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""videoId":"([a-zA-Z0-9_-]{11})""#).unwrap());
static WATCH_URL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[?&]v=([a-zA-Z0-9_-]{11})"#).unwrap());
static SHORT_URL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"youtu\.be/([a-zA-Z0-9_-]{11})"#).unwrap());

#[derive(Debug, Clone)]
pub struct YouTubeChatConfig {
    pub target: YouTubeChatTarget,
    pub min_poll_interval: Duration,
    pub adaptive_polling: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
struct LiveChatSession {
    api_key: String,
    continuation_token: String,
    video_id: String,
}

pub async fn run(client: Client, chat: ChatHandle, config: YouTubeChatConfig) {
    let (mut resolution_delay, maximum_resolution_delay) = match &config.target {
        YouTubeChatTarget::Channel(_) => (Duration::from_secs(30), Duration::from_secs(5 * 60)),
        YouTubeChatTarget::LiveChat(_) | YouTubeChatTarget::Video(_) => {
            (Duration::from_secs(5), Duration::from_secs(60))
        }
    };

    let mut session = loop {
        chat.update_youtube_status(
            "resolving",
            "Resolving the active YouTube live chat via web UI",
            None,
            None,
        );
        match resolve_live_chat_session(&client, &config.target).await {
            Ok(session) => {
                chat.update_youtube_status(
                    "connected",
                    format!(
                        "Connected to YouTube live chat for video {}",
                        session.video_id
                    ),
                    Some(now_unix_ms()),
                    None,
                );
                break session;
            }
            Err(error) => {
                let detail = format!(
                    "Could not resolve an active YouTube live stream: {error:#}. Retrying in {} seconds",
                    resolution_delay.as_secs()
                );
                tracing::warn!("{detail}");
                chat.update_youtube_status("error", detail, None, None);
                tokio::time::sleep(resolution_delay).await;
                resolution_delay = (resolution_delay * 2).min(maximum_resolution_delay);
            }
        }
    };

    let mut retry_delay = Duration::from_secs(2);
    let mut idle_polls = 0_u32;

    loop {
        match fetch_innertube_page(&client, &session).await {
            Ok(page) => {
                retry_delay = Duration::from_secs(2);
                session.continuation_token = page.next_continuation_token;
                let mut accepted = 0_u64;

                for item in page.messages {
                    if item.text.trim().is_empty() {
                        continue;
                    }
                    match chat.enqueue(item).await {
                        Ok(crate::chat::EnqueueOutcome::Accepted) => accepted += 1,
                        Ok(
                            crate::chat::EnqueueOutcome::Duplicate
                            | crate::chat::EnqueueOutcome::Dropped,
                        ) => {}
                        Err(error) => {
                            tracing::warn!("Discarding invalid YouTube chat message: {error}");
                        }
                    }
                }

                idle_polls = if accepted == 0 {
                    idle_polls.saturating_add(1)
                } else {
                    0
                };

                let api_interval = Duration::from_millis(page.timeout_millis.clamp(1000, 60_000));
                let base_interval = api_interval.max(config.min_poll_interval);
                let adaptive_delay = if config.adaptive_polling && idle_polls >= 3 {
                    Duration::from_secs(u64::from((idle_polls - 2).min(5)) * 2)
                } else {
                    Duration::ZERO
                };
                let poll_interval = base_interval + adaptive_delay;
                let detail = if accepted == 0 {
                    format!(
                        "Connected ({}); no new messages. Next poll in {:.1}s",
                        session.video_id,
                        poll_interval.as_secs_f32()
                    )
                } else {
                    format!(
                        "Received {accepted} message(s) ({}); next poll in {:.1}s",
                        session.video_id,
                        poll_interval.as_secs_f32()
                    )
                };
                chat.update_youtube_status("polling", detail, Some(now_unix_ms()), Some(accepted));
                tokio::time::sleep(poll_interval).await;
            }
            Err(PollFailure::SessionExpired(reason)) => {
                let detail = format!(
                    "YouTube live chat session ended or expired ({reason:#}). Re-resolving active stream..."
                );
                tracing::warn!("{detail}");
                chat.update_youtube_status("resolving", detail, None, None);
                tokio::time::sleep(Duration::from_secs(5)).await;
                match resolve_live_chat_session(&client, &config.target).await {
                    Ok(new_session) => {
                        session = new_session;
                    }
                    Err(error) => {
                        let err_detail = format!(
                            "Failed to re-resolve YouTube stream: {error:#}. Retrying in 10s"
                        );
                        tracing::warn!("{err_detail}");
                        chat.update_youtube_status("error", err_detail, None, None);
                        tokio::time::sleep(Duration::from_secs(10)).await;
                    }
                }
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
            Err(PollFailure::RateLimited) => {
                let detail = "YouTube quota/rate limit reached; backing off for 60 seconds";
                tracing::warn!("{detail}");
                chat.update_youtube_status("rate-limited", detail, None, None);
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    }
}

pub async fn send_message(client: &Client, live_chat_id: &str, text: &str) -> Result<()> {
    let token = std::env::var("YOUTUBE_BOT_OAUTH_TOKEN")
        .context("YOUTUBE_BOT_OAUTH_TOKEN is not configured")?;
    let text = text.replace(['\r', '\n'], " ");
    if text.trim().is_empty() || text.len() > 500 {
        bail!("invalid YouTube message");
    }
    let response = client
        .post("https://www.googleapis.com/youtube/v3/liveChat/messages?part=snippet")
        .bearer_auth(token)
        .json(&serde_json::json!({ "snippet": { "liveChatId": live_chat_id, "type": "textMessageEvent", "textMessageDetails": { "messageText": text } } }))
        .send().await.context("failed to send YouTube chat message")?;
    if !response.status().is_success() {
        bail!("YouTube chat API returned {}", response.status());
    }
    Ok(())
}

async fn resolve_live_chat_session(
    client: &Client,
    target: &YouTubeChatTarget,
) -> Result<LiveChatSession> {
    match target {
        YouTubeChatTarget::LiveChat(chat_id) => {
            if let Some(video_id) = extract_video_id(chat_id) {
                bootstrap_live_chat_session(client, &video_id).await
            } else {
                Ok(LiveChatSession {
                    api_key: DEFAULT_INNERTUBE_API_KEY.to_string(),
                    continuation_token: chat_id.clone(),
                    video_id: "live_chat".to_string(),
                })
            }
        }
        YouTubeChatTarget::Video(video_input) => {
            let video_id = extract_video_id(video_input)
                .ok_or_else(|| anyhow::anyhow!("Invalid YouTube video ID or URL: {video_input}"))?;
            bootstrap_live_chat_session(client, &video_id).await
        }
        YouTubeChatTarget::Channel(channel_input) => {
            let video_id = resolve_channel_live_video(client, channel_input).await?;
            bootstrap_live_chat_session(client, &video_id).await
        }
    }
}

pub fn normalize_channel_url(channel_input: &str) -> String {
    let trimmed = channel_input.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        if trimmed.ends_with("/live") {
            trimmed.to_string()
        } else {
            format!("{}/live", trimmed.trim_end_matches('/'))
        }
    } else if trimmed.starts_with('@') {
        format!("https://www.youtube.com/{trimmed}/live")
    } else if trimmed.starts_with("UC") && trimmed.len() == 24 {
        format!("https://www.youtube.com/channel/{trimmed}/live")
    } else {
        format!("https://www.youtube.com/@{trimmed}/live")
    }
}

pub fn extract_video_id(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if let Some(captures) = WATCH_URL_REGEX.captures(trimmed) {
        return captures.get(1).map(|m| m.as_str().to_string());
    }
    if let Some(captures) = SHORT_URL_REGEX.captures(trimmed) {
        return captures.get(1).map(|m| m.as_str().to_string());
    }
    if trimmed.len() == 11
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Some(trimmed.to_string());
    }
    None
}

async fn resolve_channel_live_video(client: &Client, channel_input: &str) -> Result<String> {
    let url = normalize_channel_url(channel_input);
    let response = client
        .get(&url)
        .header(USER_AGENT, BROWSER_USER_AGENT)
        .header(ACCEPT_LANGUAGE, BROWSER_ACCEPT_LANGUAGE)
        .send()
        .await
        .with_context(|| format!("Failed to request YouTube live URL: {url}"))?;

    let final_url = response.url().as_str().to_string();
    let body = response
        .text()
        .await
        .context("Failed to read YouTube channel response body")?;

    if let Some(captures) = WATCH_URL_REGEX.captures(&final_url) {
        return Ok(captures[1].to_string());
    }

    if let Some(captures) = CANONICAL_URL_REGEX.captures(&body) {
        let canonical = &captures[1];
        if let Some(watch_match) = WATCH_URL_REGEX.captures(canonical) {
            return Ok(watch_match[1].to_string());
        }
    }

    if let Some(captures) = VIDEO_ID_REGEX.captures(&body) {
        let video_id = &captures[1];
        let is_live = body.contains("\"isLive\":true")
            || body.contains("\"isLiveNow\":true")
            || body.contains("\"liveStreamability\"");
        if is_live {
            return Ok(video_id.to_string());
        }
    }

    bail!("The selected YouTube channel has no active live stream")
}

async fn bootstrap_live_chat_session(client: &Client, video_id: &str) -> Result<LiveChatSession> {
    let url = format!("https://www.youtube.com/live_chat?v={video_id}");
    let response = client
        .get(&url)
        .header(USER_AGENT, BROWSER_USER_AGENT)
        .header(ACCEPT_LANGUAGE, BROWSER_ACCEPT_LANGUAGE)
        .send()
        .await
        .with_context(|| {
            format!("Failed to request YouTube live chat page for video {video_id}")
        })?;

    let body = response
        .text()
        .await
        .context("Failed to read YouTube live chat HTML response")?;

    let api_key = API_KEY_REGEX
        .captures(&body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| DEFAULT_INNERTUBE_API_KEY.to_string());

    let initial_data_raw = YT_INITIAL_DATA_REGEX
        .captures(&body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .ok_or_else(|| anyhow::anyhow!("Could not find ytInitialData in live chat page"))?;

    let initial_data: serde_json::Value = serde_json::from_str(initial_data_raw)
        .context("Failed to parse ytInitialData JSON from live chat page")?;

    if let Some(message_text) = initial_data
        .pointer("/contents/messageRenderer/text")
        .map(extract_runs)
        .filter(|t| !t.is_empty())
    {
        bail!("Live chat is not available for this stream: {message_text}");
    }

    let continuations = initial_data
        .pointer("/contents/liveChatRenderer/continuations")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("No live chat continuation found in ytInitialData"))?;

    let (continuation_token, _) = parse_continuations(continuations).ok_or_else(|| {
        anyhow::anyhow!("Could not extract continuation token from live chat page")
    })?;

    Ok(LiveChatSession {
        api_key,
        continuation_token,
        video_id: video_id.to_string(),
    })
}

struct InnerTubeFetchResult {
    messages: Vec<IncomingChatMessage>,
    next_continuation_token: String,
    timeout_millis: u64,
}

#[derive(Debug)]
enum PollFailure {
    SessionExpired(anyhow::Error),
    RateLimited,
    Retry(anyhow::Error),
}

async fn fetch_innertube_page(
    client: &Client,
    session: &LiveChatSession,
) -> std::result::Result<InnerTubeFetchResult, PollFailure> {
    let _permit = REQUEST_LIMITER
        .acquire()
        .await
        .map_err(|_| PollFailure::Retry(anyhow::anyhow!("YouTube request limiter stopped")))?;
    let url = format!("{INNERTUBE_LIVE_CHAT_URL}?key={}", session.api_key);
    let payload = serde_json::json!({
        "context": {
            "client": {
                "clientName": "WEB",
                "clientVersion": "2.20240101.00.00"
            }
        },
        "continuation": session.continuation_token
    });

    let response = client
        .post(&url)
        .header(USER_AGENT, BROWSER_USER_AGENT)
        .header(ACCEPT_LANGUAGE, BROWSER_ACCEPT_LANGUAGE)
        .json(&payload)
        .send()
        .await
        .context("Failed to call YouTube InnerTube live chat endpoint")
        .map_err(PollFailure::Retry)?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let error = anyhow::anyhow!(
            "YouTube InnerTube endpoint returned {}: {}",
            status,
            body.chars().take(400).collect::<String>()
        );
        return if status.as_u16() == 404 {
            Err(PollFailure::SessionExpired(error))
        } else if status.as_u16() == 429 {
            Err(PollFailure::RateLimited)
        } else {
            Err(PollFailure::Retry(error))
        };
    }

    let data: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse InnerTube JSON response")
        .map_err(PollFailure::Retry)?;

    let live_chat_continuation = data
        .pointer("/continuationContents/liveChatContinuation")
        .ok_or_else(|| {
            PollFailure::SessionExpired(anyhow::anyhow!(
                "InnerTube response did not contain liveChatContinuation"
            ))
        })?;

    let continuations = live_chat_continuation
        .get("continuations")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            PollFailure::SessionExpired(anyhow::anyhow!("No continuations in liveChatContinuation"))
        })?;

    let (next_continuation_token, timeout_millis) =
        parse_continuations(continuations).ok_or_else(|| {
            PollFailure::SessionExpired(anyhow::anyhow!("Failed to parse next continuation token"))
        })?;

    let actions = live_chat_continuation
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut messages = Vec::new();
    for action in actions {
        if let Some(msg) = parse_action_item(&action) {
            messages.push(msg);
        }
    }

    Ok(InnerTubeFetchResult {
        messages,
        next_continuation_token,
        timeout_millis: timeout_millis.unwrap_or(3000),
    })
}

pub fn parse_continuations(continuations: &[serde_json::Value]) -> Option<(String, Option<u64>)> {
    for item in continuations {
        let candidate = item
            .get("invalidationContinuationData")
            .or_else(|| item.get("timedContinuationData"))
            .or_else(|| item.get("reloadContinuationData"))
            .or_else(|| item.get("liveChatReplayContinuationData"));

        if let Some(data) = candidate
            && let Some(token) = data.get("continuation").and_then(|c| c.as_str())
        {
            let timeout = data.get("timeoutMs").and_then(|t| t.as_u64());
            return Some((token.to_string(), timeout));
        }
    }
    None
}

pub fn parse_action_item(action: &serde_json::Value) -> Option<IncomingChatMessage> {
    let item = action
        .pointer("/addChatItemAction/item")
        .or_else(|| action.pointer("/addLiveChatTickerItemAction/item"))
        .or_else(|| action.pointer("/replayChatItemAction/actions/0/addChatItemAction/item"))?;

    if let Some(renderer) = item.get("liveChatTextMessageRenderer") {
        let external_id = renderer
            .get("id")
            .and_then(|v| v.as_str())?
            .trim()
            .to_string();
        let author = renderer
            .get("authorName")
            .map(extract_runs)
            .filter(|a| !a.trim().is_empty())
            .unwrap_or_else(|| "YouTube viewer".to_string());
        let text = renderer
            .get("message")
            .map(extract_runs)
            .unwrap_or_default();
        let avatar_url = renderer.get("authorPhoto").and_then(extract_avatar);
        let sent_at = renderer
            .get("timestampUsec")
            .and_then(|v| v.as_str())
            .and_then(format_timestamp_usec);

        return Some(IncomingChatMessage {
            source: "youtube".into(),
            external_id,
            author,
            text,
            avatar_url,
            sent_at,
        });
    }

    if let Some(renderer) = item.get("liveChatPaidMessageRenderer") {
        let external_id = renderer
            .get("id")
            .and_then(|v| v.as_str())?
            .trim()
            .to_string();
        let author = renderer
            .get("authorName")
            .map(extract_runs)
            .filter(|a| !a.trim().is_empty())
            .unwrap_or_else(|| "YouTube viewer".to_string());
        let amount = renderer
            .pointer("/purchaseAmountText/simpleText")
            .and_then(|v| v.as_str())
            .unwrap_or("Super Chat");
        let body_text = renderer
            .get("message")
            .map(extract_runs)
            .unwrap_or_default();
        let text = if body_text.is_empty() {
            format!("[Super Chat {amount}]")
        } else {
            format!("[Super Chat {amount}] {body_text}")
        };
        let avatar_url = renderer.get("authorPhoto").and_then(extract_avatar);
        let sent_at = renderer
            .get("timestampUsec")
            .and_then(|v| v.as_str())
            .and_then(format_timestamp_usec);

        return Some(IncomingChatMessage {
            source: "youtube".into(),
            external_id,
            author,
            text,
            avatar_url,
            sent_at,
        });
    }

    if let Some(renderer) = item.get("liveChatMembershipItemRenderer") {
        let external_id = renderer
            .get("id")
            .and_then(|v| v.as_str())?
            .trim()
            .to_string();
        let author = renderer
            .get("authorName")
            .map(extract_runs)
            .filter(|a| !a.trim().is_empty())
            .unwrap_or_else(|| "YouTube viewer".to_string());
        let header = renderer
            .get("headerSubtext")
            .map(extract_runs)
            .or_else(|| renderer.get("headerPrimaryText").map(extract_runs))
            .unwrap_or_else(|| "Joined membership".to_string());
        let text = format!("[Member] {header}");
        let avatar_url = renderer.get("authorPhoto").and_then(extract_avatar);
        let sent_at = renderer
            .get("timestampUsec")
            .and_then(|v| v.as_str())
            .and_then(format_timestamp_usec);

        return Some(IncomingChatMessage {
            source: "youtube".into(),
            external_id,
            author,
            text,
            avatar_url,
            sent_at,
        });
    }

    if let Some(renderer) = item.get("liveChatPaidStickerRenderer") {
        let external_id = renderer
            .get("id")
            .and_then(|v| v.as_str())?
            .trim()
            .to_string();
        let author = renderer
            .get("authorName")
            .map(extract_runs)
            .filter(|a| !a.trim().is_empty())
            .unwrap_or_else(|| "YouTube viewer".to_string());
        let amount = renderer
            .pointer("/purchaseAmountText/simpleText")
            .and_then(|v| v.as_str())
            .unwrap_or("Super Sticker");
        let text = format!("[Super Sticker {amount}]");
        let avatar_url = renderer.get("authorPhoto").and_then(extract_avatar);
        let sent_at = renderer
            .get("timestampUsec")
            .and_then(|v| v.as_str())
            .and_then(format_timestamp_usec);

        return Some(IncomingChatMessage {
            source: "youtube".into(),
            external_id,
            author,
            text,
            avatar_url,
            sent_at,
        });
    }

    if let Some(vm) = item.get("giftMessageViewModel") {
        let external_id = vm.get("id").and_then(|v| v.as_str())?.trim().to_string();
        let author = vm
            .get("authorName")
            .map(extract_runs)
            .filter(|a| !a.trim().is_empty())
            .unwrap_or_else(|| "YouTube viewer".to_string());
        let text = vm.get("text").map(extract_runs).unwrap_or_default();
        let avatar_url = vm.get("authorAvatar").and_then(extract_avatar);

        return Some(IncomingChatMessage {
            source: "youtube".into(),
            external_id,
            author,
            text,
            avatar_url,
            sent_at: None,
        });
    }

    None
}

pub fn extract_runs(value: &serde_json::Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(s) = value.get("simpleText").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = value.get("content").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(runs) = value.get("runs").and_then(|v| v.as_array()) {
        let mut result = String::new();
        for run in runs {
            if let Some(text) = run.get("text").and_then(|t| t.as_str()) {
                result.push_str(text);
            } else if let Some(emoji) = run.get("emoji") {
                if let Some(emoji_id) = emoji.get("emojiId").and_then(|e| e.as_str()) {
                    result.push_str(emoji_id);
                } else if let Some(shortcut) =
                    emoji.pointer("/shortcuts/0").and_then(|s| s.as_str())
                {
                    result.push_str(shortcut);
                }
            }
        }
        return result;
    }
    String::new()
}

pub fn extract_avatar(value: &serde_json::Value) -> Option<String> {
    if let Some(thumbnails) = value.get("thumbnails").and_then(|v| v.as_array()) {
        return thumbnails
            .last()
            .or_else(|| thumbnails.first())
            .and_then(|t| t.get("url"))
            .and_then(|u| u.as_str())
            .map(|s| s.to_string());
    }
    if let Some(sources) = value
        .pointer("/avatarViewModel/image/sources")
        .and_then(|v| v.as_array())
    {
        return sources
            .last()
            .or_else(|| sources.first())
            .and_then(|t| t.get("url"))
            .and_then(|u| u.as_str())
            .map(|s| s.to_string());
    }
    None
}

fn format_timestamp_usec(ts_str: &str) -> Option<String> {
    let usec = ts_str.parse::<i128>().ok()?;
    let nanos = usec.checked_mul(1_000)?;
    time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .ok()
        .and_then(|dt| {
            dt.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_various_channel_inputs() {
        assert_eq!(
            normalize_channel_url("LofiGirl"),
            "https://www.youtube.com/@LofiGirl/live"
        );
        assert_eq!(
            normalize_channel_url("@LofiGirl"),
            "https://www.youtube.com/@LofiGirl/live"
        );
        assert_eq!(
            normalize_channel_url("UCkszU2WH9gy1mb0dV-11UJg"),
            "https://www.youtube.com/channel/UCkszU2WH9gy1mb0dV-11UJg/live"
        );
        assert_eq!(
            normalize_channel_url("https://www.youtube.com/@LofiGirl"),
            "https://www.youtube.com/@LofiGirl/live"
        );
    }

    #[test]
    fn extracts_video_ids_from_urls_and_raw_strings() {
        assert_eq!(
            extract_video_id("rFZHOHl-L8A").as_deref(),
            Some("rFZHOHl-L8A")
        );
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=rFZHOHl-L8A").as_deref(),
            Some("rFZHOHl-L8A")
        );
        assert_eq!(
            extract_video_id("https://youtu.be/rFZHOHl-L8A").as_deref(),
            Some("rFZHOHl-L8A")
        );
        assert_eq!(extract_video_id("invalid-length-id"), None);
    }

    #[test]
    fn parses_innertube_text_message() {
        let action = serde_json::json!({
            "addChatItemAction": {
                "item": {
                    "liveChatTextMessageRenderer": {
                        "id": "msg-123",
                        "authorName": {
                            "simpleText": "@Alice"
                        },
                        "authorPhoto": {
                            "thumbnails": [{ "url": "https://example.com/avatar.jpg" }]
                        },
                        "message": {
                            "runs": [
                                { "text": "Hello world " },
                                { "emoji": { "emojiId": "😊" } }
                            ]
                        },
                        "timestampUsec": "1788101242184427"
                    }
                }
            }
        });

        let parsed = parse_action_item(&action).expect("should parse message");
        assert_eq!(parsed.source, "youtube");
        assert_eq!(parsed.external_id, "msg-123");
        assert_eq!(parsed.author, "@Alice");
        assert_eq!(parsed.text, "Hello world 😊");
        assert_eq!(
            parsed.avatar_url.as_deref(),
            Some("https://example.com/avatar.jpg")
        );
        assert!(parsed.sent_at.is_some());
    }

    #[test]
    fn parses_superchat_and_gift_messages() {
        let superchat = serde_json::json!({
            "addChatItemAction": {
                "item": {
                    "liveChatPaidMessageRenderer": {
                        "id": "superchat-456",
                        "authorName": { "simpleText": "Supporter" },
                        "purchaseAmountText": { "simpleText": "$10.00" },
                        "message": { "runs": [{ "text": "Keep it up!" }] },
                        "timestampUsec": "1788101242184427"
                    }
                }
            }
        });

        let parsed = parse_action_item(&superchat).expect("should parse superchat");
        assert_eq!(parsed.text, "[Super Chat $10.00] Keep it up!");

        let gift = serde_json::json!({
            "addChatItemAction": {
                "item": {
                    "giftMessageViewModel": {
                        "id": "gift-789",
                        "authorName": { "content": "@Generous" },
                        "text": { "content": "sent Sparkles" }
                    }
                }
            }
        });

        let parsed_gift = parse_action_item(&gift).expect("should parse gift");
        assert_eq!(parsed_gift.author, "@Generous");
        assert_eq!(parsed_gift.text, "sent Sparkles");
    }

    #[test]
    fn parses_continuations_token_and_timeout() {
        let continuations = vec![serde_json::json!({
            "invalidationContinuationData": {
                "continuation": "token_abc123",
                "timeoutMs": 4500
            }
        })];

        let (token, timeout) =
            parse_continuations(&continuations).expect("should extract continuation");
        assert_eq!(token, "token_abc123");
        assert_eq!(timeout, Some(4500));
    }
}
