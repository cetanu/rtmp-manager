use futures_util::StreamExt;
use rtmp_core::config::ConfigForm;
use rtmp_core::server::state::{AppHandle, StreamState, StreamStatus};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use topcoat::asset::{Asset, AssetBundle, RouterBuilderAssetExt, asset};
use topcoat::runtime::PrefetchMode;
use topcoat::tailwind::stylesheet;
use topcoat::{
    Result,
    context::{Cx, context},
    router::{
        Router, RouterBuilderDiscoverExt,
        content::{
            Json,
            multipart::Multipart,
            sse::{Event as SseEvent, KeepAlive, Sse},
        },
        error::{bad_request, internal_server_error, not_found},
        page, parse_query_params,
        request::{Bytes, headers},
        response::{IntoResponse, Response},
        route,
    },
    view::{View, component, view},
};

mod alerts;
pub mod auth;
pub mod components;
use components::{
    actions_panel::start_test_stream,
    app_navigation::app_navigation,
    chat_inbox::chat_inbox,
    chat_overlay::{chat_overlay_page, render_chat_overlay_messages},
    config_transfer::config_transfer,
    configuration_form::configuration_form,
    log_viewer::log_viewer,
    metrics::metrics_page,
    stream_preview::stream_preview,
    webhook_audit::webhook_audit,
};

pub(crate) const TAILWIND_STYLESHEET: Asset = stylesheet!();
pub(crate) const FAVICON: Asset = asset!("rtmp.png");
pub(crate) const OVERLAY_EVENTS_SCRIPT: Asset = asset!("static/overlay-events.js");
pub(crate) const HLS_PLAYER_SCRIPT: Asset = asset!("static/hls.min.js");
pub(crate) const STREAM_PREVIEW_SCRIPT: Asset = asset!("static/stream-preview.js");
pub(crate) const METRICS_CHARTS_SCRIPT: Asset = asset!("static/metrics-charts.js");
pub(crate) const CONNECTION_STATUS_SCRIPT: Asset = asset!("static/connection-status.js");
pub(crate) const ERROR_TOAST_SCRIPT: Asset = asset!("static/error-toast.js");
const MAX_WEBHOOK_SIZE: usize = 128 * 1024;
const MAX_CONFIG_BODY_SIZE: usize = 1024 * 1024;
const MAX_CHAT_TEST_BODY_SIZE: usize = 64 * 1024;
const MAX_POMODORO_BODY_SIZE: usize = 16 * 1024;
const MAX_POLL_BODY_SIZE: usize = 16 * 1024;

