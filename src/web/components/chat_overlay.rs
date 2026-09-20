use crate::chat::ChatMessage;
use crate::server::state::AppHandle;
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
"#;

const OVERLAY_JS: &str = r#"
(() => {
    const params = new URLSearchParams(window.location.search);
    const setChoice = (name, allowed) => {
        const value = params.get(name);
        if (allowed.includes(value)) document.body.dataset[name] = value;
    };
    setChoice("theme", ["plain", "solid"]);
    setChoice("size", ["sm", "lg", "xl"]);
    setChoice("align", ["top", "bottom"]);
    setChoice("direction", ["down", "up", "reverse"]);
    setChoice("highlight", ["true", "false"]);

    const limit = Number.parseInt(params.get("limit"), 10);
    if (Number.isInteger(limit) && limit >= 1 && limit <= 100) {
        const style = document.createElement("style");
        style.textContent = `.chat-overlay-message:nth-child(n+${limit + 1}) { display: none !important; }`;
        document.head.appendChild(style);
    }

    const fade = Number.parseInt(params.get("fade"), 10);
    if (Number.isInteger(fade) && fade > 0 && fade <= 3600) {
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
                <script src=(crate::web::OVERLAY_EVENTS_SCRIPT) defer="defer"></script>
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
    let app: &AppHandle = app_context(cx);
    let snapshot = app.chat.snapshot().await?;

    Ok(view! {
        <div id="chat-overlay-wrapper" class="flex flex-col w-full">
            chat_overlay_messages(messages: snapshot.messages)
        </div>
    })
}

#[component]
pub async fn chat_overlay_messages(messages: Vec<ChatMessage>) -> Result<impl View> {
    Ok(view! {
        <div id="chat-overlay-messages" class="flex flex-col gap-2">
            if messages.is_empty() {
                <div class="hidden" aria-hidden="true"></div>
            } else {
                for (index, message) in messages.into_iter().enumerate() {
                    chat_overlay_message(message: message, highlighted: index == 0)
                }
            }
        </div>
    })
}

pub async fn render_chat_overlay_messages(messages: Vec<ChatMessage>) -> Result<String> {
    let cx = Cx::default();
    let __cx = &cx;
    let view = view! { chat_overlay_messages(messages: messages) };
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
            overlay: true,
        )
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

    #[tokio::test]
    async fn renders_overlay_messages_as_escaped_server_html() {
        let html = render_chat_overlay_messages(vec![ChatMessage {
            id: 1,
            source: crate::chat::Source::Twitch,
            external_id: "external-1".into(),
            author: "<viewer>".into(),
            text: "<script>alert(1)</script>".into(),
            avatar_url: None,
            sent_at: None,
            received_at_unix_ms: 1,
        }])
        .await
        .unwrap();

        assert!(html.contains("id=\"chat-overlay-messages\""));
        assert!(html.contains("data-source=\"twitch\""));
        assert!(html.contains("&lt;viewer&gt;"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
    }
}
