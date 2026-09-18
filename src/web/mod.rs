use crate::config::ConfigForm;
use crate::server::state::{AppHandle, StreamStatus};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use topcoat::asset::{Asset, AssetBundle, RouterBuilderAssetExt, asset};
use topcoat::runtime::RouterBuilderRuntimeExt;
use topcoat::tailwind::stylesheet;
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
        error::{bad_request, internal_server_error, not_found},
        page, parse_query_params,
        request::{Bytes, headers},
        response::{IntoResponse, Response},
        route,
    },
    view::{View, component, view},
};

pub mod auth;
pub mod components;
use components::{
    actions_panel::start_test_stream, app_navigation::app_navigation, chat_inbox::chat_inbox,
    chat_overlay::chat_overlay_page, config_transfer::config_transfer,
    configuration_form::configuration_form, log_viewer::log_viewer, metrics::metrics_page,
    stream_preview::stream_preview, webhook_audit::webhook_audit,
};

pub(crate) const TAILWIND_STYLESHEET: Asset = stylesheet!();
pub(crate) const FAVICON: Asset = asset!("rtmp.png");
pub(crate) const CHAT_EVENTS_SCRIPT: Asset = asset!("static/chat-events.js");
pub(crate) const HLS_PLAYER_SCRIPT: Asset = asset!("static/hls.min.js");
pub(crate) const STREAM_PREVIEW_SCRIPT: Asset = asset!("static/stream-preview.js");
pub(crate) const APP_NAVIGATION_SCRIPT: Asset = asset!("static/app-navigation.js");
pub(crate) const LOG_VIEWER_SCRIPT: Asset = asset!("static/log-viewer.js");
pub(crate) const METRICS_CHARTS_SCRIPT: Asset = asset!("static/metrics-charts.js");
pub(crate) const SECRET_FIELDS_SCRIPT: Asset = asset!("static/secret-fields.js");

pub async fn run_web_server(
    app_handle: AppHandle,
    addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    let sampler_metrics = Arc::clone(&app_handle.metrics);
    let app = Router::builder()
        .discover()
        .runtime()
        .assets(AssetBundle::load()?)
        .app_context(app_handle)
        .build();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
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
async fn app_page(active_page: &'static str) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en" class="dark">
            <head>
                <meta charset="UTF-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1.0" />
                <title>"RTMP-Manager"</title>
                <meta
                    name="description"
                    content="Configuration dashboard for the RTMP Stream Multiplexer."
                />
                <link rel="icon" type="image/png" href=(FAVICON) />
                <link
                    href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700&display=swap"
                    rel="stylesheet"
                />
                <link rel="stylesheet" href=(TAILWIND_STYLESHEET) />
                topcoat::runtime::script()
                <script src=(HLS_PLAYER_SCRIPT) defer="defer"></script>
                <script src=(STREAM_PREVIEW_SCRIPT) defer="defer"></script>
                <script src=(METRICS_CHARTS_SCRIPT) defer="defer"></script>
                <script src=(CHAT_EVENTS_SCRIPT) defer="defer"></script>
                <script src=(APP_NAVIGATION_SCRIPT) defer="defer"></script>
                <script src=(LOG_VIEWER_SCRIPT) defer="defer"></script>
                <script src=(SECRET_FIELDS_SCRIPT) defer="defer"></script>
            </head>
            <body
                class="min-h-screen bg-background text-foreground font-sans antialiased"
            >
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
                        webhook_audit()
                    </section>
                    configuration_form(active_page: active_page)
                    <section data-app-page="export" hidden=(active_page != "export")>
                        config_transfer()
                    </section>
                </main>
            </body>
        </html>
    })
}

#[page("/")]
async fn home() -> Result<impl View> {
    Ok(view! { app_page(active_page: "preview") })
}

#[page("/overview")]
async fn overview_page() -> Result<impl View> {
    Ok(view! { app_page(active_page: "preview") })
}

#[page("/preview")]
async fn preview_page() -> Result<impl View> {
    Ok(view! { app_page(active_page: "preview") })
}

