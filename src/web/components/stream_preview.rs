use crate::server::state::AppHandle;
use crate::web::components::publishing_controls::publishing_controls;
use crate::web::components::ui::card::{card, card_content, card_header};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{connected, shard},
    view::{Child, View, component, emit, live, view},
};

#[component]
pub async fn stream_preview() -> Result<impl View> {
    Ok(view! {
        card(
            card_header(publishing_controls())
            card_content(stream_preview_player(stream_preview_status()))
        )
    })
}

#[component]
pub async fn stream_preview_player(#[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <rtmp-preview class="block">
            <div
                class="hud-well hud-scanlines relative mx-auto h-[calc(100dvh-10rem)] max-h-[56.25vw] max-w-full aspect-video bg-black"
            >
                <video
                    id="stream-preview-video"
                    class="h-full w-full bg-black object-contain"
                    controls="controls"
                    autoplay="autoplay"
                    muted="muted"
                    playsinline="playsinline"
                ></video>
                <span class="hud-corner hud-corner-tl"></span>
                <span class="hud-corner hud-corner-tr"></span>
                <span class="hud-corner hud-corner-bl"></span>
                <span class="hud-corner hud-corner-br"></span>
                (child)
            </div>
        </rtmp-preview>
    })
}

#[shard]
pub async fn stream_preview_status(cx: &Cx) -> Result<impl View> {
    Ok(live! {
        let app: &AppHandle = app_context(cx);
        let mut changed = app.stream.subscribe_status();
        let mut metrics_changed = app.metrics.subscribe();
        loop {
            let token = emit! { preview_status() }?;
            if !connected(cx) {
                break Ok(token);
            }
            tokio::select! {
                result = changed.changed() => if result.is_err() { break Ok(token); },
                result = metrics_changed.changed() => if result.is_err() { break Ok(token); },
            }
        }
    })
}

#[component]
async fn preview_status(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = app_context(cx);
    let status = app.stream.status();
    let ready = matches!(
        status.state,
        crate::server::state::StreamState::PreviewReady | crate::server::state::StreamState::Live
    );
    let live = status.state == crate::server::state::StreamState::Live;
    let message = format!("// {}", status.state.to_string().to_uppercase());
    let bitrate = super::metrics::format_bitrate(app.metrics.current_ingest_bps());
    Ok(view! {
        <span data-preview-state=(status.state.to_string()) hidden="hidden"></span>
        <div
            class="pointer-events-none absolute inset-x-0 top-0 flex items-center justify-between gap-2 px-4 pt-3"
            aria-hidden="true"
        >
            <span
                class="flex items-center gap-1.5 rounded-[3px] border border-white/10 bg-black/60 px-2 py-1 font-mono text-[10px] tracking-[0.18em] text-white/80 uppercase backdrop-blur"
            >
                <span
                    class=(if live {
                        "hud-dot bg-red-500 text-red-500 animate-rec"
                    } else if ready {
                        "hud-dot bg-emerald-400 text-emerald-400"
                    } else {
                        "hud-dot bg-white/30 text-white/30"
                    })
                ></span>
                <span>
                    (if live { "REC" } else if ready { "READY" } else { "STBY" })
                </span>
            </span>
            <span
                class="rounded-[3px] border border-white/10 bg-black/60 px-2 py-1 font-mono text-[10px] tracking-[0.18em] text-white/80 uppercase backdrop-blur"
            >
                "// PREVIEW"
            </span>
        </div>
        <div
            class="pointer-events-none absolute inset-x-0 bottom-0 flex items-center justify-between gap-2 px-4 pb-3"
            aria-hidden="true"
        >
            <span
                class="rounded-[3px] border border-white/10 bg-black/60 px-2 py-1 font-mono text-[10px] tracking-[0.14em] text-white/70 tabular-nums backdrop-blur"
            >
                (bitrate)
            </span>
            <span
                class="rounded-[3px] border border-white/10 bg-black/60 px-2 py-1 font-mono text-[10px] tracking-[0.14em] text-white/70 tabular-nums backdrop-blur"
            >
                "16:9"
            </span>
        </div>
        <div
            hidden=(ready)
            class="absolute inset-0 flex flex-col items-center justify-center gap-2 text-center"
        >
            <span
                class="font-mono text-[11px] tracking-[0.24em] text-white/50 uppercase"
            >
                (message)
            </span>
            <span
                class="font-mono text-[10px] tracking-[0.18em] text-white/25 uppercase"
            >
                "awaiting ingest // rtmp"
            </span>
        </div>
    })
}
