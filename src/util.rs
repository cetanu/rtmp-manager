use regex::Regex;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Compares two byte slices in constant time.
pub fn constant_time_eq(expected: impl AsRef<[u8]>, submitted: impl AsRef<[u8]>) -> bool {
    let expected = expected.as_ref();
    let submitted = submitted.as_ref();
    if expected.len() != submitted.len() {
        return false;
    }
    expected
        .iter()
        .zip(submitted)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

/// Constant-time comparison for authentication tokens and stream keys.
/// Returns `false` if the expected token is empty or length differs.
pub fn secure_token_matches(expected: &str, submitted: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    constant_time_eq(expected.as_bytes(), submitted.as_bytes())
}

/// Redacts sensitive stream keys and URLs from strings (e.g. process logs, stderr).
pub fn redact_secrets(text: &str, secrets: &[String]) -> String {
    let redacted = redact_urls(text);
    secrets
        .iter()
        .filter(|secret| !secret.is_empty())
        .fold(redacted, |redacted, secret| {
            redacted.replace(secret, "[REDACTED]")
        })
}

fn redact_urls(text: &str) -> String {
    static URL_PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = URL_PATTERN.get_or_init(|| {
        Regex::new(r#"(?i)(rtmps?|srt)://[^\s"'<>]+"#).expect("valid stream URL pattern")
    });

    pattern
        .replace_all(text, |captures: &regex::Captures<'_>| {
            let url = captures
                .get(0)
                .expect("URL match has a full capture")
                .as_str();
            let (url, trailing) = trim_url_punctuation(url);
            let scheme = &url[..url.find("://").expect("URL match has a scheme")];
            let authority_start = scheme.len() + 3;
            let authority_end = url[authority_start..]
                .find(['/', '?', '#'])
                .map_or(url.len(), |offset| authority_start + offset);
            let authority = &url[authority_start..authority_end];
            let host = authority
                .rsplit_once('@')
                .map_or(authority, |(_, host)| host);
            let label = format!("{}_URL_REDACTED", scheme.to_ascii_uppercase());
            format!("{scheme}://{host}/[{label}]{trailing}")
        })
        .into_owned()
}

fn trim_url_punctuation(value: &str) -> (&str, &str) {
    let end = value
        .trim_end_matches([',', '.', ';', ':', '!', '?', ')', ']', '}'])
        .len();
    (&value[..end], &value[end..])
}

/// Returns the current Unix timestamp in milliseconds.
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Returns the current Unix timestamp in seconds.
pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Filters an optional string, returning `None` if trimmed value is empty.
pub fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_comparison_works() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"diff"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    #[test]
    fn secure_token_matching_requires_nonempty_and_exact() {
        assert!(secure_token_matches("private-key", "private-key"));
        assert!(!secure_token_matches("private-key", "wrong-key"));
        assert!(!secure_token_matches("private-key", "private-key-extra"));
        assert!(!secure_token_matches("", ""));
    }

    #[test]
    fn secret_redaction_removes_keys_and_urls() {
        let secrets = vec!["local-key".to_owned(), "twitch-key".to_owned()];
        let line = "rtmp://localhost/live/local-key -> rtmp://twitch/app/twitch-key";
        let redacted = redact_secrets(line, &secrets);
        assert_eq!(
            redacted,
            "rtmp://localhost/[RTMP_URL_REDACTED] -> rtmp://twitch/[RTMP_URL_REDACTED]"
        );
    }

    #[test]
    fn secret_redaction_preserves_formatting_and_url_context() {
        let secrets = vec!["private-key".to_owned()];
        let line = "  before\t rtmp://example.test/live/private-key,\n after  ";
        let redacted = redact_secrets(line, &secrets);
        assert_eq!(
            redacted,
            "  before\t rtmp://example.test/[RTMP_URL_REDACTED],\n after  "
        );
    }

    #[test]
    fn secret_redaction_keeps_url_host_and_removes_srt_credentials() {
        let redacted = redact_secrets("srt://secret:pass@example.test:9000?streamid=key", &[]);
        assert_eq!(redacted, "srt://example.test:9000/[SRT_URL_REDACTED]");
    }

    #[test]
    fn non_empty_filters_blank_strings() {
        assert_eq!(non_empty(Some("  ".to_string())), None);
        assert_eq!(
            non_empty(Some("hello".to_string())),
            Some("hello".to_string())
        );
        assert_eq!(non_empty(None), None);
    }
}
