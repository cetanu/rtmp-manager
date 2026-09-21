use topcoat::{
    Result,
    view::{Attributes, StaticClass, View, class, component, view},
};

const SWITCH: StaticClass = class!(
    "peer h-4.5 w-8 shrink-0 appearance-none rounded-full \
     bg-foreground/20 shadow-xs transition-colors outline-none checked:bg-primary \
     focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
     focus-visible:ring-offset-background disabled:pointer-events-none",
);

const THUMB: StaticClass = class!(
    "pointer-events-none absolute top-1/2 left-0.5 size-3.5 -translate-y-1/2 \
     rounded-full bg-background shadow-xs transition-transform peer-checked:translate-x-3.5",
);

/// An accessible checkbox rendered as an on/off switch.
#[component]
pub async fn switch(#[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! {
        <span
            class=(class!(
                "relative inline-flex shrink-0 has-[:disabled]:opacity-50",
                attrs.remove("class"),
            ))
        >
            <input type="checkbox" role="switch" class=(SWITCH) (attrs)>
            <span class=(THUMB)></span>
        </span>
    })
}
