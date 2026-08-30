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
    let redacted = secrets
        .iter()
        .filter(|secret| !secret.is_empty())
        .fold(text.to_owned(), |redacted, secret| {
            redacted.replace(secret, "[REDACTED]")
        });
    redacted
        .split_whitespace()
        .map(|part| {
            if part.contains("rtmp://") || part.contains("rtmps://") {
                "[RTMP_URL_REDACTED]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
        assert_eq!(redacted, "[RTMP_URL_REDACTED] -> [RTMP_URL_REDACTED]");
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
