use crate::chat::ChatMessage;
use crate::server::state::AppHandle;
use crate::util::now_unix_ms;
use crate::web::components::chat_message::chat_message_card as shared_chat_message_card;
use crate::web::components::ui::button::{ButtonSize, ButtonVariant, button_variants};
use crate::web::components::ui::card::{card, card_content, card_footer};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{Event, procedure, shard, signal},
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

#[component]
async fn chat_toggle(
    label: &'static str,
    platform: String,
    enabled: &topcoat::runtime::Signal<bool>,
    pending: &topcoat::runtime::Signal<bool>,
    error: &topcoat::runtime::Signal<String>,
) -> Result<impl View> {
    let enabled = enabled.clone();
    let pending = pending.clone();
    let error = error.clone();

    Ok(view! {
        <div class="flex items-center gap-2 text-xs font-medium text-muted-foreground">
            <button
                type="button"
                role="switch"
                aria-label=(format!("Toggle {label}"))
                class="group relative inline-flex h-4.5 w-8 shrink-0 rounded-full bg-foreground/20 shadow-xs transition-colors outline-none data-[checked]:bg-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50"
                :aria-checked=$(if enabled.get() { "true" } else { "false" })
                :data-checked=$(enabled.get())
                :disabled=$(pending.get())
                @click=$(async move |_event| {
                    let next_enabled = !enabled.get();
                    enabled.set(next_enabled);
                    pending.set(true);
                    let next_error = set_chat_toggle(platform, next_enabled).await;
                    if !next_error.is_empty() {
                        enabled.set(!next_enabled);
                    }
                    error.set(next_error);
                    pending.set(false);
                })
            >
                <span
                    class="pointer-events-none absolute top-1/2 left-0.5 size-3.5 -translate-y-1/2 rounded-full bg-background shadow-xs transition-transform group-data-[checked]:translate-x-3.5"
                ></span>
            </button>
            <span>(label)</span>
        </div>
    })
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
async fn set_chat_toggle(cx: &Cx, platform: String, enabled: bool) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    match platform.as_str() {
        "youtube" => app.set_youtube_polling(enabled).await?,
        "x" => app.set_x_webhook(enabled).await?,
        "kick" => app.set_kick_webhook(enabled).await?,
        _ => return Err(anyhow::anyhow!("Unsupported chat toggle platform").into()),
    }
    Ok(String::new())
}

#[procedure]
async fn start_pomodoro(cx: &Cx) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    let chat = app.config.get().chat.clone();
    match app
        .chat
        .start_pomodoro(
            chat.pomodoro_message.clone(),
            chat.pomodoro_minutes.saturating_mul(60),
        )
        .await
    {
        Ok(_) => Ok(String::new()),
        Err(error) => Ok(error.to_string()),
    }
}

#[procedure]
async fn stop_pomodoro(cx: &Cx) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    Ok(first_message_id(&app.chat.stop_pomodoro().await?))
}

#[procedure]
async fn pomodoro_active(cx: &Cx) -> Result<bool> {
    let app: &AppHandle = app_context(cx);
    Ok(app.chat.snapshot().await?.pomodoro.is_some())
}

#[procedure]
async fn start_poll(cx: &Cx, question: String, options_text: String) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    let options = options_text
        .lines()
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .map(str::to_string)
        .collect();
    match app
        .chat
        .start_poll(
            question,
            options,
            app.config.get().chat.poll_results_seconds,
        )
        .await
    {
        Ok(_) => Ok(String::new()),
        Err(error) => Ok(error.to_string()),
    }
}

#[procedure]
async fn stop_poll(cx: &Cx) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    Ok(first_message_id(&app.chat.stop_poll().await?))
}

#[procedure]
async fn clear_poll(cx: &Cx) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    Ok(first_message_id(&app.chat.clear_poll().await?))
}

pub(crate) fn poll_percent(votes: u64, total: u64) -> u64 {
    votes
        .checked_mul(100)
        .and_then(|scaled| scaled.checked_div(total))
        .unwrap_or(0)
}

#[procedure]
async fn poll_is_visible(cx: &Cx) -> Result<bool> {
    let app: &AppHandle = app_context(cx);
    Ok(app.chat.snapshot().await?.poll.is_some())
}

#[procedure]
async fn poll_is_active(cx: &Cx) -> Result<bool> {
    let app: &AppHandle = app_context(cx);
    Ok(app
        .chat
        .snapshot()
        .await?
        .poll
        .is_some_and(|poll| poll.is_active()))
}

