use crate::web::components::config_export::exported_config;
use crate::web::components::config_import::config_import_form;
use crate::web::components::ui::button::{button_link, button_variants, ButtonSize, ButtonVariant};
use crate::web::components::ui::card::{
    card, card_content, card_description, card_header, card_title,
};
use topcoat::{
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn config_transfer() -> Result {
    let outline_button = button_variants(ButtonVariant::Outline, ButtonSize::Md);
    view! {
        signal export_open = false;
        signal import_open = false;

        card(
            attrs: attributes! { class="mt-6" },
            card_header(
                card_title("Configuration JSON")
                card_description("Export or replace the complete saved configuration, including secrets.")
            )
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
                        attrs: attributes! {
                            href="/api/config"
                            target="_blank"
                            rel="noopener"
                        },
                        "Open raw JSON"
                    )
                </div>

                exported_config(open: $(export_open.get()))

                <div class="mt-5" :hidden=$(!import_open.get())>
                    config_import_form()
                </div>
            )
        )
    }
}
