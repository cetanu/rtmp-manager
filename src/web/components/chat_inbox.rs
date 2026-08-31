use crate::chat::ChatMessage;
use crate::server::state::AppHandle;
use crate::web::components::ui::button::{ButtonSize, ButtonVariant, button_variants};
use crate::web::components::ui::card::{card, card_content, card_footer};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{procedure, shard},
    view::{attributes, component, view},
};

#[procedure]
async fn acknowledge_chat(cx: &Cx, displayed_id: String) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    if let Ok(displayed_id) = displayed_id.parse() {
        let _ = app.chat.acknowledge(displayed_id).await?;
    }
    Ok(first_message_id(&app.chat.snapshot().await?))
}

#[procedure]
async fn refresh_chat(cx: &Cx) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    Ok(first_message_id(&app.chat.snapshot().await?))
}

#[procedure]
async fn set_youtube_polling(cx: &Cx, enabled: bool) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    Ok(app
        .set_youtube_polling(enabled)
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default())
}

#[procedure]
async fn set_x_webhook(cx: &Cx, enabled: bool) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    Ok(app
        .set_x_webhook(enabled)
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default())
}

#[procedure]
async fn set_kick_webhook(cx: &Cx, enabled: bool) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    Ok(app
        .set_kick_webhook(enabled)
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default())
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
    let app: &AppHandle = app_context(cx);
    let initial_id = first_message_id(&app.chat.snapshot().await?);
    let chat = app.config.get().chat.clone();
    let youtube_configured = [
        &chat.youtube_live_chat_id,
        &chat.youtube_video_id,
        &chat.youtube_channel_id,
    ]
    .into_iter()
    .any(|value| value.as_ref().is_some_and(|value| !value.trim().is_empty()));
    let outline_button = button_variants(ButtonVariant::Outline, ButtonSize::Md);
    let primary_button = button_variants(ButtonVariant::Primary, ButtonSize::Md);

    view! {
        signal current_id = initial_id;
        signal revision = 0.0;
        signal youtube_polling_enabled = chat.youtube_polling_enabled;
        signal youtube_toggle_pending = false;
        signal x_webhook_enabled = chat.x_webhook_enabled;
        signal x_toggle_pending = false;
        signal kick_webhook_enabled = chat.kick_webhook_enabled;
        signal kick_toggle_pending = false;
        signal polling_error = String::new();

        card(
            attrs: attributes! { class="mb-8" },
            chat_inbox_content(revision: $(revision.get()))
            card_footer(
                attrs: attributes! { class="justify-between" },
                <div class="flex items-center gap-4">
                    if youtube_configured {
                        <div class="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                                <button
                                    type="button"
                                    role="switch"
                                    aria-label="Toggle YouTube polling"
                                    class="group relative inline-flex h-4.5 w-8 shrink-0 rounded-full bg-foreground/20 shadow-xs transition-colors outline-none data-[checked]:bg-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50"
                                    :aria-checked=$(if youtube_polling_enabled.get() { "true" } else { "false" })
                                    :data-checked=$(youtube_polling_enabled.get())
                                    :disabled=$(youtube_toggle_pending.get())
                                    @click=$(async |_event| {
                                        let enabled = !youtube_polling_enabled.get();
                                        youtube_polling_enabled.set(enabled);
                                        youtube_toggle_pending.set(true);
                                        let error = set_youtube_polling(enabled).await;
                                        if !error.is_empty() {
                                            youtube_polling_enabled.set(!enabled);
                                        }
                                        polling_error.set(error);
                                        youtube_toggle_pending.set(false);
                                    })
                                >
                                    <span class="pointer-events-none absolute top-1/2 left-0.5 size-3.5 -translate-y-1/2 rounded-full bg-background shadow-xs transition-transform group-data-[checked]:translate-x-3.5"></span>
                                </button>
                            <span>"YouTube polling"</span>
                        </div>
                    }
                    <div class="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                                <button
                                    type="button"
                                    role="switch"
                                    aria-label="Toggle X webhook"
                                    class="group relative inline-flex h-4.5 w-8 shrink-0 rounded-full bg-foreground/20 shadow-xs transition-colors outline-none data-[checked]:bg-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50"
                                    :aria-checked=$(if x_webhook_enabled.get() { "true" } else { "false" })
                                    :data-checked=$(x_webhook_enabled.get())
                                    :disabled=$(x_toggle_pending.get())
                                    @click=$(async |_event| {
                                        let enabled = !x_webhook_enabled.get();
                                        x_webhook_enabled.set(enabled);
                                        x_toggle_pending.set(true);
                                        let error = set_x_webhook(enabled).await;
                                        if !error.is_empty() {
                                            x_webhook_enabled.set(!enabled);
                                        }
                                        polling_error.set(error);
                                        x_toggle_pending.set(false);
                                    })
                                >
                                    <span class="pointer-events-none absolute top-1/2 left-0.5 size-3.5 -translate-y-1/2 rounded-full bg-background shadow-xs transition-transform group-data-[checked]:translate-x-3.5"></span>
                                </button>
                            <span>"X chat"</span>
                        </div>
                    <div class="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                        <button
                            type="button"
                            role="switch"
                            aria-label="Toggle Kick chat"
                            class="group relative inline-flex h-4.5 w-8 shrink-0 rounded-full bg-foreground/20 shadow-xs transition-colors outline-none data-[checked]:bg-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50"
                            :aria-checked=$(if kick_webhook_enabled.get() { "true" } else { "false" })
                            :data-checked=$(kick_webhook_enabled.get())
                            :disabled=$(kick_toggle_pending.get())
                            @click=$(async |_event| {
                                let enabled = !kick_webhook_enabled.get();
                                kick_webhook_enabled.set(enabled);
                                kick_toggle_pending.set(true);
                                let error = set_kick_webhook(enabled).await;
                                if !error.is_empty() {
                                    kick_webhook_enabled.set(!enabled);
                                }
                                polling_error.set(error);
                                kick_toggle_pending.set(false);
                            })
                        >
                            <span class="pointer-events-none absolute top-1/2 left-0.5 size-3.5 -translate-y-1/2 rounded-full bg-background shadow-xs transition-transform group-data-[checked]:translate-x-3.5"></span>
                        </button>
                        <span>"Kick chat"</span>
                    </div>
                    <p :hidden=$(polling_error.get().is_empty()) class="text-xs text-destructive">
                        $(polling_error.get())
                    </p>
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

#[shard]
pub async fn chat_inbox_content(cx: &Cx, revision: f64) -> Result {
    let _ = revision;
    let app: &AppHandle = app_context(cx);
    let snapshot = app.chat.snapshot().await?;

    view! {
        card_content(
            <div class="h-[min(22rem,calc(100dvh-10rem))] overflow-y-auto pr-1">
                <div class="flex flex-col gap-2">
                    if snapshot.messages.is_empty() {
                        "No chat messages waiting."
                    } else {
                        for (index, message) in snapshot.messages.into_iter().enumerate() {
                            chat_message_card(message: message, highlighted: index == 0)
                        }
                    }
                </div>
            </div>
            <div class="mb-4 flex justify-end gap-4 text-right">
                <span class="text-sm font-medium">(format!("{} queued", snapshot.queued))</span>
                <span class="text-sm text-muted-foreground">(format!("{} dropped", snapshot.dropped))</span>
            </div>
        )
    }
}

fn source_color(source: &str) -> &'static str {
    match source {
        "twitch" => "text-[#9146ff]",
        "youtube" => "text-[#ff0033]",
        "kick" => "text-[#53fc18]",
        "x" => "text-sky-500",
        _ => "text-muted-foreground",
    }
}

#[component]
async fn chat_source_icon(source: String) -> Result {
    let color = source_color(&source);

    view! {
        <span class=(format!("mt-0.5 shrink-0 {color}")) title=(source.clone())>
            if source == "kick" {
                <span aria-hidden="true" class="flex size-[18px] items-center justify-center text-sm font-black leading-none">"K"</span>
            } else {
                <svg aria-hidden="true" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    if source == "youtube" {
                        <path d="M21.5 7.2a2.8 2.8 0 0 0-2-2C17.7 4.7 12 4.7 12 4.7s-5.7 0-7.5.5a2.8 2.8 0 0 0-2 2C2 9 2 12 2 12s0 3 .5 4.8a2.8 2.8 0 0 0 2 2c1.8.5 7.5.5 7.5.5s5.7 0 7.5-.5a2.8 2.8 0 0 0 2-2C22 15 22 12 22 12s0-3-.5-4.8Z" />
                        <path d="m10 15 5-3-5-3v6Z" fill="currentColor" stroke="none" />
                    } else if source == "twitch" {
                        <path d="M5 3h14v12l-4 4h-4l-3 2v-2H5V3Z" />
                        <path d="M10 8v4M14 8v4" />
                    } else if source == "x" {
                        <path d="M5 4 19 20M19 4 5 20" />
                    } else {
                        <path d="M20 11.5a7.5 7.5 0 0 1-8 7.5 8.6 8.6 0 0 1-3.5-.8L4 20l1.5-4A7.2 7.2 0 0 1 4 11.5 7.5 7.5 0 0 1 12 4a7.5 7.5 0 0 1 8 7.5Z" />
                    }
                </svg>
            }
        </span>
    }
}

#[component]
pub async fn chat_message_card(message: ChatMessage, highlighted: bool) -> Result {
    let row_class = if highlighted {
        "grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2 rounded-md bg-primary/10 px-2 py-1.5 ring-1 ring-primary/30"
    } else {
        "grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2 px-2 py-1.5"
    };
    let author_color = source_color(&message.source);

    view! {
        <article class=(row_class)>
            chat_source_icon(source: message.source)
            <p class="min-w-0 break-words text-sm leading-snug">
                <span class=(format!("mr-1 font-semibold {author_color}"))>(message.author)</span>
                (message.text)
            </p>
        </article>
    }
}