pub async fn run_web_server(
    app_handle: AppHandle,
    addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    let sampler_metrics = Arc::clone(&app_handle.metrics);
    let mut stream_status = app_handle.stream.subscribe_status();
    let app = Router::builder()
        .discover()
        .with(
            topcoat::metadata::Metadata::new()
                .description("Configuration dashboard for the RTMP Stream Multiplexer.")
                .theme_color("#0b0e14"),
        )
        .assets(AssetBundle::load()?)
        .with(app_handle)
        .layer(topcoat::runtime::RuntimeLayer::new())
        .with(PrefetchMode::Never)
        .build();

    tokio::spawn(async move {
        let mut sample_timer = Box::pin(tokio::time::sleep(metrics_sample_interval(
            *stream_status.borrow(),
        )));
        loop {
            tokio::select! {
                _ = &mut sample_timer => {
                    sampler_metrics.record_sample();
                    sample_timer.as_mut().reset(
                        tokio::time::Instant::now()
                            + metrics_sample_interval(*stream_status.borrow()),
                    );
                }
                changed = stream_status.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    sample_timer.as_mut().reset(
                        tokio::time::Instant::now()
                            + metrics_sample_interval(*stream_status.borrow()),
                    );
                }
            }
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

fn metrics_sample_interval(status: StreamStatus) -> Duration {
    if status.state == StreamState::Offline {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(1)
    }
}

#[component]
async fn app_page(cx: &Cx, active_page: &'static str) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en" class="dark">
            <head>
                <meta charset="UTF-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1.0" />
                topcoat::metadata::tags()
                <link rel="icon" type="image/png" href=(FAVICON) />
                <link rel="preconnect" href="https://fonts.googleapis.com" />
                <link
                    rel="preconnect"
                    href="https://fonts.gstatic.com"
                    crossorigin="crossorigin"
                />
                <link
                    href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600;700&display=swap"
                    rel="stylesheet"
                />
                <link rel="stylesheet" href=(TAILWIND_STYLESHEET) />
                topcoat::dev::script()
                topcoat::runtime::script()
                <script src=(HLS_PLAYER_SCRIPT) defer="defer"></script>
                <script src=(STREAM_PREVIEW_SCRIPT) defer="defer"></script>
                <script src=(METRICS_CHARTS_SCRIPT) defer="defer"></script>
                <script src=(CONNECTION_STATUS_SCRIPT) defer="defer"></script>
                <script src=(ERROR_TOAST_SCRIPT) defer="defer"></script>
                <script src=(alerts::SCRIPT) defer="defer"></script>
            </head>
            <body
                data-page=(active_page)
                class="min-h-screen bg-background font-sans text-foreground antialiased selection:bg-signal/30"
            >
                <error-toaster
                    class="fixed top-4 right-4 z-50 flex max-w-sm w-full flex-col gap-2 pointer-events-none px-3"
                    style="position: fixed !important; top: 1rem !important; right: 1rem !important; z-index: 9999 !important; display: flex !important; flex-direction: column !important; gap: 0.5rem !important; pointer-events: none !important; max-width: 24rem !important; width: calc(100% - 2rem) !important;"
                ></error-toaster>
                app_navigation(active_page: active_page)
                <main class="mx-auto w-full max-w-[1600px] px-4 py-4 sm:px-5">
                    match active_page {
                        "preview" => {
                            stream_preview()
                        }
                        "metrics" => {
                            metrics_page()
                        }
                        "chat" => {
                            chat_inbox()
                            crate::web::components::chat_automations::chat_automations()
                        }
                        "media" => {
                            crate::web::components::automation_media::automation_media()
                        }
                        "logs" => {
                            log_viewer()
                            webhook_audit()
                        }
                        "settings" | "targets" => {
                            configuration_form(active_page: active_page)
                        }
                        "export" => {
                            config_transfer()
                        }
                        _ => {

                        }
                    }
                </main>
            </body>
        </html>
    })
}

#[page("/", with(topcoat::metadata::Metadata::new().title("Preview · RTMP Manager")))]
async fn home(cx: &Cx) -> Result<impl View> {
    Ok(view! { app_page(active_page: "preview") })
}

#[page(
    "/overview",
    with(topcoat::metadata::Metadata::new().title("Preview · RTMP Manager"))
)]
async fn overview_page(cx: &Cx) -> Result<impl View> {
    Ok(view! { app_page(active_page: "preview") })
}

#[page(
    "/preview",
    with(topcoat::metadata::Metadata::new().title("Preview · RTMP Manager"))
)]
async fn preview_page(cx: &Cx) -> Result<impl View> {
    Ok(view! { app_page(active_page: "preview") })
}

#[page(
    "/metrics",
    with(topcoat::metadata::Metadata::new().title("Metrics · RTMP Manager"))
)]
async fn metrics_page_route(cx: &Cx) -> Result<impl View> {
    Ok(view! { app_page(active_page: "metrics") })
}

#[page("/chat", with(topcoat::metadata::Metadata::new().title("Chat · RTMP Manager")))]
async fn chat_page(cx: &Cx) -> Result<impl View> {
    Ok(view! { app_page(active_page: "chat") })
}

#[page("/media", with(topcoat::metadata::Metadata::new().title("Media · RTMP Manager")))]
async fn media_page(cx: &Cx) -> Result<impl View> {
    Ok(view! { app_page(active_page: "media") })
}

#[page("/overlay/chat")]
async fn chat_overlay_route(cx: &Cx) -> Result<impl View> {
    Ok(view! { chat_overlay_page() })
}

#[page("/logs", with(topcoat::metadata::Metadata::new().title("Logs · RTMP Manager")))]
async fn logs_page(cx: &Cx) -> Result<impl View> {
    Ok(view! { app_page(active_page: "logs") })
}

#[page(
    "/settings",
    with(topcoat::metadata::Metadata::new().title("Settings · RTMP Manager"))
)]
async fn settings_page(cx: &Cx) -> Result<impl View> {
    Ok(view! { app_page(active_page: "settings") })
}

#[page(
    "/targets",
    with(topcoat::metadata::Metadata::new().title("Targets · RTMP Manager"))
)]
async fn targets_page(cx: &Cx) -> Result<impl View> {
    Ok(view! { app_page(active_page: "targets") })
}

#[page(
    "/export",
    with(topcoat::metadata::Metadata::new().title("Export · RTMP Manager"))
)]
async fn export_page(cx: &Cx) -> Result<impl View> {
    Ok(view! { app_page(active_page: "export") })
}

topcoat::router::path_param!(preview_file);

