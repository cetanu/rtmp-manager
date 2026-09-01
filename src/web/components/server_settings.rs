use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::form::{form_field, secret_input, switch_field};
use crate::web::components::ui::input::input;
use topcoat::{
    Result,
    context::Cx,
    view::{attributes, component, view},
};

#[component]
pub async fn server_settings(cx: &Cx) -> Result {
    let config = crate::web::request_config(cx).await?;
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
                        control_id: "srt_listen",
                        label_text: "SRT Listen Address",
                        input(attrs: attributes! {
                            type="text"
                            id="srt_listen"
                            name="server[srt_listen]"
                            value=(config.server.srt_listen.to_string())
                            placeholder="0.0.0.0:6000"
                        })
                    )
                    switch_field(
                        control_id: "srt_enabled",
                        name: "server[srt_enabled]",
                        label_text: "Enable SRT ingest",
                        checked: config.server.srt_enabled
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
                    form_field(
                        control_id: "ingest_stream_key",
                        label_text: "Ingest Stream Key",
                        secret_input(
                            control_id: "ingest_stream_key",
                            attrs: attributes! {
                                name="server[ingest_stream_key]"
                                value=(config.server.ingest_stream_key.clone())
                                placeholder="Required before streaming"
                                autocomplete="new-password"
                            }
                        )
                    )
                </div>
            )
        )
    }
}
