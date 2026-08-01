use crate::server::state::ProxyState;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::input::input;
use crate::web::components::ui::label::label;
use crate::web::components::ui::switch::switch;
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn chat_settings(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let chat = state.config.read().await.chat.clone();
    view! {
        card(
            attrs: attributes! { class="mb-8" },
            card_header(
                <div class="flex flex-col gap-1">
                    card_title("Chat Ingest")
                    <p class="text-sm text-muted-foreground">
                        "Changes are persisted to SQLite and applied without restarting the server."
                    </p>
                </div>
            )
            card_content(
                <div class="flex flex-col gap-6">
                    <div class="grid gap-6 md:grid-cols-2">
                        secret_field(
                            id: "chat_ingest_token",
                            name: "chat[ingest_token]",
                            clear_name: "chat[clear_ingest_token]",
                            label_text: "Generic ingest bearer token",
                            configured: chat
                                .ingest_token
                                .as_ref()
                                .is_some_and(|value| !value.trim().is_empty())
                        )
                        <div class="flex flex-col gap-2">
                            label(attrs: attributes! { for="chat_queue_capacity" }, "Queue capacity")
                            input(attrs: attributes! {
                                type="number"
                                id="chat_queue_capacity"
                                name="chat[queue_capacity]"
                                min="1"
                                value=(chat.queue_capacity)
                                required="true"
                            })
                        </div>
                    </div>

                    <div class="grid gap-6 md:grid-cols-2">
                        text_field(
                            id: "twitch_channel",
                            name: "chat[twitch_channel]",
                            label_text: "Twitch channel",
                            value: chat.twitch_channel
                        )
                        secret_field(
                            id: "youtube_api_key",
                            name: "chat[youtube_api_key]",
                            clear_name: "chat[clear_youtube_api_key]",
                            label_text: "YouTube API key",
                            configured: chat
                                .youtube_api_key
                                .as_ref()
                                .is_some_and(|value| !value.trim().is_empty())
                        )
                    </div>

                    <div class="grid gap-6 md:grid-cols-3">
                        text_field(
                            id: "youtube_live_chat_id",
                            name: "chat[youtube_live_chat_id]",
                            label_text: "YouTube live chat ID",
                            value: chat.youtube_live_chat_id
                        )
                        text_field(
                            id: "youtube_video_id",
                            name: "chat[youtube_video_id]",
                            label_text: "YouTube video ID",
                            value: chat.youtube_video_id
                        )
                        text_field(
                            id: "youtube_channel_id",
                            name: "chat[youtube_channel_id]",
                            label_text: "YouTube channel ID",
                            value: chat.youtube_channel_id
                        )
                    </div>
                    <p class="-mt-4 text-xs text-muted-foreground">
                        "Configure at most one YouTube selector. Video and channel IDs are resolved to the active chat."
                    </p>

                    <div class="grid gap-6 md:grid-cols-2">
                        <div class="flex flex-col gap-2">
                            label(attrs: attributes! { for="youtube_poll_interval" }, "Minimum YouTube poll interval (seconds)")
                            input(attrs: attributes! {
                                type="number"
                                id="youtube_poll_interval"
                                name="chat[youtube_min_poll_interval_secs]"
                                min="1"
                                value=(chat.youtube_min_poll_interval_secs)
                                required="true"
                            })
                        </div>
                        <div class="flex items-center gap-3 pt-7">
                            switch(attrs: attributes! {
                                id="youtube_adaptive_polling"
                                name="chat[youtube_adaptive_polling]"
                                value="true"
                                checked=(chat.youtube_adaptive_polling)
                            })
                            label(attrs: attributes! { for="youtube_adaptive_polling" }, "Back off polling while chat is idle")
                        </div>
                    </div>
                </div>
            )
        )
    }
}

#[component]
async fn secret_field(
    id: &'static str,
    name: &'static str,
    clear_name: &'static str,
    label_text: &'static str,
    configured: bool,
) -> Result {
    let clear_id = format!("clear_{id}");
    view! {
        <div class="flex flex-col gap-2">
            label(attrs: attributes! { for=(id) }, (label_text))
            input(attrs: attributes! {
                type="password"
                id=(id)
                name=(name)
                value=""
                placeholder=(if configured { "Configured — leave blank to keep it" } else { "Not configured" })
            })
            <div class="flex items-center gap-2">
                switch(attrs: attributes! {
                    id=(clear_id.clone())
                    name=(clear_name)
                    value="true"
                })
                label(
                    attrs: attributes! { for=(clear_id) class="text-muted-foreground" },
                    "Clear configured value"
                )
            </div>
        </div>
    }
}

#[component]
async fn text_field(
    id: &'static str,
    name: &'static str,
    label_text: &'static str,
    value: Option<String>,
) -> Result {
    view! {
        <div class="flex flex-col gap-2">
            label(attrs: attributes! { for=(id) }, (label_text))
            input(attrs: attributes! {
                id=(id)
                name=(name)
                value=(value.unwrap_or_default())
            })
        </div>
    }
}
