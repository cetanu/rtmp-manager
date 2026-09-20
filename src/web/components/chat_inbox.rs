use crate::chat::ChatMessage;
use crate::server::state::AppHandle;
use crate::web::components::chat_source::{chat_source_icon, source_color};
use crate::web::components::ui::button::{ButtonSize, ButtonVariant, button_variants};
use crate::web::components::ui::card::{card, card_content, card_footer};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{procedure, shard, signal},
    view::{View, attributes, component, view},
};

#[procedure]
async fn acknowledge_chat(cx: &Cx, displayed_id: String) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    let displayed_id = displayed_id
        .parse()
        .map_err(|error| anyhow::anyhow!("Invalid displayed chat message ID: {error}"))?;
    if !app.chat.acknowledge(displayed_id).await? {
        return Err(anyhow::anyhow!(
            "The displayed message changed before it could be acknowledged"
        )
        .into());
    }
    Ok(first_message_id(&app.chat.snapshot().await?))
}

#[procedure]
async fn send_test_chat(cx: &Cx) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    app.chat.enqueue_test(None).await?;
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
    app.set_youtube_polling(enabled).await?;
    Ok(String::new())
}

#[procedure]
async fn set_x_webhook(cx: &Cx, enabled: bool) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    app.set_x_webhook(enabled).await?;
    Ok(String::new())
}

#[procedure]
async fn set_kick_webhook(cx: &Cx, enabled: bool) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    app.set_kick_webhook(enabled).await?;
    Ok(String::new())
}

fn first_message_id(snapshot: &crate::chat::ChatInboxSnapshot) -> String {
    snapshot
        .messages
        .first()
        .map(|message| message.id.to_string())
        .unwrap_or_default()
}

