use crate::web::components::ui::input::input;
use crate::web::components::ui::label::label;
use crate::web::components::ui::switch::switch;
use topcoat::{
    view::{attributes, class, component, view, Attributes, View},
    Result,
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

/// A password field that preserves a configured secret unless explicitly cleared.
#[component]
pub async fn clearable_secret_field(
    #[into] control_id: String,
    #[into] name: String,
    #[into] clear_name: String,
    #[into] label_text: String,
    #[into] empty_placeholder: String,
    configured: bool,
) -> Result {
    let clear_id = format!("clear_{control_id}");
    view! {
        form_field(
            control_id: control_id.clone(),
            label_text: label_text,
            input(attrs: attributes! {
                type="password"
                id=(control_id)
                name=(name)
                value=""
                placeholder=(if configured { "Configured — leave blank to keep it" } else { &empty_placeholder })
            })
            <div class="flex items-center gap-2">
                switch(attrs: attributes! {
                    id=(clear_id.clone())
                    name=(clear_name)
                    value="true"
                })
                label(
                    attrs: attributes! { for=(clear_id) class="text-muted-foreground" },
                    "Clear configured value"
                )
            </div>
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
