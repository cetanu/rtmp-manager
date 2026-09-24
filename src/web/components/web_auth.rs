use crate::server::state::AppHandle;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::form::{field_description, form_field, secret_input};
use crate::web::components::ui::input::input;
use topcoat::{
    Result,
    context::{Cx, app_context},
    view::{View, attributes, component, view},
};

#[component]
pub async fn web_auth(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = app_context(cx);
    let auth = app.config.get().web_auth.clone();
    Ok(view! {
        card(
            attrs: attributes! { class="h-full" },
            card_header(card_title("Web Authentication"))
            card_content(
                <div class="grid gap-6 md:grid-cols-2">
                    form_field(
                        control_id: "web_auth_username",
                        label_text: "Username",
                        input(
                            attrs: attributes! {
                                id="web_auth_username"
                                name="web_auth[username]"
                                autocomplete="username"
                                value=(auth.username)
                                required="true"
                            }
                        )
                    )
                    form_field(
                        control_id: "web_auth_password",
                        label_text: "New password",
                        secret_input(
                            control_id: "web_auth_password",
                            attrs: attributes! {
                                name="web_auth[password]"
                                autocomplete="new-password"
                                value=""
                                minlength="12"
                                placeholder=(if auth.password.is_empty() {
                                    "At least 12 characters".to_string()
                                } else {
                                    "Configured — leave blank to keep".to_string()
                                })
                            }
                        )
                        field_description(
                            "Changing this takes effect immediately and prompts the browser to sign in again."
                        )
                    )
                    form_field(
                        control_id: "web_auth_overlay_token",
                        label_text: "OBS overlay access token",
                        secret_input(
                            control_id: "web_auth_overlay_token",
                            attrs: attributes! {
                                name="web_auth[overlay_token]"
                                autocomplete="new-password"
                                value=""
                                minlength="16"
                                placeholder=(if auth.overlay_token.is_empty() {
                                    "At least 16 characters".to_string()
                                } else {
                                    "Configured — leave blank to keep".to_string()
                                })
                            }
                        )
                        field_description(
                            "The OBS URL must include ?key=...; this token grants access only to the chat overlay and its chat SSE stream."
                        )
                    )
                </div>
            )
        )
    })
}
