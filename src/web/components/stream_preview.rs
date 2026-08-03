use crate::web::components::publishing_controls::publishing_controls;
use crate::web::components::stream_preview_player::{
    stream_preview_placeholder, stream_preview_player,
};
use crate::web::components::stream_preview_status::stream_preview_status;
use crate::web::components::ui::card::{card, card_content, card_header};
use topcoat::{
    Result,
    runtime::Event,
    view::{attributes, component, view},
};

#[component]
pub async fn stream_preview() -> Result {
    view! {
        signal status_revision = 0.0;

        card(
            attrs: attributes! { class="mb-8" },
            card_header(stream_preview_status(revision: $(status_revision.get())))
            card_content(
                <div class="grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(16rem,1fr)]">
                    stream_preview_player(
                        stream_preview_placeholder(revision: $(status_revision.get()))
                    )
                    publishing_controls(revision: $(status_revision.get()))
                </div>
                <p id="stream-preview-error" hidden="hidden" class="mt-4 text-sm text-destructive"></p>
                <button
                    id="stream-status-refresh"
                    type="button"
                    hidden="hidden"
                    aria-hidden="true"
                    tabindex="-1"
                    @click=$(|_event: Event| status_revision.increment())
                ></button>
                <noscript>
                    <p class="mt-4 text-sm text-destructive">"JavaScript is required for live HLS preview playback."</p>
                </noscript>
            )
        )
    }
}
