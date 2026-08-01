pub mod discord;
pub mod generic;

use crate::config::NotificationSettings;
use discord::DiscordNotifier;
use generic::GenericWebhookNotifier;
use reqwest::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct NotificationTarget {
    pub name: String,
    pub public_url: Option<String>,
}

impl From<&crate::config::TargetConfig> for NotificationTarget {
    fn from(target: &crate::config::TargetConfig) -> Self {
        Self {
            name: target.name.clone(),
            public_url: target.public_url.clone(),
        }
    }
}

pub struct NotificationDispatcher {
    discord: Option<DiscordNotifier>,
    generic: Option<GenericWebhookNotifier>,
}

impl NotificationDispatcher {
    pub fn new(settings: &NotificationSettings, http_client: Client) -> Arc<Self> {
        let discord = settings.discord_webhook.as_ref().map(|url| {
            DiscordNotifier::new(
                url.clone(),
                settings.live_message.clone(),
                http_client.clone(),
            )
        });

        let generic = settings.webhook_url.as_ref().map(|url| {
            GenericWebhookNotifier::new(url.clone(), settings.live_message.clone(), http_client)
        });

        Arc::new(Self { discord, generic })
    }

    pub async fn dispatch(&self, targets: &[NotificationTarget]) {
        if let Some(ref d) = self.discord {
            let _ = d.notify(targets).await;
        }

        if let Some(ref g) = self.generic {
            let _ = g.notify(targets).await;
        }
    }
}
