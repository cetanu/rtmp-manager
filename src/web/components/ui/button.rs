use topcoat::{
    Result,
    view::{Attributes, Child, Class, StaticClass, View, class, component, view},
};

/// The visual style of a [`button`].
///
/// [`Default`] is `ButtonVariant::Primary`, used when no variant is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ButtonVariant {
    /// The primary-filled button for the main action.
    #[default]
    Primary,
    /// A muted, tinted fill for secondary actions.
    Secondary,
    /// A hairline-bordered button on the page background.
    Outline,
    /// No fill until hovered, for toolbars and inline actions.
    Ghost,
    /// A destructive-filled button for actions such as deleting data.
    Destructive,
}

impl ButtonVariant {
    fn classes(self) -> StaticClass {
        match self {
            Self::Primary => class!(
                "border-signal/40 bg-primary text-primary-foreground shadow-xs \
                 hover:bg-signal/90 active:bg-signal/80",
            ),
            Self::Secondary => class!(
                "border-white/10 bg-white/5 text-foreground shadow-xs \
                 hover:bg-white/10 active:bg-white/15",
            ),
            Self::Outline => class!(
                "border-border bg-transparent text-foreground shadow-xs \
                 hover:border-signal/50 hover:text-signal active:bg-white/5",
            ),
            Self::Ghost => class!(
                "border-transparent text-muted-foreground hover:bg-white/5 hover:text-foreground active:bg-white/10",
            ),
            Self::Destructive => class!(
                "border-live/40 bg-destructive text-destructive-foreground shadow-xs \
                 hover:bg-destructive/90 active:bg-destructive/80",
            ),
        }
    }
}

/// The size of a [`button`].
///
/// [`Default`] is `ButtonSize::Md`, used when no size is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ButtonSize {
    /// A compact button.
    Sm,
    /// The default size.
    #[default]
    Md,
    /// A prominent button.
    Lg,
    /// A square button sized to fit an icon, matching [`ButtonSize::Md`].
    Icon,
}

impl ButtonSize {
    fn classes(self) -> StaticClass {
        match self {
            Self::Sm => class!("h-7 rounded-sm px-2.5 text-[11px] gap-1.5"),
            Self::Md => class!("h-8 rounded-sm px-3 text-[11px] gap-2"),
            Self::Lg => class!("h-9 rounded-sm px-4 text-[12px] gap-2"),
            Self::Icon => class!("size-8 rounded-sm"),
        }
    }
}

const BASE: StaticClass = class!(
    "inline-flex shrink-0 cursor-pointer items-center justify-center border \
     font-mono font-semibold tracking-[0.12em] uppercase whitespace-nowrap transition-colors outline-none select-none \
     focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
     focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50",
);

/// Builds the full class list for a button variant and size.
#[must_use]
pub fn button_variants(
    variant: ButtonVariant,
    size: ButtonSize,
) -> Class<(StaticClass, StaticClass, StaticClass)> {
    class!(BASE, variant.classes(), size.classes())
}

/// A styled button with configurable variant, size, attributes, and content.
#[component]
pub async fn button(
    #[default] variant: ButtonVariant,
    #[default] size: ButtonSize,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <button
            class=(class!(
                BASE,
                variant.classes(),
                size.classes(),
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </button>
    })
}

/// A link rendered with the same variants and sizing as [`button`].
#[component]
pub async fn button_link(
    #[default] variant: ButtonVariant,
    #[default] size: ButtonSize,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <a
            class=(class!(
                BASE,
                variant.classes(),
                size.classes(),
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </a>
    })
}
