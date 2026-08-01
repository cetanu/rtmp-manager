use crate::config::TargetConfig;
use crate::web::components::ui::button::{button, ButtonSize, ButtonVariant};
use crate::web::components::ui::form::{form_field, switch_field};
use crate::web::components::ui::icon::trash_icon;
use crate::web::components::ui::input::input;
use topcoat::{
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn target_item(index: usize, target: &TargetConfig) -> Result {
    let enabled_id = format!("target_enabled_{index}");
    let name_id = format!("target_name_{index}");
    let url_id = format!("target_url_{index}");
    let key_id = format!("target_stream_key_{index}");
    let public_url_id = format!("target_public_url_{index}");

    view! {
        <div class="mb-4 flex flex-col items-start gap-6 rounded-xl border bg-surface p-4 transition-all hover:border-primary md:flex-row md:items-center">
            <div class="flex w-full flex-1 flex-col gap-4">
                <div class="grid gap-4 md:grid-cols-3">
                    form_field(
                        control_id: name_id.clone(),
                        label_text: "Target Name",
                        input(attrs: attributes! {
                            type="text"
                            id=(name_id)
                            name=(format!("targets[{index}][name]"))
                            value=(target.name.clone())
                            placeholder="e.g. Twitch, YouTube"
                            required="required"
                        })
                    )
                    form_field(
                        control_id: url_id.clone(),
                        label_text: "RTMP URL Base",
                        attrs: attributes! { class="md:col-span-2" },
                        input(attrs: attributes! {
                            type="url"
                            id=(url_id)
                            name=(format!("targets[{index}][url]"))
                            value=(target.url.clone())
                            placeholder="rtmp://live.twitch.tv/app"
                            required="required"
                        })
                    )
                </div>
                <div class="grid gap-4 md:grid-cols-2">
                    form_field(
                        control_id: key_id.clone(),
                        label_text: "Stream Key",
                        input(attrs: attributes! {
                            type="password"
                            id=(key_id)
                            name=(format!("targets[{index}][stream_key]"))
                            value=""
                            placeholder=(if target.stream_key.is_empty() { "Optional when already included in the RTMP URL" } else { "Configured — leave blank to keep it" })
                        })
                    )
                    form_field(
                        control_id: public_url_id.clone(),
                        label_text: "Public Stream URL (Optional)",
                        input(attrs: attributes! {
                            type="url"
                            id=(public_url_id)
                            name=(format!("targets[{index}][public_url]"))
                            value=(target.public_url.clone().unwrap_or_default())
                            placeholder="https://twitch.tv/mychannel"
                        })
                    )
                </div>
            </div>

            <div class="flex items-center gap-4 md:flex-col">
                switch_field(
                    control_id: enabled_id,
                    name: format!("targets[{index}][enabled]"),
                    label_text: "Enabled",
                    checked: target.enabled
                )
                button(
                    variant: ButtonVariant::Destructive,
                    size: ButtonSize::Icon,
                    attrs: attributes! {
                        type="submit"
                        name="action"
                        value=(format!("remove_target:{index}"))
                        title="Remove Target"
                        aria-label=(format!("Remove {} target", target.name))
                        formaction="/api/config"
                        formnovalidate="formnovalidate"
                    },
                    trash_icon()
                )
            </div>
        </div>
    }
}
