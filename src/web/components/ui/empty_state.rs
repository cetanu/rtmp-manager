use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

const EMPTY_STATE: &str = "rounded-xl border border-dashed p-8 text-center text-muted-foreground";

/// A quiet placeholder for an empty collection or unavailable content.
#[component]
pub async fn empty_state(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! { <div class=(class!(EMPTY_STATE, attrs.remove("class"))) (attrs)>(child)</div> }
}
