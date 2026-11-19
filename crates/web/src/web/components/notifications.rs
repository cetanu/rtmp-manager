use crate::web::components::configuration_form::settings_section;
use crate::web::components::ui::form::{clearable_secret_field, form_field};
use crate::web::components::ui::textarea::textarea;
use rtmp_core::server::state::AppHandle;
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, attributes, component, view},
};

#[component]
pub async fn notifications(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = context(cx);
    let config = app.config.get();
    Ok(view! {
        settings_section(
            title: "Discord notifications",
            form_field(
                control_id: "live_message",
                label_text: "Go-Live Message",
                textarea(
                    attrs: attributes! {
                        id="live_message"
                        name="notifications[live_message]"
                        placeholder="Stream is LIVE!"
                    },
                    (config.notifications.live_message.clone())
                )
            )
            <div class="flex flex-col gap-6">
                clearable_secret_field(
                    control_id: "discord_webhook",
                    name: "notifications[discord_webhook]",
                    label_text: "Webhook URL",
                    empty_placeholder: "https://discord.com/api/webhooks/...",
                    value: config
                        .notifications
                        .discord_webhook
                        .clone()
                        .unwrap_or_default()
                )
            </div>
        )
    })
}
