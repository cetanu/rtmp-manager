use crate::notifications::{NotificationDispatcher, NotificationTarget};
use crate::server::state::ProxyState;
use crate::web::components::ui::button::{ButtonVariant, button};
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{Event, procedure},
    view::{attributes, component, view},
};

#[procedure]
async fn start_test_stream(cx: &Cx) -> Result<String> {
    let state: &Arc<ProxyState> = app_context(cx);
    let config = state.config.read().await;
    let duration_secs = config.server.test_stream_duration_secs;
    let targets = config
        .targets
        .iter()
        .filter(|target| target.enabled)
        .cloned()
        .collect::<Vec<_>>();
    drop(config);

    if targets.is_empty() {
        return Ok("Enable at least one target before starting a test stream".to_owned());
    }

    state.run_test_stream(duration_secs, targets);
    Ok(String::new())
}

#[procedure]
async fn send_test_webhooks(cx: &Cx) -> Result<String> {
    let state: &Arc<ProxyState> = app_context(cx);
    let config = state.config.read().await;
    let active_targets = config
        .targets
        .iter()
        .filter(|target| target.enabled)
        .map(NotificationTarget::from)
        .collect::<Vec<_>>();
    let dispatcher =
        NotificationDispatcher::new(&config.notifications, state.http_client.clone());
    drop(config);

    tokio::spawn(async move {
        dispatcher.dispatch(&active_targets).await;
    });
    Ok(String::new())
}

#[component]
pub async fn actions_panel() -> Result {
    view! {
        signal testing = false;
        signal test_error = String::new();

        <div class="flex flex-col sm:flex-row gap-4 justify-between items-center bg-surface p-4 border rounded-xl">
            <div class="flex flex-col gap-3 sm:flex-row sm:items-center">
                <div class="flex gap-4">
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="button"
                        :disabled=$(testing.get())
                        @click=$(async |_event: Event| {
                            testing.set(true);
                            test_error.set(start_test_stream().await);
                            testing.set(false);
                        })
                    },
                    "Test Stream"
                )
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="button"
                        :disabled=$(testing.get())
                        @click=$(async |_event: Event| {
                            testing.set(true);
                            test_error.set(send_test_webhooks().await);
                            testing.set(false);
                        })
                    },
                    "Test Webhooks"
                )
                </div>
                <p :hidden=$(test_error.get().is_empty()) class="text-sm text-destructive">
                    $(test_error.get())
                </p>
            </div>
            <div class="flex gap-4 w-full sm:w-auto">
                button(
                    variant: ButtonVariant::Secondary,
                    attrs: attributes! { type="reset" class="w-full sm:w-auto" },
                    "Revert"
                )
                button(
                    variant: ButtonVariant::Primary,
                    attrs: attributes! {
                        type="submit"
                        id="saveBtn"
                        name="action"
                        value="save"
                        class="w-full sm:w-auto min-w-[120px]"
                        formaction="/api/config"
                    },
                    "Save Configuration"
                )
            </div>
        </div>
    }
}
