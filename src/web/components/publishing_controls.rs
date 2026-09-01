use crate::server::state::{AppHandle, StreamState};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{Event, procedure, shard},
    view::view,
};

#[procedure]
async fn toggle_publishing(cx: &Cx, is_live: bool) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    let tenant_id = crate::web::auth::current_user(cx).tenant_id.clone();
    let result = if is_live {
        app.stream.stop_publishing(tenant_id).await
    } else {
        app.stream.publish_staged_stream(tenant_id).await
    };
    Ok(result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default())
}

#[shard]
pub async fn publishing_controls(cx: &Cx, revision: f64) -> Result {
    let _ = revision;
    let app: &AppHandle = app_context(cx);
    let status = app
        .stream
        .status(&crate::web::auth::current_user(cx).tenant_id);
    let is_live = status.state == StreamState::Live;
    let toggle_available = is_live
        || matches!(
            status.state,
            StreamState::Preparing | StreamState::PreviewReady | StreamState::PreviewFailed
        );
    let toggle_class = "inline-flex h-8 items-center justify-center rounded-md bg-foreground/10 px-3 text-xs font-medium text-muted-foreground shadow-xs transition-colors hover:bg-foreground/15 hover:text-foreground active:bg-foreground/20 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50 data-[live=true]:bg-destructive data-[live=true]:text-destructive-foreground data-[live=true]:hover:bg-destructive/90 data-[live=true]:hover:text-destructive-foreground data-[live=true]:active:bg-destructive/80";

    view! {
        signal pending = false;
        signal action_error = String::new();
        signal live = is_live;
        signal can_toggle = toggle_available;

        <div class="flex items-center gap-2">
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
                $(if live.get() { "LIVE" } else { "Go live" })
            </button>
            <p :hidden=$(action_error.get().is_empty()) class="text-sm text-destructive">
                $(action_error.get())
            </p>
        </div>
    }
}
