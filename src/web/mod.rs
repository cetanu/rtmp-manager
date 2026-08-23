use crate::chat::{ChatInboxSnapshot, EnqueueOutcome, IncomingChatMessage};
use crate::config::{AppConfig, ConfigForm, TargetConfig};
use crate::server::state::ProxyState;
use crate::util::secure_token_matches;
use futures_util::StreamExt;
use serde::Deserialize;
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
            <main class="mx-auto max-w-7xl px-3 py-3 sm:px-4 sm:py-4">
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

    if form.is_empty() {
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

    let mut config_write = state.config.write().await;
    let mut updated = match config_write.merge_form(form) {
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
        if let Err(e) = state.config_store.save(&updated).await {
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
    let imported = match AppConfig::parse_imported(&body) {
        Ok(config) => config,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    let state: &Arc<ProxyState> = app_context(cx);
    let mut config_write = state.config.write().await;
    let changed = imported != *config_write;
    let chat_changed = imported.chat != config_write.chat;
    if changed {
        if let Err(error) = state.config_store.save(&imported).await {
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
    let imported = match AppConfig::parse_imported(&config_bytes) {
        Ok(config) => config,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    let state: &Arc<ProxyState> = app_context(cx);
    let mut config_write = state.config.write().await;
    let changed = imported != *config_write;
    let chat_changed = imported.chat != config_write.chat;
    if changed {
        if let Err(error) = state.config_store.save(&imported).await {
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
    let snapshot = state.chat.snapshot().await?;
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
    if !state.chat.acknowledge(request.id).await? {
        return topcoat::router::IntoResponse::into_response(
            (
                topcoat::router::StatusCode::CONFLICT,
                "The displayed chat message has already changed",
            ),
            cx,
        );
    }

    topcoat::router::IntoResponse::into_response(Json(state.chat.snapshot().await?), cx)
}

#[route(GET "/api/events")]
async fn server_events(
    cx: &Cx,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent>> + use<>>> {
    let state: &Arc<ProxyState> = app_context(cx);
    let state = Arc::clone(state);
    let chat_changes = state.chat.subscribe_changes();
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
    if !bearer_token(cx).is_some_and(|submitted| secure_token_matches(expected_token, submitted)) {
        return Err(unauthorized().into());
    }

    let outcome = match state.chat.enqueue(message).await {
        Ok(EnqueueOutcome::Accepted) => "accepted",
        Ok(EnqueueOutcome::Duplicate) => "duplicate",
        Ok(EnqueueOutcome::Dropped) => "dropped",
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };
    let response = ChatIngestResponse {
        outcome,
        inbox: state.chat.snapshot().await?,
    };

    topcoat::router::IntoResponse::into_response(
        (topcoat::router::StatusCode::ACCEPTED, Json(response)),
        cx,
    )
}
