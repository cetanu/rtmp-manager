use crate::server::state::ProxyState;
use crate::web::components::chat_inbox_content::chat_inbox_content;
use crate::web::components::ui::button::{ButtonSize, ButtonVariant, button_variants};
use crate::web::components::ui::card::{card, card_footer};
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::procedure,
    view::{attributes, component, view},
};

#[procedure]
async fn acknowledge_chat(cx: &Cx, displayed_id: String) -> Result<String> {
    let state: &Arc<ProxyState> = app_context(cx);
    let mut inbox = state.chat_inbox.lock().await;
    if let Ok(displayed_id) = displayed_id.parse()
        && inbox.acknowledge(displayed_id).await?
    {
        state.notify_chat_changed();
    }
    Ok(first_message_id(&inbox.snapshot().await?))
}

#[procedure]
async fn refresh_chat(cx: &Cx) -> Result<String> {
    let state: &Arc<ProxyState> = app_context(cx);
    Ok(first_message_id(
        &state.chat_inbox.lock().await.snapshot().await?,
    ))
}

#[procedure]
async fn toggle_youtube_polling(cx: &Cx) -> Result<String> {
    let state: &Arc<ProxyState> = app_context(cx);
    let mut config_write = state.config.write().await;
    let mut updated = config_write.clone();
    updated.chat.youtube_polling_enabled = !updated.chat.youtube_polling_enabled;
    state.config_store.save(&updated).await?;
    let enabled = updated.chat.youtube_polling_enabled;
    *config_write = updated;
    drop(config_write);
    state.apply_chat_config().await?;
    state.notify_chat_changed();
    Ok(if enabled { "on" } else { "off" }.into())
}

#[procedure]
async fn toggle_x_polling(cx: &Cx) -> Result<String> {
    let state: &Arc<ProxyState> = app_context(cx);
    let mut config_write = state.config.write().await;
    let mut updated = config_write.clone();
    updated.chat.x_polling_enabled = !updated.chat.x_polling_enabled;
    state.config_store.save(&updated).await?;
    let enabled = updated.chat.x_polling_enabled;
    *config_write = updated;
    drop(config_write);
    state.apply_chat_config().await?;
    state.notify_chat_changed();
    Ok(if enabled { "on" } else { "off" }.into())
}

fn first_message_id(snapshot: &crate::chat::ChatInboxSnapshot) -> String {
    snapshot
        .messages
        .first()
        .map(|message| message.id.to_string())
        .unwrap_or_default()
}

#[component]
pub async fn chat_inbox(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let initial_id = first_message_id(&state.chat_inbox.lock().await.snapshot().await?);
    let chat = state.config.read().await.chat.clone();
    let youtube_configured = chat
        .youtube_api_key
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty())
        && [
            chat.youtube_live_chat_id,
            chat.youtube_video_id,
            chat.youtube_channel_id,
        ]
        .into_iter()
        .any(|value| value.is_some_and(|value| !value.trim().is_empty()));
    let x_configured = chat
        .x_media_key
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty());
    let outline_button = button_variants(ButtonVariant::Outline, ButtonSize::Md);
    let primary_button = button_variants(ButtonVariant::Primary, ButtonSize::Md);

    view! {
        signal current_id = initial_id;
        signal revision = 0.0;
        signal youtube_polling_enabled = chat.youtube_polling_enabled;
        signal youtube_toggle_pending = false;
        signal x_polling_enabled = chat.x_polling_enabled;
        signal x_toggle_pending = false;

        card(
            attrs: attributes! { class="mb-8" },
            chat_inbox_content(revision: $(revision.get()))
            card_footer(
                attrs: attributes! { class="justify-between" },
                <div class="flex items-center gap-4">
                    if youtube_configured {
                        <label class="flex cursor-pointer items-center gap-2 text-xs font-medium text-muted-foreground has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-50">
                            <span class="relative inline-flex shrink-0 has-[:disabled]:opacity-50">
                                <input
                                type="checkbox"
                                role="switch"
                                aria-label="Toggle YouTube polling"
                                class="peer h-4.5 w-8 shrink-0 appearance-none rounded-full bg-foreground/20 shadow-xs transition-colors outline-none checked:bg-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none"
                                :checked=$(youtube_polling_enabled.get())
                                :disabled=$(youtube_toggle_pending.get())
                                @click=$(async |_event| {
                                youtube_toggle_pending.set(true);
                                let state = toggle_youtube_polling().await;
                                youtube_polling_enabled.set(state == "on");
                                revision.set(revision.get() + 1.0);
                                youtube_toggle_pending.set(false);
                                })
                                />
                                <span class="pointer-events-none absolute top-1/2 left-0.5 size-3.5 -translate-y-1/2 rounded-full bg-background shadow-xs transition-transform peer-checked:translate-x-3.5"></span>
                            </span>
                            "YouTube polling"
                        </label>
                    }
                    if x_configured {
                        <label class="flex cursor-pointer items-center gap-2 text-xs font-medium text-muted-foreground has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-50">
                            <span class="relative inline-flex shrink-0 has-[:disabled]:opacity-50">
                                <input
                                type="checkbox"
                                role="switch"
                                aria-label="Toggle X polling"
                                class="peer h-4.5 w-8 shrink-0 appearance-none rounded-full bg-foreground/20 shadow-xs transition-colors outline-none checked:bg-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none"
                                :checked=$(x_polling_enabled.get())
                                :disabled=$(x_toggle_pending.get())
                                @click=$(async |_event| {
                                x_toggle_pending.set(true);
                                let state = toggle_x_polling().await;
                                x_polling_enabled.set(state == "on");
                                revision.set(revision.get() + 1.0);
                                x_toggle_pending.set(false);
                                })
                                />
                                <span class="pointer-events-none absolute top-1/2 left-0.5 size-3.5 -translate-y-1/2 rounded-full bg-background shadow-xs transition-transform peer-checked:translate-x-3.5"></span>
                            </span>
                            "X polling"
                        </label>
                    }
                </div>
                <div class="ml-auto flex items-center gap-2">
                    <button
                        id="chat-refresh-button"
                        type="button"
                        class=(outline_button)
                        @click=$(async |_event| {
                            let refreshed_id = refresh_chat().await;
                            current_id.set(refreshed_id);
                            revision.set(revision.get() + 1.0);
                        })
                    >
                        "Check"
                    </button>
                    <button
                        type="button"
                        class=(primary_button)
                        :disabled=$(current_id.get().is_empty())
                        @click=$(async |_event| {
                            let next_id = acknowledge_chat(current_id.get()).await;
                            current_id.set(next_id);
                            revision.set(revision.get() + 1.0);
                        })
                    >
                        "Acknowledge"
                    </button>
                </div>
            )
        )
    }
}
