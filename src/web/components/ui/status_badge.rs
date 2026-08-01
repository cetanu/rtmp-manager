use topcoat::{
    view::{class, component, view, Attributes, View},
    Result,
};

const STATUS_BADGE: &str = "stream-status-badge";

/// A compact state label whose color is selected by its `data-state` attribute.
#[component]
pub async fn status_badge(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! { <span class=(class!(STATUS_BADGE, attrs.remove("class"))) (attrs)>(child)</span> }
}
