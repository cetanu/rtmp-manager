use crate::config::ChatSettings;
use crate::web::components::ui::form::form_field;
use crate::web::components::ui::input::input;
use topcoat::{
    Result,
    view::{attributes, component, view},
};

#[component]
pub async fn chat_ingest_fields(chat: &ChatSettings) -> Result {
    view! {
        <div class="grid gap-6 md:grid-cols-2">
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
