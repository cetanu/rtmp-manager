use crate::chat::ChatMessage;
use crate::server::state::AppHandle;
use crate::util::now_unix_ms;
use crate::web::components::chat_message::chat_message_card as shared_chat_message_card;
use crate::web::components::ui::button::{ButtonSize, ButtonVariant, button_variants};
use crate::web::components::ui::card::{card, card_content, card_footer};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{Event, connected, procedure, record, shard, signal},
    view::{View, attributes, component, emit, live, view},
};

#[procedure]
async fn acknowledge_chat(cx: &Cx, displayed_id: String) -> Result<()> {
    let app: &AppHandle = app_context(cx);
    let displayed_id = displayed_id.parse().map_err(|error| {
        topcoat::router::error::bad_request(format!("Invalid displayed chat message ID: {error}"))
    })?;
    if !app
        .chat
        .acknowledge(displayed_id)
        .await
        .map_err(topcoat::Error::from_anyhow)?
    {
        return Err(topcoat::router::error::bad_request(
            "The displayed message changed before it could be acknowledged",
        )
        .into());
    }
    Ok(())
}

#[procedure]
async fn clear_chat_messages(cx: &Cx) -> Result<()> {
    let app: &AppHandle = app_context(cx);
    app.chat
        .clear_messages()
        .await
        .map_err(topcoat::Error::from_anyhow)?;
    Ok(())
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
        <div class="flex items-center">
            <button
                type="button"
                role="switch"
                aria-label=(format!("Toggle {label}"))
                class="group inline-flex h-8 cursor-pointer items-center gap-2 rounded-sm border border-border bg-white/5 px-3 font-mono text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase shadow-xs transition-colors outline-none hover:border-signal/50 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50 data-[checked=true]:border-signal/50 data-[checked=true]:bg-signal/10 data-[checked=true]:text-signal"
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
                    class="hud-dot bg-white/30 text-white/30 group-data-[checked=true]:bg-signal group-data-[checked=true]:text-signal"
                ></span>
                <span>(label)</span>
            </button>
        </div>
    })
}

#[procedure("/api/chat/test-message")]
async fn send_test_chat(cx: &Cx) -> Result<()> {
    let app: &AppHandle = app_context(cx);
    app.chat
        .enqueue_test(None)
        .await
        .map_err(topcoat::Error::from_anyhow)?;
    Ok(())
}

