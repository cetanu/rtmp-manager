use crate::server::state::ProxyState;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::input::input;
use crate::web::components::ui::label::label;
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
            attrs: attributes! { class="mb-8" },
            card_header(
                card_title("Web Authentication")
            )
            card_content(
                <div class="grid gap-6 md:grid-cols-2">
                    <div class="flex flex-col gap-2">
                        label(attrs: attributes! { for="web_auth_username" }, "Username")
                        input(attrs: attributes! {
                            id="web_auth_username"
                            name="web_auth[username]"
                            autocomplete="username"
                            value=(auth.username)
                            required="true"
                        })
                    </div>
                    <div class="flex flex-col gap-2">
                        label(attrs: attributes! { for="web_auth_password" }, "New password")
                        input(attrs: attributes! {
                            type="password"
                            id="web_auth_password"
                            name="web_auth[password]"
                            autocomplete="new-password"
                            value=""
                            minlength="12"
                            placeholder=(if auth.password.is_empty() { "At least 12 characters" } else { "Configured — leave blank to keep it" })
                        })
                        <p class="text-xs text-muted-foreground">
                            "Changing this takes effect immediately and prompts the browser to sign in again."
                        </p>
                    </div>
                </div>
            )
        )
    }
}
