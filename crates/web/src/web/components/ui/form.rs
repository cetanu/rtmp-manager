use crate::web::components::ui::input::input;
use crate::web::components::ui::label::label;
use crate::web::components::ui::switch::switch;
use topcoat::{
    Result,
    context::Cx,
    runtime::signal,
    view::{Attributes, Child, View, attributes, class, component, view},
};

/// A labeled form field with consistent vertical spacing.
#[component]
pub async fn form_field(
    cx: &Cx,
    #[into] control_id: String,
    #[into] label_text: String,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!("flex min-w-0 flex-col gap-2", attrs.remove("class")))
            (attrs)
        >
            label(attrs: attributes! { for=(control_id) }, (label_text))
            (child)
        </div>
    })
}

/// Supporting copy shown below a form control.
#[component]
pub async fn field_description(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <p
            class=(class!("text-xs text-muted-foreground", attrs.remove("class")))
            (attrs)
        >
            (child)
        </p>
    })
}

#[component]
pub async fn secret_input(
    cx: &Cx,
    #[into] control_id: String,
    #[default] attrs: Attributes,
) -> Result<impl View> {
    Ok(view! { input(attrs: attributes! { type="password" id=(control_id) (attrs) }) })
}

/// A password field whose configured value can be replaced or cleared.
/// The existing secret is never rendered into the DOM: blank preserves it.
#[component]
pub async fn clearable_secret_field(
    cx: &Cx,
    #[into] control_id: String,
    #[into] name: String,
    #[into] label_text: String,
    #[into] empty_placeholder: String,
    #[into] value: String,
    #[default] attrs: Attributes,
) -> Result<impl View> {
    let clear_name = name
        .strip_suffix(']')
        .and_then(|name| name.rsplit_once('['))
        .map(|(group, field)| format!("{group}[clear_{field}]"))
        .unwrap_or_else(|| format!("clear_{name}"));
    let configured = !value.is_empty();
    let cleared = signal(cx, || false);
    let placeholder = if configured {
        "••••••••••••••••••••••••".to_string()
    } else {
        empty_placeholder
    };
    Ok(view! {
        form_field(
            control_id: control_id.clone(),
            label_text: label_text,
            <div class="group relative" :data-cleared=$(cleared.get())>
                secret_input(
                    control_id: control_id,
                    attrs: attributes! {
                        (attrs)
                        name=(name)
                        value=""
                        autocomplete="new-password"
                        placeholder=(placeholder)
                        class="pr-9 group-data-[cleared=true]:placeholder:text-transparent"
                    }
                )
                <input
                    type="hidden"
                    name=(clear_name)
                    value="false"
                    :value=$(if cleared.get() { "true" } else { "false" })
                />
                <button
                    type="button"
                    class="absolute inset-y-0 right-0 flex w-8 items-center justify-center rounded-r-sm text-red-400/60 transition-colors hover:bg-red-400/10 hover:text-red-400 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-red-400/60 disabled:pointer-events-none disabled:opacity-30"
                    aria-label="Clear configured value"
                    title="Clear configured value"
                    disabled=(!configured)
                    :hidden=$(cleared.get())
                    @click=$(move |_event| cleared.set(true))
                >
                    <svg
                        aria-hidden="true"
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    >
                        <path d="m6 6 12 12M6 18 18 6" />
                    </svg>
                </button>
            </div>
        )
    })
}

/// A labeled switch aligned as a single setting row.
#[component]
pub async fn switch_field(
    cx: &Cx,
    #[into] control_id: String,
    #[into] name: String,
    #[into] label_text: String,
    checked: bool,
    #[default] mut attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <div class=(class!("flex items-center gap-3", attrs.remove("class"))) (attrs)>
            switch(
                attrs: attributes! {
                    id=(control_id.clone())
                    name=(name)
                    value="true"
                    checked=(checked)
                }
            )
            label(attrs: attributes! { for=(control_id) }, (label_text))
        </div>
    })
}
