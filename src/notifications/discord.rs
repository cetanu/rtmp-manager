use crate::notifications::NotificationTarget;
use anyhow::Result;
use reqwest::Client;
use tracing::{info, warn};

pub struct DiscordNotifier {
    webhook_url: String,
    live_message: String,
    http_client: Client,
}

impl DiscordNotifier {
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

        info!("Sending Discord going-live webhook notification");

        let mut links = Vec::new();
        for target in targets {
            if let Some(url) = &target.public_url {
                if !url.trim().is_empty() {
                    links.push(format!("[{}]({})", target.name, url.trim()));
                }
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

        let payload = serde_json::json!({
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
        });

        if self
            .http_client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .is_err()
        {
            warn!("Failed to send Discord webhook notification");
        }

        Ok(())
    }
}
