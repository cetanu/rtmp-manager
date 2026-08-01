use crate::config::ChatSettings;
use crate::web::components::ui::form::{
    clearable_secret_field, field_description, form_field, switch_field,
};
use crate::web::components::ui::input::input;
use topcoat::{
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn youtube_chat_fields(chat: &ChatSettings) -> Result {
    view! {
        <div class="flex flex-col gap-6">
            clearable_secret_field(
                control_id: "youtube_api_key",
                name: "chat[youtube_api_key]",
                clear_name: "chat[clear_youtube_api_key]",
                label_text: "YouTube API key",
                empty_placeholder: "Not configured",
                configured: chat
                    .youtube_api_key
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
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
            field_description(
                "Configure at most one YouTube selector. Video and channel IDs are resolved to the active chat."
            )
            <div class="grid gap-6 md:grid-cols-2">
                form_field(
                    control_id: "youtube_poll_interval",
                    label_text: "Minimum YouTube poll interval (seconds)",
                    input(attrs: attributes! {
                        type="number"
                        id="youtube_poll_interval"
                        name="chat[youtube_min_poll_interval_secs]"
                        min="1"
                        value=(chat.youtube_min_poll_interval_secs)
                        required="true"
                    })
                )
                switch_field(
                    control_id: "youtube_adaptive_polling",
                    name: "chat[youtube_adaptive_polling]",
                    label_text: "Back off polling while chat is idle",
                    checked: chat.youtube_adaptive_polling,
                    attrs: attributes! { class="pt-7" }
                )
            </div>
        </div>
    }
}
