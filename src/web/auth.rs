use crate::server::state::ProxyState;
use crate::util::constant_time_eq;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::Arc;
use topcoat::{
    Result,
    context::{CxBuilder, app_context},
    router::{Body, IntoResponse, Next, Response, StatusCode, header, layer},
};

const PUBLIC_INGEST_PATHS: [&str; 1] = ["/api/chat/ingest"];

#[layer("/")]
async fn basic_auth(cx: &mut CxBuilder, body: Body, next: Next<'_>) -> Result<Response> {
    if PUBLIC_INGEST_PATHS.contains(&topcoat::router::uri(cx).path()) {
        return next.run(cx, body).await;
    }

    let state: &Arc<ProxyState> = app_context(cx);
    let auth = state.config.read().await.web_auth.clone();
    if auth.username.is_empty() && auth.password.is_empty() {
        return next.run(cx, body).await;
    }

    if submitted_credentials(cx).is_some_and(|(username, password)| {
        constant_time_eq(auth.username.as_bytes(), username.as_bytes())
            && constant_time_eq(auth.password.as_bytes(), password.as_bytes())
    }) {
        return next.run(cx, body).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_comparison_rejects_different_values_and_lengths() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"diff"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }
}
