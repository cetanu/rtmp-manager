use crate::server::state::ProxyState;
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::shard,
    view::{View, component, view},
};

#[component]
pub async fn stream_preview_player(child: View) -> Result {
    view! {
        <div class="relative aspect-video overflow-hidden rounded-xl border bg-black">
            <video
                id="stream-preview-video"
                class="h-full w-full bg-black object-contain"
                controls="controls"
                autoplay="autoplay"
                muted="muted"
                playsinline="playsinline"
            ></video>
            (child)
        </div>
    }
}

#[shard]
pub async fn stream_preview_placeholder(cx: &Cx, revision: f64) -> Result {
    let _ = revision;
    let state: &Arc<ProxyState> = app_context(cx);
    let status = state.stream_status().await;
    let message = if !status.active {
        "Start streaming to the RTMP ingest to create a preview."
    } else if status.preview_failed {
        "HLS preview unavailable."
    } else {
        "Preparing HLS preview…"
    };

    view! {
        <div
            hidden=(status.preview_ready)
            class="absolute inset-0 flex items-center justify-center p-6 text-center text-sm text-white/70"
        >
            (message)
        </div>
    }
}
