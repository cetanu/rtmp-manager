use crate::chat::youtube::YouTubeIngestStatus;
use topcoat::{
    view::{component, view},
    Result,
};

#[component]
pub async fn youtube_ingest_status(status: YouTubeIngestStatus) -> Result {
    view! {
        <aside class="mb-4 rounded-lg border bg-muted/30 px-4 py-3 text-sm">
            <div class="flex flex-wrap items-center justify-between gap-2">
                <span class="font-semibold">"YouTube ingest"</span>
                <span class="uppercase tracking-wide text-muted-foreground">(status.state)</span>
            </div>
            <p class="mt-1 text-muted-foreground">(status.detail)</p>
            <p class="mt-1 text-xs text-muted-foreground">
                (format!("{} messages received", status.messages_received))
            </p>
        </aside>
    }
}
