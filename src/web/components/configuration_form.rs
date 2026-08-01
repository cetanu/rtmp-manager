use crate::web::components::actions_panel::actions_panel;
use crate::web::components::chat_settings::chat_settings;
use crate::web::components::notifications::notifications;
use crate::web::components::server_settings::server_settings;
use crate::web::components::targets::targets;
use crate::web::components::web_auth::web_auth;
use topcoat::{
    view::{component, view},
    Result,
};

#[component]
pub async fn configuration_form() -> Result {
    view! {
        <form id="configForm" method="post" action="/api/config" class="relative grid grid-cols-1 gap-6 lg:grid-cols-2">
            <section class="min-w-0">server_settings()</section>
            <section class="min-w-0">web_auth()</section>
            <section class="min-w-0">chat_settings()</section>
            <section class="min-w-0">notifications()</section>
            <section class="min-w-0 lg:col-span-2">targets()</section>
            <section class="min-w-0 lg:col-span-2">actions_panel()</section>
        </form>
    }
}
