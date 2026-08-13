use crate::config::ChatSettings;
use crate::web::components::ui::form::{field_description, form_field, switch_field};
use crate::web::components::ui::input::input;
use topcoat::{
    Result,
    view::{attributes, component, view},
};

#[component]
pub async fn x_chat_fields(chat: &ChatSettings) -> Result {
    view! {
        <div class="flex flex-col gap-4">
            form_field(
                control_id: "x_media_key",
                label_text: "X broadcast/media key",
                input(attrs: attributes! {
                    id="x_media_key"
                    name="chat[x_media_key]"
                    value=(chat.x_media_key.clone().unwrap_or_default())
                })
            )
            field_description(
                "The media key is the identifier used by X's live broadcast status endpoint."
            )
            switch_field(
                control_id: "x_polling_enabled",
                name: "chat[x_polling_enabled]",
                label_text: "Enable X live chat polling",
                checked: chat.x_polling_enabled
            )
        </div>
    }
}