#[route(GET "/api/preview/{preview_file}")]
async fn get_preview_file(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = context(cx);
    let name = preview_file(cx);
    let Some(path) = app.stream.preview_file(name) else {
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
    let mut response = Response::new(topcoat::router::Body::from(bytes));
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
async fn get_stream_status(cx: &Cx) -> Result<Json<StreamStatus>> {
    let app: &AppHandle = context(cx);
    Ok(Json(app.stream.status()))
}

#[derive(Debug, Deserialize)]
struct AcknowledgeChatMessage {
    id: u64,
}

#[route(POST "/api/config", with(topcoat::router::BodyLimit::max(MAX_CONFIG_BODY_SIZE)))]
async fn update_config(cx: &Cx, body: Bytes) -> Result<Response> {
    let app: &AppHandle = context(cx);

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

    let return_to = match form.return_to.as_deref() {
        Some("/targets") => "/targets",
        _ => "/settings",
    };

    let redirect = (
        topcoat::router::StatusCode::SEE_OTHER,
        [(topcoat::router::header::LOCATION, return_to)],
    );

    let (_, _changed, chat_changed) = match app.config.save_form(form).await {
        Ok(res) => res,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    if chat_changed && let Err(error) = app.apply_chat_config().await {
        tracing::error!("Failed to apply chat configuration: {error:#}");
        return Err(internal_server_error(topcoat::Error::from_anyhow(error)).into());
    }

    redirect.into_response(cx)
}

#[route(
    POST "/api/test-stream",
    with(topcoat::router::BodyLimit::max(MAX_POLL_BODY_SIZE))
)]
async fn run_test_stream(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = context(cx);
    let error = start_test_stream(app);
    if !error.is_empty() {
        return Err(bad_request(error).into());
    }

    (
        topcoat::router::StatusCode::SEE_OTHER,
        [(topcoat::router::header::LOCATION, "/metrics")],
    )
        .into_response(cx)
}

#[route(GET "/api/config", with(topcoat::router::BodyLimit::max(MAX_CONFIG_BODY_SIZE)))]
async fn get_config(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = context(cx);
    (
        [(topcoat::router::header::CACHE_CONTROL, "no-store, private")],
        Json(app.config.get().redacted()),
    )
        .into_response(cx)
}

#[route(
    POST "/api/config/import",
    with(topcoat::router::BodyLimit::max(MAX_CONFIG_BODY_SIZE))
)]
async fn import_config(cx: &Cx, body: Bytes) -> Result<Response> {
    let app: &AppHandle = context(cx);
    let (_, _changed, chat_changed) = match app.config.import(&body).await {
        Ok(res) => res,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    if chat_changed && let Err(error) = app.apply_chat_config().await {
        return Err(internal_server_error(topcoat::Error::from_anyhow(error)).into());
    }

    topcoat::router::StatusCode::NO_CONTENT.into_response(cx)
}

#[route(
    POST "/api/config/import-file",
    with(topcoat::router::BodyLimit::max(MAX_CONFIG_BODY_SIZE))
)]
async fn import_config_file(cx: &Cx, mut multipart: Multipart) -> Result<Response> {
    let mut config_bytes = None;
    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("config_file") {
            let bytes = field.bytes().await?;
            config_bytes = Some(bytes);
            break;
        }
    }
    let config_bytes =
        config_bytes.ok_or_else(|| bad_request("The form did not contain a config_file upload"))?;

    let app: &AppHandle = context(cx);
    let (_, _changed, chat_changed) = match app.config.import(&config_bytes).await {
        Ok(res) => res,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    if chat_changed && let Err(error) = app.apply_chat_config().await {
        return Err(internal_server_error(topcoat::Error::from_anyhow(error)).into());
    }

    redirect_to(cx, "/export")
}

fn redirect_to(cx: &Cx, location: &'static str) -> Result<Response> {
    (
        topcoat::router::StatusCode::SEE_OTHER,
        [(topcoat::router::header::LOCATION, location)],
    )
        .into_response(cx)
}

#[route(GET "/api/chat")]
async fn get_chat_inbox(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = context(cx);
    let snapshot = app
        .chat
        .snapshot()
        .await
        .map_err(topcoat::Error::from_anyhow)?;
    (
        [(topcoat::router::header::CACHE_CONTROL, "no-store")],
        Json(snapshot),
    )
        .into_response(cx)
}

#[route(
    POST "/api/chat/acknowledge",
    with(topcoat::router::BodyLimit::max(MAX_POLL_BODY_SIZE))
)]
async fn acknowledge_chat_message(
    cx: &Cx,
    Json(request): Json<AcknowledgeChatMessage>,
) -> Result<Response> {
    let app: &AppHandle = context(cx);
    if !app
        .chat
        .acknowledge(request.id)
        .await
        .map_err(topcoat::Error::from_anyhow)?
    {
        return (
            topcoat::router::StatusCode::CONFLICT,
            "The displayed chat message has already changed",
        )
            .into_response(cx);
    }

    Json(
        app.chat
            .snapshot()
            .await
            .map_err(topcoat::Error::from_anyhow)?,
    )
    .into_response(cx)
}

