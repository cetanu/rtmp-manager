use super::button::{ButtonSize, ButtonVariant, button_variants};
use topcoat::{
    Result,
    context::Cx,
    runtime::{Event, signal},
    view::{Attributes, View, class, component, view},
};

#[component]
pub async fn confirm_delete(cx: &Cx, #[default] attrs: Attributes) -> Result<impl View> {
    let confirming = signal(cx, || false);
    Ok(view! {
        <button
            class="group grid cursor-pointer overflow-hidden rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50"
            :data-confirming=$(confirming.get())
            @click=$(|event: Event| {
                if !confirming.get() {
                    event.prevent_default();
                    event.stop_propagation();
                    confirming.set(true);
                }
            })
            @blur=$(|_event: Event| confirming.set(false))
            @keydown=$(|event: Event| {
                if event.key == "Escape" {
                    confirming.set(false);
                }
            })
            aria-live="polite"
            (attrs)
        >
            <span
                class=(class!(
                    button_variants(ButtonVariant::Destructive, ButtonSize::Md),
                    "!bg-destructive/20 !text-red-300 pointer-events-none col-start-1 row-start-1 !transition-transform duration-200 ease-out group-data-[confirming=true]:-translate-x-full motion-reduce:!transition-none",
                ))
                :aria-hidden=$(confirming.get())
            >
                "Delete"
            </span>
            <span
                class=(class!(
                    button_variants(ButtonVariant::Destructive, ButtonSize::Md),
                    "pointer-events-none col-start-1 row-start-1 translate-x-full !transition-transform duration-200 ease-out group-data-[confirming=true]:translate-x-0 motion-reduce:!transition-none",
                ))
                :aria-hidden=$(!confirming.get())
            >
                "Are you sure?"
            </span>
        </button>
    })
}
