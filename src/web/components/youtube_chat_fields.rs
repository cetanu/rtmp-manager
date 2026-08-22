use crate::config::ChatSettings;
use crate::web::components::ui::form::{clearable_secret_field, form_field};
use crate::web::components::ui::input::input;
use topcoat::{
    Result,
    view::{attributes, component, view},
};

#[component]
pub async fn youtube_chat_fields(chat: &ChatSettings) -> Result {
    view! {
        <div class="flex flex-col gap-6">
            clearable_secret_field(
                control_id: "youtube_api_key",
                name: "chat[youtube_api_key]",
                label_text: "YouTube API key",
                empty_placeholder: "Not configured",
                value: chat.youtube_api_key.clone().unwrap_or_default()
            )
            <div class="grid gap-6 md:grid-cols-3">
                form_field(
                    control_id: "youtube_live_chat_id",
                    label_text: "YouTube live chat ID",
                    input(attrs: attributes! {
                        id="youtube_live_chat_id"
                        name="chat[youtube_live_chat_id]"
                        value=(chat.youtube_live_chat_id.clone().unwrap_or_default())
                    })
                )
                form_field(
                    control_id: "youtube_video_id",
                    label_text: "YouTube video ID",
                    input(attrs: attributes! {
                        id="youtube_video_id"
                        name="chat[youtube_video_id]"
                        value=(chat.youtube_video_id.clone().unwrap_or_default())
                    })
                )
                form_field(
                    control_id: "youtube_channel_id",
                    label_text: "YouTube channel ID",
                    input(attrs: attributes! {
                        id="youtube_channel_id"
                        name="chat[youtube_channel_id]"
                        value=(chat.youtube_channel_id.clone().unwrap_or_default())
                    })
                )
            </div>
        </div>
    }
}
