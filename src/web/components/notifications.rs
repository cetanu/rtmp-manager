use crate::server::state::ProxyState;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::form::{clearable_secret_field, form_field};
use crate::web::components::ui::textarea::textarea;
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn notifications(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let config = state.config.read().await;
    view! {
        card(
            attrs: attributes! { class="h-full" },
            card_header(
                card_title("Notifications")
            )
            card_content(
                <div class="flex flex-col gap-6">
                    form_field(
                        control_id: "live_message",
                        label_text: "Live Message (Sent when stream starts)",
                        textarea(attrs: attributes! {
                            id="live_message"
                            name="notifications[live_message]"
                            placeholder="Stream is LIVE!"
                        }, (config.notifications.live_message.clone()))
                    )

                    <div class="flex flex-col gap-6">
                        clearable_secret_field(
                            control_id: "discord_webhook",
                            name: "notifications[discord_webhook]",
                            clear_name: "notifications[clear_discord_webhook]",
                            label_text: "Discord Webhook URL (Optional)",
                            empty_placeholder: "https://discord.com/api/webhooks/...",
                            configured: config.notifications.discord_webhook.is_some()
                        )
                        clearable_secret_field(
                            control_id: "generic_webhook",
                            name: "notifications[webhook_url]",
                            clear_name: "notifications[clear_webhook_url]",
                            label_text: "Generic Webhook URL (Optional)",
                            empty_placeholder: "https://api.example.com/webhook",
                            configured: config.notifications.webhook_url.is_some()
                        )
                    </div>
                </div>
            )
        )
    }
}
