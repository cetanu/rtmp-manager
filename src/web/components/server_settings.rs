use crate::server::state::ProxyState;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::form::form_field;
use crate::web::components::ui::input::input;
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
            attrs: attributes! { class="h-full" },
            card_header(
                card_title("Server Settings")
            )
            card_content(
                <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                    form_field(
                        control_id: "server_listen",
                        label_text: "RTMP Listen Address",
                        input(attrs: attributes! {
                            type="text"
                            id="server_listen"
                            name="server[listen]"
                            value=(config.server.listen.to_string())
                            placeholder="0.0.0.0:1935"
                        })
                    )
                    form_field(
                        control_id: "health_listen",
                        label_text: "Health Listen Address",
                        input(attrs: attributes! {
                            type="text"
                            id="health_listen"
                            name="server[health_listen]"
                            value=(config.server.health_listen.to_string())
                            placeholder="127.0.0.1:8080"
                        })
                    )
                    form_field(
                        control_id: "api_listen",
                        label_text: "Web UI Listen Address",
                        input(attrs: attributes! {
                            type="text"
                            id="api_listen"
                            name="server[api_listen]"
                            value=(config.server.api_listen.to_string())
                            placeholder="0.0.0.0:3000"
                        })
                    )
                    form_field(
                        control_id: "test_stream_duration_secs",
                        label_text: "Test Stream Duration (seconds)",
                        input(attrs: attributes! {
                            type="number"
                            id="test_stream_duration_secs"
                            name="server[test_stream_duration_secs]"
                            value=(config.server.test_stream_duration_secs.to_string())
                            min="1"
                            max="86400"
                            step="1"
                        })
                    )
                </div>
            )
        )
    }
}