#[page("/metrics")]
async fn metrics_page_route() -> Result<impl View> {
    Ok(view! { app_page(active_page: "metrics") })
}

#[page("/chat")]
async fn chat_page() -> Result<impl View> {
    Ok(view! { app_page(active_page: "chat") })
}

#[page("/overlay/chat")]
async fn chat_overlay_route() -> Result<impl View> {
    Ok(view! { chat_overlay_page() })
}

#[page("/chat/overlay")]
async fn chat_overlay_alias_route() -> Result<impl View> {
    Ok(view! { chat_overlay_page() })
}

#[page("/logs")]
async fn logs_page() -> Result<impl View> {
    Ok(view! { app_page(active_page: "logs") })
}

#[page("/settings")]
async fn settings_page() -> Result<impl View> {
    Ok(view! { app_page(active_page: "settings") })
}

#[page("/targets")]
async fn targets_page() -> Result<impl View> {
    Ok(view! { app_page(active_page: "targets") })
}

#[page("/export")]
async fn export_page() -> Result<impl View> {
    Ok(view! { app_page(active_page: "export") })
}

topcoat::router::path_param!(preview_file);

#[route(GET "/api/preview/{preview_file}")]
async fn get_preview_file(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = app_context(cx);
    let name = topcoat::router::path_param::<PreviewFile>(cx);
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
    let app: &AppHandle = app_context(cx);
    Ok(Json(app.stream.status()))
}

#[derive(Debug, Deserialize)]
struct AcknowledgeChatMessage {
    id: u64,
}

#[route(POST "/api/config")]
async fn update_config(cx: &Cx, body: Bytes) -> Result<Response> {
    let app: &AppHandle = app_context(cx);

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
        return Err(internal_server_error(error).into());
    }

    redirect.into_response(cx)
}

