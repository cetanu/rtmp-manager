use crate::server::state::AppHandle;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::form::{field_description, form_field, secret_input};
use crate::web::components::ui::input::input;
use topcoat::{
    Result,
    context::{Cx, app_context},
    view::{attributes, component, view},
};

#[component]
pub async fn web_auth(cx: &Cx) -> Result {
    let app: &AppHandle = app_context(cx);
    let auth = app.config.get().web_auth.clone();
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
                        secret_input(
                            control_id: "web_auth_password",
                            attrs: attributes! {
                                name="web_auth[password]"
                                autocomplete="new-password"
                                value=(auth.password)
                                minlength="12"
                                placeholder="At least 12 characters"
                            }
                        )
                        field_description(
                            "Changing this takes effect immediately and prompts the browser to sign in again."
                        )
                    )
                </div>
            )
        )
    }
}
