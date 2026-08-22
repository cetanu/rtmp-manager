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
        <div class="relative mx-auto h-[calc(100dvh-10rem)] max-h-[56.25vw] max-w-full aspect-video overflow-hidden bg-black">
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
    let message = status.state.to_string();
    view! {
        <div
            class="absolute inset-0 flex items-center justify-center text-center text-sm text-white/70"
        >
            (message)
        </div>
    }
}
