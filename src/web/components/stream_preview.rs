use crate::server::state::AppHandle;
use crate::web::components::publishing_controls::publishing_controls;
use crate::web::components::ui::card::{card, card_content, card_header};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{Event, shard},
    view::{View, component, view},
};

#[component]
pub async fn stream_preview() -> Result {
    view! {
        signal status_revision = 0.0;

        card(
            card_header(
                publishing_controls(revision: $(status_revision.get()))
            )
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
    }
}

#[component]
pub async fn stream_preview_player(child: View) -> Result {
    view! {
        <div class="relative mx-auto h-[calc(100dvh-10rem)] max-h-[56.25vw] max-w-full aspect-video overflow-hidden bg-black">
            <video
                id="stream-preview-video"
                class="h-full w-full bg-black object-contain"
                controls="controls"
                autoplay="autoplay"
                muted="muted"
                playsinline="playsinline"
            ></video>
            (child)
        </div>
    }
}

#[shard]
pub async fn stream_preview_placeholder(cx: &Cx, revision: f64) -> Result {
    let _ = revision;
    let app: &AppHandle = app_context(cx);
    let status = app
        .stream
        .status(&crate::web::auth::current_user(cx).tenant_id);
    let message = status.state.to_string();
    view! {
        <div
            class="absolute inset-0 flex items-center justify-center text-center text-sm text-white/70"
        >
            (message)
        </div>
    }
}
