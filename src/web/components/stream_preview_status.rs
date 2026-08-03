use crate::server::state::ProxyState;
use crate::web::components::ui::card::card_title;
use crate::web::components::ui::status_badge::status_badge;
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::shard,
    view::{attributes, view},
};

#[shard]
pub async fn stream_preview_status(cx: &Cx, revision: f64) -> Result {
    let _ = revision;
    let state: &Arc<ProxyState> = app_context(cx);
    let status = state.stream_status().await;
    let (label, detail, state_name) = if !status.active {
        (
            "Offline",
            "Waiting for an RTMP stream. Nothing will be sent to external targets automatically.",
            "offline",
        )
    } else if status.preview_failed {
        (
            "Preview error",
            "The HLS preview process stopped. Check the server logs, then reconnect the RTMP stream.",
            "error",
        )
    } else if status.published {
        (
            "Live",
            "Publishing to enabled targets. The local preview remains available.",
            "live",
        )
    } else {
        (
            "Staged",
            if status.preview_ready {
                "Preview ready. Review it before publishing to enabled targets."
            } else {
                "Stream connected. Preparing the HLS preview…"
            },
            "staged",
        )
    };

    view! {
        <div class="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
            <div>
                card_title("Staged Stream")
                <p class="mt-1 text-sm text-muted-foreground">(detail)</p>
            </div>
            status_badge(attrs: attributes! { data-state=(state_name) },
                (label)
            )
        </div>
    }
}
