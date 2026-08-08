use crate::chat::{ChatInboxSnapshot, EnqueueOutcome, IncomingChatMessage};
use crate::config::{
    AppConfig, ChatSettings, NotificationSettings, ServerSettings, TargetConfig, WebAuthSettings,
};
use crate::server::state::ProxyState;
use anyhow::Context;
use futures_util::StreamExt;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use topcoat::asset::{AssetBundle, RouterBuilderAssetExt};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        Router, RouterBuilderDiscoverExt,
        content::{
            Json,
            multipart::Multipart,
            sse::{Event as SseEvent, KeepAlive, Sse},
        },
        error::{bad_request, internal_server_error, not_found, unauthorized},
        page, route,
    },
    view::{component, view},
};

pub mod auth;
pub mod components;
use components::{
    app_navigation::app_navigation, chat_inbox::chat_inbox, config_transfer::config_transfer,
    configuration_form::configuration_form, log_viewer::log_viewer, metrics::metrics_page,
    stream_preview::stream_preview,
};

pub(crate) const TAILWIND_STYLESHEET: topcoat::asset::Asset = topcoat::tailwind::stylesheet!();
pub(crate) const CHAT_EVENTS_SCRIPT: topcoat::asset::Asset =
    topcoat::asset::asset!("static/chat-events.js");
pub(crate) const HLS_PLAYER_SCRIPT: topcoat::asset::Asset =
    topcoat::asset::asset!("static/hls.min.js");
pub(crate) const HLS_PLAYER_LICENSE: topcoat::asset::Asset =
    topcoat::asset::asset!("static/hls.LICENSE.txt");
pub(crate) const STREAM_PREVIEW_SCRIPT: topcoat::asset::Asset =
    topcoat::asset::asset!("static/stream-preview.js");
pub(crate) const APP_NAVIGATION_SCRIPT: topcoat::asset::Asset =
    topcoat::asset::asset!("static/app-navigation.js");
pub(crate) const LOG_VIEWER_SCRIPT: topcoat::asset::Asset =
    topcoat::asset::asset!("static/log-viewer.js");
pub(crate) const METRICS_CHARTS_SCRIPT: topcoat::asset::Asset =
    topcoat::asset::asset!("static/metrics-charts.js");
pub(crate) const SECRET_FIELDS_SCRIPT: topcoat::asset::Asset =
    topcoat::asset::asset!("static/secret-fields.js");

pub async fn run_web_server(
    state: Arc<ProxyState>,
    addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    let sampler_metrics = Arc::clone(&state.metrics);
    let app = Router::builder()
        .discover()
        .assets(AssetBundle::load()?)
        .app_context(state)
        .build();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            sampler_metrics.record_sample();
        }
    });

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(
        "Web interface listening on http://{}",
        listener.local_addr()?
    );
    topcoat::serve(listener, app).await?;
    Ok(())
}

#[component]
async fn app_page(active_page: &'static str) -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="en" class="dark">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1.0" />
            <title>"RTMP-Manager"</title>
            <meta name="description" content="Configuration dashboard for the RTMP Stream Multiplexer." />
            <link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700&display=swap" rel="stylesheet" />
            <link rel="stylesheet" href=(TAILWIND_STYLESHEET) />
            topcoat::runtime::script()
            <script src=(HLS_PLAYER_SCRIPT) defer="defer"></script>
            <script src=(STREAM_PREVIEW_SCRIPT) defer="defer"></script>
            <script src=(CHAT_EVENTS_SCRIPT) defer="defer"></script>
            <script src=(APP_NAVIGATION_SCRIPT) defer="defer"></script>
            <script src=(LOG_VIEWER_SCRIPT) defer="defer"></script>
            <script src=(METRICS_CHARTS_SCRIPT) defer="defer"></script>
            <script src=(SECRET_FIELDS_SCRIPT) defer="defer"></script>
        </head>
        <body class="min-h-screen bg-background text-foreground font-sans antialiased">
            app_navigation(active_page: active_page)
            <main class="mx-auto max-w-7xl px-4 py-8 sm:py-10">
                <section data-app-page="preview" hidden=(active_page != "preview")>
                    stream_preview()
                </section>
                <section data-app-page="metrics" hidden=(active_page != "metrics")>
                    metrics_page()
                </section>
                <section data-app-page="chat" hidden=(active_page != "chat")>
                    chat_inbox()
                </section>
                <section data-app-page="logs" hidden=(active_page != "logs")>
                    log_viewer()
                </section>
                configuration_form(active_page: active_page)
                <section data-app-page="export" hidden=(active_page != "export")>
                    config_transfer()
                </section>
            </main>
        </body>
        </html>
    }
}

