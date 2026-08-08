use topcoat::{
    Result,
    view::{component, view},
};

#[component]
pub async fn log_viewer() -> Result {
    view! {
        <div class="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
            <div data-log-status="true" class="border-b border-border px-4 py-2 text-xs text-muted-foreground">"Connecting…"</div>
            <div data-log-output="true" role="log" aria-live="polite" class="h-[65vh] overflow-auto bg-black p-4 font-mono text-xs leading-5 text-zinc-200">
                <div class="text-zinc-500">"Waiting for log entries…"</div>
            </div>
        </div>
    }
}
