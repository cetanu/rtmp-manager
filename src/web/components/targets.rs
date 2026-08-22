use crate::server::state::ProxyState;
use crate::web::components::target_item::target_item;
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, app_context},
    view::{attributes, component, view},
};

use crate::web::components::ui::button::{ButtonVariant, button};
use crate::web::components::ui::card::{card, card_content, card_footer, card_header, card_title};
use crate::web::components::ui::empty_state::empty_state;

#[component]
pub async fn targets(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let config = state.config.read().await;
    let targets = &config.targets;

    view! {
        card(
            card_content(
                <div id="targetsContainer">
                    for (index, target) in targets.iter().enumerate() {
                        target_item(index: index, target: target)
                    }
                </div>

                empty_state(attrs: attributes! { id="emptyTargets" hidden=(!targets.is_empty()) },
                    "No targets configured."
                )
            )
            card_footer(
                button(
                    variant: ButtonVariant::Secondary,
                    attrs: attributes! {
                        type="submit"
                        name="action"
                        value="add_target"
                        formaction="/api/config"
                    },
                    "+ Add Target"
                )
            )
        )
    }
}
