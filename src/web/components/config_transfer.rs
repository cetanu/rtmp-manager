use crate::server::state::ProxyState;
use crate::web::components::ui::button::{button_variants, ButtonSize, ButtonVariant};
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    runtime::shard,
    view::{attributes, component, view},
    Result,
};

#[shard]
async fn exported_config(cx: &Cx, open: bool) -> Result {
    if !open {
        return view! {};
    }

    let state: &Arc<ProxyState> = app_context(cx);
    let config_json = serde_json::to_string_pretty(&*state.config.read().await)?;
    view! {
        <div class="mt-5">
            <label for="exported_config_json" class="mb-2 block text-sm font-medium">
                "Saved configuration"
            </label>
            <textarea
                id="exported_config_json"
                readonly="readonly"
                rows="18"
                class="w-full rounded-lg border border-border bg-background px-3 py-2 font-mono text-xs shadow-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >(config_json)</textarea>
            <p class="mt-2 text-xs text-muted-foreground">"Select the text and use your browser's copy command."</p>
        </div>
    }
}

#[component]
pub async fn config_transfer() -> Result {
    let outline_button = button_variants(ButtonVariant::Outline, ButtonSize::Md);
    let primary_button = button_variants(ButtonVariant::Primary, ButtonSize::Md);

    view! {
        signal export_open = false;
        signal import_open = false;

        card(
            attrs: attributes! { class="mt-6" },
            card_header(
                card_title("Configuration JSON")
                <p class="text-sm text-muted-foreground">"Export or replace the complete saved configuration, including secrets."</p>
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
                    <a
                        href="/api/config"
                        target="_blank"
                        rel="noopener"
                        class=(button_variants(ButtonVariant::Secondary, ButtonSize::Md))
                    >
                        "Open raw JSON"
                    </a>
                </div>

                exported_config(open: $(export_open.get()))

                <div class="mt-5" :hidden=$(!import_open.get())>
                    <form
                        method="post"
                        action="/api/config/import-file"
                        enctype="multipart/form-data"
                        class="flex flex-col gap-4 rounded-xl border p-4"
                    >
                        <div>
                            <label for="config_json_file" class="mb-2 block text-sm font-medium">
                                "JSON configuration file"
                            </label>
                            <input
                                id="config_json_file"
                                name="config_file"
                                type="file"
                                accept="application/json,.json"
                                required="required"
                                class="block w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                            />
                        </div>
                        <p class="text-sm text-destructive">"Importing replaces the saved configuration immediately."</p>
                        <button type="submit" class=(format!("{} self-start", primary_button))>
                            "Import and replace configuration"
                        </button>
                    </form>
                </div>
            )
        )
    }
}
