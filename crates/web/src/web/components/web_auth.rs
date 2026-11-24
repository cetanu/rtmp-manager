use crate::web::components::configuration_form::settings_section;
use crate::web::components::ui::form::{
    clearable_secret_field, field_description, form_field, secret_input,
};
use crate::web::components::ui::input::input;
use rtmp_core::server::state::AppHandle;
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, attributes, component, view},
};

#[component]
pub async fn web_auth(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = context(cx);
    let auth = app.config.get().web_auth.clone();
    let backup_configured = !auth.backup_passphrase_hash.is_empty();
    Ok(view! {
        settings_section(
            title: "Access & security",
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
            clearable_secret_field(
                control_id: "web_auth_password",
                name: "web_auth[password]",
                label_text: "Password",
                empty_placeholder: "At least 12 characters",
                value: auth.password,
                attrs: attributes! { minlength="12" }
            )
            if backup_configured {
                form_field(
                    control_id: "current_backup_passphrase",
                    label_text: "Current backup passphrase",
                    secret_input(
                        control_id: "current_backup_passphrase",
                        attrs: attributes! {
                            name="web_auth[current_backup_passphrase]"
                            autocomplete="current-password"
                            placeholder="Required to change or clear"
                        }
                    )
                    field_description(
                        "Required to change or clear the backup passphrase."
                    )
                )
            }
            clearable_secret_field(
                control_id: "backup_passphrase",
                name: "web_auth[backup_passphrase]",
                label_text: if backup_configured { "New backup passphrase" } else { "Backup passphrase" },
                empty_placeholder: "At least 12 characters",
                value: auth.backup_passphrase_hash,
                attrs: attributes! { minlength="12" maxlength="1024" }
            )
            clearable_secret_field(
                control_id: "web_auth_overlay_token",
                name: "web_auth[overlay_token]",
                label_text: "OBS overlay access token",
                empty_placeholder: "At least 16 characters",
                value: auth.overlay_token,
                attrs: attributes! { minlength="16" }
            )
        )
    })
}
