use crate::web::components::ui::button::{ButtonVariant, button};
use crate::web::components::ui::card::{card, card_content, card_footer};
use crate::web::components::ui::confirm_delete::confirm_delete;
use crate::web::components::ui::form::{form_field, secret_input};
use crate::web::components::ui::input::input;
use rtmp_core::config::TargetConfig;
use rtmp_core::server::state::AppHandle;
use topcoat::{
    Result,
    context::{Cx, context},
    runtime::{procedure, signal},
    view::{View, attributes, component, element_id, view},
};

#[procedure]
async fn set_target_enabled(cx: &Cx, index: usize, name: String, enabled: bool) -> Result<String> {
    let app: &AppHandle = context(cx);
    match app
        .config
        .set_target_enabled(index, Some(&name), enabled)
        .await
    {
        Ok(_) => Ok(String::new()),
        Err(error) => {
            tracing::error!("Failed to update target enabled state: {error:#}");
            Ok(error.to_string())
        }
    }
}

#[component]
pub async fn targets(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = context(cx);
    let targets = app.config.get().targets.clone();

    Ok(view! {
        card(
            card_content(
                attrs: attributes! { class="pt-4" },
                if !targets.is_empty() {
                    <div id="targetsContainer" class="grid gap-3 sm:grid-cols-2">
                        #[key(index)]
                        for (index, target) in targets.into_iter().enumerate() {
                            target_item(index: index, target: target)
                        }
                    </div>
                } else {
                    <p class="text-sm text-muted-foreground">
                        "No targets configured."
                    </p>
                }
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
    })
}

#[component]
pub async fn target_item(cx: &Cx, index: usize, target: TargetConfig) -> Result<impl View> {
    let name_id = element_id(cx);
    let public_url_id = element_id(cx);
    let url_id = element_id(cx);
    let key_id = element_id(cx);
    let enabled = signal(cx, || target.enabled);
    let pending = signal(cx, || false);
    let target_name = target.name.clone();

    Ok(view! {
        <div
            class="min-w-0 rounded-sm border border-border bg-background/40 p-3 transition-colors hover:border-signal/50 hover:bg-signal/5 focus-within:border-signal/50 focus-within:bg-signal/5"
        >
            <input
                type="hidden"
                name=(format!("targets[{index}][original_index]"))
                value=(index.to_string())
            />
            <div class="grid gap-3 lg:grid-cols-2">
                form_field(
                    control_id: name_id.clone(),
                    label_text: "Target Name",
                    input(
                        attrs: attributes! {
                            type="text"
                            id=(name_id)
                            name=(format!("targets[{index}][name]"))
                            value=(target.name.clone())
                            placeholder="e.g. Twitch, YouTube"
                            required="required"
                        }
                    )
                )
                form_field(
                    control_id: public_url_id.clone(),
                    label_text: "Public URL (Optional)",
                    input(
                        attrs: attributes! {
                            type="url"
                            id=(public_url_id)
                            name=(format!("targets[{index}][public_url]"))
                            value=(target.public_url.clone().unwrap_or_default())
                            placeholder="https://twitch.tv/mychannel"
                        }
                    )
                )

                form_field(
                    control_id: url_id.clone(),
                    label_text: "RTMP URL Base",
                    input(
                        attrs: attributes! {
                            type="url"
                            id=(url_id)
                            name=(format!("targets[{index}][url]"))
                            value=(target.url.clone())
                            placeholder="rtmp://live.twitch.tv/app"
                            required="required"
                        }
                    )
                )
                form_field(
                    control_id: key_id.clone(),
                    label_text: "Stream Key",
                    secret_input(
                        control_id: key_id,
                        attrs: attributes! {
                            name=(format!("targets[{index}][stream_key]"))
                            value=""
                            autocomplete="new-password"
                            placeholder=(if target.stream_key.is_empty() {
                                "Optional when already included in the RTMP URL".to_string()
                            } else {
                                "••••••••••••••••••••••••".to_string()
                            })
                        }
                    )
                )
            </div>
            <div class="mt-3 flex flex-wrap items-center justify-between gap-2 border-t border-border pt-3">
                <div class="flex items-center">
                    <input
                        type="checkbox"
                        class="sr-only"
                        tabindex="-1"
                        aria-hidden="true"
                        name=(format!("targets[{index}][enabled]"))
                        value="true"
                        checked=(target.enabled)
                        :checked=$(enabled.get())
                    />
                    <button
                        type="button"
                        role="switch"
                        aria-label=(format!("Toggle Enabled for {}", target.name))
                        class="group inline-flex h-8 cursor-pointer items-center gap-2 rounded-sm border border-border bg-white/5 px-3 font-mono text-[11px] font-semibold tracking-[0.12em] text-muted-foreground uppercase shadow-xs transition-colors outline-none hover:border-signal/50 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50 data-[checked=true]:border-signal/50 data-[checked=true]:bg-signal/10 data-[checked=true]:text-signal"
                        :aria-checked=$(if enabled.get() { "true" } else { "false" })
                        :data-checked=$(enabled.get())
                        :disabled=$(pending.get())
                        @click=$(async move |_event| {
                            let next_enabled = !enabled.get();
                            enabled.set(next_enabled);
                            pending.set(true);
                            let next_error = set_target_enabled(
                                index,
                                target_name.clone(),
                                next_enabled,
                            ).await;
                            if !next_error.is_empty() {
                                enabled.set(!next_enabled);
                            }
                            pending.set(false);
                        })
                    >
                        <span
                            class="hud-dot bg-white/30 text-white/30 group-data-[checked=true]:bg-signal group-data-[checked=true]:text-signal"
                        ></span>
                        <span>"Enabled"</span>
                    </button>
                </div>
                confirm_delete(
                    attrs: attributes! {
                        type="submit"
                        name="action"
                        value=(format!("remove_target:{index}"))
                        formaction="/api/config"
                        formnovalidate="formnovalidate"
                    }
                )
            </div>
        </div>
    })
}
