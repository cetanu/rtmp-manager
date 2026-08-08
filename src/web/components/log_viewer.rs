use topcoat::{
    Result,
    view::{component, view},
};

#[component]
pub async fn log_viewer() -> Result {
    view! {
        <div class="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
            <div class="flex items-center justify-between border-b border-border px-4 py-3">
                <div>
                    <h2 class="font-semibold">"Live service logs"</h2>
                    <p class="text-xs text-muted-foreground">"The latest 500 entries are retained in memory."</p>
                </div>
                <button type="button" data-log-clear="true" class="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-foreground/5">"Clear view"</button>
            </div>
            <div data-log-status="true" class="border-b border-border px-4 py-2 text-xs text-muted-foreground">"Connecting…"</div>
            <div data-log-output="true" role="log" aria-live="polite" class="h-[65vh] overflow-auto bg-black p-4 font-mono text-xs leading-5 text-zinc-200">
                <div class="text-zinc-500">"Waiting for log entries…"</div>
            </div>
        </div>
    }
}
