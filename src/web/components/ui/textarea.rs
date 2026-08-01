use topcoat::{
    view::{class, component, view, Attributes, View},
    Result,
};

const TEXTAREA: &str = "min-h-20 w-full resize-y rounded-lg border border-border bg-background \
    px-3 py-2 text-sm shadow-xs transition-colors outline-none \
    placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring \
    focus-visible:ring-offset-2 focus-visible:ring-offset-background \
    disabled:pointer-events-none disabled:opacity-50";

/// A multiline text control matching the shared input component.
#[component]
pub async fn textarea(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! {
        <textarea class=(class!(TEXTAREA, attrs.remove("class"))) (attrs)>(child)</textarea>
    }
}