#[route(
    POST "/api/chat/test",
    with(topcoat::router::BodyLimit::max(MAX_CHAT_TEST_BODY_SIZE))
)]
async fn send_test_chat_message(cx: &Cx, body: Bytes) -> Result<Response> {
    let app: &AppHandle = context(cx);
    let request = if body.is_empty() {
        None
    } else {
        match serde_json::from_slice::<rtmp_core::chat::TestChatMessageRequest>(&body) {
            Ok(req) => Some(req),
            Err(error) => return Err(bad_request(format!("Invalid JSON payload: {error}")).into()),
        }
    };
    app.chat
        .enqueue_test(request)
        .await
        .map_err(topcoat::Error::from_anyhow)
        .map_err(internal_server_error)?;
    Json(
        app.chat
            .snapshot()
            .await
            .map_err(topcoat::Error::from_anyhow)?,
    )
    .into_response(cx)
}

#[derive(Debug, Deserialize)]
struct StartPomodoroRequest {
    #[serde(default)]
    duration_minutes: Option<u64>,
    #[serde(default)]
    duration_secs: Option<u64>,
    #[serde(default)]
    message: Option<String>,
}

#[route(
    POST "/api/chat/pomodoro",
    with(topcoat::router::BodyLimit::max(MAX_POMODORO_BODY_SIZE))
)]
async fn start_chat_pomodoro(cx: &Cx, body: Bytes) -> Result<Response> {
    let app: &AppHandle = context(cx);
    let request: StartPomodoroRequest = if body.is_empty() {
        StartPomodoroRequest {
            duration_minutes: None,
            duration_secs: None,
            message: None,
        }
    } else {
        match serde_json::from_slice(&body) {
            Ok(req) => req,
            Err(error) => return Err(bad_request(format!("Invalid JSON payload: {error}")).into()),
        }
    };
    let duration_secs = request.duration_secs.unwrap_or_else(|| {
        request
            .duration_minutes
            .unwrap_or(rtmp_core::chat::POMODORO_MIN_DURATION_SECS / 60)
            .saturating_mul(60)
    });
    let message = request.message.unwrap_or_default();
    let snapshot = app
        .chat
        .start_pomodoro(message, duration_secs)
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    Json(snapshot).into_response(cx)
}

#[route(
    DELETE "/api/chat/pomodoro",
    with(topcoat::router::BodyLimit::max(MAX_POMODORO_BODY_SIZE))
)]
async fn stop_chat_pomodoro(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = context(cx);
    Json(
        app.chat
            .stop_pomodoro()
            .await
            .map_err(topcoat::Error::from_anyhow)?,
    )
    .into_response(cx)
}

#[derive(Debug, Deserialize)]
struct StartPollRequest {
    question: String,
    options: Vec<String>,
    #[serde(default)]
    results_seconds: Option<u64>,
}

#[route(POST "/api/chat/poll", with(topcoat::router::BodyLimit::max(MAX_POLL_BODY_SIZE)))]
async fn start_chat_poll(cx: &Cx, Json(request): Json<StartPollRequest>) -> Result<Response> {
    let app: &AppHandle = context(cx);
    let results_seconds = request
        .results_seconds
        .unwrap_or(app.config.get().chat.poll_results_seconds);
    let snapshot = app
        .chat
        .start_poll(request.question, request.options, results_seconds)
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    Json(snapshot).into_response(cx)
}

#[route(
    DELETE "/api/chat/poll",
    with(topcoat::router::BodyLimit::max(MAX_POLL_BODY_SIZE))
)]
async fn stop_chat_poll(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = context(cx);
    Json(
        app.chat
            .stop_poll()
            .await
            .map_err(topcoat::Error::from_anyhow)?,
    )
    .into_response(cx)
}

#[route(
    DELETE "/api/chat/poll/clear",
    with(topcoat::router::BodyLimit::max(MAX_POLL_BODY_SIZE))
)]
async fn clear_chat_poll(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = context(cx);
    Json(
        app.chat
            .clear_poll()
            .await
            .map_err(topcoat::Error::from_anyhow)?,
    )
    .into_response(cx)
}

