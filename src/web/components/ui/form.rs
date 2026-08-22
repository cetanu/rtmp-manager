use crate::web::components::ui::icon::eye_icon;
use crate::web::components::ui::input::input;
use crate::web::components::ui::label::label;
use crate::web::components::ui::switch::switch;
use topcoat::{
    Result,
    view::{Attributes, View, attributes, class, component, view},
};

/// A labeled form field with consistent vertical spacing.
#[component]
pub async fn form_field(
    #[into] control_id: String,
    #[into] label_text: String,
    #[default] mut attrs: Attributes,
    #[default] child: View,
) -> Result {
    view! {
        <div class=(class!("flex min-w-0 flex-col gap-2", attrs.remove("class"))) (attrs)>
            label(attrs: attributes! { for=(control_id) }, (label_text))
            (child)
        </div>
    }
}

/// Supporting copy shown below a form control.
#[component]
pub async fn field_description(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! {
        <p class=(class!("text-xs text-muted-foreground", attrs.remove("class"))) (attrs)>
            (child)
        </p>
    }
}

#[component]
pub async fn secret_input(#[into] control_id: String, #[default] mut attrs: Attributes) -> Result {
    view! {
        <div class="relative">
            input(attrs: attributes! {
                type="password"
                id=(control_id.clone())
                class=(class!("pr-11", attrs.remove("class")))
                (attrs)
            })
            <button
                type="button"
                data-secret-toggle=(control_id.clone())
                aria-controls=(control_id)
                aria-pressed="false"
                aria-label="Show value"
                title="Show value"
                class="absolute inset-y-0 right-0 flex w-11 items-center justify-center text-muted-foreground transition-colors hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >eye_icon()</button>
        </div>
    }
}

/// A password field whose configured value can be revealed, edited, or cleared.
#[component]
pub async fn clearable_secret_field(
    #[into] control_id: String,
    #[into] name: String,
    #[into] label_text: String,
    #[into] empty_placeholder: String,
    #[into] value: String,
) -> Result {
    view! {
        form_field(
            control_id: control_id.clone(),
            label_text: label_text,
            secret_input(
                control_id: control_id,
                attrs: attributes! {
                    name=(name)
                    value=(value)
                    placeholder=(empty_placeholder)
                }
            )
        )
    }
}

/// A labeled switch aligned as a single setting row.
#[component]
pub async fn switch_field(
    #[into] control_id: String,
    #[into] name: String,
    #[into] label_text: String,
    checked: bool,
    #[default] mut attrs: Attributes,
) -> Result {
    view! {
        <div class=(class!("flex items-center gap-3", attrs.remove("class"))) (attrs)>
            switch(attrs: attributes! {
                id=(control_id.clone())
                name=(name)
                value="true"
                checked=(checked)
            })
            label(attrs: attributes! { for=(control_id) }, (label_text))
        </div>
    }
}
