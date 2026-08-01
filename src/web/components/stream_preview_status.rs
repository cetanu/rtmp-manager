use crate::web::components::ui::card::card_title;
use crate::web::components::ui::status_badge::status_badge;
use topcoat::{
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn stream_preview_status() -> Result {
    view! {
        <div class="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
            <div>
                card_title("Staged Stream")
                <p id="stream-stage-detail" class="mt-1 text-sm text-muted-foreground">
                    "Waiting for an RTMP stream. Nothing will be sent to external targets automatically."
                </p>
            </div>
            status_badge(attrs: attributes! { id="stream-stage-badge" data-state="offline" },
                "Offline"
            )
        </div>
    }
}
