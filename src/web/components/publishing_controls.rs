use crate::server::state::{AppHandle, StreamState};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{Event, procedure, shard, signal},
    view::{View, view},
};

#[procedure]
async fn toggle_publishing(cx: &Cx, is_live: bool) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    let result = if is_live {
        app.stream.stop_publishing().await
    } else {
        app.stream.publish_staged_stream().await
    };
    Ok(result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default())
}

#[shard]
pub async fn publishing_controls(cx: &Cx, revision: f64) -> Result<impl View> {
    let _ = revision;
    let app: &AppHandle = app_context(cx);
    let status = app.stream.status();
    let is_live = status.state == StreamState::Live;
    let toggle_available = is_live
        || matches!(
            status.state,
            StreamState::Preparing | StreamState::PreviewReady | StreamState::PreviewFailed
        );
    let toggle_class = "inline-flex h-8 cursor-pointer items-center justify-center gap-2 rounded-sm border border-border bg-white/5 px-3 font-mono text-[11px] font-semibold tracking-[0.14em] text-muted-foreground uppercase shadow-xs transition-colors outline-none hover:border-signal/50 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background active:bg-white/10 disabled:pointer-events-none disabled:opacity-50 data-[live=true]:border-live/50 data-[live=true]:bg-live/10 data-[live=true]:text-live data-[live=true]:hover:bg-live/20 data-[live=true]:active:bg-live/30";

    let pending = signal(cx, || false);
    let action_error = signal(cx, String::new);
    let live = signal(cx, || is_live);
    let can_toggle = signal(cx, || toggle_available);

    Ok(view! {
        <div class="flex flex-wrap items-center gap-2">
            <span class="hud-label hidden sm:inline">"TRANSMISSION CONTROL //"</span>
            <button
                type="button"
                class=(toggle_class)
                :data-live=$(live.get())
                :disabled=$(if pending.get() { true } else { !can_toggle.get() })
                @click=$(async |_event: Event| {
                    pending.set(true);
                    let error = toggle_publishing(live.get()).await;
                    if error.is_empty() {
                        live.set(!live.get());
                    }
                    action_error.set(error);
                    pending.set(false);
                })
            >
                <span
                    :class=$(if live.get() {
                        "hud-dot bg-live text-live animate-rec"
                    } else if can_toggle.get() {
                        "hud-dot bg-signal text-signal"
                    } else {
                        "hud-dot bg-white/30 text-white/30"
                    })
                ></span>
                $(if live.get() { "LIVE" } else if can_toggle.get() { "PUBLISH" } else { "INACTIVE" })
            </button>
            <p
                :hidden=$(action_error.get().is_empty())
                class="font-mono text-[11px] text-live"
            >
                $(action_error.get())
            </p>
        </div>
    })
}
