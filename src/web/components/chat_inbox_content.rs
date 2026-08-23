use crate::server::state::ProxyState;
use crate::web::components::chat_message::chat_message;
use crate::web::components::ui::card::card_content;
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::shard,
    view::view,
};

#[shard]
pub async fn chat_inbox_content(cx: &Cx, revision: f64) -> Result {
    let _ = revision;
    let state: &Arc<ProxyState> = app_context(cx);
    let snapshot = state.chat_inbox.lock().await.snapshot().await?;

    view! {
        card_content(
            <div class="h-[min(22rem,calc(100dvh-10rem))] overflow-y-auto pr-1">
                    <div class="flex flex-col gap-2">
                if snapshot.messages.is_empty() {
                    "No chat messages waiting."
                } else {
                    for (index, message) in snapshot.messages.into_iter().enumerate() {
                        chat_message(message: message, highlighted: index == 0)
                    }
                }
                    </div>
            </div>
            <div class="mb-4 flex justify-end gap-4 text-right">
                <span class="text-sm font-medium">(format!("{} queued", snapshot.queued))</span>
                <span class="text-sm text-muted-foreground">(format!("{} dropped", snapshot.dropped))</span>
            </div>
        )
    }
}
