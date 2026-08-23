use crate::server::state::AppHandle;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::form::{clearable_secret_field, form_field};
use crate::web::components::ui::input::input;
use topcoat::{
    Result,
    context::{Cx, app_context},
    view::{attributes, component, view},
};

#[component]
pub async fn chat_settings(cx: &Cx) -> Result {
    let app: &AppHandle = app_context(cx);
    let chat = app.config.get().chat.clone();

    view! {
        card(
            attrs: attributes! { class="h-full" },
            card_header(
                card_title("Chat Ingest")
            )
            card_content(
                <div class="flex flex-col gap-6">
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
                    </div>
                </div>
            )
        )
    }
}