#[route(POST "/api/test-stream")]
async fn run_test_stream(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = app_context(cx);
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

#[route(GET "/api/config")]
async fn get_config(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = app_context(cx);
    (
        [(topcoat::router::header::CACHE_CONTROL, "no-store, private")],
        Json(app.config.get().as_ref().clone()),
    )
        .into_response(cx)
}

#[route(POST "/api/config/import")]
async fn import_config(cx: &Cx, body: Bytes) -> Result<Response> {
    let app: &AppHandle = app_context(cx);
    let (_, _changed, chat_changed) = match app.config.import(&body).await {
        Ok(res) => res,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    if chat_changed && let Err(error) = app.apply_chat_config().await {
        return Err(internal_server_error(error).into());
    }

    topcoat::router::StatusCode::NO_CONTENT.into_response(cx)
}

#[route(POST "/api/config/import-file")]
async fn import_config_file(cx: &Cx, mut multipart: Multipart) -> Result<Response> {
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

    let app: &AppHandle = app_context(cx);
    let (_, _changed, chat_changed) = match app.config.import(&config_bytes).await {
        Ok(res) => res,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    if chat_changed && let Err(error) = app.apply_chat_config().await {
        return Err(internal_server_error(error).into());
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
    let app: &AppHandle = app_context(cx);
    let snapshot = app.chat.snapshot().await?;
    (
        [(topcoat::router::header::CACHE_CONTROL, "no-store")],
        Json(snapshot),
    )
        .into_response(cx)
}

#[route(POST "/api/chat/acknowledge")]
async fn acknowledge_chat_message(
    cx: &Cx,
    Json(request): Json<AcknowledgeChatMessage>,
) -> Result<Response> {
    let app: &AppHandle = app_context(cx);
    if !app.chat.acknowledge(request.id).await? {
        return (
            topcoat::router::StatusCode::CONFLICT,
            "The displayed chat message has already changed",
        )
            .into_response(cx);
    }

    Json(app.chat.snapshot().await?).into_response(cx)
}

#[route(POST "/api/chat/test")]
async fn send_test_chat_message(cx: &Cx, body: Bytes) -> Result<Response> {
    let app: &AppHandle = app_context(cx);
    let request = if body.is_empty() {
        None
    } else {
        match serde_json::from_slice::<crate::chat::TestChatMessageRequest>(&body) {
            Ok(req) => Some(req),
            Err(error) => return Err(bad_request(format!("Invalid JSON payload: {error}")).into()),
        }
    };
    app.chat
        .enqueue_test(request)
        .await
        .map_err(internal_server_error)?;
    Json(app.chat.snapshot().await?).into_response(cx)
}

#[route(GET "/api/events")]
async fn server_events(
    cx: &Cx,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent>> + use<>>> {
    let app: &AppHandle = app_context(cx);
    let status_rx = app.stream.subscribe_status();
    let chat_changes = app.chat.subscribe_changes();
    let metric_samples = app.metrics.subscribe();

    let initial_status = SseEvent::new()
        .event("stream_status")
        .json_data(&*status_rx.borrow())?;
    let initial_events = futures_util::stream::iter([
        Ok(initial_status),
        Ok(SseEvent::new().event("chat_changed").data("changed")),
        SseEvent::new()
            .event("metrics_history")
            .json_data(&app.metrics.history()),
    ]);

    let changes = futures_util::stream::unfold(
        (status_rx, chat_changes, metric_samples),
        |(mut status_rx, mut chat_changes, mut metric_samples)| async move {
            tokio::select! {
                changed = status_rx.changed() => {
                    if changed.is_err() {
                        return None;
                    }
                    let status = *status_rx.borrow();
                    let event = SseEvent::new()
                        .event("stream_status")
                        .json_data(&status);
                    Some((event, (status_rx, chat_changes, metric_samples)))
                }
                changed = chat_changes.changed() => {
                    if changed.is_err() {
                        return None;
                    }
                    Some((
                        Ok(SseEvent::new().event("chat_changed").data("changed")),
                        (status_rx, chat_changes, metric_samples),
                    ))
                }
                changed = metric_samples.changed() => {
                    if changed.is_err() {
                        return None;
                    }
                    let event = metric_samples
                        .borrow()
                        .as_ref()
                        .map(|sample| SseEvent::new().event("metrics_sample").json_data(sample))
                        .unwrap_or_else(|| Ok(SseEvent::new().event("metrics_sample").data("null")));
                    Some((event, (status_rx, chat_changes, metric_samples)))
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

#[route(POST "/api/webhook")]
async fn receive_webhook(cx: &Cx, body: Bytes) -> Result<Response> {
    const MAX_WEBHOOK_SIZE: usize = 128 * 1024;
    let app: &AppHandle = app_context(cx);
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
    let event = crate::server::state::WebhookEvent { headers, body };
    let platform = if event.header("kick-event-signature").is_some() {
        "kick"
    } else if event.header("x-twitter-webhooks-signature").is_some() {
        "x"
    } else {
        "unknown"
    };
    app.webhook_audit
        .record(platform, event.header("content-type"), &event.body);
    if event.body.len() > MAX_WEBHOOK_SIZE {
        return Err(bad_request("Webhook body exceeds 128 KiB").into());
    }
    let settings = app.config.get().chat.clone();
    let platform = if event.header("kick-event-signature").is_some() {
        let message = match crate::chat::kick::process_event(&settings, &event) {
            Ok(message) => message,
            Err(error) => {
                tracing::warn!("Rejected Kick webhook: {error:#}");
                return Err(bad_request("Rejected Kick webhook").into());
            }
        };
        app.chat
            .enqueue(message)
            .await
            .map_err(internal_server_error)?;
        "kick"
    } else if event.header("x-twitter-webhooks-signature").is_some() {
        let message = match crate::chat::x::process_event(&settings, &event) {
            Ok(message) => message,
            Err(error) => {
                tracing::warn!("Rejected X webhook: {error:#}");
                return Err(bad_request("Rejected X webhook").into());
            }
        };
        if let Some(message) = message {
            app.chat
                .enqueue(message)
                .await
                .map_err(internal_server_error)?;
        }
        "x"
    } else {
        tracing::warn!("Rejected webhook without a recognized platform signature");
        return Err(bad_request("Webhook signature is missing").into());
    };
    tracing::info!(platform, body_bytes, "Webhook accepted");
    topcoat::router::StatusCode::OK.into_response(cx)
}

#[derive(Deserialize)]
struct WebhookCrcQuery {
    crc_token: String,
}

#[derive(Serialize)]
struct WebhookCrcResponse {
    response_token: String,
}

#[route(GET "/api/webhook")]
async fn verify_webhook_crc(cx: &Cx) -> Result<Response> {
    let query: WebhookCrcQuery =
        parse_query_params(cx).map_err(|_| bad_request("Missing crc_token"))?;
    if query.crc_token.is_empty() {
        return Err(bad_request("Missing crc_token").into());
    }
    let app: &AppHandle = app_context(cx);
    let config = app.config.get();
    let secret = config
        .chat
        .x_api_secret
        .as_deref()
        .filter(|secret| !secret.is_empty())
        .ok_or_else(|| bad_request("X API secret key is not configured"))?;
    let response_token =
        crate::chat::x::response_token(&query.crc_token, secret).map_err(internal_server_error)?;
    Json(WebhookCrcResponse { response_token }).into_response(cx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_PORT_COUNTER: AtomicU64 = AtomicU64::new(45000);
    static ASSET_BUNDLE: Once = Once::new();

    fn ensure_asset_bundle() {
        ASSET_BUNDLE.call_once(|| {
            let status = std::process::Command::new("topcoat")
                .args(["asset", "bundle", "--bin", "rtmp-proxy"])
                .status()
                .expect("topcoat CLI should be installed");
            assert!(status.success(), "topcoat asset bundle failed");
        });
    }

    #[tokio::test]
    async fn overlay_routes_are_registered_and_serve_html() {
        let temp_dir = std::env::temp_dir().join(format!(
            "rtmp-overlay-test-{}",
            TEST_PORT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_path = temp_dir.join("config.sqlite3");
        let client = reqwest::Client::new();
        let (config_handle, _config) = crate::config::ConfigHandle::open(&config_path)
            .await
            .unwrap();
        let metrics = Arc::new(crate::metrics::Metrics::default());
        let app_handle = AppHandle::new(metrics, config_handle, client.clone(), 1935)
            .await
            .unwrap();

        ensure_asset_bundle();

        let app = Router::builder()
            .discover()
            .runtime()
            .assets(AssetBundle::load().unwrap())
            .app_context(app_handle.clone())
            .build();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let _ = topcoat::serve(listener, app).await;
        });

        app_handle
            .chat
            .enqueue(crate::chat::IncomingChatMessage {
                source: "twitch".into(),
                external_id: "test1".into(),
                author: "TestViewer".into(),
                text: "OBS overlay test message".into(),
                avatar_url: None,
                sent_at: None,
            })
            .await
            .unwrap();

        let resp1 = client
            .get(format!("http://{local_addr}/overlay/chat"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp1.status(), reqwest::StatusCode::OK);
        let body1 = resp1.text().await.unwrap();
        assert!(body1.contains("RTMP-Manager Chat Overlay"));
        assert!(body1.contains("TestViewer"));
        assert!(body1.contains("OBS overlay test message"));
        assert!(body1.contains("data-source=\"twitch\""));

        let resp2 = client
            .get(format!("http://{local_addr}/chat/overlay"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp2.status(), reqwest::StatusCode::OK);
        let body2 = resp2.text().await.unwrap();
        assert!(body2.contains("RTMP-Manager Chat Overlay"));
        assert!(body2.contains("TestViewer"));

        // Verify web auth with query token works
        let mut authed_config = app_handle.config.get().as_ref().clone();
        authed_config.web_auth.username = "admin".into();
        authed_config.web_auth.password = "secretpassword123".into();
        app_handle
            .config
            .import(&serde_json::to_vec(&authed_config).unwrap())
            .await
            .unwrap();

        // Dashboard routes (/chat, /settings) require authentication
        let unauthed_dashboard = client
            .get(format!("http://{local_addr}/chat"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            unauthed_dashboard.status(),
            reqwest::StatusCode::UNAUTHORIZED
        );

        // Overlay (/overlay/chat) works WITHOUT authentication!
        let overlay_no_auth = client
            .get(format!("http://{local_addr}/overlay/chat"))
            .send()
            .await
            .unwrap();
        assert_eq!(overlay_no_auth.status(), reqwest::StatusCode::OK);
        let overlay_body = overlay_no_auth.text().await.unwrap();
        assert!(overlay_body.contains("RTMP-Manager Chat Overlay"));
        assert!(overlay_body.contains("TestViewer"));

        // Overlay alias (/chat/overlay) also works without authentication
        let alias_no_auth = client
            .get(format!("http://{local_addr}/chat/overlay"))
            .send()
            .await
            .unwrap();
        assert_eq!(alias_no_auth.status(), reqwest::StatusCode::OK);

        server_task.abort();
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn test_chat_endpoint_and_acknowledge_flow() {
        let temp_dir = std::env::temp_dir().join(format!(
            "rtmp-chat-test-{}",
            TEST_PORT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_path = temp_dir.join("config.sqlite3");
        let client = reqwest::Client::new();
        let (config_handle, _config) = crate::config::ConfigHandle::open(&config_path)
            .await
            .unwrap();
        let metrics = Arc::new(crate::metrics::Metrics::default());
        let app_handle = AppHandle::new(metrics, config_handle, client.clone(), 1935)
            .await
            .unwrap();

        ensure_asset_bundle();

        let app = Router::builder()
            .discover()
            .runtime()
            .assets(AssetBundle::load().unwrap())
            .app_context(app_handle.clone())
            .build();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let _ = topcoat::serve(listener, app).await;
        });

        let html_content = client
            .get(format!("http://{local_addr}/chat"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        // Extract procedure ID for send_test_chat
        let proc_id = html_content
            .split("chat-test-button")
            .nth(1)
            .unwrap()
            .split("Procedure")
            .nth(1)
            .unwrap()
            .split("&quot;id&quot;:&quot;")
            .nth(1)
            .unwrap()
            .split("&quot;")
            .next()
            .unwrap();
        println!("PROCEDURE ID: {proc_id}");

        let proc_resp = client
            .post(format!(
                "http://{local_addr}/_topcoat/runtime/procedures/{proc_id}"
            ))
            .header("Content-Type", "application/json")
            .body("null")
            .send()
            .await
            .unwrap();
        println!("PROCEDURE STATUS: {}", proc_resp.status());
        let proc_text = proc_resp.text().await.unwrap();
        println!("PROCEDURE RESPONSE: {proc_text}");

        // 1. Post to /api/chat/test with empty body (generates sample message)
        let resp = client
            .post(format!("http://{local_addr}/api/chat/test"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let snapshot: crate::chat::ChatInboxSnapshot = resp.json().await.unwrap();
        assert_eq!(snapshot.messages.len(), 1);
        let message_id = snapshot.messages[0].id;

        // 2. Post to /api/chat/test with custom payload
        let custom_resp = client
            .post(format!("http://{local_addr}/api/chat/test"))
            .header("Content-Type", "application/json")
            .body(r#"{"source":"twitch","author":"CustomTester","text":"Testing custom text"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(custom_resp.status(), reqwest::StatusCode::OK);
        let snapshot: crate::chat::ChatInboxSnapshot = custom_resp.json().await.unwrap();
        assert_eq!(snapshot.messages.len(), 2);

        // 3. Acknowledge first message
        let ack_resp = client
            .post(format!("http://{local_addr}/api/chat/acknowledge"))
            .header("Content-Type", "application/json")
            .body(format!(r#"{{"id":{message_id}}}"#))
            .send()
            .await
            .unwrap();
        assert_eq!(ack_resp.status(), reqwest::StatusCode::OK);
        let snapshot_after_ack: crate::chat::ChatInboxSnapshot = ack_resp.json().await.unwrap();
        assert_eq!(snapshot_after_ack.messages.len(), 1);
        assert_eq!(snapshot_after_ack.messages[0].author, "CustomTester");
        assert_eq!(snapshot_after_ack.messages[0].text, "Testing custom text");

        server_task.abort();
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
