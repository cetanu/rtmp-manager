use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::form::{form_field, secret_input, switch_field};
use crate::web::components::ui::input::input;
use topcoat::{
    Result,
    context::Cx,
    view::{attributes, component, view},
};

#[component]
pub async fn overlay_settings(cx: &Cx) -> Result {
    let config = crate::web::request_config(cx).await?;
    let overlay = &config.overlay;
    let overlay_url = format!("/overlay/chat?key={}", overlay.key);
    view! {
        card(
            attrs: attributes! { class="h-full" },
            card_header(card_title("OBS Chat Overlay"))
            card_content(
                <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
                    form_field(
                        control_id: "overlay_key",
                        label_text: "Private Overlay Key",
                        secret_input(
                            control_id: "overlay_key",
                            attrs: attributes! {
                                name="overlay[key]"
                                value=(overlay.key.clone())
                                autocomplete="new-password"
                            }
                        )
                    )
                    form_field(
                        control_id: "overlay_url",
                        label_text: "OBS Browser Source URL",
                        input(attrs: attributes! {
                            id="overlay_url"
                            value=(overlay_url)
                            readonly="readonly"
                        })
                    )
                    form_field(
                        control_id: "overlay_theme",
                        label_text: "Theme",
                        <select id="overlay_theme" name="overlay[theme]" class="h-9 rounded-lg border border-border bg-background px-3 text-sm">
                            <option value="dark" selected=(overlay.theme == "dark")>"Dark"</option>
                            <option value="minimal" selected=(overlay.theme == "minimal")>"Minimal"</option>
                            <option value="comic" selected=(overlay.theme == "comic")>"Comic"</option>
                            <option value="transparent-box" selected=(overlay.theme == "transparent-box")>"Transparent Box"</option>
                        </select>
                    )
                    form_field(
                        control_id: "overlay_font_size",
                        label_text: "Font Size (px)",
                        input(attrs: attributes! {
                            type="number" id="overlay_font_size" name="overlay[font_size_px]"
                            min="12" max="72" value=(overlay.font_size_px.to_string())
                        })
                    )
                    form_field(
                        control_id: "overlay_opacity",
                        label_text: "Message Background Opacity (%)",
                        input(attrs: attributes! {
                            type="number" id="overlay_opacity" name="overlay[background_opacity_percent]"
                            min="0" max="100" value=(overlay.background_opacity_percent.to_string())
                        })
                    )
                    form_field(
                        control_id: "overlay_fade_duration",
                        label_text: "Message Fade Duration (seconds; 0 disables)",
                        input(attrs: attributes! {
                            type="number" id="overlay_fade_duration" name="overlay[fade_duration_secs]"
                            min="0" max="300" value=(overlay.fade_duration_secs.to_string())
                        })
                    )
                    <div class="flex flex-col gap-3 md:col-span-2">
                        switch_field(control_id: "overlay_show_badges", name: "overlay[show_badges]", label_text: "Show platform badges", checked: overlay.show_badges)
                        switch_field(control_id: "overlay_show_avatars", name: "overlay[show_avatars]", label_text: "Show avatars", checked: overlay.show_avatars)
                        switch_field(control_id: "overlay_show_emotes", name: "overlay[show_emotes]", label_text: "Render emoji and emotes", checked: overlay.show_emotes)
                    </div>
                </div>
            )
        )
    }
}
