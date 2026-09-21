use topcoat::{
    Result,
    view::{Attributes, StaticClass, View, class, component, view},
};

const INPUT: StaticClass = class!(
    "h-9 w-full min-w-0 rounded-lg border border-border bg-background px-3 \
     text-sm shadow-xs transition-colors outline-none \
     placeholder:text-muted-foreground \
     file:mr-3 file:h-full file:border-0 file:bg-transparent file:text-sm file:font-medium \
     focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
     focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50",
);

/// A styled text input that forwards its attributes and custom classes.
#[component]
pub async fn input(#[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! { <input class=(class!(INPUT, attrs.remove("class"))) (attrs)> })
}
