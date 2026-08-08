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

    tokio::spawn(async move {
        tracing::info!(
            duration_secs,
            target_count = targets.len(),
            "Starting direct test stream to enabled targets"
        );
        let mut tasks = tokio::task::JoinSet::new();
        for target in targets {
            tasks.spawn(async move {
                let destination = if target.stream_key.is_empty() {
                    target.url.clone()
                } else if target.url.ends_with('/') {
                    format!("{}{}", target.url, target.stream_key)
                } else {
                    format!("{}/{}", target.url, target.stream_key)
                };
                let video_source = format!("testsrc=duration={duration_secs}:size=1280x720:rate=30");
                let audio_source = format!("sine=frequency=1000:duration={duration_secs}");
                let output = tokio::process::Command::new("ffmpeg")
                    .args([
                        "-hide_banner", "-loglevel", "warning", "-re", "-f", "lavfi", "-i",
                        &video_source, "-f", "lavfi", "-i", &audio_source, "-c:v", "libx264",
                        "-preset", "veryfast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a",
                        "128k", "-f", "flv", &destination,
                    ])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::piped())
                    .output()
                    .await;
                match output {
                    Ok(output) if output.status.success() => {
                        tracing::info!(name = %target.name, "Direct target test completed successfully");
                    }
                    Ok(output) => {
                        let detail = safe_ffmpeg_failure(&String::from_utf8_lossy(&output.stderr));
                        tracing::error!(name = %target.name, status = %output.status, %detail, "Direct target test failed");
                    }
                    Err(error) => {
                        tracing::error!(name = %target.name, %error, "Direct target test FFmpeg failed to start");
                    }
                }
            });
        }
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                tracing::error!(%error, "Direct target test task failed");
            }
        }
    });
    Ok(String::new())
}

fn safe_ffmpeg_failure(stderr: &str) -> &'static str {
    let detail = stderr.to_ascii_lowercase();
    for (needle, message) in [
        ("connection refused", "Connection refused by target"),
        ("connection timed out", "Connection to target timed out"),
        ("network is unreachable", "Target network is unreachable"),
        (
            "name or service not known",
            "Target hostname could not be resolved",
        ),
        ("authentication", "Target rejected authentication"),
        ("broken pipe", "Target closed the connection"),
        ("server error", "Target returned a server error"),
        ("unknown encoder", "Required FFmpeg encoder is unavailable"),
    ] {
        if detail.contains(needle) {
            return message;
        }
    }
    "FFmpeg failed; destination and credentials omitted from logs"
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

#[cfg(test)]
mod tests {
    use super::safe_ffmpeg_failure;

    #[test]
    fn ffmpeg_failure_summary_does_not_echo_diagnostics() {
        let stderr = "rtmp://example.test/app/private-key: Connection refused";
        assert_eq!(safe_ffmpeg_failure(stderr), "Connection refused by target");
        assert!(!safe_ffmpeg_failure(stderr).contains("private-key"));
    }
}
