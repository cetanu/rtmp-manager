use topcoat::{
    Result,
    view::{View, component, view},
};

#[component]
pub async fn log_viewer() -> Result<impl View> {
    Ok(view! {
        <div class="hud-panel !gap-0 overflow-hidden">
            <div
                data-log-status="true"
                class="flex items-center gap-2 border-b border-border px-4 py-2"
            >
                <span class="hud-dot bg-signal text-signal"></span>
                <span class="hud-label">"SYS.LOG // TAIL -F"</span>
                <span data-log-status-text="true" class="ml-auto hidden font-mono text-[10px] tracking-[0.16em] text-muted-foreground uppercase sm:inline">"Connecting…"</span>
            </div>
            <div
                data-log-output="true"
                role="log"
                aria-live="polite"
                class="hud-scanlines h-[65vh] overflow-auto bg-black/80 p-4 font-mono text-[11px] leading-5 text-signal/80"
            >
                <div class="text-muted-foreground">"// Waiting for log entries…"</div>
            </div>
        </div>
    })
}
