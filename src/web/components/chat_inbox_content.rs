use crate::server::state::ProxyState;
use crate::web::components::chat_message::chat_message;
use crate::web::components::ui::card::card_content;
use crate::web::components::ui::empty_state::empty_state;
use crate::web::components::youtube_ingest_status::youtube_ingest_status;
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
    let youtube_status = state.youtube_status.read().await.clone();

    view! {
        card_content(
            <div class="mb-4 flex justify-end gap-4 text-right">
                <span class="text-sm font-medium">(format!("{} queued", snapshot.queued))</span>
                <span class="text-sm text-muted-foreground">(format!("{} dropped", snapshot.dropped))</span>
            </div>
            if let Some(status) = youtube_status {
                youtube_ingest_status(status: status)
            }
            if snapshot.messages.is_empty() {
                empty_state("No chat messages waiting.")
            } else {
                <div class="flex flex-col gap-2">
                    for (index, message) in snapshot.messages.into_iter().enumerate() {
                        chat_message(message: message, highlighted: index == 0)
                    }
                </div>
            }
        )
    }
}