#[route(GET "/api/metrics/events")]
async fn metrics_events(
    cx: &Cx,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent>> + use<>>> {
    let app: &AppHandle = context(cx);
    let samples = app.metrics.subscribe();
    let initial = futures_util::stream::once(std::future::ready(
        SseEvent::new()
            .event("metrics_history")
            .json_data(&app.metrics.history()),
    ));
    let changes = futures_util::stream::unfold(samples, |mut samples| async move {
        samples.changed().await.ok()?;
        let event = samples
            .borrow_and_update()
            .as_ref()
            .map(|sample| SseEvent::new().event("metrics_sample").json_data(sample))
            .unwrap_or_else(|| Ok(SseEvent::new().event("metrics_sample").data("null")));
        Some((event, samples))
    });
    Ok(Sse::new(initial.chain(changes)).keep_alive(KeepAlive::new()))
}

#[route(GET "/api/overlay/events")]
async fn overlay_events(
    cx: &Cx,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent>> + use<>>> {
    let app: &AppHandle = context(cx);
    let chat = app.chat.clone();
    let chat_changes = app.chat.subscribe_changes();
    let initial_revision = chat_changes.borrow().revision;
    let changes = futures_util::stream::unfold(
        (chat, chat_changes, initial_revision),
        |(chat, mut chat_changes, mut last_revision)| async move {
            loop {
                if chat_changes.changed().await.is_err() {
                    return None;
                }
                let revision = chat_changes.borrow().revision;
                if revision == last_revision {
                    continue;
                }
                last_revision = revision;
                let event = match chat.snapshot().await {
                    Ok(snapshot) => {
                        match render_chat_overlay_messages(
                            snapshot.messages,
                            snapshot.pomodoro,
                            snapshot.poll,
                            snapshot.queued,
                        )
                        .await
                        {
                            Ok(html) => Ok(SseEvent::new().event("chat").data(html)),
                            Err(error) => {
                                tracing::warn!("Failed to render chat overlay update: {error:#}");
                                Ok(SseEvent::new().event("chat_error").data("unavailable"))
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!("Failed to snapshot chat for overlay update: {error:#}");
                        Ok(SseEvent::new().event("chat_error").data("unavailable"))
                    }
                };
                return Some((event, (chat, chat_changes, last_revision)));
            }
        },
    );

    Ok(Sse::new(changes).keep_alive(KeepAlive::new()))
}

#[route(POST "/api/webhook", with(topcoat::router::BodyLimit::max(MAX_WEBHOOK_SIZE)))]
async fn receive_webhook(cx: &Cx, body: Bytes) -> Result<Response> {
    let app: &AppHandle = context(cx);
    app.metrics.add_webhooks_received(1);
    if body.len() > MAX_WEBHOOK_SIZE {
        app.metrics.add_webhooks_rejected(1);
        return Err(bad_request("Webhook body exceeds 128 KiB").into());
    }

    let headers = headers(cx)
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_ascii_lowercase(), value.to_owned()))
        })
        .collect();
    let body_bytes = body.len();
    let event = rtmp_core::chat::util::WebhookEvent {
        headers,
        body: body.to_vec(),
    };
    let platform = Platform::from_event(&event);
    let settings = app.config.get().chat.clone();
    let Some(platform) = platform else {
        app.metrics.add_webhooks_rejected(1);
        tracing::warn!("Rejected webhook without a recognized platform signature");
        return Err(bad_request("Webhook signature is missing").into());
    };
    let message = match platform {
        Platform::Kick => match rtmp_core::chat::kick::process_event(&settings, &event) {
            Ok(message) => Some(message),
            Err(error) => {
                app.metrics.add_webhooks_rejected(1);
                tracing::warn!("Rejected Kick webhook: {error:#}");
                return Err(bad_request("Rejected Kick webhook").into());
            }
        },
        Platform::X => match rtmp_core::chat::x::process_event(&settings, &event) {
            Ok(message) => message,
            Err(error) => {
                app.metrics.add_webhooks_rejected(1);
                tracing::warn!("Rejected X webhook: {error:#}");
                return Err(bad_request("Rejected X webhook").into());
            }
        },
    };
    if let Some(message) = message {
        let outcome = app
            .chat
            .enqueue(message)
            .await
            .map_err(topcoat::Error::from_anyhow)
            .map_err(internal_server_error)?;
        match outcome {
            rtmp_core::chat::EnqueueOutcome::Accepted
            | rtmp_core::chat::EnqueueOutcome::Dropped => {
                app.webhook_audit.record(
                    platform.as_str(),
                    event.header("content-type"),
                    &event.body,
                );
            }
            rtmp_core::chat::EnqueueOutcome::Duplicate => {
                tracing::info!(
                    platform = platform.as_str(),
                    "Ignored duplicate webhook event"
                );
            }
        }
    } else {
        app.webhook_audit
            .record(platform.as_str(), event.header("content-type"), &event.body);
    }
    tracing::info!(platform = platform.as_str(), body_bytes, "Webhook accepted");
    topcoat::router::StatusCode::OK.into_response(cx)
}