#[procedure]
async fn set_chat_toggle(cx: &Cx, platform: String, enabled: bool) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    match platform.as_str() {
        "queue" => app
            .set_queue_mode(enabled)
            .await
            .map_err(topcoat::Error::from_anyhow)?,
        "youtube" => app
            .set_youtube_polling(enabled)
            .await
            .map_err(topcoat::Error::from_anyhow)?,
        "x" => app
            .set_x_webhook(enabled)
            .await
            .map_err(topcoat::Error::from_anyhow)?,
        "kick" => app
            .set_kick_webhook(enabled)
            .await
            .map_err(topcoat::Error::from_anyhow)?,
        _ => {
            return Err(
                topcoat::router::error::bad_request("Unsupported chat toggle platform").into(),
            );
        }
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
async fn stop_pomodoro(cx: &Cx) -> Result<()> {
    let app: &AppHandle = app_context(cx);
    app.chat
        .stop_pomodoro()
        .await
        .map_err(topcoat::Error::from_anyhow)?;
    Ok(())
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
async fn stop_poll(cx: &Cx) -> Result<()> {
    let app: &AppHandle = app_context(cx);
    app.chat
        .stop_poll()
        .await
        .map_err(topcoat::Error::from_anyhow)?;
    Ok(())
}

#[procedure]
async fn clear_poll(cx: &Cx) -> Result<()> {
    let app: &AppHandle = app_context(cx);
    app.chat
        .clear_poll()
        .await
        .map_err(topcoat::Error::from_anyhow)?;
    Ok(())
}

pub(crate) fn poll_percent(votes: u64, total: u64) -> u64 {
    votes
        .checked_mul(100)
        .and_then(|scaled| scaled.checked_div(total))
        .unwrap_or(0)
}

#[derive(Clone)]
struct PollSetupSignals {
    question: topcoat::runtime::Signal<String>,
    options: topcoat::runtime::Signal<String>,
    pending: topcoat::runtime::Signal<bool>,
    error: topcoat::runtime::Signal<String>,
    form_open: topcoat::runtime::Signal<bool>,
}

#[component]
async fn poll_setup(signals: PollSetupSignals) -> Result<impl View> {
    let PollSetupSignals {
        question,
        options,
        pending,
        error,
        form_open,
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
                                form_open.set(false);
                            }
                            pending.set(false);
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

#[record]
#[derive(Clone)]
struct InboxControls {
    current_id: String,
    pomodoro_active: bool,
    poll_visible: bool,
    poll_active: bool,
}

#[shard]
pub async fn chat_inbox(cx: &Cx) -> Result<impl View> {
    Ok(live! {
        let app: &AppHandle = app_context(cx);
        let mut changed = app.chat.subscribe_changes();
        let mut config_changed = app.config.subscribe();
        loop {
            let token = emit! { chat_inbox_panel() }?;
            if !connected(cx) {
                break Ok(token);
            }
            tokio::select! {
                result = changed.changed() => if result.is_err() { break Ok(token); },
                result = config_changed.changed() => if result.is_err() { break Ok(token); },
            }
        }
    })
}

#[component]
async fn chat_inbox_panel(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = app_context(cx);
    let overlay_token = app.config.get().web_auth.overlay_token.clone();
    let initial_snapshot = app
        .chat
        .snapshot()
        .await
        .map_err(topcoat::Error::from_anyhow)?;
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

    let controls = InboxControls {
        current_id: first_message_id(&initial_snapshot),
        pomodoro_active: initial_snapshot.pomodoro.is_some(),
        poll_visible: initial_snapshot.poll.is_some(),
        poll_active: initial_snapshot
            .poll
            .as_ref()
            .is_some_and(|poll| poll.is_active()),
    };
    let youtube_polling_enabled = signal(cx, || chat.youtube_polling_enabled);
    let queue_mode_enabled = signal(cx, || chat.queue_mode);
    let queue_toggle_pending = signal(cx, || false);
    let youtube_toggle_pending = signal(cx, || false);
    let x_webhook_enabled = signal(cx, || chat.x_webhook_enabled);
    let x_toggle_pending = signal(cx, || false);
    let kick_webhook_enabled = signal(cx, || chat.kick_webhook_enabled);
    let kick_toggle_pending = signal(cx, || false);
    let polling_error = signal(cx, String::new);
    let pomo_error = signal(cx, String::new);
    let pomo_pending = signal(cx, || false);
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
    };

    Ok(view! {
        card(
            attrs: attributes! {
                data-chat-inbox="true"
                class="mb-2 h-full min-h-[18rem] !gap-2 !py-0"
            },
            if poll_form_open.get() {
                poll_setup(signals: poll_setup_signals.clone())
            } else {
                chat_inbox_content()
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
                        :hidden=$(if controls.pomodoro_active {
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
                            if pomo_error.get().is_empty() {}
                            pomo_pending.set(false);
                        })
                    >
                        "Focus"
                    </button>
                    <button
                        id="chat-pomodoro-stop"
                        type="button"
                        class=(destructive_button.clone())
                        :hidden=$(if controls.pomodoro_active { false } else { true })
                        :disabled=$(pomo_pending.get())
                        @click=$(async |_event| {
                            pomo_pending.set(true);
                            stop_pomodoro().await;

                            pomo_pending.set(false);
                        })
                    >
                        "Stop"
                    </button>
                    <button
                        id="chat-poll-start"
                        type="button"
                        class=(outline_button.clone())
                        :hidden=$(if controls.poll_visible {
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
                        :hidden=$(if controls.poll_active { false } else { true })
                        :disabled=$(poll_pending.get())
                        @click=$(async |_event| {
                            poll_pending.set(true);
                            stop_poll().await;

                            poll_pending.set(false);
                        })
                    >
                        "Stop poll"
                    </button>
                    <button
                        id="chat-poll-clear"
                        type="button"
                        class=(outline_button.clone())
                        :hidden=$(if controls.poll_visible {
                            if controls.poll_active { true } else { false }
                        } else {
                            true
                        })
                        :disabled=$(poll_pending.get())
                        @click=$(async |_event| {
                            poll_pending.set(true);
                            clear_poll().await;

                            poll_pending.set(false);
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
                            send_test_chat().await;
                        })
                    >
                        "Test Message"
                    </button>

                    <button
                        type="button"
                        class=(outline_button.clone())
                        :hidden=$(queue_mode_enabled.get())
                        @click=$(async |_event| {
                            clear_chat_messages().await;
                        })
                    >
                        "Clear all"
                    </button>
                    <button
                        type="button"
                        class=(primary_button)
                        :hidden=$(!queue_mode_enabled.get())
                        :disabled=$(controls.current_id.clone().is_empty())
                        @click=$(async |_event| {
                            acknowledge_chat(controls.current_id.clone()).await;
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
                <div class="flex flex-wrap items-center gap-2">
                    chat_toggle(
                        label: "Queue incoming chat",
                        platform: "queue".to_string(),
                        enabled: &queue_mode_enabled,
                        pending: &queue_toggle_pending,
                        error: &polling_error
                    )
                    <div aria-hidden="true" class="mx-1 h-6 w-px shrink-0 bg-border"></div>
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
                    class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))
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
pub async fn chat_inbox_content(cx: &Cx) -> Result<impl View> {
    Ok(live! {
        let app: &AppHandle = app_context(cx);
        let mut changed = app.chat.subscribe_changes();
        loop {
            let timer_active = changed.borrow_and_update().pomodoro.is_some();
            let token = emit! { inbox_messages() }?;
            if !connected(cx) {
                break Ok(token);
            }
            tokio::select! {
                result = changed.changed() => if result.is_err() { break Ok(token); },
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)), if timer_active => {},
            }
        }
    })
}

#[component]
async fn inbox_messages(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = app_context(cx);
    let queue_mode = app.config.get().chat.queue_mode;
    let snapshot = app
        .chat
        .snapshot()
        .await
        .map_err(topcoat::Error::from_anyhow)?;
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
                <div
                    data-chat-messages="true"
                    class="flex min-h-0 flex-1 flex-col-reverse overflow-y-auto pr-1"
                >
                    if snapshot.messages.is_empty() {
                        <div
                            class="flex h-full min-h-32 items-center justify-center border border-border bg-black/30 px-4 text-center font-mono text-[11px] text-muted-foreground"
                        >
                            "// No chat messages waiting."
                        </div>
                    } else {
                        <div class="flex flex-col gap-2">
                            #[key(message.id)]
                            for (index, message) in snapshot
                                .messages
                                .into_iter()
                                .enumerate() {
                                chat_message_card(
                                    message: message,
                                    highlighted: queue_mode && index == 0
                                )
                            }
                        </div>
                    }
                </div>
            }
            if show_chat {
                <div
                    class="mt-1 flex justify-end gap-4 border-t border-border pt-2 pb-1 font-mono text-[10px] tracking-[0.12em] uppercase"
                >
                    if queue_mode {
                        <span class="text-muted-foreground">
                            <span class="font-semibold text-foreground tabular-nums">
                                (snapshot.queued)
                            </span>
                            " queued"
                        </span>
                        <span class="text-muted-foreground">
                            <span class="font-semibold text-foreground tabular-nums">
                                (snapshot.dropped)
                            </span>
                            " dropped"
                        </span>
                    } else {
                        <span class="text-muted-foreground">
                            <span class="font-semibold text-foreground tabular-nums">
                                (snapshot.queued)
                            </span>
                            " messages"
                        </span>
                    }
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
