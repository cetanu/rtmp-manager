use crate::server::state::ProxyState;
use crate::web::components::ui::button::{ButtonVariant, button};
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{Event, procedure, shard},
    view::{attributes, view},
};

#[procedure]
async fn publish_stream(cx: &Cx) -> Result<String> {
    let state: &Arc<ProxyState> = app_context(cx);
    Ok(state
        .publish_staged_stream()
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default())
}

#[procedure]
async fn stop_stream(cx: &Cx) -> Result<String> {
    let state: &Arc<ProxyState> = app_context(cx);
    Ok(state
        .stop_publishing()
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default())
}

#[shard]
pub async fn publishing_controls(cx: &Cx, revision: f64) -> Result {
    let _ = revision;
    let state: &Arc<ProxyState> = app_context(cx);
    let status = state.stream_status().await;

    view! {
        signal pending = false;
        signal action_error = String::new();
        signal can_publish = status.active && !status.published;
        signal can_stop = status.active && status.published;

        <div class="flex flex-col justify-between gap-6 rounded-xl border bg-surface p-5">
            <div>
                <h3 class="font-semibold">"Publishing controls"</h3>
                <p class="mt-2 text-sm leading-relaxed text-muted-foreground">
                    "Review the staged preview, then publish it to every enabled RTMP target. Going-live notifications are sent only when you publish."
                </p>
            </div>
            <div class="flex flex-col gap-3">
                button(
                    attrs: attributes! {
                        type="button"
                        :disabled=$(if pending.get() { true } else { !can_publish.get() })
                        @click=$(async |_event: Event| {
                            pending.set(true);
                            action_error.set(publish_stream().await);
                            pending.set(false);
                        })
                    },
                    "Publish Live"
                )
                button(
                    variant: ButtonVariant::Destructive,
                    attrs: attributes! {
                        type="button"
                        :disabled=$(if pending.get() { true } else { !can_stop.get() })
                        @click=$(async |_event: Event| {
                            pending.set(true);
                            action_error.set(stop_stream().await);
                            pending.set(false);
                        })
                    },
                    "Stop Publishing"
                )
                <p :hidden=$(action_error.get().is_empty()) class="text-sm text-destructive">
                    $(action_error.get())
                </p>
            </div>
        </div>
    }
}
