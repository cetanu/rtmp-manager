use crate::config::ChatSettings;
use crate::web::components::ui::form::{clearable_secret_field, form_field};
use crate::web::components::ui::input::input;
use topcoat::{
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn chat_ingest_fields(chat: &ChatSettings) -> Result {
    view! {
        <div class="grid gap-6 md:grid-cols-2">
            clearable_secret_field(
                control_id: "chat_ingest_token",
                name: "chat[ingest_token]",
                clear_name: "chat[clear_ingest_token]",
                label_text: "Generic ingest bearer token",
                empty_placeholder: "Not configured",
                configured: chat
                    .ingest_token
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
            )
            form_field(
                control_id: "chat_queue_capacity",
                label_text: "Queue capacity",
                input(attrs: attributes! {
                    type="number"
                    id="chat_queue_capacity"
                    name="chat[queue_capacity]"
                    min="1"
                    value=(chat.queue_capacity)
                    required="true"
                })
            )
            form_field(
                control_id: "twitch_channel",
                label_text: "Twitch channel",
                attrs: attributes! { class="md:col-span-2" },
                input(attrs: attributes! {
                    id="twitch_channel"
                    name="chat[twitch_channel]"
                    value=(chat.twitch_channel.clone().unwrap_or_default())
                })
            )
        </div>
    }
}
