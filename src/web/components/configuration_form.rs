use crate::web::components::actions_panel::actions_panel;
use crate::web::components::chat_settings::chat_settings;
use crate::web::components::notifications::notifications;
use crate::web::components::overlay_settings::overlay_settings;
use crate::web::components::server_settings::server_settings;
use crate::web::components::targets::targets;
use topcoat::{
    Result,
    context::{Cx, request_context},
    view::{component, view},
};

#[component]
pub async fn configuration_form(cx: &Cx, active_page: &'static str) -> Result {
    let is_admin = request_context::<crate::web::auth::AuthenticatedUser>(cx)
        .0
        .role
        == crate::accounts::Role::Admin;
    view! {
        <form
            id="configForm"
            method="post"
            action="/api/config"
            data-app-config="true"
            hidden=(active_page != "settings" && active_page != "targets")
            class="relative"
        >
            <input
                type="hidden"
                name="return_to"
                data-app-return-to="true"
                value=(if active_page == "targets" { "/targets" } else { "/settings" })
            />
            <section data-app-page="settings" hidden=(active_page != "settings")>
                <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
                    if is_admin {
                        <div class="min-w-0">server_settings()</div>
                    }
                    <div class="min-w-0">chat_settings()</div>
                    <div class="min-w-0">notifications()</div>
                    <div class="min-w-0 lg:col-span-2">overlay_settings()</div>
                </div>
            </section>
            <section data-app-page="targets" hidden=(active_page != "targets")>
                targets()
            </section>
            <div class="mt-6">actions_panel()</div>
        </form>
    }
}
