use crate::server::state::AppHandle;
use crate::web::components::publishing_controls::publishing_controls;
use crate::web::components::ui::card::{card, card_content, card_header};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{Event, shard, signal},
    view::{Child, View, component, view},
};

#[component]
pub async fn stream_preview(cx: &Cx) -> Result<impl View> {
    let status_revision = signal(cx, || 0.0);

    Ok(view! {
        card(
            card_header(publishing_controls(revision: $(status_revision.get())))
            card_content(
                stream_preview_player(
                    stream_preview_placeholder(revision: $(status_revision.get()))
                )
                <button
                    id="stream-status-refresh"
                    type="button"
                    hidden="hidden"
                    aria-hidden="true"
                    tabindex="-1"
                    @click=$(|_event: Event| status_revision.increment())
                ></button>
            )
        )
    })
}

#[component]
pub async fn stream_preview_player(#[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
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
            <div
                class="pointer-events-none absolute inset-x-0 top-0 flex items-center justify-between gap-2 px-4 pt-3"
                aria-hidden="true"
            >
                <span
                    class="flex items-center gap-1.5 rounded-[3px] border border-white/10 bg-black/60 px-2 py-1 font-mono text-[10px] tracking-[0.18em] text-white/80 uppercase backdrop-blur"
                >
                    <span
                        data-preview-rec-dot="true"
                        class="hud-dot bg-white/30 text-white/30"
                    ></span>
                    <span data-preview-rec-label="true">"STBY"</span>
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
                    data-preview-bitrate="true"
                    class="rounded-[3px] border border-white/10 bg-black/60 px-2 py-1 font-mono text-[10px] tracking-[0.14em] text-white/70 tabular-nums backdrop-blur"
                >
                    "-- Mbps"
                </span>
                <span
                    class="rounded-[3px] border border-white/10 bg-black/60 px-2 py-1 font-mono text-[10px] tracking-[0.14em] text-white/70 tabular-nums backdrop-blur"
                >
                    "16:9"
                </span>
            </div>
            (child)
        </div>
    })
}

#[shard]
pub async fn stream_preview_placeholder(cx: &Cx, revision: f64) -> Result<impl View> {
    let _ = revision;
    let app: &AppHandle = app_context(cx);
    let status = app.stream.status();
    let message = if status.state.to_string().is_empty() {
        "// NO SIGNAL".to_string()
    } else {
        format!("// {}", status.state.to_string().to_uppercase())
    };
    Ok(view! {
        <div
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
