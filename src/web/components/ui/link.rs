use topcoat::{
    view::{class, component, view, Attributes, View},
    Result,
};

const TEXT_LINK: &str = "text-sm font-medium underline underline-offset-4 outline-none \
    hover:text-foreground/80 focus-visible:ring-2 focus-visible:ring-ring \
    focus-visible:ring-offset-2 focus-visible:ring-offset-background";

/// A conventional inline navigation link with a visible focus treatment.
#[component]
pub async fn text_link(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! { <a class=(class!(TEXT_LINK, attrs.remove("class"))) (attrs)>(child)</a> }
}
