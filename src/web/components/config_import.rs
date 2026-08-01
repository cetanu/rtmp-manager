use crate::web::components::ui::button::{button, ButtonVariant};
use crate::web::components::ui::form::form_field;
use crate::web::components::ui::input::input;
use topcoat::{
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn config_import_form() -> Result {
    view! {
        <form
            method="post"
            action="/api/config/import-file"
            enctype="multipart/form-data"
            class="flex flex-col gap-4 rounded-xl border p-4"
        >
            form_field(
                control_id: "config_json_file",
                label_text: "JSON configuration file",
                input(attrs: attributes! {
                    id="config_json_file"
                    name="config_file"
                    type="file"
                    accept="application/json,.json"
                    required="required"
                })
            )
            <p class="text-sm text-destructive">"Importing replaces the saved configuration immediately."</p>
            button(
                variant: ButtonVariant::Primary,
                attrs: attributes! { type="submit" class="self-start" },
                "Import and replace configuration"
            )
        </form>
    }
}
