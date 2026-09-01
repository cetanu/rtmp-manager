use crate::config::{AppConfig, TargetConfig};
use crate::server::state::AppHandle;
use serde::Deserialize;
use std::time::Duration;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        error::{bad_request, redirect},
        page, route,
    },
    view::{component, view},
};

#[derive(Debug, Deserialize)]
struct SetupForm {
    admin_email: String,
    admin_password: String,
    ingest_stream_key: String,
    destination: String,
    destination_url: String,
    destination_stream_key: String,
    twitch_channel: String,
    youtube_api_key: String,
}

#[component]
async fn setup_document() -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="en" class="dark">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1.0" />
            <title>"Set up RTMP Manager"</title>
            <link rel="stylesheet" href=(super::TAILWIND_STYLESHEET) />
        </head>
        <body class="min-h-screen bg-background px-4 py-10 text-foreground">
            <main class="mx-auto max-w-2xl">
                <header class="mb-8">
                    <p class="text-sm font-medium text-primary">"First-run setup"</p>
                    <h1 class="text-3xl font-semibold">"Configure RTMP Manager"</h1>
                    <p class="mt-2 text-muted-foreground">"Complete these four steps. Settings take effect immediately."</p>
                </header>
                <form method="post" action="/api/setup" class="space-y-6">
                    <fieldset class="rounded-xl border border-border bg-card p-6">
                        <legend class="px-2 font-semibold">"1. Dashboard administrator"</legend>
                        <label class="mt-2 block text-sm">"Email"</label>
                        <input class="mt-1 h-10 w-full rounded-md border bg-background px-3" type="email" name="admin_email" required="required" autocomplete="email" />
                        <label class="mt-4 block text-sm">"Password (at least 12 characters)"</label>
                        <input class="mt-1 h-10 w-full rounded-md border bg-background px-3" type="password" name="admin_password" required="required" minlength="12" autocomplete="new-password" />
                    </fieldset>
                    <fieldset class="rounded-xl border border-border bg-card p-6">
                        <legend class="px-2 font-semibold">"2. RTMP ingest key"</legend>
                        <label class="block text-sm">"Stream key"</label>
                        <input class="mt-1 h-10 w-full rounded-md border bg-background px-3" name="ingest_stream_key" pattern="[A-Za-z0-9_-]*" placeholder="Leave blank to generate a secure key" />
                    </fieldset>
                    <fieldset class="rounded-xl border border-border bg-card p-6">
                        <legend class="px-2 font-semibold">"3. First destination"</legend>
                        <label class="block text-sm">"Platform"</label>
                        <select class="mt-1 h-10 w-full rounded-md border bg-background px-3" name="destination">
                            <option value="none">"Configure later"</option>
                            <option value="twitch">"Twitch"</option>
                            <option value="youtube">"YouTube"</option>
                            <option value="kick">"Kick"</option>
                            <option value="x">"X"</option>
                            <option value="custom">"Custom RTMP"</option>
                        </select>
                        <label class="mt-4 block text-sm">"Custom RTMP URL (only for Custom RTMP)"</label>
                        <input class="mt-1 h-10 w-full rounded-md border bg-background px-3" name="destination_url" placeholder="rtmps://example.com/app" />
                        <label class="mt-4 block text-sm">"Destination stream key"</label>
                        <input class="mt-1 h-10 w-full rounded-md border bg-background px-3" type="password" name="destination_stream_key" />
                        <p class="mt-2 text-xs text-muted-foreground">"The endpoint is checked with a connection dry run before setup is saved."</p>
                    </fieldset>
                    <fieldset class="rounded-xl border border-border bg-card p-6">
                        <legend class="px-2 font-semibold">"4. Optional chat integration"</legend>
                        <label class="block text-sm">"Twitch channel"</label>
                        <input class="mt-1 h-10 w-full rounded-md border bg-background px-3" name="twitch_channel" />
                        <label class="mt-4 block text-sm">"YouTube API key"</label>
                        <input class="mt-1 h-10 w-full rounded-md border bg-background px-3" type="password" name="youtube_api_key" />
                        <p class="mt-2 text-xs text-muted-foreground">"Additional OAuth providers can be configured from Settings after setup."</p>
                    </fieldset>
                    <button class="h-11 w-full rounded-md bg-primary px-5 font-medium text-primary-foreground" type="submit">"Validate and finish setup"</button>
                </form>
            </main>
        </body>
        </html>
    }
}

#[page("/setup")]
async fn setup_page(cx: &Cx) -> Result {
    let app: &AppHandle = app_context(cx);
    if app.config.get().initialized {
        return Err(redirect("/").into());
    }
    view! { setup_document() }
}

