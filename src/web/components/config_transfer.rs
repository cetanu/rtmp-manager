use crate::server::state::AppHandle;
use crate::web::components::ui::button::{
    ButtonSize, ButtonVariant, button, button_link, button_variants,
};
use crate::web::components::ui::card::{card, card_content};
use crate::web::components::ui::form::{field_description, form_field};
use crate::web::components::ui::input::input;
use crate::web::components::ui::textarea::textarea;
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{shard, signal},
    view::{View, attributes, component, view},
};

#[component]
pub async fn config_transfer(cx: &Cx) -> Result<impl View> {
    let outline_button = button_variants(ButtonVariant::Outline, ButtonSize::Md);
    let export_open = signal(cx, || false);
    let import_open = signal(cx, || false);

    Ok(view! {
        card(
            card_content(
                <div class="flex flex-wrap gap-3">
                    <button
                        type="button"
                        class=(outline_button.clone())
                        @click=$(|_event| export_open.set(!export_open.get()))
                    >
                        "Show config as JSON"
                    </button>
                    <button
                        type="button"
                        class=(outline_button)
                        @click=$(|_event| import_open.set(!import_open.get()))
                    >
                        "Import config from JSON"
                    </button>
                    button_link(
                        variant: ButtonVariant::Secondary,
                        attrs: attributes! { href="/api/config" target="_blank" rel="noopener" },
                        "Open raw JSON"
                    )
                </div>

                exported_config(open: $(export_open.get()))

                <div class="mt-5" :hidden=$(!import_open.get())>
                    config_import_form()
                </div>
            )
        )
    })
}

#[shard]
pub async fn exported_config(cx: &Cx, open: bool) -> Result<impl View> {
    let app: &AppHandle = app_context(cx);
    let config_json = if open {
        Some(serde_json::to_string_pretty(&*app.config.get())?)
    } else {
        None
    };

    Ok(view! {
        if let Some(config_json) = config_json {
            <div class="mt-5">
                form_field(
                    control_id: "exported_config_json",
                    label_text: "Saved configuration",
                    textarea(
                        attrs: attributes! {
                            id="exported_config_json"
                            readonly="readonly"
                            rows="18"
                            class="font-mono text-xs"
                        },
                        (config_json)
                    )
                    field_description(
                        "Select the text and use your browser's copy command."
                    )
                )
            </div>
        }
    })
}

#[component]
pub async fn config_import_form() -> Result<impl View> {
    Ok(view! {
        <form
            method="post"
            action="/api/config/import-file"
            enctype="multipart/form-data"
            class="flex flex-col gap-4 rounded-xl border p-4"
        >
            form_field(
                control_id: "config_json_file",
                label_text: "JSON configuration file",
                input(
                    attrs: attributes! {
                        id="config_json_file"
                        name="config_file"
                        type="file"
                        accept="application/json,.json"
                        required="required"
                    }
                )
            )
            <p class="text-sm text-destructive">
                "Importing replaces the saved configuration immediately."
            </p>
            button(
                variant: ButtonVariant::Primary,
                attrs: attributes! { type="submit" class="self-start" },
                "Import and replace configuration"
            )
        </form>
    })
}