#[derive(Clone)]
struct PollSetupSignals {
    question: topcoat::runtime::Signal<String>,
    options: topcoat::runtime::Signal<String>,
    pending: topcoat::runtime::Signal<bool>,
    error: topcoat::runtime::Signal<String>,
    form_open: topcoat::runtime::Signal<bool>,
    visible: topcoat::runtime::Signal<bool>,
    active: topcoat::runtime::Signal<bool>,
    revision: topcoat::runtime::Signal<f64>,
}

#[component]
async fn poll_setup(signals: PollSetupSignals) -> Result<impl View> {
    let PollSetupSignals {
        question,
        options,
        pending,
        error,
        form_open,
        visible,
        active,
        revision,
    } = signals;

    Ok(view! {
        card_content(
            attrs: attributes! {
                class="!px-6 flex min-h-0 flex-1 flex-col justify-center"
            },
            <div class="mx-auto flex w-full max-w-2xl flex-col gap-6">
                <div>
                    <h2 class="text-lg font-semibold">"Start a poll"</h2>
                    <p class="mt-1 text-sm text-muted-foreground">
                        "Chatters vote by sending the number of their choice. Add one option per line."
                    </p>
                </div>
                <label class="text-sm font-medium">
                    "Question"
                    <input
                        id="chat-poll-question"
                        type="text"
                        maxlength="280"
                        placeholder="What should we do next?"
                        class="mt-2 h-10 w-full rounded-lg border border-border bg-background px-3"
                        @input=$(move |event: Event| question.set(event.target.value))
                    />
                </label>
                <label class="text-sm font-medium">
                    "Options (2–6, one per line)"
                    <textarea
                        id="chat-poll-options"
                        rows="6"
                        placeholder="Option one&#10;Option two"
                        class="mt-2 min-h-32 w-full rounded-lg border border-border bg-background px-3 py-2"
                        @input=$(move |event: Event| options.set(event.target.value))
                    ></textarea>
                </label>
                <p :hidden=$(error.get().is_empty()) class="text-sm text-destructive">
                    $(error.get())
                </p>
                <div class="flex justify-center gap-3">
                    <button
                        type="button"
                        class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))
                        @click=$(move |_event| {
                            error.set("".to_owned());
                            form_open.set(false);
                        })
                    >
                        "Cancel"
                    </button>
                    <button
                        id="chat-poll-confirm"
                        type="button"
                        class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                        :disabled=$(pending.get())
                        @click=$(async move |_event| {
                            pending.set(true);
                            let next_error = start_poll(question.get(), options.get()).await;
                            error.set(next_error);
                            if error.get().is_empty() {
                                visible.set(true);
                                active.set(true);
                                form_open.set(false);
                            }
                            pending.set(false);
                            revision.set(revision.get() + 1.0);
                        })
                    >
                        "Start"
                    </button>
                </div>
            </div>
        )
    })
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
    let overlay_token = app.config.get().web_auth.overlay_token.clone();
    let initial_snapshot = app.chat.snapshot().await?;
    let initial_id = first_message_id(&initial_snapshot);
    let initial_pomodoro_active = initial_snapshot.pomodoro.is_some();
    let initial_poll_visible = initial_snapshot.poll.is_some();
    let initial_poll_active = initial_snapshot
        .poll
        .as_ref()
        .is_some_and(|poll| poll.is_active());
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
    let pomo_active = signal(cx, || initial_pomodoro_active);
    let pomo_error = signal(cx, String::new);
    let pomo_pending = signal(cx, || false);
    let poll_visible = signal(cx, || initial_poll_visible);
    let poll_active = signal(cx, || initial_poll_active);
    let poll_form_open = signal(cx, || false);
    let poll_question = signal(cx, String::new);
    let poll_options = signal(cx, String::new);
    let poll_error = signal(cx, String::new);
    let poll_pending = signal(cx, || false);
    let destructive_button = button_variants(ButtonVariant::Destructive, ButtonSize::Md);
    let poll_setup_signals = PollSetupSignals {
        question: poll_question,
        options: poll_options,
        pending: poll_pending.clone(),
        error: poll_error,
        form_open: poll_form_open.clone(),
        visible: poll_visible.clone(),
        active: poll_active.clone(),
        revision: revision.clone(),
    };

    Ok(view! {
        card(
            attrs: attributes! {
                class="mb-1 h-[calc(100dvh-6.5rem)] max-h-[calc(100dvh-6.5rem)] min-h-[18rem] !gap-2 !py-0"
            },
            if poll_form_open.get() {
                poll_setup(signals: poll_setup_signals.clone())
            } else {
                chat_inbox_content(revision: $(revision.get()))
            }
            card_footer(
                attrs: attributes! {
                    class="!px-3 !pt-2 !pb-2 justify-between flex-wrap gap-y-2"
                },
                <div class="flex items-center gap-2">
                    <button
                        id="chat-pomodoro-focus"
                        type="button"
                        class=(outline_button.clone())
                        :hidden=$(if pomo_active.get() {
                            true
                        } else if poll_form_open.get() {
                            true
                        } else {
                            false
                        })
                        :disabled=$(pomo_pending.get())
                        @click=$(async |_event| {
                            pomo_pending.set(true);
                            let error = start_pomodoro().await;
                            pomo_error.set(error);
                            if pomo_error.get().is_empty() {
                                pomo_active.set(true);
                            }
                            pomo_pending.set(false);
                            revision.set(revision.get() + 1.0);
                        })
                    >
                        "Focus"
                    </button>
                    <button
                        id="chat-pomodoro-stop"
                        type="button"
                        class=(destructive_button.clone())
                        :hidden=$(if pomo_active.get() { false } else { true })
                        :disabled=$(pomo_pending.get())
                        @click=$(async |_event| {
                            pomo_pending.set(true);
                            let next_id = stop_pomodoro().await;
                            current_id.set(next_id);
                            pomo_active.set(false);
                            pomo_pending.set(false);
                            revision.set(revision.get() + 1.0);
                        })
                    >
                        "Stop"
                    </button>
                    <button
                        id="chat-poll-start"
                        type="button"
                        class=(outline_button.clone())
                        :hidden=$(if poll_visible.get() {
                            true
                        } else if poll_form_open.get() {
                            true
                        } else {
                            false
                        })
                        @click=$(|_event| poll_form_open.set(!poll_form_open.get()))
                    >
                        "Poll"
                    </button>
                    <button
                        id="chat-poll-stop"
                        type="button"
                        class=(destructive_button.clone())
                        :hidden=$(if poll_active.get() { false } else { true })
                        :disabled=$(poll_pending.get())
                        @click=$(async |_event| {
                            poll_pending.set(true);
                            let next_id = stop_poll().await;
                            current_id.set(next_id);
                            poll_active.set(false);
                            poll_pending.set(false);
                            revision.set(revision.get() + 1.0);
                        })
                    >
                        "Stop poll"
                    </button>
                    <button
                        id="chat-poll-clear"
                        type="button"
                        class=(outline_button.clone())
                        :hidden=$(if poll_visible.get() {
                            if poll_active.get() { true } else { false }
                        } else {
                            true
                        })
                        :disabled=$(poll_pending.get())
                        @click=$(async |_event| {
                            poll_pending.set(true);
                            let next_id = clear_poll().await;
                            current_id.set(next_id);
                            poll_visible.set(false);
                            poll_active.set(false);
                            poll_pending.set(false);
                            revision.set(revision.get() + 1.0);
                        })
                    >
                        "Clear results"
                    </button>
                    <p
                        :hidden=$(pomo_error.get().is_empty())
                        class="text-xs text-destructive"
                    >
                        $(pomo_error.get())
                    </p>
                </div>
                <div
                    class="ml-auto flex items-center gap-2"
                    :hidden=$(poll_form_open.get())
                >
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
                            pomo_active.set(pomodoro_active().await);
                            poll_visible.set(poll_is_visible().await);
                            poll_active.set(poll_is_active().await);
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
        card(
            attrs: attributes! { class="mb-2 !gap-0 !py-1" },
            card_content(
                attrs: attributes! {
                    class="!px-3 !pt-2 !pb-2 flex items-center justify-between gap-3 flex-wrap"
                },
                <div class="flex items-center gap-4">
                    if youtube_configured {
                        chat_toggle(
                            label: "YouTube polling",
                            platform: "youtube".to_string(),
                            enabled: &youtube_polling_enabled,
                            pending: &youtube_toggle_pending,
                            error: &polling_error
                        )
                    }
                    chat_toggle(
                        label: "X chat",
                        platform: "x".to_string(),
                        enabled: &x_webhook_enabled,
                        pending: &x_toggle_pending,
                        error: &polling_error
                    )
                    chat_toggle(
                        label: "Kick chat",
                        platform: "kick".to_string(),
                        enabled: &kick_webhook_enabled,
                        pending: &kick_toggle_pending,
                        error: &polling_error
                    )
                    <p
                        :hidden=$(polling_error.get().is_empty())
                        class="text-xs text-destructive"
                    >
                        $(polling_error.get())
                    </p>
                </div>
                <a
                    href=(format!("/overlay/chat?key={overlay_token}"))
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
            )
        )
    })
}

#[shard]
pub async fn chat_inbox_content(cx: &Cx, revision: f64) -> Result<impl View> {
    let _ = revision;
    let app: &AppHandle = app_context(cx);
    let snapshot = app.chat.snapshot().await?;
    let now = now_unix_ms();
    let pomodoro = snapshot
        .pomodoro
        .as_ref()
        .filter(|state| !state.is_expired(now))
        .map(|state| {
            (
                state.message.clone(),
                state.ends_at_unix_ms.to_string(),
                state.remaining_mm_ss(now),
            )
        });
    let poll = snapshot.poll.clone();
    let poll_total_votes = poll.as_ref().map(|poll| poll.total_votes()).unwrap_or(0);
    let poll_max_votes = poll
        .as_ref()
        .and_then(|poll| poll.options.iter().map(|option| option.votes).max())
        .unwrap_or(0);
    let show_chat = pomodoro.is_none() && poll.is_none();

    Ok(view! {
        card_content(
            attrs: attributes! { class="!px-3 !pb-2 flex min-h-0 flex-1 flex-col gap-1" },
            if let Some((message, ends_at, remaining)) = pomodoro {
                <div
                    id="chat-pomodoro-status"
                    data-ends-at=(ends_at.clone())
                    class="flex min-h-0 flex-1 flex-col items-center justify-center gap-1 px-2 text-center"
                >
                    <div
                        class="text-xs font-semibold uppercase tracking-[0.2em] text-muted-foreground"
                    >
                        "Focus mode"
                    </div>
                    <div class="text-lg font-semibold">(message)</div>
                    <div
                        data-pomodoro-countdown="true"
                        data-ends-at=(ends_at)
                        class="mt-1 text-4xl font-bold tabular-nums"
                    >
                        (remaining)
                    </div>
                    <div
                        id="chat-focus-waiting"
                        class="mt-1 text-sm text-muted-foreground"
                    >
                        (if snapshot.queued == 1 {
                            "1 message waiting".to_string()
                        } else {
                            format!("{} messages waiting", snapshot.queued)
                        })
                    </div>
                </div>
            }
            if let Some(poll) = poll {
                <div
                    id="chat-poll-status"
                    class="flex min-h-0 flex-1 flex-col justify-center gap-4 px-2"
                >
                    <div
                        class="text-center text-xs font-semibold uppercase tracking-[0.2em] text-muted-foreground"
                    >
                        (if poll.is_active() { "Poll" } else { "Poll results" })
                    </div>
                    <div class="text-center text-xl font-semibold">
                        (poll.question.clone())
                    </div>
                    <div class="flex flex-col gap-2">
                        for option in poll.options.iter() {
                            <div
                                class=(if poll.stopped_at_unix_ms.is_some()
                                    && poll_max_votes > 0
                                    && option.votes == poll_max_votes {
                                    "relative flex items-center justify-between gap-3 overflow-hidden rounded-md border border-green-500 px-3 py-2"
                                } else {
                                    "relative flex items-center justify-between gap-3 overflow-hidden rounded-md border border-border px-3 py-2"
                                })
                            >
                                if poll.stopped_at_unix_ms.is_some() {
                                    <div
                                        class="pointer-events-none absolute inset-y-0 left-0 bg-muted"
                                        style=(format!(
                                            "width: {}%",
                                            poll_percent(option.votes, poll_total_votes),
                                        ))
                                        aria-hidden="true"
                                    ></div>
                                }
                                <span class="relative min-w-0 break-words">
                                    <span class="mr-2 font-bold text-primary">
                                        (format!("{}.", option.number))
                                    </span>
                                    (option.label.clone())
                                </span>
                                <span class="relative shrink-0 font-semibold tabular-nums">
                                    (format!("{}", option.votes))
                                </span>
                            </div>
                        }
                    </div>
                    <div class="text-center text-sm text-muted-foreground">
                        (if poll.is_active() {
                            format!("{} votes", poll_total_votes)
                        } else if poll.total_votes() == 1 {
                            "1 vote".to_string()
                        } else {
                            format!("{} votes", poll.total_votes())
                        })
                    </div>
                </div>
            }
            if show_chat {
                <div class="min-h-0 flex-1 overflow-y-auto pr-1">
                    <div class="flex flex-col gap-2">
                        if snapshot.messages.is_empty() {
                            "No chat messages waiting."
                        } else {
                            for (index, message) in snapshot
                                .messages
                                .into_iter()
                                .enumerate() {
                                chat_message_card(
                                    message: message,
                                    highlighted: index == 0
                                )
                            }
                        }
                    </div>
                </div>
            }
            if show_chat {
                <div class="mt-1 mb-1 flex justify-end gap-4 text-right">
                    <span class="text-sm font-medium">
                        (format!("{} queued", snapshot.queued))
                    </span>
                    <span class="text-sm text-muted-foreground">
                        (format!("{} dropped", snapshot.dropped))
                    </span>
                </div>
            }
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
    Ok(view! {
        shared_chat_message_card(
            message: message,
            row_class: row_class,
            highlighted: highlighted,
            overlay: false
        )
    })
}
