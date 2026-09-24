use topcoat::{
    Result,
    view::{Attributes, StaticClass, View, class, component, view},
};

const INPUT: StaticClass = class!(
    "h-8 w-full min-w-0 rounded-sm border border-border bg-black/50 px-2.5 \
     font-mono text-xs tabular-nums shadow-xs transition-colors outline-none \
     placeholder:text-muted-foreground placeholder:font-sans \
     file:mr-3 file:h-full file:border-0 file:bg-transparent file:text-xs file:font-medium \
     hover:border-white/15 focus-visible:border-signal/60 focus-visible:ring-1 \
     focus-visible:ring-signal/40 disabled:pointer-events-none disabled:opacity-50",
);

/// A styled text input that forwards its attributes and custom classes.
#[component]
pub async fn input(#[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! { <input class=(class!(INPUT, attrs.remove("class"))) (attrs)> })
}
