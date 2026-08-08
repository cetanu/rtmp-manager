use crate::web::components::actions_panel::actions_panel;
use crate::web::components::app_navigation::page_heading;
use crate::web::components::chat_settings::chat_settings;
use crate::web::components::notifications::notifications;
use crate::web::components::server_settings::server_settings;
use crate::web::components::targets::targets;
use crate::web::components::web_auth::web_auth;
use topcoat::{
    Result,
    view::{component, view},
};

#[component]
pub async fn configuration_form(active_page: &'static str) -> Result {
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
                    <div class="min-w-0">server_settings()</div>
                    <div class="min-w-0">web_auth()</div>
                    <div class="min-w-0">chat_settings()</div>
                    <div class="min-w-0">notifications()</div>
                </div>
            </section>
            <section data-app-page="targets" hidden=(active_page != "targets")>
                targets()
            </section>
            <div class="mt-6">actions_panel()</div>
        </form>
    }
}
