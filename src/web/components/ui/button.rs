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
    /// The Tailwind classes for this variant.
    ///
    /// Hover and press states apply the fill or foreground color at reduced
    /// opacity, so they hold up in both color schemes without `dark:`
    /// overrides. Every variant with a resting fill or border casts the
    /// theme's control shadow; `Ghost` is flat until hovered, so it casts
    /// none.
    ///
    /// Each variant sets its own border color rather than inheriting a
    /// transparent one from [`BASE`]: with two border-color classes on the
    /// same element, stylesheet order (not class order) would decide the
    /// winner.
    fn classes(self) -> StaticClass {
        match self {
            Self::Primary => class!(
                "border-transparent bg-primary text-primary-foreground shadow-xs \
                 hover:bg-primary/90 active:bg-primary/80",
            ),
            Self::Secondary => class!(
                "border-transparent bg-foreground/5 text-foreground shadow-xs \
                 hover:bg-foreground/10 active:bg-foreground/15",
            ),
            Self::Outline => class!(
                "border-border text-foreground shadow-xs hover:bg-foreground/5 \
                 active:bg-foreground/10",
            ),
            Self::Ghost => class!(
                "border-transparent text-foreground hover:bg-foreground/5 active:bg-foreground/10",
            ),
            Self::Destructive => class!(
                "border-transparent bg-destructive text-destructive-foreground shadow-xs \
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
    /// The Tailwind classes for this size.
    ///
    /// The height and padding are calibrated so the button sits on the 4px
    /// grid while centering its label baseline against sibling form controls.
    /// `Icon` uses equal width and height to remain square regardless of its
    /// child's natural aspect ratio.
    fn classes(self) -> StaticClass {
        match self {
            Self::Sm => class!("h-8 rounded-md px-3 text-xs gap-1.5"),
            Self::Md => class!("h-9 rounded-lg px-4 text-sm gap-2"),
            Self::Lg => class!("h-10 rounded-lg px-5 text-sm gap-2"),
            Self::Icon => class!("size-9 rounded-lg"),
        }
    }
}

/// The base classes applied to every button, before variant and size styles.
///
/// Flex layout centers child elements (such as an icon and label) and aligns
/// them along the row. Transitions animate fill and border colors on hover
/// and active states. The focus ring is styled through `focus-visible:` so
/// pointer clicks avoid an outline, while keyboard navigation produces the
/// theme's two-color focus ring.
const BASE: StaticClass = class!(
    "inline-flex shrink-0 items-center justify-center border \
     font-medium whitespace-nowrap transition-colors outline-none select-none \
     focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
     focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50",
);

/// Builds the full class list for a button of the given `variant` and `size`.
///
/// Use it to give button styling to an element that is not a `<button>`, such
/// as a link styled as a button:
///
/// ```ignore
/// view! {
///     <a href="/login" class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>
///         "Sign in"
///     </a>
/// }
/// ```
#[must_use]
pub fn button_variants(
    variant: ButtonVariant,
    size: ButtonSize,
) -> Class<(StaticClass, StaticClass, StaticClass)> {
    class!(BASE, variant.classes(), size.classes())
}

/// A button component.
///
/// The `variant` and `size` parameters select the styling, defaulting to
/// `Primary` and `Md`. The `attrs` (such as `class`, `type`, `disabled`, or
/// event handlers) are forwarded to the underlying `<button>`; a `class` among
/// them is appended to the computed classes. Child nodes become the button's
/// content.
///
/// ```ignore
/// view! {
///     button(
///         variant: ButtonVariant::Destructive,
///         attrs: attributes! { type="submit" },
///         "Delete"
///     )
/// }
/// ```
///
/// To style a non-`<button>` element like a button, use [`button_variants`]
/// directly.
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
