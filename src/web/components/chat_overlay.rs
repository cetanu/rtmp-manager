use crate::chat::{ChatMessage, PollState, PomodoroState};
use crate::server::state::AppHandle;
use crate::util::now_unix_ms;
use crate::web::components::chat_message::chat_message_card;
use topcoat::{
    Result,
    context::{Cx, app_context},
    view::{View, ViewExt, component, view},
};

const OVERLAY_CSS: &str = r#"
html, body {
    background: transparent !important;
    background-color: transparent !important;
    margin: 0;
    padding: 0;
    overflow: hidden;
    height: 100%;
}
/* Full-window focus banner: vh first for older Chromium/CEF builds
   (e.g. OBS Browser Source), dvh as progressive enhancement. */
body {
    min-height: 100vh;
    --overlay-alpha: 0.8;
}
#chat-overlay-wrapper {
    min-height: calc(100vh - 1rem);
    min-height: calc(100dvh - 1rem);
}
#chat-overlay-messages {
    flex: 1;
}
#chat-overlay-pomodoro {
    flex: 1;
    min-height: calc(100vh - 1rem);
    min-height: calc(100dvh - 1rem);
}
#chat-overlay-poll {
    flex: 1;
    box-sizing: border-box;
    height: calc(100vh - 1rem);
    height: calc(100dvh - 1rem);
    min-height: 0;
    max-height: calc(100vh - 1rem);
    max-height: calc(100dvh - 1rem);
    overflow-y: auto;
    overscroll-behavior: contain;
    justify-content: flex-start;
    justify-content: safe center;
    padding: clamp(1rem, 4vh, 2.5rem) clamp(1rem, 4vw, 2rem);
}
.chat-overlay-poll-question {
    margin-top: clamp(0.5rem, 2vh, 0.75rem) !important;
    font-size: clamp(1.5rem, 6vh, 2.25rem) !important;
}
.chat-overlay-poll-options {
    gap: clamp(0.5rem, 1.5vh, 1rem) !important;
    margin-top: clamp(1rem, 4vh, 2rem) !important;
}
.chat-overlay-poll-option {
    padding: clamp(0.5rem, 1.5vh, 1rem) clamp(0.75rem, 2vw, 1.25rem) !important;
}
.chat-overlay-poll-option-row {
    font-size: clamp(0.95rem, 3vh, 1.25rem) !important;
}
.chat-overlay-poll-footer {
    margin-top: clamp(0.75rem, 2vh, 1.5rem) !important;
    font-size: clamp(0.9rem, 2.5vh, 1.125rem) !important;
}
@media (max-height: 500px) {
    #chat-overlay-poll {
        padding: 0.5rem 1rem;
    }
    .chat-overlay-poll-question {
        margin-top: 0.5rem !important;
        font-size: 1.75rem !important;
    }
    .chat-overlay-poll-options {
        gap: 0.5rem !important;
        margin-top: 0.75rem !important;
    }
    .chat-overlay-poll-option {
        padding: 0.625rem 0.75rem !important;
    }
    .chat-overlay-poll-option-row {
        font-size: 1.125rem !important;
    }
    .chat-overlay-poll-footer {
        margin-top: 0.75rem !important;
        font-size: 0.875rem !important;
    }
    .chat-overlay-poll-options[data-dense="true"] {
        gap: 0.375rem !important;
    }
    .chat-overlay-poll-options[data-dense="true"] .chat-overlay-poll-option {
        padding: 0.375rem 0.75rem !important;
    }
    .chat-overlay-poll-options[data-dense="true"] .chat-overlay-poll-option-row {
        font-size: 1rem !important;
    }
    #chat-overlay-poll #chat-overlay-poll-waiting {
        display: none;
    }
}
.chat-overlay-message {
    transition: opacity 0.2s ease, transform 0.2s ease;
}
body[data-theme="plain"] .chat-overlay-message {
    background-color: transparent !important;
    border-color: transparent !important;
    box-shadow: none !important;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.9);
}
body[data-theme="plain"] #chat-overlay-pomodoro {
    background-color: transparent !important;
    border-color: transparent !important;
    box-shadow: none !important;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.9);
}
body[data-theme="plain"] #chat-overlay-poll {
    background-color: transparent !important;
    border-color: transparent !important;
    box-shadow: none !important;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.9);
}
body[data-theme="solid"] .chat-overlay-message {
    background-color: #18181b !important;
    border-color: #27272a !important;
}
body[data-theme="solid"] #chat-overlay-pomodoro {
    background-color: #18181b !important;
    border-color: #27272a !important;
}
body[data-theme="solid"] #chat-overlay-poll {
    background-color: #18181b !important;
    border-color: #27272a !important;
}
/* NOTE: the message text lives in a `p.text-sm`, which beats inherited
   font-size, so the size options must target it directly. */
