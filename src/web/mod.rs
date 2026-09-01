use crate::accounts::Role;
use crate::config::ConfigForm;
use crate::server::state::{AppHandle, StreamState, StreamStatus};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::net::TcpListener;
use topcoat::asset::{AssetBundle, RouterBuilderAssetExt};
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::session::{RouterBuilderSessionExt, SessionConfig};
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
        page, parse_query_params, route,
    },
    view::{component, view},
};

pub mod auth;
pub mod components;
mod oauth;
mod overlay;
mod setup;
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
pub(crate) const CHAT_OVERLAY_SCRIPT: topcoat::asset::Asset =
    topcoat::asset::asset!("static/chat-overlay.js");

pub async fn request_config(cx: &Cx) -> anyhow::Result<crate::config::AppConfig> {
    let app: &AppHandle = app_context(cx);
    app.tenant_config(&auth::current_user(cx).tenant_id).await
}

pub async fn request_chat(cx: &Cx) -> anyhow::Result<crate::chat::ChatHandle> {
    let app: &AppHandle = app_context(cx);
    app.tenant_chat(&auth::current_user(cx).tenant_id).await
}

pub async fn run_web_server(
    app_handle: AppHandle,
    addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    let sampler_metrics = Arc::clone(&app_handle.metrics);
    let app = Router::builder()
        .discover()
        .assets(AssetBundle::load()?)
        .app_context(app_handle)
        .cookies()
        .sessions(SessionConfig::default())
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
async fn home(cx: &Cx) -> Result {
    let app: &AppHandle = app_context(cx);
    if !app.config.get().initialized {
        return Err(topcoat::router::error::redirect("/setup").into());
    }
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
    let app: &AppHandle = app_context(cx);
    let name = topcoat::router::path_param::<PreviewFile>(cx);
    let Some(path) = app
        .stream
        .preview_file(&auth::current_user(cx).tenant_id, name)
    else {
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
async fn get_stream_status(cx: &Cx) -> Result<Json<StreamStatus>> {
    let app: &AppHandle = app_context(cx);
    Ok(Json(app.stream.status(&auth::current_user(cx).tenant_id)))
}

#[route(GET "/api/admin/streams")]
async fn get_admin_stream_status(cx: &Cx) -> Result<Json<Vec<AdminStreamStatus>>> {
    let app: &AppHandle = app_context(cx);
    Ok(Json(
        app.stream
            .all_status()
            .iter()
            .map(|(tenant_id, status)| AdminStreamStatus {
                tenant_id: tenant_id.as_str().to_owned(),
                status: *status,
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize)]
struct AdminStreamStatus {
    tenant_id: String,
    status: StreamStatus,
}

#[route(GET "/api/metrics/history")]
async fn get_metrics_history(cx: &Cx) -> Result<Json<Vec<crate::metrics::MetricsSample>>> {
    let app: &AppHandle = app_context(cx);
    let tenant_id = auth::current_user(cx).tenant_id.as_str().to_owned();
    let history = app
        .metrics
        .history()
        .into_iter()
        .map(|mut sample| {
            sample
                .targets
                .retain(|target| target.tenant_id == tenant_id);
            sample
        })
        .collect();
    Ok(Json(history))
}

#[route(GET "/api/metrics/prometheus")]
async fn get_metrics_prometheus(cx: &Cx) -> Result<topcoat::router::Response> {
    let app: &AppHandle = app_context(cx);
    let tenant_id = auth::current_user(cx).tenant_id.as_str();
    let mut output = format!(
        "# TYPE rtmp_manager_ingest_bps gauge\nrtmp_manager_ingest_bps{{tenant_id=\"{}\"}} {}\n",
        prometheus_label(tenant_id),
        app.metrics.current_ingest_bps()
    );
    for sample in app
        .metrics
        .current_target_bitrates()
        .into_iter()
        .filter(|sample| sample.tenant_id == tenant_id)
    {
        output.push_str(&format!(
            "rtmp_manager_target_outbound_bps{{tenant_id=\"{}\",target=\"{}\"}} {}\n",
            prometheus_label(tenant_id),
            prometheus_label(&sample.name),
            sample.outbound_bps
        ));
        output.push_str(&format!(
            "rtmp_manager_target_dropped_frames{{tenant_id=\"{}\",target=\"{}\"}} {}\n",
            prometheus_label(tenant_id),
            prometheus_label(&sample.name),
            sample.dropped_frames
        ));
        output.push_str(&format!(
            "rtmp_manager_target_reconnections_total{{tenant_id=\"{}\",target=\"{}\"}} {}\n",
            prometheus_label(tenant_id),
            prometheus_label(&sample.name),
            sample.reconnections
        ));
    }
    let mut response = topcoat::router::Response::new(topcoat::router::Body::from(output));
    response.headers_mut().insert(
        topcoat::router::header::CONTENT_TYPE,
        topcoat::router::HeaderValue::from_static("text/plain; version=0.0.4"),
    );
    Ok(response)
}

fn prometheus_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[route(GET "/healthz")]
async fn healthz(cx: &Cx) -> Result<topcoat::router::Response> {
    let app: &AppHandle = app_context(cx);
    if app.database.health_check().await.is_err() {
        return topcoat::router::IntoResponse::into_response(
            (
                topcoat::router::StatusCode::SERVICE_UNAVAILABLE,
                "database unavailable",
            ),
            cx,
        );
    }
    topcoat::router::IntoResponse::into_response("ok", cx)
}

#[route(POST "/api/admin/emergency-stop")]
async fn emergency_stop(cx: &Cx) -> Result<topcoat::router::Response> {
    let app: &AppHandle = app_context(cx);
    let user = auth::current_user(cx);
    app.record_admin_action(&user.id, "emergency_stop_all", None)
        .await?;
    app.stream.emergency_stop().await;
    topcoat::router::IntoResponse::into_response(topcoat::router::StatusCode::NO_CONTENT, cx)
}

#[topcoat::router::path_param]
struct AdminTenantPath(str);

#[route(POST "/api/admin/tenants/{tenant_id}/emergency-stop")]
async fn emergency_stop_tenant(cx: &Cx) -> Result<topcoat::router::Response> {
    let tenant_id =
        crate::tenant::TenantId::new(topcoat::router::path_param::<AdminTenantPath>(cx))
            .map_err(|_| bad_request("Invalid tenant ID"))?;
    let app: &AppHandle = app_context(cx);
    if app.tenants.find(&tenant_id).await?.is_none() {
        return Err(not_found().into());
    }
    let user = auth::current_user(cx);
    app.record_admin_action(&user.id, "emergency_stop_tenant", Some(tenant_id.as_str()))
        .await?;
    app.stream.emergency_stop_tenant(tenant_id).await;
    topcoat::router::IntoResponse::into_response(topcoat::router::StatusCode::NO_CONTENT, cx)
}

#[derive(Debug, Deserialize)]
struct AcknowledgeChatMessage {
    id: u64,
}

#[route(POST "/api/config")]
async fn update_config(cx: &Cx, body: topcoat::router::Bytes) -> Result<topcoat::router::Response> {
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

    let user = auth::current_user(cx);
    let chat_changed = if user.role == Role::Admin
        && user.tenant_id == crate::tenant::TenantId::default_tenant()
    {
        match app.config.save_form(form).await {
            Ok((_, _, chat_changed)) => chat_changed,
            Err(error) => return Err(bad_request(error.to_string()).into()),
        }
    } else {
        if form.server.is_some() {
            return Err(bad_request("Only administrators can change server settings").into());
        }
        match app.save_tenant_form(&user.tenant_id, form).await {
            Ok((_, chat_changed)) => chat_changed,
            Err(error) => return Err(bad_request(error.to_string()).into()),
        }
    };

    if chat_changed {
        let result = if user.tenant_id == crate::tenant::TenantId::default_tenant() {
            app.apply_chat_config().await
        } else {
            let config = app.tenant_config(&user.tenant_id).await?;
            app.chat.apply_config(&user.tenant_id, config.chat).await
        };
        if let Err(error) = result {
            tracing::error!("Failed to apply chat configuration: {error:#}");
            return Err(internal_server_error(error).into());
        }
    }

    topcoat::router::IntoResponse::into_response(redirect, cx)
}

#[route(GET "/api/config")]
async fn get_config(cx: &Cx) -> Result<topcoat::router::Response> {
    topcoat::router::IntoResponse::into_response(
        (
            [(topcoat::router::header::CACHE_CONTROL, "no-store, private")],
            Json(request_config(cx).await?),
        ),
        cx,
    )
}

#[derive(Deserialize)]
struct BillingPlanUpdate {
    tenant_id: String,
    plan: String,
}

#[route(POST "/api/billing/webhook")]
async fn billing_webhook(
    cx: &Cx,
    body: topcoat::router::Bytes,
) -> Result<topcoat::router::Response> {
    let secret = std::env::var("BILLING_WEBHOOK_SECRET")
        .map_err(|_| bad_request("Billing webhook is not configured"))?;
    let signature = topcoat::router::headers(cx)
        .get("x-billing-signature")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| bad_request("Missing billing webhook signature"))?;
    if !crate::billing::UsageRepository::verify_webhook(&body, signature, &secret) {
        return Err(bad_request("Invalid billing webhook signature").into());
    }
    let update: BillingPlanUpdate = serde_json::from_slice(&body)
        .map_err(|_| bad_request("Invalid billing webhook payload"))?;
    let app: &AppHandle = app_context(cx);
    app.usage
        .set_plan(
            &update.tenant_id,
            &update.plan,
            crate::util::now_unix_secs() as i64,
        )
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    topcoat::router::IntoResponse::into_response(topcoat::router::StatusCode::NO_CONTENT, cx)
}

#[route(GET "/api/billing/usage")]
async fn billing_usage(cx: &Cx) -> Result<Json<crate::billing::UsageSnapshot>> {
    let app: &AppHandle = app_context(cx);
    let tenant_id = &auth::current_user(cx).tenant_id;
    Ok(Json(
        app.usage
            .current_usage(tenant_id.as_str(), crate::util::now_unix_secs() as i64)
            .await?,
    ))
}

#[route(POST "/api/billing/stripe")]
async fn stripe_billing_webhook(
    cx: &Cx,
    body: topcoat::router::Bytes,
) -> Result<topcoat::router::Response> {
    let secret = std::env::var("STRIPE_WEBHOOK_SECRET")
        .map_err(|_| bad_request("Stripe billing is not configured"))?;
    let signature = topcoat::router::headers(cx)
        .get("stripe-signature")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| bad_request("Missing Stripe signature"))?;
    if !crate::billing::UsageRepository::verify_stripe_signature(
        &body,
        signature,
        &secret,
        crate::util::now_unix_secs() as i64,
    ) {
        return Err(bad_request("Invalid Stripe signature").into());
    }
    let payload: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| bad_request("Invalid Stripe event"))?;
    let event_type = payload
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if matches!(
        event_type,
        "customer.subscription.created"
            | "customer.subscription.updated"
            | "customer.subscription.deleted"
    ) {
        let object = payload
            .pointer("/data/object")
            .ok_or_else(|| bad_request("Stripe event has no subscription"))?;
        let tenant_id = object
            .pointer("/metadata/tenant_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| bad_request("Stripe subscription is missing tenant metadata"))?;
        let plan = object
            .pointer("/metadata/plan")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("free");
        let app: &AppHandle = app_context(cx);
        app.usage
            .set_plan(
                tenant_id,
                if event_type.ends_with("deleted") {
                    "free"
                } else {
                    plan
                },
                crate::util::now_unix_secs() as i64,
            )
            .await
            .map_err(|error| bad_request(error.to_string()))?;
    }
    topcoat::router::IntoResponse::into_response(topcoat::router::StatusCode::NO_CONTENT, cx)
}

#[route(POST "/api/billing/lemonsqueezy")]
async fn lemonsqueezy_billing_webhook(
    cx: &Cx,
    body: topcoat::router::Bytes,
) -> Result<topcoat::router::Response> {
    let secret = std::env::var("LEMONSQUEEZY_WEBHOOK_SECRET")
        .map_err(|_| bad_request("LemonSqueezy billing is not configured"))?;
    let signature = topcoat::router::headers(cx)
        .get("x-signature")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| bad_request("Missing LemonSqueezy signature"))?;
    if !crate::billing::UsageRepository::verify_hex_signature(&body, signature, &secret) {
        return Err(bad_request("Invalid LemonSqueezy signature").into());
    }
    let payload: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| bad_request("Invalid LemonSqueezy event"))?;
    let event = payload
        .pointer("/meta/event_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if matches!(
        event,
        "subscription_created"
            | "subscription_updated"
            | "subscription_cancelled"
            | "subscription_expired"
    ) {
        let custom = payload
            .pointer("/meta/custom_data")
            .ok_or_else(|| bad_request("LemonSqueezy event is missing custom data"))?;
        let tenant_id = custom
            .get("tenant_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| bad_request("LemonSqueezy event is missing tenant metadata"))?;
        let plan = custom
            .get("plan")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("free");
        let app: &AppHandle = app_context(cx);
        app.usage
            .set_plan(
                tenant_id,
                if event.ends_with("cancelled") || event.ends_with("expired") {
                    "free"
                } else {
                    plan
                },
                crate::util::now_unix_secs() as i64,
            )
            .await
            .map_err(|error| bad_request(error.to_string()))?;
    }
    topcoat::router::IntoResponse::into_response(topcoat::router::StatusCode::NO_CONTENT, cx)
}

#[route(POST "/api/config/import")]
async fn import_config(cx: &Cx, body: topcoat::router::Bytes) -> Result<topcoat::router::Response> {
    let app: &AppHandle = app_context(cx);
    let (_, _changed, chat_changed) = match app.config.import(&body).await {
        Ok(res) => res,
        Err(error) => return Err(bad_request(error.to_string()).into()),
    };

    if chat_changed && let Err(error) = app.apply_chat_config().await {
        return Err(internal_server_error(error).into());
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
    let snapshot = request_chat(cx).await?.snapshot().await?;
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
    let chat = request_chat(cx).await?;
    if !chat.acknowledge(request.id).await? {
        return topcoat::router::IntoResponse::into_response(
            (
                topcoat::router::StatusCode::CONFLICT,
                "The displayed chat message has already changed",
            ),
            cx,
        );
    }

    topcoat::router::IntoResponse::into_response(Json(chat.snapshot().await?), cx)
}

#[route(GET "/api/events")]
async fn server_events(
    cx: &Cx,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent>> + use<>>> {
    let app: &AppHandle = app_context(cx);
    let status_rx = app.stream.subscribe_status();
    let tenant_id = auth::current_user(cx).tenant_id.clone();
    let chat_changes = request_chat(cx).await?.subscribe_changes();

    let initial_status = SseEvent::new()
        .event("stream_status")
        .json_data(&app.stream.status(&tenant_id))?;
    let initial_events = futures_util::stream::iter([
        Ok(initial_status),
        Ok(SseEvent::new().event("chat_changed").data("changed")),
    ]);

    let changes = futures_util::stream::unfold(
        (status_rx, chat_changes, tenant_id),
        |(mut status_rx, mut chat_changes, tenant_id)| async move {
            tokio::select! {
                changed = status_rx.changed() => {
                    if changed.is_err() {
                        return None;
                    }
                    let status = status_rx
                        .borrow()
                        .get(&tenant_id)
                        .copied()
                        .unwrap_or(StreamStatus { state: StreamState::Offline });
                    let event = SseEvent::new()
                        .event("stream_status")
                        .json_data(&status);
                    Some((event, (status_rx, chat_changes, tenant_id)))
                }
                changed = chat_changes.changed() => {
                    if changed.is_err() {
                        return None;
                    }
                    Some((
                        Ok(SseEvent::new().event("chat_changed").data("changed")),
                        (status_rx, chat_changes, tenant_id),
                    ))
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

#[route(POST "/api/v1/webhooks/kick")]
async fn receive_kick_webhook(
    cx: &Cx,
    body: topcoat::router::Bytes,
) -> Result<topcoat::router::Response> {
    receive_webhook_for_platform(cx, body, "kick").await
}

#[route(POST "/api/v1/webhooks/x")]
async fn receive_x_webhook(
    cx: &Cx,
    body: topcoat::router::Bytes,
) -> Result<topcoat::router::Response> {
    receive_webhook_for_platform(cx, body, "x").await
}

#[route(POST "/api/v1/webhooks/twitch")]
async fn receive_twitch_webhook(
    cx: &Cx,
    body: topcoat::router::Bytes,
) -> Result<topcoat::router::Response> {
    receive_webhook_for_platform(cx, body, "twitch").await
}

async fn receive_webhook_for_platform(
    cx: &Cx,
    body: topcoat::router::Bytes,
    platform: &str,
) -> Result<topcoat::router::Response> {
    const MAX_WEBHOOK_SIZE: usize = 128 * 1024;
    if body.len() > MAX_WEBHOOK_SIZE {
        return Err(bad_request("Webhook body exceeds 128 KiB").into());
    }
    let app: &AppHandle = app_context(cx);
    let headers = topcoat::router::headers(cx)
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
    let stream_key = event
        .header("x-tenant-stream-key")
        .ok_or_else(|| bad_request("X-Tenant-Stream-Key header is required"))?;
    let tenant = app
        .tenants
        .authenticate(stream_key)
        .await?
        .ok_or_else(|| bad_request("Invalid tenant stream key"))?;
    let settings = tenant.chat;
    let chat = app.tenant_chat(&tenant.id).await?;
    if platform == "twitch" {
        let secret = std::env::var("TWITCH_EVENTSUB_SECRET")
            .map_err(|_| bad_request("Twitch EventSub is not configured"))?;
        let (challenge, message) = crate::chat::twitch_eventsub::parse(&event, &secret)
            .map_err(|error| bad_request(error.to_string()))?;
        if let Some(message) = message {
            dispatch_configured_relay(&settings, &message, app.http_client.clone()).await;
            chat.enqueue(message).await.map_err(internal_server_error)?;
        }
        if let Some(challenge) = challenge {
            return Ok(topcoat::router::Response::new(topcoat::router::Body::from(
                challenge,
            )));
        }
    } else if platform == "kick" {
        let message = match crate::chat::kick::process_event(&settings, &event) {
            Ok(message) => message,
            Err(error) => {
                tracing::warn!("Rejected Kick webhook: {error:#}");
                return Err(bad_request("Rejected Kick webhook").into());
            }
        };
        dispatch_configured_relay(&settings, &message, app.http_client.clone()).await;
        chat.enqueue(message).await.map_err(internal_server_error)?;
    } else {
        let message = match crate::chat::x::process_event(&settings, &event) {
            Ok(message) => message,
            Err(error) => {
                tracing::warn!("Rejected X webhook: {error:#}");
                return Err(bad_request("Rejected X webhook").into());
            }
        };
        if let Some(message) = message {
            dispatch_configured_relay(&settings, &message, app.http_client.clone()).await;
            chat.enqueue(message).await.map_err(internal_server_error)?;
        }
    }
    tracing::info!(platform, body_bytes, "Webhook accepted");
    topcoat::router::IntoResponse::into_response(topcoat::router::StatusCode::NO_CONTENT, cx)
}

async fn dispatch_configured_relay(
    settings: &crate::config::ChatSettings,
    message: &crate::chat::IncomingChatMessage,
    client: reqwest::Client,
) {
    if !settings.relay_enabled && std::env::var("CHAT_RELAY_DESTINATION").is_err() {
        return;
    }
    let Some(destination) = settings
        .relay_destination
        .clone()
        .or_else(|| std::env::var("CHAT_RELAY_DESTINATION").ok())
    else {
        return;
    };
    let rule = crate::chat::relay::RelayRule {
        source: settings
            .relay_source
            .clone()
            .or_else(|| std::env::var("CHAT_RELAY_SOURCE").ok())
            .unwrap_or_else(|| message.source.clone()),
        destination,
        prefix: "[relay] ".to_owned(),
    };
    if let Err(error) = crate::chat::relay::dispatch(&rule, message, settings, &client).await {
        tracing::warn!(%error, "Chat relay dispatch failed");
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

#[route(GET "/api/webhook")]
async fn verify_webhook_crc(cx: &Cx) -> Result<topcoat::router::Response> {
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
    topcoat::router::IntoResponse::into_response(Json(WebhookCrcResponse { response_token }), cx)
}