#[component]
pub async fn chat_inbox(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = app_context(cx);
    let initial_id = first_message_id(&app.chat.snapshot().await?);
    let youtube_status = app.chat.youtube_status();
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

    let current_id = signal(cx, || initial_id);
    let revision = signal(cx, || 0.0);
    let youtube_polling_enabled = signal(cx, || chat.youtube_polling_enabled);
    let youtube_toggle_pending = signal(cx, || false);
    let x_webhook_enabled = signal(cx, || chat.x_webhook_enabled);
    let x_toggle_pending = signal(cx, || false);
    let kick_webhook_enabled = signal(cx, || chat.kick_webhook_enabled);
    let kick_toggle_pending = signal(cx, || false);
    let polling_error = signal(cx, String::new);

    Ok(view! {
        card(
            attrs: attributes! { class="mb-2" },
            chat_inbox_content(revision: $(revision.get()))
            card_footer(
                attrs: attributes! { class="justify-between" },
                <div class="flex items-center gap-4">
                    if youtube_configured {
                        <div
                            class="flex items-center gap-2 text-xs font-medium text-muted-foreground"
                        >
                            <button
                                type="button"
                                role="switch"
                                aria-label="Toggle YouTube polling"
                                class="group relative inline-flex h-4.5 w-8 shrink-0 rounded-full bg-foreground/20 shadow-xs transition-colors outline-none data-[checked]:bg-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50"
                                :aria-checked=$(if youtube_polling_enabled.get() {
                                    "true"
                                } else {
                                    "false"
                                })
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
                                <span
                                    class="pointer-events-none absolute top-1/2 left-0.5 size-3.5 -translate-y-1/2 rounded-full bg-background shadow-xs transition-transform group-data-[checked]:translate-x-3.5"
                                ></span>
                            </button>
                            <span>"YouTube polling"</span>
                        </div>
                    }
                    <div
                        class="flex items-center gap-2 text-xs font-medium text-muted-foreground"
                    >
                        <button
                            type="button"
                            role="switch"
                            aria-label="Toggle X webhook"
                            class="group relative inline-flex h-4.5 w-8 shrink-0 rounded-full bg-foreground/20 shadow-xs transition-colors outline-none data-[checked]:bg-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50"
                            :aria-checked=$(if x_webhook_enabled.get() {
                                "true"
                            } else {
                                "false"
                            })
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
                            <span
                                class="pointer-events-none absolute top-1/2 left-0.5 size-3.5 -translate-y-1/2 rounded-full bg-background shadow-xs transition-transform group-data-[checked]:translate-x-3.5"
                            ></span>
                        </button>
                        <span>"X chat"</span>
                    </div>
                    <div
                        class="flex items-center gap-2 text-xs font-medium text-muted-foreground"
                    >
                        <button
                            type="button"
                            role="switch"
                            aria-label="Toggle Kick chat"
                            class="group relative inline-flex h-4.5 w-8 shrink-0 rounded-full bg-foreground/20 shadow-xs transition-colors outline-none data-[checked]:bg-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50"
                            :aria-checked=$(if kick_webhook_enabled.get() {
                                "true"
                            } else {
                                "false"
                            })
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
                            <span
                                class="pointer-events-none absolute top-1/2 left-0.5 size-3.5 -translate-y-1/2 rounded-full bg-background shadow-xs transition-transform group-data-[checked]:translate-x-3.5"
                            ></span>
                        </button>
                        <span>"Kick chat"</span>
                    </div>
                    <p
                        :hidden=$(polling_error.get().is_empty())
                        class="text-xs text-destructive"
                    >
                        $(polling_error.get())
                    </p>
                    if let Some(status) = youtube_status {
                        <p
                            class="max-w-md truncate text-xs text-muted-foreground"
                            title=(status.detail.clone())
                        >
                            (format!(
                                "YouTube {}: {}",
                                status.state.as_str(),
                                status.detail,
                            ))
                        </p>
                    }
                </div>
                <div class="ml-auto flex items-center gap-2">
                    <button
                        id="chat-test-button"
                        type="button"
                        class=(outline_button.clone())
                        @click=$(async |_event| {
                            let next_id = send_test_chat().await;
                            current_id.set(next_id);
                            revision.set(revision.get() + 1.0);
                        })
                    >
                        "Test Message"
                    </button>
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
        <div class="mb-8 flex justify-end px-1">
            <a
                href="/overlay/chat"
                target="_blank"
                rel="noopener noreferrer"
                class="inline-flex items-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground"
                title="Open OBS browser source overlay in a new tab"
            >
                <svg
                    aria-hidden="true"
                    width="13"
                    height="13"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path
                        d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"
                    />
                    <polyline points="15 3 21 3 21 9" />
                    <line x1="10" y1="14" x2="21" y2="3" />
                </svg>
                <span>"OBS Overlay"</span>
            </a>
        </div>
    })
}

#[shard]
pub async fn chat_inbox_content(cx: &Cx, revision: f64) -> Result<impl View> {
    let _ = revision;
    let app: &AppHandle = app_context(cx);
    let snapshot = app.chat.snapshot().await?;

    Ok(view! {
        card_content(
            <div class="h-[min(22rem,calc(100dvh-10rem))] overflow-y-auto pr-1">
                <div class="flex flex-col gap-2">
                    if snapshot.messages.is_empty() {
                        "No chat messages waiting."
                    } else {
                        for (index, message) in snapshot
                            .messages
                            .into_iter()
                            .enumerate() {
                            chat_message_card(message: message, highlighted: index == 0)
                        }
                    }
                </div>
            </div>
            <div class="mb-4 flex justify-end gap-4 text-right">
                <span class="text-sm font-medium">
                    (format!("{} queued", snapshot.queued))
                </span>
                <span class="text-sm text-muted-foreground">
                    (format!("{} dropped", snapshot.dropped))
                </span>
            </div>
        )
    })
}

#[component]
pub async fn chat_message_card(message: ChatMessage, highlighted: bool) -> Result<impl View> {
    let row_class = if highlighted {
        "grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2 rounded-md bg-primary/10 px-2 py-1.5 ring-1 ring-primary/30"
    } else {
        "grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2 px-2 py-1.5"
    };
    let author_color = source_color(message.source);

    Ok(view! {
        <article class=(row_class)>
            chat_source_icon(source: message.source)
            <p class="min-w-0 break-words text-sm leading-snug">
                <span class=(format!("mr-1 font-semibold {author_color}"))>
                    (message.author)
                </span>
                (message.text)
            </p>
        </article>
    })
}
