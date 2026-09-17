use crate::notifications::{NotificationDispatcher, NotificationTarget};
use crate::server::state::AppHandle;
use crate::web::components::ui::button::{ButtonVariant, button};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{Event, procedure, signal},
    view::{View, attributes, component, view},
};

#[procedure]
async fn start_test_stream(cx: &Cx) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    if app.stream.status().state == crate::server::preview::StreamState::Live {
        return Ok("Cannot run a test stream while publishing live".to_owned());
    }
    if app.stream.is_test_stream_running() {
        return Ok("A test stream is already in progress".to_owned());
    }

    let config = app.config.get();
    let duration_secs = config.server.test_stream_duration_secs;
    let targets = config
        .targets
        .iter()
        .filter(|target| target.enabled)
        .cloned()
        .collect::<Vec<_>>();

    if targets.is_empty() {
        return Ok("Enable at least one target before starting a test stream".to_owned());
    }

    app.stream.run_test_stream(duration_secs, targets);
    Ok(String::new())
}

#[procedure]
async fn send_test_webhooks(cx: &Cx) -> Result<String> {
    let app: &AppHandle = app_context(cx);
    let config = app.config.get();
    let active_targets = config
        .targets
        .iter()
        .filter(|target| target.enabled)
        .map(NotificationTarget::from)
        .collect::<Vec<_>>();

    if active_targets.is_empty() {
        return Ok("Enable at least one target before sending test webhooks".to_owned());
    }

    let dispatcher = NotificationDispatcher::new(&config.notifications, app.http_client.clone());

    tokio::spawn(async move {
        dispatcher.dispatch(&active_targets).await;
    });
    Ok(String::new())
}

#[component]
pub async fn actions_panel(cx: &Cx) -> Result<impl View> {
    let testing = signal(cx, || false);
    let test_error = signal(cx, String::new);

    Ok(view! {
        <div
            class="flex flex-col sm:flex-row gap-4 justify-between items-center bg-surface p-4 border rounded-xl"
        >
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
                <p
                    :hidden=$(test_error.get().is_empty())
                    class="text-sm text-destructive"
                >
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
    })
}
