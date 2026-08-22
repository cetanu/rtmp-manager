use crate::config::TargetConfig;
use crate::web::components::ui::button::{ButtonSize, ButtonVariant, button};
use crate::web::components::ui::form::{form_field, secret_input, switch_field};
use crate::web::components::ui::icon::trash_icon;
use crate::web::components::ui::input::input;
use topcoat::{
    Result,
    view::{attributes, component, view},
};

#[component]
pub async fn target_item(index: usize, target: &TargetConfig) -> Result {
    let enabled_id = format!("target_enabled_{index}");
    let name_id = format!("target_name_{index}");
    let url_id = format!("target_url_{index}");
    let key_id = format!("target_stream_key_{index}");
    let public_url_id = format!("target_public_url_{index}");

    view! {
        <div class="mb-4 flex flex-col items-start gap-6 border bg-surface p-6 transition-all hover:border-primary hover:bg-foreground/5 hover:shadow-sm md:flex-row md:items-center">
            <div class="flex w-full flex-1 flex-col gap-1">
                <div class="grid gap-1 md:grid-cols-2">
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
                        control_id: public_url_id.clone(),
                        label_text: "Public URL (Optional)",
                        input(attrs: attributes! {
                            type="url"
                            id=(public_url_id)
                            name=(format!("targets[{index}][public_url]"))
                            value=(target.public_url.clone().unwrap_or_default())
                            placeholder="https://twitch.tv/mychannel"
                        })
                    )
                </div>
                <div class="grid gap-1 md:grid-cols-2">
                    form_field(
                        control_id: url_id.clone(),
                        label_text: "RTMP URL Base",
                        input(attrs: attributes! {
                            type="url"
                            id=(url_id)
                            name=(format!("targets[{index}][url]"))
                            value=(target.url.clone())
                            placeholder="rtmp://live.twitch.tv/app"
                            required="required"
                        })
                    )
                    form_field(
                        control_id: key_id.clone(),
                        label_text: "Stream Key",
                        secret_input(
                            control_id: key_id,
                            attrs: attributes! {
                                name=(format!("targets[{index}][stream_key]"))
                                value=(target.stream_key.clone())
                                placeholder="Optional when already included in the RTMP URL"
                            }
                        )
                    )
                </div>
                <div class="mt-4 flex items-center justify-between gap-3">
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
                            class="!size-6 !rounded-md"
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
        </div>
    }
}
