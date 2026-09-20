use crate::chat::ChatMessage;
use crate::server::state::AppHandle;
use crate::web::components::chat_inbox::{chat_source_icon, source_color};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{shard, signal},
    view::{View, component, view},
};

const OVERLAY_CSS: &str = r#"
html, body {
    background: transparent !important;
    background-color: transparent !important;
    margin: 0;
    padding: 0;
    overflow: hidden;
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
body[data-theme="solid"] .chat-overlay-message {
    background-color: #18181b !important;
    border-color: #27272a !important;
}
body[data-size="sm"] .chat-overlay-message { font-size: 0.75rem; }
body[data-size="lg"] .chat-overlay-message { font-size: 1.125rem; }
body[data-size="xl"] .chat-overlay-message { font-size: 1.25rem; }
body[data-align="bottom"] #chat-overlay-wrapper {
    justify-content: flex-end;
    min-height: 100vh;
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
body[data-limit="1"] .chat-overlay-message:nth-child(n+2) { display: none !important; }
body[data-limit="2"] .chat-overlay-message:nth-child(n+3) { display: none !important; }
body[data-limit="3"] .chat-overlay-message:nth-child(n+4) { display: none !important; }
body[data-limit="4"] .chat-overlay-message:nth-child(n+5) { display: none !important; }
body[data-limit="5"] .chat-overlay-message:nth-child(n+6) { display: none !important; }
body[data-limit="6"] .chat-overlay-message:nth-child(n+7) { display: none !important; }
body[data-limit="7"] .chat-overlay-message:nth-child(n+8) { display: none !important; }
body[data-limit="8"] .chat-overlay-message:nth-child(n+9) { display: none !important; }
body[data-limit="9"] .chat-overlay-message:nth-child(n+10) { display: none !important; }
"#;

const OVERLAY_JS: &str = r#"
(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.has("theme")) document.body.dataset.theme = params.get("theme");
    if (params.has("size")) document.body.dataset.size = params.get("size");
    if (params.has("limit")) document.body.dataset.limit = params.get("limit");
    if (params.has("align")) document.body.dataset.align = params.get("align");
    if (params.has("direction")) document.body.dataset.direction = params.get("direction");
    if (params.has("highlight")) document.body.dataset.highlight = params.get("highlight");

    const fade = parseInt(params.get("fade"), 10);
    if (fade > 0) {
        const style = document.createElement("style");
        style.textContent = "@keyframes chatOverlayFade { 0%, 75% { opacity: 1; transform: translateY(0); } 100% { opacity: 0; transform: translateY(-4px); pointer-events: none; } } .chat-overlay-message { animation: chatOverlayFade " + fade + "s forwards ease-in-out; }";
        document.head.appendChild(style);
    }
})();
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
                topcoat::runtime::script()
                <script src=(crate::web::CHAT_EVENTS_SCRIPT) defer="defer"></script>
                <style>(OVERLAY_CSS)</style>
            </head>
            <body
                class="min-h-screen bg-transparent p-2 font-sans text-foreground antialiased selection:bg-none"
                style="background: transparent !important; background-color: transparent !important;"
            >
                chat_overlay()
                <script>(OVERLAY_JS)</script>
            </body>
        </html>
    })
}

#[component]
pub async fn chat_overlay(cx: &Cx) -> Result<impl View> {
    let revision = signal(cx, || 0.0);

    Ok(view! {
        <div id="chat-overlay-wrapper" class="flex flex-col w-full">
            chat_overlay_content(revision: $(revision.get()))
            <button
                id="chat-refresh-button"
                type="button"
                hidden="hidden"
                aria-hidden="true"
                tabindex="-1"
                class="hidden"
                @click=$(|_event: topcoat::runtime::Event| revision.increment())
            ></button>
        </div>
    })
}

#[shard]
pub async fn chat_overlay_content(cx: &Cx, revision: f64) -> Result<impl View> {
    let _ = revision;
    let app: &AppHandle = app_context(cx);
    let snapshot = app.chat.snapshot().await?;

    Ok(view! {
        <div id="chat-overlay-messages" class="flex flex-col gap-2">
            if snapshot.messages.is_empty() {
                <div class="hidden" aria-hidden="true"></div>
            } else {
                for (index, message) in snapshot.messages.into_iter().enumerate() {
                    chat_overlay_message(message: message, highlighted: index == 0)
                }
            }
        </div>
    })
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
    let author_color = source_color(message.source);

    Ok(view! {
        <article
            class=(row_class)
            data-source=(message.source.to_string())
            data-highlighted=(if highlighted { "true" } else { "false" })
        >
            chat_source_icon(source: message.source)
            <p class="min-w-0 break-words text-sm leading-snug">
                <span class=(format!("mr-1 font-semibold {author_color}"))>
                    (message.author)
                </span>
                <span class="text-zinc-100">(message.text)</span>
            </p>
        </article>
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_message_class_sets_highlighted_and_standard_styles() {
        let highlighted = overlay_message_class(true);
        assert!(highlighted.contains("bg-primary/20"));
        assert!(highlighted.contains("ring-primary/40"));
        assert!(highlighted.contains("chat-overlay-message"));

        let standard = overlay_message_class(false);
        assert!(standard.contains("bg-black/60"));
        assert!(standard.contains("border-white/10"));
        assert!(standard.contains("chat-overlay-message"));
    }
}
