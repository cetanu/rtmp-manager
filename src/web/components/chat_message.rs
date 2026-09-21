use crate::chat::{ChatMessage, ChatMessagePart};
use crate::web::components::chat_source::{chat_source_icon, source_color};
use topcoat::{
    Result,
    view::{View, ViewExt, component, view},
};

#[component]
async fn chat_message_part(part: ChatMessagePart, text_class: &'static str) -> Result<impl View> {
    Ok(match part {
        ChatMessagePart::Text(text) => view! {
            <span class=(text_class)>(text)</span>
        }
        .boxed(),
        ChatMessagePart::Emoji { alt, url } => view! {
            <img
                src=(url)
                alt=(alt.clone())
                title=(alt)
                class="inline-block h-[1.2em] w-auto align-[-0.2em] object-contain"
                loading="lazy"
            />
        }
        .boxed(),
    })
}

#[component]
async fn chat_message_parts(
    parts: Vec<ChatMessagePart>,
    text_class: &'static str,
) -> Result<impl View> {
    Ok(view! {
        for part in parts {
            chat_message_part(part: part, text_class: text_class)
        }
    })
}

#[component]
pub(crate) async fn chat_message_card(
    message: ChatMessage,
    row_class: &'static str,
    highlighted: bool,
    overlay: bool,
) -> Result<impl View> {
    let author_color = source_color(message.source);
    let data_source = overlay.then(|| message.source.to_string());
    let data_highlighted = overlay.then_some(if highlighted { "true" } else { "false" });
    let text_class = if overlay { "text-zinc-100" } else { "" };
    let parts = if message.parts.is_empty() {
        vec![ChatMessagePart::Text(message.text.clone())]
    } else {
        message.parts
    };

    Ok(view! {
        <article
            class=(row_class)
            data-source=(data_source)
            data-highlighted=(data_highlighted)
        >
            chat_source_icon(source: message.source)
            <p class="min-w-0 break-words text-sm leading-snug">
                <span class=(format!("mr-1 font-semibold {author_color}"))>
                    (message.author)
                </span>
                chat_message_parts(parts: parts, text_class: text_class)
            </p>
        </article>
    })
}
