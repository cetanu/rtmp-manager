use crate::server::state::ProxyState;
use crate::web::components::ui::form::{field_description, form_field};
use crate::web::components::ui::textarea::textarea;
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    runtime::shard,
    view::{attributes, view},
    Result,
};

#[shard]
pub async fn exported_config(cx: &Cx, open: bool) -> Result {
    if !open {
        return view! {};
    }

    let state: &Arc<ProxyState> = app_context(cx);
    let config_json = serde_json::to_string_pretty(&*state.config.read().await)?;
    view! {
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
                field_description("Select the text and use your browser's copy command.")
            )
        </div>
    }
}
