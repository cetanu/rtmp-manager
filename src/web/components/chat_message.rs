use crate::chat::ChatMessage;
use topcoat::{
    Result,
    view::{component, view},
};

fn source_color(source: &str) -> &'static str {
    match source {
        "twitch" => "text-[#9146ff]",
        "youtube" => "text-[#ff0033]",
        "x" => "text-sky-500",
        _ => "text-muted-foreground",
    }
}

#[component]
async fn chat_source_icon(source: String) -> Result {
    let color = source_color(&source);

    view! {
        <span class=(format!("mt-0.5 shrink-0 {color}")) title=(source.clone())>
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
        </span>
    }
}

#[component]
pub async fn chat_message(message: ChatMessage, highlighted: bool) -> Result {
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