body[data-size="sm"] .chat-overlay-message,
body[data-size="sm"] .chat-overlay-message p { font-size: 0.75rem; }
body[data-size="lg"] .chat-overlay-message,
body[data-size="lg"] .chat-overlay-message p { font-size: 1.125rem; }
body[data-size="xl"] .chat-overlay-message,
body[data-size="xl"] .chat-overlay-message p { font-size: 1.25rem; }
body[data-align="bottom"] #chat-overlay-wrapper {
    justify-content: flex-end;
    min-height: calc(100vh - 1rem);
    min-height: calc(100dvh - 1rem);
}
body[data-direction="up"] #chat-overlay-messages,
body[data-direction="reverse"] #chat-overlay-messages {
    flex-direction: column-reverse;
}
body[data-highlight="false"] .chat-overlay-message[data-highlighted="true"] {
    background-color: rgba(0, 0, 0, 0.6) !important;
    border-color: rgba(255, 255, 255, 0.1) !important;
    box-shadow: none !important;
}
/* Configurable container opacity (?opacity=0..100, default 80). Applies to the
   default translucent surfaces: regular messages, the focus banner and the
   flattened/accent highlights below (the accent tint is veiled rather than
   replaced, so a hint of it survives below 100). The plain/solid themes keep
   their explicit treatments. */
body:not([data-theme]) .chat-overlay-message:not([data-highlighted="true"]) {
    background-color: rgba(0, 0, 0, var(--overlay-alpha, 0.8)) !important;
}
body:not([data-theme]) #chat-overlay-pomodoro {
    background-color: rgba(0, 0, 0, var(--overlay-alpha, 0.8)) !important;
}
body:not([data-theme]) #chat-overlay-poll {
    background-color: rgba(0, 0, 0, var(--overlay-alpha, 0.8)) !important;
}
body:not([data-theme])[data-highlight="false"] .chat-overlay-message[data-highlighted="true"] {
    background-color: rgba(0, 0, 0, var(--overlay-alpha, 0.8)) !important;
}
body:not([data-theme]):not([data-highlight="false"]) .chat-overlay-message[data-highlighted="true"] {
    background-image: linear-gradient(rgba(0, 0, 0, var(--overlay-alpha, 0.8)), rgba(0, 0, 0, var(--overlay-alpha, 0.8))) !important;
}
"#;

#[component]
pub async fn chat_overlay_page() -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html
            lang="en"
            class="dark"
            style="background: transparent !important; background-color: transparent !important;"
        >
            <head>
                <meta charset="UTF-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1.0" />
                <title>"RTMP-Manager Chat Overlay"</title>
                <meta
                    name="description"
                    content="OBS browser source overlay for aggregated live stream chat."
                />
                <link
                    href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700&display=swap"
                    rel="stylesheet"
                />
                <link rel="stylesheet" href=(crate::web::TAILWIND_STYLESHEET) />
                <script src=(crate::web::OVERLAY_EVENTS_SCRIPT) defer="defer"></script>
                <style>(OVERLAY_CSS)</style>
            </head>
            <body
                class="min-h-screen bg-transparent p-2 font-sans text-foreground antialiased selection:bg-none"
                style="background: transparent !important; background-color: transparent !important;"
            >
                chat_overlay()
            </body>
        </html>
    })
}

#[component]
pub async fn chat_overlay(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = app_context(cx);
    let snapshot = app.chat.snapshot().await?;

    Ok(view! {
        <div id="chat-overlay-wrapper" class="flex flex-col w-full">
            chat_overlay_messages(
                messages: snapshot.messages,
                pomodoro: snapshot.pomodoro,
                poll: snapshot.poll,
                queued: snapshot.queued
            )
        </div>
    })
}

