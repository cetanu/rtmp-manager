use topcoat::{
    Result,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

const CARD: StaticClass = class!("hud-panel");

/// A bordered surface that stacks its child sections vertically.
#[component]
pub async fn card(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! { <div class=(class!(CARD, attrs.remove("class"))) (attrs)>(child)</div> })
}

/// The opening section of a [`card`], stacking a [`card_title`] and an
/// optional [`card_description`].
#[component]
pub async fn card_header(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!("flex flex-col gap-1 px-4 pt-3", attrs.remove("class")))
            (attrs)
        >
            (child)
        </div>
    })
}

/// The heading of a [`card`], rendered as an `<h3>`.
#[component]
pub async fn card_title(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <h3 class=(class!("font-mono text-[11px] font-semibold tracking-[0.16em] text-foreground uppercase", attrs.remove("class"))) (attrs)>
            (child)
        </h3>
    })
}

/// The supporting text under a [`card_title`].
#[component]
#[allow(dead_code)]
pub async fn card_description(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <p
            class=(class!("font-mono text-[11px] text-muted-foreground", attrs.remove("class")))
            (attrs)
        >
            (child)
        </p>
    })
}

/// The main body of a [`card`].
#[component]
pub async fn card_content(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! { <div class=(class!("px-4 pb-4", attrs.remove("class"))) (attrs)>(child)</div> })
}

/// The closing section of a [`card`], a horizontal row for actions.
#[component]
pub async fn card_footer(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!("flex items-center gap-2 border-t border-border px-4 pt-3 pb-3", attrs.remove("class")))
            (attrs)
        >
            (child)
        </div>
    })
}