#[derive(Clone, Copy)]
enum Platform {
    Kick,
    X,
}

impl Platform {
    fn from_event(event: &rtmp_core::chat::util::WebhookEvent) -> Option<Self> {
        if event.header("kick-event-signature").is_some() {
            Some(Self::Kick)
        } else if event.header("x-twitter-webhooks-signature").is_some() {
            Some(Self::X)
        } else {
            None
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Kick => "kick",
            Self::X => "x",
        }
    }
}

#[derive(Deserialize)]
struct WebhookCrcQuery {
    crc_token: String,
}

#[derive(Serialize)]
struct WebhookCrcResponse {
    response_token: String,
}

#[route(GET "/api/webhook", with(topcoat::router::BodyLimit::max(MAX_WEBHOOK_SIZE)))]
async fn verify_webhook_crc(cx: &Cx) -> Result<Response> {
    let query: WebhookCrcQuery =
        parse_query_params(cx).map_err(|_| bad_request("Missing crc_token"))?;
    if query.crc_token.is_empty() {
        return Err(bad_request("Missing crc_token").into());
    }
    let app: &AppHandle = context(cx);
    let config = app.config.get();
    let secret = config
        .chat
        .x_api_secret
        .as_deref()
        .filter(|secret| !secret.is_empty())
        .ok_or_else(|| bad_request("X API secret key is not configured"))?;
    let response_token = rtmp_core::chat::x::response_token(&query.crc_token, secret)
        .map_err(topcoat::Error::from_anyhow)
        .map_err(internal_server_error)?;
    Json(WebhookCrcResponse { response_token }).into_response(cx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};
    use topcoat::asset::{Bundler, BundlerConfig};

    static TEST_PORT_COUNTER: AtomicU64 = AtomicU64::new(45000);
    static ASSET_BUNDLE: OnceLock<AssetBundle> = OnceLock::new();

    fn test_asset_bundle() -> AssetBundle {
        ASSET_BUNDLE
            .get_or_init(|| {
                let test_exe =
                    std::env::current_exe().expect("test executable path should be available");
                let binary = std::fs::read(test_exe).expect("test executable should be readable");
                let bundle_dir = std::env::temp_dir()
                    .join(format!("rtmp-proxy-test-assets-{}", std::process::id()));
                Bundler::new(&BundlerConfig::new())
                    .bundle(&binary, &bundle_dir)
                    .expect("test assets should bundle");
                AssetBundle::load_dir(bundle_dir).expect("test asset bundle should load")
            })
            .clone()
    }

    #[tokio::test]
    async fn overlay_routes_are_registered_and_serve_html() {
        let temp_dir = std::env::temp_dir().join(format!(
            "rtmp-overlay-test-{}-{}",
            std::process::id(),
            TEST_PORT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_path = temp_dir.join("config.sqlite3");
        let client = reqwest::Client::new();
        let (config_handle, _config) = rtmp_core::config::ConfigHandle::open(&config_path)
            .await
            .unwrap();
        let metrics = Arc::new(rtmp_core::metrics::Metrics::default());
        let app_handle = AppHandle::new(metrics, config_handle, client.clone(), 1935)
            .await
            .unwrap();

        let app = Router::builder()
            .discover()
            .assets(test_asset_bundle())
            .with(app_handle.clone())
            .layer(topcoat::runtime::RuntimeLayer::new())
            .build();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let _ = topcoat::serve(listener, app).await;
        });

        let mut overlay_config = app_handle.config.get().as_ref().clone();
        overlay_config.web_auth.overlay_token = "overlay-token-for-tests".into();
        app_handle
            .config
            .import(&serde_json::to_vec(&overlay_config).unwrap())
            .await
            .unwrap();

        app_handle
            .chat
            .enqueue(rtmp_core::chat::IncomingChatMessage {
                source: rtmp_core::chat::Source::Twitch,
                external_id: "test1".into(),
                author: "TestViewer".into(),
                user_id: None,
                text: "OBS overlay test message".into(),
                parts: Vec::new(),
                avatar_url: None,
                sent_at: None,
            })
            .await
            .unwrap();

        let resp1 = client
            .get(format!(
                "http://{local_addr}/overlay/chat?key=overlay-token-for-tests"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(resp1.status(), reqwest::StatusCode::OK);
        let body1 = resp1.text().await.unwrap();
        assert!(body1.contains("RTMP-Manager Chat Overlay"));
        assert!(body1.contains("TestViewer"));
        assert!(body1.contains("OBS overlay test message"));
        assert!(body1.contains("data-source=\"twitch\""));

        let overlay_without_key = client
            .get(format!("http://{local_addr}/overlay/chat"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            overlay_without_key.status(),
            reqwest::StatusCode::UNAUTHORIZED
        );

        let mut authed_config = app_handle.config.get().as_ref().clone();
        authed_config.web_auth.username = "admin".into();
        authed_config.web_auth.password = "secretpassword123".into();
        app_handle
            .config
            .import(&serde_json::to_vec(&authed_config).unwrap())
            .await
            .unwrap();

        let unauthed_dashboard = client
            .get(format!("http://{local_addr}/chat"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            unauthed_dashboard.status(),
            reqwest::StatusCode::UNAUTHORIZED
        );

        let overlay_no_auth = client
            .get(format!("http://{local_addr}/overlay/chat"))
            .send()
            .await
            .unwrap();
        assert_eq!(overlay_no_auth.status(), reqwest::StatusCode::UNAUTHORIZED);
        let overlay_body = client
            .get(format!(
                "http://{local_addr}/overlay/chat?key=overlay-token-for-tests"
            ))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(overlay_body.contains("RTMP-Manager Chat Overlay"));
        assert!(overlay_body.contains("TestViewer"));

        let chat_api = client
            .get(format!(
                "http://{local_addr}/api/chat?key=overlay-token-for-tests"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(chat_api.status(), reqwest::StatusCode::UNAUTHORIZED);

        let shared_events = client
            .get(format!(
                "http://{local_addr}/api/metrics/events?key=overlay-token-for-tests"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(shared_events.status(), reqwest::StatusCode::UNAUTHORIZED);

        let overlay_events_response = client
            .get(format!(
                "http://{local_addr}/api/overlay/events?key=overlay-token-for-tests"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(overlay_events_response.status(), reqwest::StatusCode::OK);

        for path in ["/overlay/alerts", "/api/alerts/events"] {
            let denied = client
                .get(format!("http://{local_addr}{path}"))
                .send()
                .await
                .unwrap();
            assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
            let allowed = client
                .get(format!(
                    "http://{local_addr}{path}?key=overlay-token-for-tests"
                ))
                .send()
                .await
                .unwrap();
            assert_eq!(allowed.status(), reqwest::StatusCode::OK);
        }
        let upload_body = "--media-test\r\nContent-Disposition: form-data; name=\"media\"; filename=\"winner.gif\"\r\nContent-Type: image/gif\r\n\r\nGIF89aexample\r\n--media-test--\r\n";
        let denied = client
            .post(format!(
                "http://{local_addr}/api/alerts/media?key=overlay-token-for-tests"
            ))
            .header("Content-Type", "multipart/form-data; boundary=media-test")
            .body(upload_body)
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
        let uploaded = client
            .post(format!("http://{local_addr}/api/alerts/media"))
            .basic_auth("admin", Some("secretpassword123"))
            .header("Content-Type", "multipart/form-data; boundary=media-test")
            .body(upload_body)
            .send()
            .await
            .unwrap();
        assert_eq!(uploaded.status(), reqwest::StatusCode::OK);
        let repeat_url =
            format!("http://{local_addr}/api/alerts/media/repeats?name=winner.gif&allow=false");
        let denied = client
            .post(&repeat_url)
            .query(&[("key", "overlay-token-for-tests")])
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
        let updated = client
            .post(&repeat_url)
            .basic_auth("admin", Some("secretpassword123"))
            .send()
            .await
            .unwrap();
        assert_eq!(updated.status(), reqwest::StatusCode::OK);
        assert!(updated.text().await.unwrap().contains("Repeats blocked"));
        for auth_with_token in [true, false] {
            let mut request = client.get(format!(
                "http://{local_addr}/api/alerts/media?name=winner.gif"
            ));
            if auth_with_token {
                request = request.query(&[("key", "overlay-token-for-tests")]);
            } else {
                request = request.basic_auth("admin", Some("secretpassword123"));
            }
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::OK);
            assert_eq!(response.headers()["content-type"], "image/gif");
            assert_eq!(response.bytes().await.unwrap().as_ref(), b"GIF89aexample");
        }
        let denied = client
            .get(format!(
                "http://{local_addr}/api/alerts/media?name=winner.gif"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
        let library = client
            .get(format!("http://{local_addr}/media"))
            .basic_auth("admin", Some("secretpassword123"))
            .send()
            .await
            .unwrap();
        assert!(library.text().await.unwrap().contains("winner.gif"));

        server_task.abort();
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn pomodoro_focus_survives_page_refresh() {
        let temp_dir = std::env::temp_dir().join(format!(
            "rtmp-pomodoro-refresh-{}-{}",
            std::process::id(),
            TEST_PORT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_path = temp_dir.join("config.sqlite3");
        let client = reqwest::Client::new();
        let (config_handle, _config) = rtmp_core::config::ConfigHandle::open(&config_path)
            .await
            .unwrap();
        let metrics = Arc::new(rtmp_core::metrics::Metrics::default());
        let app_handle = AppHandle::new(metrics, config_handle, client.clone(), 1935)
            .await
            .unwrap();

        let app = Router::builder()
            .discover()
            .assets(test_asset_bundle())
            .with(app_handle.clone())
            .layer(topcoat::runtime::RuntimeLayer::new())
            .build();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let _ = topcoat::serve(listener, app).await;
        });

        let start = client
            .post(format!("http://{local_addr}/api/chat/pomodoro"))
            .header("Content-Type", "application/json")
            .body(r#"{"duration_minutes":25,"message":"Deep work"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(start.status(), reqwest::StatusCode::OK);

        let first_load = client
            .get(format!("http://{local_addr}/chat"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            first_load.contains("chat-pomodoro-status"),
            "first load should render the focus panel"
        );

        let second_load = client
            .get(format!("http://{local_addr}/chat"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            second_load.contains("chat-pomodoro-status"),
            "refresh must preserve focus mode"
        );

        server_task.abort();
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn test_chat_endpoint_and_acknowledge_flow() {
        let temp_dir = std::env::temp_dir().join(format!(
            "rtmp-chat-test-{}-{}",
            std::process::id(),
            TEST_PORT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_path = temp_dir.join("config.sqlite3");
        let client = reqwest::Client::new();
        let (config_handle, _config) = rtmp_core::config::ConfigHandle::open(&config_path)
            .await
            .unwrap();
        let metrics = Arc::new(rtmp_core::metrics::Metrics::default());
        let app_handle = AppHandle::new(metrics, config_handle, client.clone(), 1935)
            .await
            .unwrap();

        let app = Router::builder()
            .discover()
            .assets(test_asset_bundle())
            .with(app_handle.clone())
            .layer(topcoat::runtime::RuntimeLayer::new())
            .build();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let _ = topcoat::serve(listener, app).await;
        });

        for path_and_limit in [
            ("/api/config", MAX_CONFIG_BODY_SIZE),
            ("/api/config/import", MAX_CONFIG_BODY_SIZE),
            ("/api/chat/test", MAX_CHAT_TEST_BODY_SIZE),
        ] {
            let response = client
                .post(format!("http://{local_addr}{}", path_and_limit.0))
                .body(vec![b'x'; path_and_limit.1 + 1])
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);
        }

        let html_content = client
            .get(format!("http://{local_addr}/chat"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        assert!(html_content.contains("<title>Chat · RTMP Manager</title>"));
        assert!(html_content.contains("chat-test-button"));
        assert!(html_content.contains("/api/chat/test-message"));
        let resp = client
            .post(format!("http://{local_addr}/api/chat/test"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let snapshot: rtmp_core::chat::ChatInboxSnapshot = resp.json().await.unwrap();
        assert_eq!(snapshot.messages.len(), 1);
        let message_id = snapshot.messages[0].id;

        let custom_resp = client
            .post(format!("http://{local_addr}/api/chat/test"))
            .header("Content-Type", "application/json")
            .body(r#"{"source":"twitch","author":"CustomTester","text":"Testing custom text"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(custom_resp.status(), reqwest::StatusCode::OK);
        let snapshot: rtmp_core::chat::ChatInboxSnapshot = custom_resp.json().await.unwrap();
        assert_eq!(snapshot.messages.len(), 2);

        let ack_resp = client
            .post(format!("http://{local_addr}/api/chat/acknowledge"))
            .header("Content-Type", "application/json")
            .body(format!(r#"{{"id":{message_id}}}"#))
            .send()
            .await
            .unwrap();
        assert_eq!(ack_resp.status(), reqwest::StatusCode::OK);
        let snapshot_after_ack: rtmp_core::chat::ChatInboxSnapshot = ack_resp.json().await.unwrap();
        assert_eq!(snapshot_after_ack.messages.len(), 1);
        assert_eq!(snapshot_after_ack.messages[0].author, "CustomTester");
        assert_eq!(snapshot_after_ack.messages[0].text, "Testing custom text");

        let proc_resp = client
            .post(format!("http://{local_addr}/api/chat/test-message"))
            .header("Content-Type", "application/json")
            .body("[]")
            .send()
            .await
            .unwrap();
        assert_eq!(proc_resp.status(), reqwest::StatusCode::OK);

        server_task.abort();
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
