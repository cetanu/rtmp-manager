use crate::server::state::AppHandle;
use crate::util::constant_time_eq;
use base64::{Engine, engine::general_purpose::STANDARD};
use topcoat::{
    Result,
    context::{CxBuilder, app_context},
    router::{Body, IntoResponse, Next, Response, StatusCode, header, layer},
};

pub(crate) fn is_public_path(path: &str) -> bool {
    path == "/api/webhook"
        || path == "/overlay/chat"
        || path == "/chat/overlay"
        || path == "/api/chat"
        || path == "/api/events"
        || path.starts_with("/assets/")
        || path.starts_with("/_topcoat/")
}

#[layer("/")]
async fn basic_auth(cx: &mut CxBuilder, body: Body, next: Next<'_>) -> Result<Response> {
    if is_public_path(topcoat::router::uri(cx).path()) {
        return next.run(cx, body).await;
    }

    let app: &AppHandle = app_context(cx);
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

    if let Some(token) = submitted_token(cx) {
        let stream_key = &app.config.get().server.ingest_stream_key;
        if (!auth.password.is_empty()
            && constant_time_eq(auth.password.as_bytes(), token.as_bytes()))
            || (!stream_key.is_empty()
                && constant_time_eq(stream_key.as_bytes(), token.as_bytes()))
        {
            return next.run(cx, body).await;
        }
    }

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
    let encoded = topcoat::router::headers(cx)
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Basic ")?;
    let decoded = STANDARD.decode(encoded).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (username, password) = decoded.split_once(':')?;
    Some((username.to_string(), password.to_string()))
}

fn submitted_token(cx: &topcoat::context::Cx) -> Option<String> {
    extract_query_token(topcoat::router::uri(cx).query()?)
}

fn extract_query_token(query: &str) -> Option<String> {
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=')?;
        if k == "key" || k == "token" {
            return Some(v.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_comparison_rejects_different_values_and_lengths() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"diff"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    #[test]
    fn extracts_query_token_from_key_or_token_param() {
        assert_eq!(extract_query_token("key=mysecret"), Some("mysecret".into()));
        assert_eq!(extract_query_token("token=mysecret"), Some("mysecret".into()));
        assert_eq!(
            extract_query_token("theme=plain&key=stream123&align=bottom"),
            Some("stream123".into())
        );
        assert_eq!(extract_query_token("theme=plain&align=bottom"), None);
    }

    #[test]
    fn public_paths_include_overlay_and_assets() {
        assert!(is_public_path("/api/webhook"));
        assert!(is_public_path("/overlay/chat"));
        assert!(is_public_path("/chat/overlay"));
        assert!(is_public_path("/api/chat"));
        assert!(is_public_path("/api/events"));
        assert!(is_public_path("/assets/tailwind-123.css"));
        assert!(is_public_path("/_topcoat/shards/chat"));

        assert!(!is_public_path("/"));
        assert!(!is_public_path("/chat"));
        assert!(!is_public_path("/settings"));
        assert!(!is_public_path("/targets"));
        assert!(!is_public_path("/api/config"));
        assert!(!is_public_path("/api/chat/acknowledge"));
    }
}
