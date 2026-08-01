use crate::notifications::NotificationTarget;
use anyhow::Result;
use reqwest::Client;
use tracing::{info, warn};

pub struct GenericWebhookNotifier {
    webhook_url: String,
    live_message: String,
    http_client: Client,
}

impl GenericWebhookNotifier {
    pub fn new(webhook_url: String, live_message: String, http_client: Client) -> Self {
        Self {
            webhook_url,
            live_message,
            http_client,
        }
    }

    pub async fn notify(&self, targets: &[NotificationTarget]) -> Result<()> {
        if self.webhook_url.trim().is_empty() {
            return Ok(());
        }

        info!("Sending generic stream.started webhook notification");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let payload = self.payload(targets, now);

        if self
            .http_client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .is_err()
        {
            warn!("Failed to send generic webhook notification");
        }

        Ok(())
    }

    fn payload(&self, targets: &[NotificationTarget], timestamp: u64) -> serde_json::Value {
        let target_names: Vec<String> = targets.iter().map(|t| t.name.clone()).collect();
        let target_urls: Vec<String> = targets
            .iter()
            .filter_map(|t| t.public_url.clone())
            .collect();

        serde_json::json!({
            "event": "stream.started",
            "message": self.live_message,
            "targets": target_names,
            "public_urls": target_urls,
            "timestamp": timestamp
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_payload_cannot_contain_stream_keys() {
        let notifier = GenericWebhookNotifier::new(
            "https://example.test/hook".into(),
            "Live".into(),
            Client::new(),
        );
        let payload = notifier.payload(
            &[NotificationTarget {
                name: "Twitch".into(),
                public_url: Some("https://example.test/watch".into()),
            }],
            123,
        );

        assert!(payload.get("stream_key").is_none());
        assert!(!payload.to_string().contains("super-secret-stream-key"));
    }
}
