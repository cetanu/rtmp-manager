use crate::web::components::publishing_controls::publishing_controls;
use crate::web::components::stream_preview_player::stream_preview_player;
use crate::web::components::stream_preview_status::stream_preview_status;
use crate::web::components::ui::card::{card, card_content, card_header};
use topcoat::{
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn stream_preview() -> Result {
    view! {
        card(
            attrs: attributes! { class="mb-8" },
            card_header(stream_preview_status())
            card_content(
                <div class="grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(16rem,1fr)]">
                    stream_preview_player()
                    publishing_controls()
                </div>
                <noscript>
                    <p class="mt-4 text-sm text-destructive">"JavaScript is required for the live HLS preview and publishing controls."</p>
                </noscript>
            )
        )
    }
}
