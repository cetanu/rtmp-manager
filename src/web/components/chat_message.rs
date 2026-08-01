use crate::chat::ChatMessage;
use topcoat::{
    view::{component, view},
    Result,
};

#[component]
pub async fn chat_message(message: ChatMessage) -> Result {
    view! {
        <article class="flex flex-col gap-4 rounded-xl border bg-surface p-5">
            <header class="flex items-center justify-between gap-4">
                <div class="min-w-0">
                    <div class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                        (message.source)
                    </div>
                    <div class="truncate font-semibold">(message.author)</div>
                </div>
                <time class="shrink-0 text-xs text-muted-foreground">
                    (message.sent_at.unwrap_or_default())
                </time>
            </header>
            <p class="whitespace-pre-wrap break-words text-lg leading-relaxed">
                (message.text)
            </p>
        </article>
    }
}