#[component]
pub async fn chat_overlay_messages(
    messages: Vec<ChatMessage>,
    pomodoro: Option<PomodoroState>,
    poll: Option<PollState>,
    queued: usize,
) -> Result<impl View> {
    let now = now_unix_ms();
    let pomodoro = pomodoro.filter(|state| !state.is_expired(now));
    let poll = poll.filter(|poll| poll.is_active() || poll.is_showing_results(now));
    let poll_total_votes = poll.as_ref().map(|poll| poll.total_votes()).unwrap_or(0);
    let poll_max_votes = poll
        .as_ref()
        .and_then(|poll| poll.options.iter().map(|option| option.votes).max())
        .unwrap_or(0);
    let show_messages = pomodoro.is_none() && poll.is_none();
    Ok(view! {
        <div id="chat-overlay-messages" class="flex flex-col gap-2">
            if let Some(state) = pomodoro {
                <div
                    id="chat-overlay-pomodoro"
                    data-ends-at=(state.ends_at_unix_ms.to_string())
                    class="flex min-h-[calc(100dvh-1rem)] w-full flex-1 flex-col items-center justify-center rounded-xl border border-white/10 bg-black/60 px-8 py-10 text-center shadow-xs backdrop-blur-xs"
                >
                    <div
                        class="text-xs font-semibold uppercase tracking-[0.2em] text-zinc-400"
                    >
                        "Focus mode"
                    </div>
                    <div
                        class="mt-3 w-full text-4xl font-bold leading-tight break-words text-zinc-100"
                    >
                        (state.message.clone())
                    </div>
                    <div
                        data-pomodoro-countdown="true"
                        data-ends-at=(state.ends_at_unix_ms.to_string())
                        class="mt-4 text-7xl font-bold tabular-nums text-zinc-100"
                    >
                        (state.remaining_mm_ss(now))
                    </div>
                    <div id="chat-overlay-waiting" class="mt-3 text-xl text-zinc-400">
                        (if queued == 1 {
                            "1 message waiting".to_string()
                        } else {
                            format!("{queued} messages waiting")
                        })
                    </div>
                </div>
            }
            if let Some(poll) = poll {
                <div
                    id="chat-overlay-poll"
                    class="chat-overlay-poll flex min-h-[calc(100dvh-1rem)] w-full flex-1 flex-col justify-center rounded-xl border border-white/10 bg-black/60 px-8 py-10 shadow-xs backdrop-blur-xs"
                >
                    <div
                        class="text-center text-xs font-semibold uppercase tracking-[0.2em] text-zinc-400"
                    >
                        (if poll.is_active() { "Poll" } else { "Poll results" })
                    </div>
                    <div
                        class="chat-overlay-poll-question mt-3 text-center text-4xl font-bold leading-tight break-words text-zinc-100"
                    >
                        (poll.question.clone())
                    </div>
                    <div
                        class="chat-overlay-poll-options mt-8 flex flex-col gap-4"
                        data-dense=((poll.options.len() > 4).to_string())
                    >
                        for option in poll.options.iter() {
                            <div
                                class=(if poll.stopped_at_unix_ms.is_some() && poll_max_votes > 0 && option.votes == poll_max_votes {
                                    "chat-overlay-poll-option relative overflow-hidden rounded-lg border border-green-500 bg-white/5 px-5 py-4"
                                } else {
                                    "chat-overlay-poll-option relative overflow-hidden rounded-lg border border-white/10 bg-white/5 px-5 py-4"
                                })
                            >
                                if poll.stopped_at_unix_ms.is_some() {
                                    <div
                                        class="pointer-events-none absolute inset-y-0 left-0 bg-white/10"
                                        style=(format!(
                                            "width: {}%",
                                            crate::web::components::chat_inbox::poll_percent(
                                                option.votes,
                                                poll_total_votes
                                            ),
                                        ))
                                        aria-hidden="true"
                                    ></div>
                                }
                                <div
                                    class="chat-overlay-poll-option-row relative flex items-center justify-between gap-4 text-xl text-zinc-100"
                                >
                                    <span class="min-w-0 break-words">
                                        <span class="mr-3 font-bold text-primary">
                                            (format!("{}.", option.number))
                                        </span>
                                        (option.label.clone())
                                    </span>
                                    if poll.stopped_at_unix_ms.is_some() {
                                        <span class="shrink-0 font-bold tabular-nums">
                                            (format!("{}", option.votes))
                                        </span>
                                    }
                                </div>
                            </div>
                        }
                    </div>
                    <div
                        class="chat-overlay-poll-footer mt-6 text-center text-lg text-zinc-400"
                    >
                        (if poll.is_active() {
                            format!(
                                "{} votes · vote by sending the number of your choice",
                                poll_total_votes,
                            )
                        } else if poll.total_votes() == 1 {
                            "1 vote".to_string()
                        } else {
                            format!("{} votes", poll.total_votes())
                        })
                    </div>
                    <div
                        id="chat-overlay-poll-waiting"
                        class="mt-3 text-center text-xl text-zinc-400"
                    >
                        (if queued == 1 {
                            "1 message waiting".to_string()
                        } else {
                            format!("{queued} messages waiting")
                        })
                    </div>
                </div>
            }
            if show_messages {
                if messages.is_empty() {
                    <div class="hidden" aria-hidden="true"></div>
                } else {
                    for (index, message) in messages.into_iter().enumerate() {
                        chat_overlay_message(message: message, highlighted: index == 0)
                    }
                }
            }
        </div>
    })
}

