use topcoat::{
    view::{component, view},
    Result,
};

#[component]
pub async fn stream_preview_player() -> Result {
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
            <div id="stream-preview-placeholder" class="absolute inset-0 flex items-center justify-center p-6 text-center text-sm text-white/70">
                "Start streaming to the RTMP ingest to create a preview."
            </div>
        </div>
    }
}
