use crate::web::components::configuration_form::settings_section;
use crate::web::components::ui::form::{clearable_secret_field, form_field, switch_field};
use crate::web::components::ui::input::input;
use rtmp_core::server::state::AppHandle;
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, attributes, component, view},
};

#[component]
pub async fn server_settings(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = context(cx);
    let config = app.config.get();
    Ok(view! {
        settings_section(
            title: "Stream ingest",
            form_field(
                control_id: "server_listen",
                label_text: "RTMP Listen Address",
                input(
                    attrs: attributes! {
                        type="text"
                        id="server_listen"
                        name="server[listen]"
                        value=(config.server.listen.to_string())
                        placeholder="0.0.0.0:1935"
                    }
                )
            )
            form_field(
                control_id: "server_srt_listen",
                label_text: "SRT Listen Address",
                input(
                    attrs: attributes! {
                        type="text"
                        id="server_srt_listen"
                        name="server[srt_listen]"
                        value=(config.server.srt_listen.to_string())
                        placeholder="0.0.0.0:6000"
                    }
                )
            )
            form_field(
                control_id: "api_listen",
                label_text: "Web UI Listen Address",
                input(
                    attrs: attributes! {
                        type="text"
                        id="api_listen"
                        name="server[api_listen]"
                        value=(config.server.api_listen.to_string())
                        placeholder="0.0.0.0:3000"
                    }
                )
            )
            form_field(
                control_id: "test_stream_duration_secs",
                label_text: "Test Stream Duration (seconds)",
                input(
                    attrs: attributes! {
                        type="number"
                        class="max-w-28"
                        id="test_stream_duration_secs"
                        name="server[test_stream_duration_secs]"
                        value=(config.server.test_stream_duration_secs.to_string())
                        min="1"
                        max="300"
                        step="1"
                    }
                )
            )
            clearable_secret_field(
                control_id: "ingest_stream_key",
                name: "server[ingest_stream_key]",
                label_text: "Ingest Stream Key",
                empty_placeholder: "Required before streaming",
                value: config.server.ingest_stream_key.clone()
            )
            <div class="flex items-center">
                switch_field(
                    control_id: "server_srt_enabled",
                    name: "server[srt_enabled]",
                    label_text: "Enable SRT Ingest",
                    checked: config.server.srt_enabled
                )
            </div>
        )
    })
}
