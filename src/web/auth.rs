use crate::accounts::{Role, User};
use crate::server::state::AppHandle;
use serde::Deserialize;
use topcoat::{
    Result,
    context::{Cx, CxBuilder, app_context, request_context},
    router::{
        Body, IntoResponse, Next, Response, StatusCode,
        error::{bad_request, redirect},
        layer, page, route,
    },
    session,
    view::{component, view},
};

#[derive(Clone)]
pub struct AuthenticatedUser(pub User);

pub fn current_user(cx: &Cx) -> &User {
    &request_context::<AuthenticatedUser>(cx).0
}

fn public_path(path: &str) -> bool {
    matches!(path, "/setup" | "/api/setup" | "/login" | "/register")
        || path.starts_with("/oauth/")
        || path == "/api/webhook"
        || path.starts_with("/api/v1/webhooks/")
        || path.starts_with("/overlay/chat")
}

fn admin_path(path: &str) -> bool {
    path == "/logs"
        || path == "/export"
        || path == "/api/logs"
        || path == "/api/admin/emergency-stop"
        || path.starts_with("/api/config/import")
}

#[layer("/")]
async fn session_auth(cx: &mut CxBuilder, body: Body, next: Next<'_>) -> Result<Response> {
    let path = topcoat::router::uri(cx).path();
    if public_path(path) {
        return next.run(cx, body).await;
    }

    let app: &AppHandle = app_context(cx);
    let user = match session::token_hash(cx).await? {
        Some(hash) => app.accounts.user_for_session(&hash).await?,
        None => None,
    };
    let Some(user) = user else {
        if path.starts_with("/api/") {
            return StatusCode::UNAUTHORIZED.into_response(cx);
        }
        return Err(redirect("/login").into());
    };
    if admin_path(path) && user.role != Role::Admin {
        return StatusCode::FORBIDDEN.into_response(cx);
    }
    cx.insert(AuthenticatedUser(user));
    next.run(cx, body).await
}

#[component]
async fn account_document(title: &'static str, content: topcoat::view::View) -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="en" class="dark">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1.0" />
            <title>(title)" · RTMP Manager"</title>
            <link rel="stylesheet" href=(super::TAILWIND_STYLESHEET) />
        </head>
        <body class="min-h-screen bg-background px-4 py-12 text-foreground">
            <main class="mx-auto max-w-md rounded-xl border border-border bg-card p-6 shadow-sm">
                <h1 class="text-2xl font-semibold">(title)</h1>
                (content)
            </main>
        </body>
        </html>
    }
}

#[page("/login")]
async fn login_page() -> Result {
    let providers = super::oauth::configured_providers();
    view! {
        account_document(
            title: "Sign in",
            content: (view! {
                <form method="post" action="/login" class="mt-6 space-y-4">
                    <label class="block text-sm">"Email"</label>
                    <input class="h-10 w-full rounded-md border bg-background px-3" type="email" name="email" required="required" autocomplete="email" />
                    <label class="block text-sm">"Password"</label>
                    <input class="h-10 w-full rounded-md border bg-background px-3" type="password" name="password" required="required" autocomplete="current-password" />
                    <button class="h-10 w-full rounded-md bg-primary font-medium text-primary-foreground" type="submit">"Sign in"</button>
                </form>
                <div hidden=(providers.is_empty()) class="mt-5 space-y-2 border-t border-border pt-5">
                    for (provider, title) in providers {
                        <a class="flex h-10 w-full items-center justify-center rounded-md border border-border font-medium" href=(format!("/oauth/{provider}"))>"Continue with "(title)</a>
                    }
                </div>
                <p class="mt-4 text-sm text-muted-foreground">"Need an account? "<a class="text-primary underline" href="/register">"Register"</a></p>
            })?
        )
    }
}

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
}

#[route(POST "/login")]
async fn login(cx: &Cx, body: topcoat::router::Bytes) -> Result<Response> {
    let form: Credentials = serde_qs::from_bytes(&body)
        .map_err(|error| bad_request(format!("Invalid login form: {error}")))?;
    let app: &AppHandle = app_context(cx);
    let Some(user) = app
        .accounts
        .authenticate_password(&form.email, &form.password)
        .await
        .map_err(|error| bad_request(error.to_string()))?
    else {
        return Err(bad_request("Invalid email or password").into());
    };
    let new_session = session::start(cx).await?;
    app.accounts.create_session(&user.id, &new_session).await?;
    super::redirect_to(cx, "/")
}

#[page("/register")]
async fn register_page() -> Result {
    view! {
        account_document(
            title: "Create account",
            content: (view! {
                <form method="post" action="/register" class="mt-6 space-y-4">
                    <label class="block text-sm">"Email"</label>
                    <input class="h-10 w-full rounded-md border bg-background px-3" type="email" name="email" required="required" autocomplete="email" />
                    <label class="block text-sm">"Password (at least 12 characters)"</label>
                    <input class="h-10 w-full rounded-md border bg-background px-3" type="password" name="password" required="required" minlength="12" autocomplete="new-password" />
                    <button class="h-10 w-full rounded-md bg-primary font-medium text-primary-foreground" type="submit">"Register"</button>
                </form>
            })?
        )
    }
}