#[route(POST "/api/setup")]
async fn complete_setup(
    cx: &Cx,
    body: topcoat::router::Bytes,
) -> Result<topcoat::router::Response> {
    let app: &AppHandle = app_context(cx);
    if app.config.get().initialized {
        return super::redirect_to(cx, "/");
    }
    let form: SetupForm = serde_qs::from_bytes(&body)
        .map_err(|error| bad_request(format!("Invalid setup form: {error}")))?;
    let target = destination(&form).map_err(bad_request)?;
    let mut config = AppConfig {
        initialized: true,
        ..AppConfig::default()
    };
    let admin_email = form.admin_email.trim().to_owned();
    let admin_password = form.admin_password;
    config.server.ingest_stream_key = if form.ingest_stream_key.trim().is_empty() {
        crate::util::generate_secure_token().map_err(|error| bad_request(error.to_string()))?
    } else {
        form.ingest_stream_key.trim().to_owned()
    };
    config.overlay.key =
        crate::util::generate_secure_token().map_err(|error| bad_request(error.to_string()))?;
    config.chat.twitch_channel = optional(form.twitch_channel);
    config.chat.youtube_api_key = optional(form.youtube_api_key);

    if let Some(target) = target {
        check_endpoint(&target.url).await.map_err(bad_request)?;
        config.targets.push(target);
    }
    config
        .validate()
        .map_err(|error| bad_request(error.to_string()))?;
    if app
        .accounts
        .has_users()
        .await
        .map_err(|error| bad_request(error.to_string()))?
    {
        return Err(bad_request("An administrator account already exists").into());
    }
    app.accounts
        .create_local_user(
            &crate::tenant::TenantId::default_tenant(),
            &admin_email,
            &admin_password,
            crate::accounts::Role::Admin,
        )
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    app.config
        .complete_setup(config)
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    app.apply_chat_config()
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    super::redirect_to(cx, "/")
}

fn optional(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn destination(form: &SetupForm) -> std::result::Result<Option<TargetConfig>, String> {
    let (name, url) = match form.destination.as_str() {
        "none" => return Ok(None),
        "twitch" => ("Twitch", "rtmps://live.twitch.tv/app"),
        "youtube" => ("YouTube", "rtmps://a.rtmp.youtube.com/live2"),
        "kick" => (
            "Kick",
            "rtmps://fa723fc1b171.global-contribute.live-video.net/app",
        ),
        "x" => ("X", "rtmps://va.pscp.tv/x"),
        "custom" => ("Custom RTMP", form.destination_url.trim()),
        _ => return Err("Unknown destination preset".to_owned()),
    };
    if url.is_empty() {
        return Err("Custom RTMP requires an endpoint URL".to_owned());
    }
    if form.destination_stream_key.trim().is_empty() {
        return Err("The selected destination requires a stream key".to_owned());
    }
    Ok(Some(TargetConfig {
        name: name.to_owned(),
        url: url.to_owned(),
        stream_key: form.destination_stream_key.trim().to_owned(),
        public_url: None,
        enabled: true,
    }))
}

async fn check_endpoint(endpoint: &str) -> std::result::Result<(), String> {
    let url = reqwest::Url::parse(endpoint).map_err(|_| "Invalid destination URL".to_owned())?;
    if !matches!(url.scheme(), "rtmp" | "rtmps") {
        return Err("Destination URL must use RTMP or RTMPS".to_owned());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "Destination URL has no host".to_owned())?;
    let port = url
        .port()
        .unwrap_or(if url.scheme() == "rtmps" { 443 } else { 1935 });
    tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect((host, port)),
    )
    .await
    .map_err(|_| format!("Timed out connecting to {host}:{port}"))?
    .map_err(|error| format!("Could not connect to {host}:{port}: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(destination: &str) -> SetupForm {
        SetupForm {
            admin_email: "admin@example.com".into(),
            admin_password: "long-enough-password".into(),
            ingest_stream_key: String::new(),
            destination: destination.into(),
            destination_url: String::new(),
            destination_stream_key: "target-key".into(),
            twitch_channel: String::new(),
            youtube_api_key: String::new(),
        }
    }

    #[test]
    fn destination_presets_produce_enabled_targets() {
        let target = destination(&form("twitch")).unwrap().unwrap();
        assert_eq!(target.name, "Twitch");
        assert_eq!(target.url, "rtmps://live.twitch.tv/app");
        assert!(target.enabled);
    }

    #[test]
    fn generated_stream_keys_are_valid_and_unique() {
        let first = crate::util::generate_secure_token().unwrap();
        let second = crate::util::generate_secure_token().unwrap();
        assert_ne!(first, second);
        assert!(
            first
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "_-".contains(character))
        );
    }
}
