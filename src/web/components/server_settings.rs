use crate::server::state::ProxyState;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::input::input;
use crate::web::components::ui::label::label;
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn server_settings(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let config = state.config.read().await;
    view! {
        card(
            attrs: attributes! { class="mb-8" },
            card_header(
                card_title("Server Settings")
            )
            card_content(
                <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                    <div class="flex-1 flex flex-col gap-2">
                        label(attrs: attributes! { for="server_listen" }, "RTMP Listen Address")
                        input(attrs: attributes! {
                            type="text"
                            id="server_listen"
                            name="server[listen]"
                            value=(config.server.listen.to_string())
                            placeholder="0.0.0.0:1935"
                        })
                    </div>
                    <div class="flex-1 flex flex-col gap-2">
                        label(attrs: attributes! { for="health_listen" }, "Health Listen Address")
                        input(attrs: attributes! {
                            type="text"
                            id="health_listen"
                            name="server[health_listen]"
                            value=(config.server.health_listen.to_string())
                            placeholder="127.0.0.1:8080"
                        })
                    </div>
                    <div class="flex-1 flex flex-col gap-2">
                        label(attrs: attributes! { for="api_listen" }, "Web UI Listen Address")
                        input(attrs: attributes! {
                            type="text"
                            id="api_listen"
                            name="server[api_listen]"
                            value=(config.server.api_listen.to_string())
                            placeholder="0.0.0.0:3000"
                        })
                    </div>
                </div>
            )
        )
    }
}