#[route(POST "/register")]
async fn register(cx: &Cx, body: topcoat::router::Bytes) -> Result<Response> {
    let form: Credentials = serde_qs::from_bytes(&body)
        .map_err(|error| bad_request(format!("Invalid registration form: {error}")))?;
    let app: &AppHandle = app_context(cx);
    let (user, stream_key) = app
        .accounts
        .register(&form.email, &form.password)
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    let new_session = session::start(cx).await?;
    app.accounts.create_session(&user.id, &new_session).await?;
    let page = (view! {
        account_document(
            title: "Account created",
            content: (view! {
                <p class="mt-4 text-sm text-muted-foreground">"Save this ingest key now. It is shown only once."</p>
                <code class="mt-3 block break-all rounded-md bg-background p-3">(stream_key)</code>
                <a class="mt-6 inline-flex h-10 w-full items-center justify-center rounded-md bg-primary font-medium text-primary-foreground" href="/">"Open dashboard"</a>
            })?
        )
    })?;
    page.into_response(cx)
}

#[page("/profile")]
async fn profile_page(cx: &Cx) -> Result {
    let user = current_user(cx);
    view! {
        account_document(
            title: "Profile",
            content: (view! {
                <p class="mt-2 text-sm text-muted-foreground">"Role: "(user.role.as_str())</p>
                <form method="post" action="/profile/update" class="mt-6 space-y-4">
                    <label class="block text-sm">"Email"</label>
                    <input class="h-10 w-full rounded-md border bg-background px-3" type="email" name="email" value=(user.email.clone()) required="required" autocomplete="email" />
                    <label class="block text-sm">"New password"</label>
                    <input class="h-10 w-full rounded-md border bg-background px-3" type="password" name="password" minlength="12" autocomplete="new-password" placeholder="Leave blank to keep current password" />
                    <button class="h-10 w-full rounded-md bg-primary font-medium text-primary-foreground" type="submit">"Update profile"</button>
                </form>
                <form method="post" action="/profile/stream-key" class="mt-4">
                    <button class="h-10 w-full rounded-md border border-border font-medium" type="submit">"Reset ingest stream key"</button>
                </form>
                <form method="post" action="/profile/sessions/revoke" class="mt-4">
                    <button class="h-10 w-full rounded-md border border-destructive text-destructive font-medium" type="submit">"Sign out every session"</button>
                </form>
                <a class="mt-6 block text-center text-sm text-primary underline" href="/">"Back to dashboard"</a>
            })?
        )
    }
}

#[derive(Deserialize)]
struct ProfileUpdate {
    email: String,
    password: String,
}

#[route(POST "/profile/update")]
async fn update_profile(cx: &Cx, body: topcoat::router::Bytes) -> Result<Response> {
    let form: ProfileUpdate = serde_qs::from_bytes(&body)
        .map_err(|error| bad_request(format!("Invalid profile form: {error}")))?;
    let user = current_user(cx);
    let app: &AppHandle = app_context(cx);
    app.accounts
        .update_profile(&user.id, &form.email, Some(&form.password))
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    app.accounts.revoke_sessions(&user.id).await?;
    let _ = session::stop(cx).await?;
    super::redirect_to(cx, "/login")
}

#[route(POST "/profile/stream-key")]
async fn reset_stream_key(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = app_context(cx);
    let stream_key = app
        .accounts
        .reset_stream_key(&current_user(cx).tenant_id)
        .await?;
    let page = (view! {
        account_document(
            title: "New ingest key",
            content: (view! {
                <p class="mt-4 text-sm text-muted-foreground">"Update OBS now. The previous key no longer authenticates."</p>
                <code class="mt-3 block break-all rounded-md bg-background p-3">(stream_key)</code>
                <a class="mt-6 inline-flex h-10 w-full items-center justify-center rounded-md bg-primary font-medium text-primary-foreground" href="/profile">"Return to profile"</a>
            })?
        )
    })?;
    page.into_response(cx)
}

#[route(POST "/profile/sessions/revoke")]
async fn revoke_sessions(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = app_context(cx);
    app.accounts.revoke_sessions(&current_user(cx).id).await?;
    let _ = session::stop(cx).await?;
    super::redirect_to(cx, "/login")
}

#[route(POST "/logout")]
async fn logout(cx: &Cx) -> Result<Response> {
    let app: &AppHandle = app_context(cx);
    if let Some(hash) = session::stop(cx).await? {
        app.accounts.delete_session(&hash).await?;
    }
    super::redirect_to(cx, "/login")
}
