use crate::server::state::ProxyState;
use crate::web::components::target_item::target_item;
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    view::{attributes, component, view},
    Result,
};

use crate::web::components::ui::button::{button, ButtonVariant};
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::empty_state::empty_state;
use crate::web::components::ui::icon::plus_icon;

#[component]
pub async fn targets(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let config = state.config.read().await;
    let targets = &config.targets;

    view! {
        card(
            card_header(
                attrs: attributes! { class="flex flex-row justify-between items-center" },
                card_title("RTMP Targets")
                button(
                    variant: ButtonVariant::Secondary,
                    attrs: attributes! {
                        type="submit"
                        name="action"
                        value="add_target"
                        formaction="/api/config"
                    },
                    plus_icon()
                    "Add Target"
                )
            )
            card_content(
                <div id="targetsContainer">
                    for (index, target) in targets.iter().enumerate() {
                        target_item(index: index, target: target)
                    }
                </div>

                empty_state(attrs: attributes! { id="emptyTargets" hidden=(!targets.is_empty()) },
                    "No targets configured. Stream will be ingested locally without forwarding."
                )
            )
        )
    }
}
