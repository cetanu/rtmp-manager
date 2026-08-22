use crate::web::components::publishing_controls::publishing_controls;
use crate::web::components::stream_preview_player::{
    stream_preview_placeholder, stream_preview_player,
};
use crate::web::components::ui::card::{card, card_content, card_header};
use topcoat::{
    Result,
    runtime::Event,
    view::{component, view},
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