pub async fn render_chat_overlay_messages(
    messages: Vec<ChatMessage>,
    pomodoro: Option<PomodoroState>,
    poll: Option<PollState>,
    queued: usize,
) -> Result<String> {
    let cx = Cx::default();
    let __cx = &cx;
    let view = view! {
        chat_overlay_messages(
            messages: messages,
            pomodoro: pomodoro,
            poll: poll,
            queued: queued
        )
    };
    Ok(view.single().await?.render(&cx))
}

pub(crate) fn overlay_message_class(highlighted: bool) -> &'static str {
    if highlighted {
        "chat-overlay-message grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2 rounded-md bg-primary/20 backdrop-blur-xs px-2.5 py-1.5 ring-1 ring-primary/40 shadow-xs border border-primary/20"
    } else {
        "chat-overlay-message grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2 rounded-md bg-black/60 backdrop-blur-xs px-2.5 py-1.5 shadow-xs border border-white/10"
    }
}

#[component]
pub async fn chat_overlay_message(message: ChatMessage, highlighted: bool) -> Result<impl View> {
    let row_class = overlay_message_class(highlighted);

    Ok(view! {
        chat_message_card(
            message: message,
            row_class: row_class,
            highlighted: highlighted,
            overlay: true
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn renders_overlay_messages_as_escaped_server_html() {
        let html = render_chat_overlay_messages(
            vec![ChatMessage {
                id: 1,
                source: crate::chat::Source::Twitch,
                external_id: "external-1".into(),
                author: "<viewer>".into(),
                text: "<script>alert(1)</script>".into(),
                parts: vec![crate::chat::ChatMessagePart::Text(
                    "<script>alert(1)</script>".into(),
                )],
                avatar_url: None,
                sent_at: None,
                received_at_unix_ms: 1,
            }],
            None,
            None,
            1,
        )
        .await
        .unwrap();

        assert!(html.contains("id=\"chat-overlay-messages\""));
        assert!(html.contains("data-source=\"twitch\""));
        assert!(html.contains("&lt;viewer&gt;"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[tokio::test]
    async fn renders_youtube_emoji_parts_as_images() {
        let html = render_chat_overlay_messages(
            vec![ChatMessage {
                id: 2,
                source: crate::chat::Source::YouTube,
                external_id: "external-2".into(),
                author: "Viewer".into(),
                text: "Hello customEmoji".into(),
                parts: vec![
                    crate::chat::ChatMessagePart::Text("Hello ".into()),
                    crate::chat::ChatMessagePart::Emoji {
                        alt: "customEmoji".into(),
                        url: "https://example.com/custom-emoji.png".into(),
                    },
                ],
                avatar_url: None,
                sent_at: None,
                received_at_unix_ms: 2,
            }],
            None,
            None,
            1,
        )
        .await
        .unwrap();

        assert!(html.contains("src=\"https://example.com/custom-emoji.png\""));
        assert!(html.contains("alt=\"customEmoji\""));
        assert!(html.contains("Hello "));
    }

    #[tokio::test]
    async fn pomodoro_banner_hides_chat_messages() {
        let now = now_unix_ms();
        let html = render_chat_overlay_messages(
            vec![ChatMessage {
                id: 3,
                source: crate::chat::Source::Twitch,
                external_id: "external-3".into(),
                author: "Viewer".into(),
                text: "should be hidden".into(),
                parts: vec![],
                avatar_url: None,
                sent_at: None,
                received_at_unix_ms: now,
            }],
            Some(PomodoroState {
                message: "Deep work <focus>".into(),
                started_at_unix_ms: now,
                ends_at_unix_ms: now + 25 * 60 * 1000,
            }),
            None,
            3,
        )
        .await
        .unwrap();

        assert!(html.contains("id=\"chat-overlay-pomodoro\""));
        assert!(html.contains("Deep work &lt;focus&gt;"));
        assert!(!html.contains("should be hidden"));
        assert!(html.contains("data-pomodoro-countdown"));
        assert!(html.contains("25:00"));
        assert!(html.contains("id=\"chat-overlay-waiting\""));
        assert!(html.contains("3 messages waiting"));
    }

    #[test]
    fn container_opacity_is_configurable_with_opaque_default() {
        assert!(OVERLAY_CSS.contains("--overlay-alpha: 0.8"));
        assert!(OVERLAY_CSS.contains("#chat-overlay-pomodoro"));
        assert!(OVERLAY_CSS.contains(".chat-overlay-message"));
        const OVERLAY_PARAMS_JS: &str = include_str!("../../../static/overlay-events.js");
        assert!(OVERLAY_PARAMS_JS.contains("\"opacity\""));
        assert!(OVERLAY_PARAMS_JS.contains("--overlay-alpha"));
    }
}
