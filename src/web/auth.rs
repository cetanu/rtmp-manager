use crate::server::state::AppHandle;
use crate::util::constant_time_eq;
use base64::{Engine, engine::general_purpose::STANDARD};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        Body, Next, StatusCode, header, layer, parse_query_params,
        request::{headers, method, uri},
        response::{IntoResponse, Response},
    },
};

const OVERLAY_PATH: &str = "/overlay/chat";
const OVERLAY_EVENTS_PATH: &str = "/api/overlay/events";

pub(crate) fn is_public_path(path: &str) -> bool {
    path == "/api/webhook" || path.starts_with("/_topcoat/assets/")
}

fn is_overlay_path(path: &str) -> bool {
    path == OVERLAY_PATH || path == OVERLAY_EVENTS_PATH
}

#[derive(Debug, serde::Deserialize)]
struct OverlayAccessQuery {
    key: Option<String>,
}

fn has_overlay_access(cx: &Cx, app: &AppHandle) -> bool {
    if !is_overlay_path(uri(cx).path()) || method(cx).as_str() != "GET" {
        return false;
    }

    let config = app.config.get();
    let expected = config.web_auth.overlay_token.as_bytes();
    if expected.is_empty() {
        return false;
    }

    // Preferred: Authorization: Bearer <token> (not logged, not in history).
    if let Some(bearer) = headers(cx)
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        && constant_time_eq(expected, bearer.trim().as_bytes())
    {
        return true;
    }

    // Legacy: ?key=... for OBS Browser Sources that can't set headers.
    // Keep working, but prefer Bearer; never log the query value.
    parse_query_params::<OverlayAccessQuery>(cx)
        .ok()
        .and_then(|query| query.key)
        .is_some_and(|key| constant_time_eq(expected, key.as_bytes()))
}

#[layer("/")]
async fn basic_auth(cx: &Cx, body: Body, next: Next<'_>) -> Result<Response> {
    let path = uri(cx).path();
    if is_public_path(path) {
        return next.run(cx, body).await;
    }

    // Basic CSRF guard: state-changing browser requests must be same-origin.
    // Allows empty Origin (curl, non-browser) but rejects cross-site POSTs
    // that browsers attach Origin/Referer to while auto-sending Basic auth.
    if matches!(method(cx).as_str(), "POST" | "PUT" | "PATCH" | "DELETE")
        && path.starts_with("/api/")
        && let Some(origin) = headers(cx)
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            .or_else(|| {
                headers(cx)
                    .get(header::REFERER)
                    .and_then(|value| value.to_str().ok())
            })
    {
        let host = headers(cx)
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        // Origin is `scheme://host[:port]`; require suffix match on Host.
        let origin_host = origin
            .split("://")
            .last()
            .unwrap_or(origin)
            .split('/')
            .next()
            .unwrap_or_default();
        if !origin_host.is_empty() && !host.is_empty() && origin_host != host {
            return (StatusCode::FORBIDDEN, "Cross-origin request rejected").into_response(cx);
        }
    }

    let app: &AppHandle = app_context(cx);
    if is_overlay_path(path) {
        if has_overlay_access(cx, app) {
            return next.run(cx, body).await;
        }
        return unauthorized_response(cx);
    }

    let auth = app.config.get().web_auth.clone();
    if auth.username.is_empty() && auth.password.is_empty() {
        return next.run(cx, body).await;
    }

    if submitted_credentials(cx).is_some_and(|(username, password)| {
        constant_time_eq(auth.username.as_bytes(), username.as_bytes())
            && constant_time_eq(auth.password.as_bytes(), password.as_bytes())
    }) {
        return next.run(cx, body).await;
    }

    unauthorized_response(cx)
}

fn unauthorized_response(cx: &Cx) -> Result<Response> {
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            r#"Basic realm="rtmp-manager", charset="UTF-8""#,
        )],
        "Authentication required",
    )
        .into_response(cx)
}

fn submitted_credentials(cx: &topcoat::context::Cx) -> Option<(String, String)> {
    let encoded = headers(cx)
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Basic ")?;
    let decoded = STANDARD.decode(encoded).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (username, password) = decoded.split_once(':')?;
    Some((username.to_string(), password.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_paths_only_include_webhooks_and_assets() {
        assert!(is_public_path("/api/webhook"));
        assert!(is_public_path("/_topcoat/assets/tailwind-123.css"));

        assert!(!is_public_path("/overlay/chat"));
        assert!(!is_public_path("/api/overlay/events"));
        assert!(!is_public_path("/api/chat"));
        assert!(!is_public_path("/api/events"));
        assert!(!is_public_path("/assets/tailwind-123.css"));
        assert!(!is_public_path("/_topcoat/runtime/shards/chat"));
        assert!(!is_public_path("/"));
        assert!(!is_public_path("/chat"));
        assert!(!is_public_path("/settings"));
        assert!(!is_public_path("/targets"));
        assert!(!is_public_path("/api/config"));
        assert!(!is_public_path("/api/chat/acknowledge"));
    }
}
