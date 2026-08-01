use crate::web::components::ui::button::{button, ButtonVariant};
use topcoat::{
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn publishing_controls() -> Result {
    view! {
        <div class="flex flex-col justify-between gap-6 rounded-xl border bg-surface p-5">
            <div>
                <h3 class="font-semibold">"Publishing controls"</h3>
                <p class="mt-2 text-sm leading-relaxed text-muted-foreground">
                    "Review the staged preview, then publish it to every enabled RTMP target. Going-live notifications are sent only when you publish."
                </p>
            </div>
            <div class="flex flex-col gap-3">
                button(
                    attrs: attributes! { id="publish-staged-stream" type="button" disabled="disabled" },
                    "Publish Live"
                )
                button(
                    variant: ButtonVariant::Destructive,
                    attrs: attributes! { id="stop-publishing-stream" type="button" disabled="disabled" },
                    "Stop Publishing"
                )
                <p id="stream-action-error" class="hidden text-sm text-destructive"></p>
            </div>
        </div>
    }
}
