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
                            label_text: "YouTube API key (optional)",
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
                                label_text: "YouTube channel handle or ID",
                                input(attrs: attributes! {
                                    id="youtube_channel_id"
                                    name="chat[youtube_channel_id]"
                                    value=(chat.youtube_channel_id.clone().unwrap_or_default())
                                })
                            )
                        </div>
                    </div>

                    <div class="grid gap-6 md:grid-cols-2">
                        clearable_secret_field(control_id: "x_api_key", name: "chat[x_api_key]", label_text: "X API key", empty_placeholder: "Not configured", value: chat.x_api_key.clone().unwrap_or_default())
                        clearable_secret_field(control_id: "x_api_secret", name: "chat[x_api_secret]", label_text: "X API secret key / consumer secret", empty_placeholder: "Required for webhook verification", value: chat.x_api_secret.clone().unwrap_or_default())
                        clearable_secret_field(control_id: "x_client_id", name: "chat[x_client_id]", label_text: "X OAuth client ID", empty_placeholder: "Not configured", value: chat.x_client_id.clone().unwrap_or_default())
                        clearable_secret_field(control_id: "x_client_secret", name: "chat[x_client_secret]", label_text: "X OAuth client secret", empty_placeholder: "Not configured", value: chat.x_client_secret.clone().unwrap_or_default())
                    </div>

                    <div class="grid gap-6 md:grid-cols-2">
                        form_field(
                            control_id: "kick_client_id",
                            label_text: "Kick client ID",
                            input(attrs: attributes! {
                                id="kick_client_id"
                                name="chat[kick_client_id]"
                                value=(chat.kick_client_id.clone().unwrap_or_default())
                            })
                        )
                        clearable_secret_field(
                            control_id: "kick_client_secret",
                            name: "chat[kick_client_secret]",
                            label_text: "Kick client secret",
                            empty_placeholder: "Required to manage webhook subscriptions",
                            value: chat.kick_client_secret.clone().unwrap_or_default()
                        )
                        form_field(
                            control_id: "kick_broadcaster_user_id",
                            label_text: "Kick broadcaster user ID",
                            attrs: attributes! { class="md:col-span-2" },
                            input(attrs: attributes! {
                                id="kick_broadcaster_user_id"
                                name="chat[kick_broadcaster_user_id]"
                                type="number"
                                min="1"
                                value=(chat.kick_broadcaster_user_id.map(|id| id.to_string()).unwrap_or_default())
                            })
                        )
                    </div>
                </div>
            )
        )
    }
}
