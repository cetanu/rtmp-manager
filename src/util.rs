use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn generate_secure_token() -> anyhow::Result<String> {
    let mut bytes = [0_u8; 24];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("Failed to generate a secure token"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn stream_key_digest(stream_key: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(stream_key.as_bytes()))
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

    #[test]
    fn generated_tokens_are_url_safe_and_unique() {
        let first = generate_secure_token().unwrap();
        let second = generate_secure_token().unwrap();
        assert_ne!(first, second);
        assert!(
            first
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "_-".contains(character))
        );
    }

    #[test]
    fn stream_key_digests_are_stable_without_exposing_the_key() {
        let digest = stream_key_digest("private-key");
        assert_eq!(digest, stream_key_digest("private-key"));
        assert_ne!(digest, stream_key_digest("different-key"));
        assert!(!digest.contains("private-key"));
    }
}
