//! Alert overlay transport and authenticated media management.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use rtmp_core::chat::automations::alerts;
use topcoat::context::Cx;
use topcoat::view::ViewExt;

pub(crate) const SCRIPT: Asset = asset!("static/alerts.js");

#[page("/overlay/alerts")]
async fn alert_overlay(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="UTF-8" />
                <title>"RTMP Manager Alerts Overlay"</title>
                <script src=(SCRIPT) defer="defer"></script>
            </head>
            <body
                style="margin:0;background:transparent;overflow:hidden"
                data-alert-overlay=""
            >
                <div
                    id="alert-stage"
                    style="position:relative;width:100vw;height:100vh;color:white;font:bold 48px sans-serif;text-align:center;text-shadow:0 2px 6px black"
                ></div>
            </body>
        </html>
    })
}

#[route(GET "/api/alerts/events")]
async fn alert_events(
    cx: &Cx,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent>> + use<>>> {
    let app: &AppHandle = context(cx);
    // A new connection starts with future alerts only; reconnecting never replays old sounds.
    let receiver = app.chat.subscribe_alerts();
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(alert) => {
                    return Some((SseEvent::new().event("alert").json_data(&alert), receiver));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new()))
}

#[derive(Deserialize)]
struct MediaQuery {
    name: String,
}

#[route(
    GET "/api/alerts/media",
    with(topcoat::router::BodyLimit::max(
        rtmp_core::chat::automations::alerts::MAX_MEDIA_BYTES + 64 * 1024,
    ))
)]
async fn serve_alert_media(cx: &Cx) -> Result<Response> {
    let query: MediaQuery = parse_query_params(cx)?;
    let app: &AppHandle = context(cx);
    let media = alerts::get(app.config.database(), &query.name)
        .await
        .map_err(|_| not_found())?;
    let bytes = STANDARD
        .decode(&media.data)
        .map_err(internal_server_error)?;
    let mut response = Response::new(topcoat::router::Body::from(bytes));
    response.headers_mut().insert(
        topcoat::router::header::CONTENT_TYPE,
        media.content_type.parse()?,
    );
    response
        .headers_mut()
        .insert(topcoat::router::header::CACHE_CONTROL, "no-store".parse()?);
    response.headers_mut().insert(
        topcoat::router::header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse()?,
    );
    Ok(response)
}

#[route(
    POST "/api/alerts/media",
    with(topcoat::router::BodyLimit::max(
        rtmp_core::chat::automations::alerts::MAX_MEDIA_BYTES + 64 * 1024,
    ))
)]
async fn upload_alert_media(cx: &Cx, mut multipart: Multipart) -> Result<Response> {
    let app: &AppHandle = context(cx);
    while let Some(field) = multipart.next_field().await? {
        if field.name() != Some("media") {
            continue;
        }
        let name = field.file_name().unwrap_or_default().to_owned();
        let bytes = field.bytes().await?;
        alerts::upload(app.config.database(), name, &bytes)
            .await
            .map_err(|e| bad_request(e.to_string()))?;
        return media_library_response(cx).await;
    }
    Err(bad_request("Select a media file").into())
}

#[route(POST "/api/alerts/media/delete")]
async fn delete_alert_media(cx: &Cx) -> Result<Response> {
    let query: MediaQuery = parse_query_params(cx)?;
    let app: &AppHandle = context(cx);
    alerts::delete(app.config.database(), &query.name)
        .await
        .map_err(|e| bad_request(e.to_string()))?;
    media_library_response(cx).await
}

async fn media_library_response(cx: &Cx) -> Result<Response> {
    let __cx = cx;
    let fragment = view! { crate::web::components::automation_media::automation_media() };
    topcoat::router::content::Html(fragment.single().await?.render(cx)).into_response(cx)
}
