use crate::chat::ChatMessage;
use crate::web::components::chat_source::{chat_source_icon, source_color};
use topcoat::{
    Result,
    view::{View, component, view},
};

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
                <span class=(text_class)>(message.text)</span>
            </p>
        </article>
    })
}
