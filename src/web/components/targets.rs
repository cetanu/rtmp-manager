use crate::config::TargetConfig;
use crate::server::state::ProxyState;
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    view::{attributes, component, view},
    Result,
};

use crate::web::components::ui::button::{button, ButtonSize, ButtonVariant};
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::input::input;
use crate::web::components::ui::label::label;
use crate::web::components::ui::switch::switch;

#[component]
pub async fn target_item(index: usize, target: &TargetConfig) -> Result {
    let switch_id = format!("target-enabled-{}", index);

    view! {
        <div class="flex flex-col md:flex-row gap-6 p-4 mb-4 border rounded-xl bg-surface items-start md:items-center transition-all hover:border-primary">
            <div class="flex-1 flex flex-col gap-4 w-full">
                <div class="flex flex-col md:flex-row gap-4">
                    <div class="flex-1">
                        label(attrs: attributes! { class="mb-2" }, "Target Name")
                        input(attrs: attributes! {
                            type="text"
                            name=(format!("targets[{}][name]", index))
                            value=(target.name.clone())
                            placeholder="e.g. Twitch, YouTube"
                            required="required"
                        })
                    </div>
                    <div class="flex-[2]">
                        label(attrs: attributes! { class="mb-2" }, "RTMP URL Base")
                        input(attrs: attributes! {
                            type="url"
                            name=(format!("targets[{}][url]", index))
                            value=(target.url.clone())
                            placeholder="rtmp://live.twitch.tv/app"
                            required="required"
                        })
                    </div>
                </div>
                <div class="flex flex-col md:flex-row gap-4">
                    <div class="flex-1">
                        label(attrs: attributes! { class="mb-2" }, "Stream Key")
                        input(attrs: attributes! {
                            type="password"
                            name=(format!("targets[{}][stream_key]", index))
                            value=""
                            placeholder=(if target.stream_key.is_empty() { "Optional when already included in the RTMP URL" } else { "Configured — leave blank to keep it" })
                        })
                    </div>
                    <div class="flex-1">
                        label(attrs: attributes! { class="mb-2" }, "Public Stream URL (Optional)")
                        input(attrs: attributes! {
                            type="url"
                            name=(format!("targets[{}][public_url]", index))
                            value=(target.public_url.clone().unwrap_or_default())
                            placeholder="https://twitch.tv/mychannel"
                        })
                    </div>
                </div>
            </div>

            <div class="flex md:flex-col items-center gap-4">
                <div class="flex items-center gap-2">
                    label(attrs: attributes! { for=(switch_id.clone()) class="mb-0 cursor-pointer" }, "Enabled")
                    switch(attrs: attributes! {
                        id=(switch_id)
                        name=(format!("targets[{}][enabled]", index))
                        value="true"
                        checked=(target.enabled)
                    })
                </div>
                button(
                    variant: ButtonVariant::Destructive,
                    size: ButtonSize::Icon,
                    attrs: attributes! {
                        type="submit"
                        name="action"
                        value=(format!("remove_target:{}", index))
                        title="Remove Target"
                        formaction="/api/config"
                        formnovalidate="formnovalidate"
                    },
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 6h18M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M10 11v6M14 11v6"/></svg>
                )
            </div>
        </div>
    }
}

#[component]
pub async fn targets(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let config = state.config.read().await;
    let targets = &config.targets;

    view! {
        card(
            attrs: attributes! { class="mb-8" },
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
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 5v14M5 12h14"/></svg>
                    "Add Target"
                )
            )
            card_content(
                <div id="targetsContainer">
                    for (index, target) in targets.iter().enumerate() {
                        target_item(index: index, target: target)
                    }
                </div>

                <div id="emptyTargets" class=(if targets.is_empty() { "text-center p-8 border border-dashed rounded-xl text-muted-foreground" } else { "hidden text-center p-8 border border-dashed rounded-xl text-muted-foreground" })>
                    "No targets configured. Stream will be ingested locally without forwarding."
                </div>
            )
        )
    }
}