#[page("/")]
async fn home() -> Result {
    view! { app_page(active_page: "preview") }
}

#[page("/overview")]
async fn overview_page() -> Result {
    view! { app_page(active_page: "preview") }
}

#[page("/preview")]
async fn preview_page() -> Result {
    view! { app_page(active_page: "preview") }
}

#[page("/metrics")]
async fn metrics_page_route() -> Result {
    view! { app_page(active_page: "metrics") }
}

#[page("/chat")]
async fn chat_page() -> Result {
    view! { app_page(active_page: "chat") }
}

#[page("/logs")]
async fn logs_page() -> Result {
    view! { app_page(active_page: "logs") }
}

#[page("/settings")]
async fn settings_page() -> Result {
    view! { app_page(active_page: "settings") }
}

#[page("/targets")]
async fn targets_page() -> Result {
    view! { app_page(active_page: "targets") }
}

#[page("/export")]
async fn export_page() -> Result {
    view! { app_page(active_page: "export") }
}

#[topcoat::router::path_param]
struct PreviewFile(str);

#[route(GET "/api/preview/{preview_file}")]
async fn get_preview_file(cx: &Cx) -> Result<topcoat::router::Response> {
    let state: &Arc<ProxyState> = app_context(cx);
    let name = topcoat::router::path_param::<PreviewFile>(cx);
    let Some(path) = state.preview_file(name) else {
        return Err(not_found().into());
    };
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(not_found().into());
        }
        Err(error) => return Err(internal_server_error(error).into()),
    };
    let content_type = if name.ends_with(".m3u8") {
        "application/vnd.apple.mpegurl"
    } else {
        "video/mp2t"
    };
    let mut response = topcoat::router::Response::new(topcoat::router::Body::from(bytes));
    response.headers_mut().insert(
        topcoat::router::header::CONTENT_TYPE,
        topcoat::router::HeaderValue::from_static(content_type),
    );
    response.headers_mut().insert(
        topcoat::router::header::CACHE_CONTROL,
        topcoat::router::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

#[route(GET "/api/stream/status")]
async fn get_stream_status(cx: &Cx) -> Result<Json<crate::server::state::StreamStatus>> {
    let state: &Arc<ProxyState> = app_context(cx);
    Ok(Json(state.stream_status().await))
}

#[route(GET "/api/metrics/history")]
async fn get_metrics_history(cx: &Cx) -> Result<Json<Vec<crate::metrics::MetricsSample>>> {
    let state: &Arc<ProxyState> = app_context(cx);
    Ok(Json(state.metrics.history()))
}

#[derive(Debug, Deserialize)]
struct AcknowledgeChatMessage {
    id: u64,
}

#[derive(Debug, serde::Serialize)]
struct ChatIngestResponse {
    outcome: &'static str,
    inbox: ChatInboxSnapshot,
}

fn bearer_token(cx: &Cx) -> Option<&str> {
    topcoat::router::headers(cx)
        .get(topcoat::router::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

fn token_matches(expected: &str, submitted: &str) -> bool {
    if expected.len() != submitted.len() {
        return false;
    }

    expected
        .bytes()
        .zip(submitted.bytes())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn parse_imported_config(body: &[u8]) -> anyhow::Result<AppConfig> {
    let value: serde_json::Value =
        serde_json::from_slice(body).context("Invalid JSON configuration")?;
    let object = value
        .as_object()
        .context("The JSON configuration must be an object")?;

    for field in ["server", "notifications", "targets", "web_auth", "chat"] {
        if !object.contains_key(field) {
            anyhow::bail!("JSON configuration is missing required field '{field}'");
        }
    }

    let config: AppConfig =
        serde_json::from_value(value).context("Invalid configuration structure")?;
    config.validate()?;
    Ok(config)
}

#[derive(Debug, Default, Deserialize)]
struct ServerForm {
    listen: Option<String>,
    health_listen: Option<String>,
    api_listen: Option<String>,
    test_stream_duration_secs: Option<u64>,
    ingest_stream_key: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct NotificationsForm {
    discord_webhook: Option<String>,
    #[serde(default)]
    clear_discord_webhook: bool,
    live_message: Option<String>,
    webhook_url: Option<String>,
    #[serde(default)]
    clear_webhook_url: bool,
}

#[derive(Debug, Default, Deserialize)]
struct WebAuthForm {
    username: Option<String>,
    password: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatForm {
    ingest_token: Option<String>,
    #[serde(default)]
    clear_ingest_token: bool,
    queue_capacity: Option<usize>,
    twitch_channel: Option<String>,
    youtube_api_key: Option<String>,
    #[serde(default)]
    clear_youtube_api_key: bool,
    youtube_live_chat_id: Option<String>,
    youtube_video_id: Option<String>,
    youtube_channel_id: Option<String>,
    youtube_min_poll_interval_secs: Option<u64>,
    #[serde(default)]
    youtube_adaptive_polling: bool,
}

#[derive(Debug, Deserialize)]
struct TargetForm {
    name: String,
    url: String,
    #[serde(default)]
    stream_key: Option<String>,
    public_url: Option<String>,
    #[serde(default)]
    enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigForm {
    pub server: Option<ServerForm>,
    pub web_auth: Option<WebAuthForm>,
    pub chat: Option<ChatForm>,
    pub notifications: Option<NotificationsForm>,
    pub targets: Option<Vec<TargetForm>>,
    pub action: Option<String>,
    pub return_to: Option<String>,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn updated_secret(
    submitted: Option<String>,
    clear: bool,
    current: Option<String>,
) -> Option<String> {
    if clear {
        None
    } else {
        non_empty(submitted).or(current)
    }
}

fn merge_form(current: &AppConfig, form: ConfigForm) -> anyhow::Result<AppConfig> {
    let mut config = current.clone();

    if let Some(server) = form.server {
        config.server = ServerSettings {
            listen: parse_address(server.listen, config.server.listen, "RTMP listen")?,
            health_listen: parse_address(
                server.health_listen,
                config.server.health_listen,
                "health listen",
            )?,
            api_listen: parse_address(server.api_listen, config.server.api_listen, "API listen")?,
            test_stream_duration_secs: server
                .test_stream_duration_secs
                .unwrap_or(config.server.test_stream_duration_secs),
            ingest_stream_key: non_empty(server.ingest_stream_key)
                .unwrap_or(config.server.ingest_stream_key),
        };
    }
    if let Some(notification_fields) = form.notifications {
        config.notifications = NotificationSettings {
            discord_webhook: updated_secret(
                notification_fields.discord_webhook,
                notification_fields.clear_discord_webhook,
                config.notifications.discord_webhook,
            ),
            live_message: notification_fields
                .live_message
                .unwrap_or(config.notifications.live_message),
            webhook_url: updated_secret(
                notification_fields.webhook_url,
                notification_fields.clear_webhook_url,
                config.notifications.webhook_url,
            ),
        };
    }
    if let Some(auth_fields) = form.web_auth {
        config.web_auth = WebAuthSettings {
            username: auth_fields
                .username
                .unwrap_or(config.web_auth.username)
                .trim()
                .to_string(),
            password: non_empty(auth_fields.password).unwrap_or(config.web_auth.password),
        };
    }
    if let Some(chat) = form.chat {
        config.chat = ChatSettings {
            ingest_token: updated_secret(
                chat.ingest_token,
                chat.clear_ingest_token,
                config.chat.ingest_token,
            ),
            queue_capacity: chat.queue_capacity.unwrap_or(config.chat.queue_capacity),
            twitch_channel: non_empty(chat.twitch_channel)
                .map(|channel| channel.trim_start_matches('#').to_ascii_lowercase()),
            youtube_api_key: updated_secret(
                chat.youtube_api_key,
                chat.clear_youtube_api_key,
                config.chat.youtube_api_key,
            ),
            youtube_live_chat_id: non_empty(chat.youtube_live_chat_id),
            youtube_video_id: non_empty(chat.youtube_video_id),
            youtube_channel_id: non_empty(chat.youtube_channel_id),
            youtube_min_poll_interval_secs: chat
                .youtube_min_poll_interval_secs
                .unwrap_or(config.chat.youtube_min_poll_interval_secs),
            youtube_adaptive_polling: chat.youtube_adaptive_polling,
        };
    }
    if let Some(target_fields) = form.targets {
        config.targets = target_fields
            .into_iter()
            .enumerate()
            .map(|(index, target)| TargetConfig {
                name: target.name,
                url: target.url,
                stream_key: non_empty(target.stream_key).unwrap_or_else(|| {
                    config
                        .targets
                        .get(index)
                        .map(|target| target.stream_key.clone())
                        .unwrap_or_default()
                }),
                public_url: non_empty(target.public_url),
                enabled: target.enabled,
            })
            .collect();
    }

    Ok(config)
}

fn parse_address(
    submitted: Option<String>,
    current: SocketAddr,
    field: &str,
) -> anyhow::Result<SocketAddr> {
    submitted
        .map(|value| {
            value
                .parse()
                .with_context(|| format!("Invalid {field} address"))
        })
        .transpose()
        .map(|value| value.unwrap_or(current))
}

#[route(POST "/api/config")]
async fn update_config(cx: &Cx, body: topcoat::router::Bytes) -> Result<topcoat::router::Response> {
    let state: &Arc<ProxyState> = app_context(cx);

    let form: ConfigForm = match serde_qs::Config::new()
        .use_form_encoding(true)
        .deserialize_bytes(&body)
    {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Failed to parse form: {}", e);
            return Err(bad_request("Invalid form data").into());
        }
    };

    if form.server.is_none()
        && form.web_auth.is_none()
        && form.chat.is_none()
        && form.notifications.is_none()
        && form.targets.is_none()
    {
        tracing::error!("Config form contained no recognized configuration fields");
        return Err(
            bad_request("No configuration fields were recognized; nothing was saved").into(),
        );
    }

    let action = form.action.clone().unwrap_or_default();
    let return_to = match form.return_to.as_deref() {
        Some("/targets") => "/targets",
        _ => "/settings",
    };

    let redirect = (
        topcoat::router::StatusCode::SEE_OTHER,
        [(topcoat::router::header::LOCATION, return_to)],
    );

    // Serialize read/merge/write so concurrent edits cannot both merge from the
    // same stale snapshot and silently overwrite one another.
    let mut config_write = state.config.write().await;
    let mut updated = match merge_form(&config_write, form) {
        Ok(config) => config,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    if action == "add_target" {
        updated.targets.push(TargetConfig {
            name: "New Target".to_string(),
            url: "".to_string(),
            stream_key: "".to_string(),
            public_url: None,
            enabled: false,
        });
    } else if action.starts_with("remove_target:")
        && let Some(idx_str) = action.split(':').nth(1)
        && let Ok(idx) = idx_str.parse::<usize>()
        && idx < updated.targets.len()
    {
        updated.targets.remove(idx);
    }

    if let Err(error) = updated.validate() {
        return Err(bad_request(error.to_string()).into());
    }

    let changed = updated != *config_write;
    let chat_changed = updated.chat != config_write.chat;
    if changed {
        if let Err(e) = state.config_store.save(&updated) {
            tracing::error!("Failed to save configuration to SQLite: {}", e);
            return Err(internal_server_error(e).into());
        }
        *config_write = updated;
    }
    drop(config_write);
    if chat_changed && let Err(error) = state.apply_chat_config().await {
        tracing::error!("Failed to apply chat configuration: {error:#}");
        return Err(internal_server_error(error).into());
    }
    topcoat::router::IntoResponse::into_response(redirect, cx)
}

#[route(GET "/api/config")]
async fn get_config(cx: &Cx) -> Result<topcoat::router::Response> {
    let state: &Arc<ProxyState> = app_context(cx);
    topcoat::router::IntoResponse::into_response(
        (
            [(topcoat::router::header::CACHE_CONTROL, "no-store, private")],
            Json(state.config.read().await.clone()),
        ),
        cx,
    )
}

#[route(POST "/api/config/import")]
async fn import_config(cx: &Cx, body: topcoat::router::Bytes) -> Result<topcoat::router::Response> {
    let imported = match parse_imported_config(&body) {
        Ok(config) => config,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    let state: &Arc<ProxyState> = app_context(cx);
    let mut config_write = state.config.write().await;
    let changed = imported != *config_write;
    let chat_changed = imported.chat != config_write.chat;
    if changed {
        if let Err(error) = state.config_store.save(&imported) {
            tracing::error!("Failed to import configuration into SQLite: {}", error);
            return Err(internal_server_error(error).into());
        }
        *config_write = imported;
    }
    drop(config_write);
    if chat_changed {
        state.apply_chat_config().await?;
    }

    topcoat::router::IntoResponse::into_response(topcoat::router::StatusCode::NO_CONTENT, cx)
}

#[route(POST "/api/config/import-file")]
async fn import_config_file(
    cx: &Cx,
    mut multipart: Multipart,
) -> Result<topcoat::router::Response> {
    const MAX_CONFIG_SIZE: usize = 1024 * 1024;

    let mut config_bytes = None;
    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("config_file") {
            let bytes = field.bytes().await?;
            if bytes.len() > MAX_CONFIG_SIZE {
                return Err(bad_request("JSON configuration must be no larger than 1 MiB").into());
            }
            config_bytes = Some(bytes);
            break;
        }
    }
    let config_bytes =
        config_bytes.ok_or_else(|| bad_request("The form did not contain a config_file upload"))?;
    let imported = match parse_imported_config(&config_bytes) {
        Ok(config) => config,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    let state: &Arc<ProxyState> = app_context(cx);
    let mut config_write = state.config.write().await;
    let changed = imported != *config_write;
    let chat_changed = imported.chat != config_write.chat;
    if changed {
        if let Err(error) = state.config_store.save(&imported) {
            tracing::error!("Failed to import configuration into SQLite: {}", error);
            return Err(internal_server_error(error).into());
        }
        *config_write = imported;
    }
    drop(config_write);
    if chat_changed {
        state.apply_chat_config().await?;
    }

    redirect_to(cx, "/export")
}

fn redirect_to(cx: &Cx, location: &'static str) -> Result<topcoat::router::Response> {
    topcoat::router::IntoResponse::into_response(
        (
            topcoat::router::StatusCode::SEE_OTHER,
            [(topcoat::router::header::LOCATION, location)],
        ),
        cx,
    )
}

#[route(GET "/api/chat")]
async fn get_chat_inbox(cx: &Cx) -> Result<topcoat::router::Response> {
    let state: &Arc<ProxyState> = app_context(cx);
    let snapshot = state.chat_inbox.lock().await.snapshot()?;
    topcoat::router::IntoResponse::into_response(
        (
            [(topcoat::router::header::CACHE_CONTROL, "no-store")],
            Json(snapshot),
        ),
        cx,
    )
}

#[route(POST "/api/chat/acknowledge")]
async fn acknowledge_chat_message(
    cx: &Cx,
    Json(request): Json<AcknowledgeChatMessage>,
) -> Result<topcoat::router::Response> {
    let state: &Arc<ProxyState> = app_context(cx);
    let mut inbox = state.chat_inbox.lock().await;
    if !inbox.acknowledge(request.id)? {
        return topcoat::router::IntoResponse::into_response(
            (
                topcoat::router::StatusCode::CONFLICT,
                "The displayed chat message has already changed",
            ),
            cx,
        );
    }
    state.notify_chat_changed();

    topcoat::router::IntoResponse::into_response(Json(inbox.snapshot()?), cx)
}

#[route(GET "/api/events")]
async fn server_events(
    cx: &Cx,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent>> + use<>>> {
    let state: &Arc<ProxyState> = app_context(cx);
    let state = Arc::clone(state);
    let chat_changes = state.subscribe_chat_changes();
    let status_interval = tokio::time::interval(std::time::Duration::from_millis(250));
    let last_status = state.stream_status().await;
    let initial_status = SseEvent::new()
        .event("stream_status")
        .json_data(&last_status)?;
    let initial_events = futures_util::stream::iter([
        Ok(initial_status),
        Ok(SseEvent::new().event("chat_changed").data("changed")),
    ]);

    let changes = futures_util::stream::unfold(
        (state, chat_changes, status_interval, last_status),
        |(state, mut chat_changes, mut status_interval, mut last_status)| async move {
            loop {
                tokio::select! {
                    _ = status_interval.tick() => {
                        let status = state.stream_status().await;
                        if status != last_status {
                            last_status = status;
                            let event = SseEvent::new()
                                .event("stream_status")
                                .json_data(&status);
                            return Some((event, (state, chat_changes, status_interval, last_status)));
                        }
                    }
                    changed = chat_changes.changed() => {
                        if changed.is_err() {
                            return None;
                        }
                        return Some((
                            Ok(SseEvent::new().event("chat_changed").data("changed")),
                            (state, chat_changes, status_interval, last_status),
                        ));
                    }
                }
            }
        },
    );

    Ok(Sse::new(initial_events.chain(changes)).keep_alive(KeepAlive::new()))
}

#[route(GET "/api/logs")]
async fn service_logs(
    _cx: &Cx,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent>> + use<>>> {
    let logs = crate::log_buffer::global();
    let receiver = logs.subscribe();
    let initial = futures_util::stream::iter(
        logs.snapshot()
            .into_iter()
            .map(|entry| SseEvent::new().event("log").json_data(&entry)),
    );
    let live = futures_util::stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(entry) => {
                    return Some((SseEvent::new().event("log").json_data(&entry), receiver));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Ok(Sse::new(initial.chain(live)).keep_alive(KeepAlive::new()))
}

#[route(POST "/api/chat/ingest")]
async fn ingest_chat_message(
    cx: &Cx,
    Json(message): Json<IncomingChatMessage>,
) -> Result<topcoat::router::Response> {
    let state: &Arc<ProxyState> = app_context(cx);
    let expected_token = state
        .config
        .read()
        .await
        .chat
        .ingest_token
        .clone()
        .filter(|value| !value.trim().is_empty());
    let Some(expected_token) = expected_token.as_deref() else {
        return topcoat::router::IntoResponse::into_response(
            (
                topcoat::router::StatusCode::SERVICE_UNAVAILABLE,
                "Chat ingest is disabled; configure a chat ingest token",
            ),
            cx,
        );
    };
    if !bearer_token(cx).is_some_and(|submitted| token_matches(expected_token, submitted)) {
        return Err(unauthorized().into());
    }

    let mut inbox = state.chat_inbox.lock().await;
    let outcome = match inbox.enqueue(message) {
        Ok(EnqueueOutcome::Accepted) => "accepted",
        Ok(EnqueueOutcome::Duplicate) => "duplicate",
        Ok(EnqueueOutcome::Dropped) => "dropped",
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };
    if outcome != "duplicate" {
        state.notify_chat_changed();
    }
    let response = ChatIngestResponse {
        outcome,
        inbox: inbox.snapshot()?,
    };

    topcoat::router::IntoResponse::into_response(
        (topcoat::router::StatusCode::ACCEPTED, Json(response)),
        cx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn populated_config() -> AppConfig {
        AppConfig {
            server: ServerSettings {
                listen: "0.0.0.0:1935".parse().unwrap(),
                health_listen: "127.0.0.1:8080".parse().unwrap(),
                api_listen: "10.0.0.1:3000".parse().unwrap(),
                test_stream_duration_secs: 15,
                ingest_stream_key: "existing-ingest-key".into(),
            },
            notifications: NotificationSettings {
                discord_webhook: Some("https://discord.test/hook".into()),
                live_message: "Still live".into(),
                webhook_url: Some("https://example.test/hook".into()),
            },
            targets: vec![TargetConfig {
                name: "Twitch".into(),
                url: "rtmps://example.test/app".into(),
                stream_key: "secret".into(),
                public_url: Some("https://example.test/watch".into()),
                enabled: true,
            }],
            web_auth: WebAuthSettings {
                username: "operator".into(),
                password: "correct horse battery staple".into(),
            },
            chat: ChatSettings {
                ingest_token: Some("generic-ingest-token".into()),
                twitch_channel: Some("streamer".into()),
                youtube_api_key: Some("youtube-api-key".into()),
                ..ChatSettings::default()
            },
        }
    }

    #[test]
    fn partial_form_update_preserves_every_omitted_field() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("server%5Blisten%5D=127.0.0.1%3A1936&action=save")
            .unwrap();
        let updated = merge_form(&populated_config(), form).unwrap();

        assert_eq!(updated.server.listen, "127.0.0.1:1936".parse().unwrap());
        assert_eq!(updated.server.api_listen, "10.0.0.1:3000".parse().unwrap());
        assert_eq!(updated.server.test_stream_duration_secs, 15);
        assert_eq!(updated.server.ingest_stream_key, "existing-ingest-key");
        assert_eq!(updated.notifications.live_message, "Still live");
        assert_eq!(updated.targets.len(), 1);
        assert_eq!(updated.targets[0].stream_key, "secret");
        assert_eq!(updated.web_auth.password, "correct horse battery staple");
        assert_eq!(
            updated.chat.ingest_token.as_deref(),
            Some("generic-ingest-token")
        );
    }

    #[test]
    fn test_stream_duration_is_configurable() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("server%5Btest_stream_duration_secs%5D=30&action=save")
            .unwrap();
        let updated = merge_form(&populated_config(), form).unwrap();

        assert_eq!(updated.server.test_stream_duration_secs, 30);
        updated.validate().unwrap();
    }

    #[test]
    fn blank_ingest_stream_key_preserves_existing_key() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("server%5Bingest_stream_key%5D=")
            .unwrap();
        let updated = merge_form(&populated_config(), form).unwrap();

        assert_eq!(updated.server.ingest_stream_key, "existing-ingest-key");
    }

    #[test]
    fn ingest_stream_key_is_configurable() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("server%5Bingest_stream_key%5D=new-private-key")
            .unwrap();
        let updated = merge_form(&populated_config(), form).unwrap();

        assert_eq!(updated.server.ingest_stream_key, "new-private-key");
    }

    #[test]
    fn unchecked_target_decodes_as_disabled() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "targets%5B0%5D%5Bname%5D=Twitch&\
                 targets%5B0%5D%5Burl%5D=rtmps%3A%2F%2Fexample.test%2Fapp&\
                 targets%5B0%5D%5Bstream_key%5D=secret&action=save",
            )
            .unwrap();
        let updated = merge_form(&populated_config(), form).unwrap();

        assert!(!updated.targets[0].enabled);
    }

    #[test]
    fn checked_target_decodes_as_enabled() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "targets%5B0%5D%5Bname%5D=Twitch&\
                 targets%5B0%5D%5Burl%5D=rtmps%3A%2F%2Fexample.test%2Fapp&\
                 targets%5B0%5D%5Bstream_key%5D=secret&\
                 targets%5B0%5D%5Benabled%5D=true&action=save",
            )
            .unwrap();
        let updated = merge_form(&populated_config(), form).unwrap();

        assert!(updated.targets[0].enabled);
        assert_eq!(updated.targets[0].stream_key, "secret");
    }

    #[test]
    fn blank_secret_fields_preserve_existing_secrets() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "notifications%5Blive_message%5D=Updated&\
                 notifications%5Bdiscord_webhook%5D=&\
                 notifications%5Bwebhook_url%5D=&\
                 targets%5B0%5D%5Bname%5D=Twitch&\
                 targets%5B0%5D%5Burl%5D=rtmps%3A%2F%2Fexample.test%2Fapp&\
                 targets%5B0%5D%5Bstream_key%5D=&action=save",
            )
            .unwrap();
        let updated = merge_form(&populated_config(), form).unwrap();

        assert_eq!(updated.targets[0].stream_key, "secret");
        assert_eq!(
            updated.notifications.discord_webhook.as_deref(),
            Some("https://discord.test/hook")
        );
        assert_eq!(
            updated.notifications.webhook_url.as_deref(),
            Some("https://example.test/hook")
        );
    }

    #[test]
    fn explicit_clear_removes_webhook_credentials() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "notifications%5Blive_message%5D=Updated&\
                 notifications%5Bclear_discord_webhook%5D=true&\
                 notifications%5Bclear_webhook_url%5D=true&action=save",
            )
            .unwrap();
        let updated = merge_form(&populated_config(), form).unwrap();

        assert!(updated.notifications.discord_webhook.is_none());
        assert!(updated.notifications.webhook_url.is_none());
    }

    #[test]
    fn blank_web_and_chat_secrets_preserve_existing_credentials() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "web_auth%5Busername%5D=operator&web_auth%5Bpassword%5D=&\
                 chat%5Bingest_token%5D=&chat%5Btwitch_channel%5D=streamer&\
                 chat%5Byoutube_api_key%5D=&chat%5Bqueue_capacity%5D=250&action=save",
            )
            .unwrap();
        let updated = merge_form(&populated_config(), form).unwrap();

        assert_eq!(updated.web_auth.password, "correct horse battery staple");
        assert_eq!(
            updated.chat.ingest_token.as_deref(),
            Some("generic-ingest-token")
        );
        assert_eq!(updated.chat.twitch_channel.as_deref(), Some("streamer"));
        assert_eq!(
            updated.chat.youtube_api_key.as_deref(),
            Some("youtube-api-key")
        );
        assert_eq!(updated.chat.queue_capacity, 250);
    }

    #[test]
    fn query_mode_does_not_decode_browser_form_keys() {
        let form: ConfigForm =
            serde_qs::from_str("server%5Blisten%5D=127.0.0.1%3A1936&action=save").unwrap();

        assert!(form.server.is_none());
    }

    #[test]
    fn exported_config_json_can_be_imported_without_losing_secrets() {
        let original = populated_config();
        let json = serde_json::to_vec_pretty(&original).unwrap();
        let imported = parse_imported_config(&json).unwrap();

        assert_eq!(imported.server.api_listen, original.server.api_listen);
        assert_eq!(
            imported.notifications.discord_webhook,
            original.notifications.discord_webhook
        );
        assert_eq!(imported.targets[0].stream_key, "secret");
        assert!(imported.targets[0].enabled);
    }

    #[test]
    fn import_rejects_incomplete_or_invalid_configs() {
        let incomplete = br#"{"server": {}, "targets": []}"#;
        assert!(
            parse_imported_config(incomplete)
                .unwrap_err()
                .to_string()
                .contains("notifications")
        );

        let legacy_export = br#"{"server": {}, "notifications": {}, "targets": []}"#;
        assert!(
            parse_imported_config(legacy_export)
                .unwrap_err()
                .to_string()
                .contains("web_auth")
        );

        let invalid_target = br#"{
            "server": {},
            "notifications": {},
            "web_auth": {},
            "chat": {},
            "targets": [{
                "name": "Twitch",
                "url": "https://example.test/app",
                "stream_key": "secret",
                "public_url": null,
                "enabled": true
            }]
        }"#;
        assert!(
            parse_imported_config(invalid_target)
                .unwrap_err()
                .to_string()
                .contains("invalid URL")
        );
    }
}

#[derive(serde::Serialize)]
struct WebMetrics {
    active_connections: u64,
    total_connections: u64,
    active_streams: usize,
    active_relays: usize,
}

#[route(GET "/api/metrics")]
async fn get_metrics(cx: &Cx) -> Result<Json<WebMetrics>> {
    let state: &Arc<ProxyState> = app_context(cx);
    let active_connections = state
        .metrics
        .active_connections
        .load(std::sync::atomic::Ordering::Relaxed);
    let total_connections = state
        .metrics
        .total_connections
        .load(std::sync::atomic::Ordering::Relaxed);

    let active_streams = usize::from(state.stream_status().await.active);
    let relays_guard = state.active_relays.lock().await;
    let active_relays = relays_guard.values().map(|v| v.len()).sum();
    drop(relays_guard);

    Ok(Json(WebMetrics {
        active_connections,
        total_connections,
        active_streams,
        active_relays,
    }))
}
