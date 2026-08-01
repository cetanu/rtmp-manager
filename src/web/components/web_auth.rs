use crate::server::state::ProxyState;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::form::{field_description, form_field};
use crate::web::components::ui::input::input;
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn web_auth(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let auth = state.config.read().await.web_auth.clone();
    view! {
        card(
            attrs: attributes! { class="h-full" },
            card_header(
                card_title("Web Authentication")
            )
            card_content(
                <div class="grid gap-6 md:grid-cols-2">
                    form_field(
                        control_id: "web_auth_username",
                        label_text: "Username",
                        input(attrs: attributes! {
                            id="web_auth_username"
                            name="web_auth[username]"
                            autocomplete="username"
                            value=(auth.username)
                            required="true"
                        })
                    )
                    form_field(
                        control_id: "web_auth_password",
                        label_text: "New password",
                        input(attrs: attributes! {
                            type="password"
                            id="web_auth_password"
                            name="web_auth[password]"
                            autocomplete="new-password"
                            value=""
                            minlength="12"
                            placeholder=(if auth.password.is_empty() { "At least 12 characters" } else { "Configured — leave blank to keep it" })
                        })
                        field_description(
                            "Changing this takes effect immediately and prompts the browser to sign in again."
                        )
                    )
                </div>
            )
        )
    }
}
