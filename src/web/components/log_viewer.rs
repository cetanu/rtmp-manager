use crate::log_buffer;
use topcoat::{
    Result,
    context::Cx,
    runtime::{connected, shard},
    view::{View, emit, live},
};

#[shard]
pub async fn log_viewer(cx: &Cx) -> Result<impl View> {
    Ok(live! {
        let logs = log_buffer::global().map_err(topcoat::Error::from_anyhow)?;
        let mut changed = logs.subscribe();
        loop {
            let entries = logs.snapshot();
            let token = emit! {
                <div class="hud-panel !gap-0 overflow-hidden">
                    <div
                        class="flex items-center gap-2 border-b border-border px-4 py-2"
                    >
                        <span class="hud-dot bg-signal text-signal"></span>
                        <span class="hud-label">"SYSTEM LOG //"</span>
                        <span class="ml-auto hidden font-mono text-[10px] tracking-[0.16em] text-muted-foreground uppercase sm:inline">
                            (if connected(cx) { "● LIVE // FOLLOWING" } else { "CONNECTING…" })
                        </span>
                    </div>
                    <div
                        role="log"
                        aria-live="polite"
                        class="hud-scanlines flex h-[65vh] flex-col-reverse overflow-auto bg-black/80 p-4 font-mono text-[11px] leading-5 text-signal/80"
                    >
                        <div>
                            if entries.is_empty() {
                                <div class="text-muted-foreground">
                                    "// Waiting for log entries…"
                                </div>
                            }
                            for entry in entries {
                                <div
                                    class=(match entry.level.as_str() {
                                        "ERROR" => "text-red-400",
                                        "WARN" => "text-amber-300",
                                        _ => "text-emerald-200/80",
                                    })
                                >
                                    (format!(
                                        "{} {:5} {} — {}",
                                        entry.timestamp_ms,
                                        entry.level,
                                        entry.target,
                                        entry.message,
                                    ))
                                </div>
                            }
                        </div>
                    </div>
                </div>
            }?;
            if !connected(cx) {
                break Ok(token);
            }
            match changed.recv().await {
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break Ok(token),
            }
        }
    })
}
