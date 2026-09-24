use topcoat::{
    Result,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

const LABEL: StaticClass = class!(
    "flex items-center gap-2 font-mono text-[10px] leading-none font-medium tracking-[0.16em] \
     text-muted-foreground uppercase select-none \
     peer-disabled:pointer-events-none peer-disabled:opacity-50 \
     has-[:disabled]:pointer-events-none has-[:disabled]:opacity-50",
);

/// A styled caption for a form control.
#[component]
pub async fn label(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <label class=(class!(LABEL, attrs.remove("class"))) (attrs)>(child)</label>
    })
}
