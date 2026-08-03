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
    let stream_key = config.server.ingest_stream_key.clone();
    drop(config);
    if stream_key.is_empty() {
        return Ok("Configure an ingest stream key before starting a test stream".to_owned());
    }

    let url = format!("rtmp://127.0.0.1:{}/live/{}", state.listen_port, stream_key);
    tokio::spawn(async move {
        tracing::info!(
            duration_secs,
            "Starting test stream via ffmpeg to local ingest..."
        );
        let video_source = format!("testsrc=duration={duration_secs}:size=1280x720:rate=30");
        let audio_source = format!("sine=frequency=1000:duration={duration_secs}");
        let result = tokio::process::Command::new("ffmpeg")
            .args([
                "-re",
                "-f",
                "lavfi",
                "-i",
                &video_source,
                "-f",
                "lavfi",
                "-i",
                &audio_source,
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-f",
                "flv",
                &url,
            ])
            .output()
            .await;
        if let Err(error) = result {
            tracing::error!(%error, "Test stream failed to start");
        }
        tracing::info!("Test stream finished.");
    });
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
        .map(crate::notifications::NotificationTarget::from)
        .collect::<Vec<_>>();
    let dispatcher = crate::notifications::NotificationDispatcher::new(
        &config.notifications,
        state.http_client.clone(),
    );
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
