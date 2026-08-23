use crate::config::{NotificationSettings, TargetConfig};
use crate::util::now_unix_secs;
use reqwest::Client;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct NotificationTarget {
    pub name: String,
    pub public_url: Option<String>,
}

impl From<&TargetConfig> for NotificationTarget {
    fn from(target: &TargetConfig) -> Self {
        Self {
            name: target.name.clone(),
            public_url: target.public_url.clone(),
        }
    }
}

/// Unified notification dispatcher for going-live webhooks (Discord & generic HTTP webhooks).
pub struct NotificationDispatcher {
    discord_webhook: Option<String>,
    webhook_url: Option<String>,
    live_message: String,
    http_client: Client,
}

impl NotificationDispatcher {
    pub fn new(settings: &NotificationSettings, http_client: Client) -> Arc<Self> {
        let discord_webhook = settings
            .discord_webhook
            .clone()
            .filter(|url| !url.trim().is_empty());
        let webhook_url = settings
            .webhook_url
            .clone()
            .filter(|url| !url.trim().is_empty());

        Arc::new(Self {
            discord_webhook,
            webhook_url,
            live_message: settings.live_message.clone(),
            http_client,
        })
    }

    /// Dispatches notifications to all configured channels (Discord and Generic Webhook).
    pub async fn dispatch(&self, targets: &[NotificationTarget]) {
        let discord_fut = async {
            if let Some(ref url) = self.discord_webhook {
                self.send_discord(url, targets).await;
            }
        };

        let generic_fut = async {
            if let Some(ref url) = self.webhook_url {
                self.send_generic(url, targets).await;
            }
        };

        tokio::join!(discord_fut, generic_fut);
    }

    async fn send_discord(&self, webhook_url: &str, targets: &[NotificationTarget]) {
        info!("Sending Discord going-live webhook notification");
        let payload = self.discord_payload(targets);

        if self
            .http_client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await
            .is_err()
        {
            warn!("Failed to send Discord webhook notification");
        }
    }

    fn discord_payload(&self, targets: &[NotificationTarget]) -> serde_json::Value {
        let mut links = Vec::new();
        for target in targets {
            if let Some(url) = &target.public_url
                && !url.trim().is_empty()
            {
                links.push(format!("[{}]({})", target.name, url.trim()));
            }
        }

        let description = if links.is_empty() {
            "The stream has started.".to_string()
        } else {
            format!(
                "The stream has started.\n\n**Watch live on:**\n{}",
                links.join("\n")
            )
        };

        serde_json::json!({
            "content": self.live_message,
            "allowed_mentions": {
                "parse": ["everyone", "roles", "users"]
            },
            "embeds": [
                {
                    "title": "🔴 We are LIVE",
                    "description": description,
                    "color": 15258703
                }
            ]
        })
    }

    async fn send_generic(&self, webhook_url: &str, targets: &[NotificationTarget]) {
        info!("Sending generic stream.started webhook notification");
        let now = now_unix_secs();
        let payload = self.generic_payload(targets, now);

        if self
            .http_client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await
            .is_err()
        {
            warn!("Failed to send generic webhook notification");
        }
    }

    fn generic_payload(&self, targets: &[NotificationTarget], timestamp: u64) -> serde_json::Value {
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
        let dispatcher = NotificationDispatcher::new(
            &NotificationSettings {
                discord_webhook: None,
                live_message: "Live".into(),
                webhook_url: Some("https://example.test/hook".into()),
            },
            Client::new(),
        );
        let payload = dispatcher.generic_payload(
            &[NotificationTarget {
                name: "Twitch".into(),
                public_url: Some("https://example.test/watch".into()),
            }],
            123,
        );

        assert!(payload.get("stream_key").is_none());
        assert!(!payload.to_string().contains("super-secret-stream-key"));
    }

    #[test]
    fn discord_payload_formats_links() {
        let dispatcher = NotificationDispatcher::new(
            &NotificationSettings {
                discord_webhook: Some("https://discord.test/hook".into()),
                live_message: "Live now!".into(),
                webhook_url: None,
            },
            Client::new(),
        );
        let payload = dispatcher.discord_payload(&[
            NotificationTarget {
                name: "Twitch".into(),
                public_url: Some("https://twitch.tv/example".into()),
            },
            NotificationTarget {
                name: "YouTube".into(),
                public_url: None,
            },
        ]);

        assert_eq!(payload["content"], "Live now!");
        let desc = payload["embeds"][0]["description"].as_str().unwrap();
        assert!(desc.contains("[Twitch](https://twitch.tv/example)"));
    }
}
