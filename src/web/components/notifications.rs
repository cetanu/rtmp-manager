use crate::server::state::AppHandle;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::form::{clearable_secret_field, form_field};
use crate::web::components::ui::textarea::textarea;
use topcoat::{
    Result,
    context::{Cx, app_context},
    view::{attributes, component, view},
};

#[component]
pub async fn notifications(cx: &Cx) -> Result {
    let app: &AppHandle = app_context(cx);
    let config = app.config.get();
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
                            label_text: "Discord Webhook URL",
                            empty_placeholder: "https://discord.com/api/webhooks/...",
                            value: config.notifications.discord_webhook.clone().unwrap_or_default()
                        )
                    </div>
                </div>
            )
        )
    }
}
