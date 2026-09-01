use crate::accounts::AccountRepository;
use crate::server::state::AppHandle;
use anyhow::{Context, Result, bail};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl, basic::BasicClient,
};
use serde::Deserialize;
use topcoat::{
    context::{Cx, app_context},
    router::{error::bad_request, page, parse_query_params, route},
    session,
};

type OAuthClient = oauth2::Client<
    oauth2::StandardErrorResponse<oauth2::basic::BasicErrorResponseType>,
    oauth2::StandardTokenResponse<oauth2::EmptyExtraTokenFields, oauth2::basic::BasicTokenType>,
    oauth2::StandardTokenIntrospectionResponse<
        oauth2::EmptyExtraTokenFields,
        oauth2::basic::BasicTokenType,
    >,
    oauth2::StandardRevocableToken,
    oauth2::StandardErrorResponse<oauth2::RevocationErrorResponseType>,
    oauth2::EndpointSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointSet,
>;

#[derive(Clone, Copy)]
enum Provider {
    Twitch,
    Google,
    Discord,
    Github,
}

impl Provider {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "twitch" => Some(Self::Twitch),
            "google" => Some(Self::Google),
            "discord" => Some(Self::Discord),
            "github" => Some(Self::Github),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Twitch => "twitch",
            Self::Google => "google",
            Self::Discord => "discord",
            Self::Github => "github",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Twitch => "Twitch",
            Self::Google => "Google",
            Self::Discord => "Discord",
            Self::Github => "GitHub",
        }
    }

    fn endpoints(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Twitch => (
                "https://id.twitch.tv/oauth2/authorize",
                "https://id.twitch.tv/oauth2/token",
                "https://api.twitch.tv/helix/users",
            ),
            Self::Google => (
                "https://accounts.google.com/o/oauth2/v2/auth",
                "https://oauth2.googleapis.com/token",
                "https://openidconnect.googleapis.com/v1/userinfo",
            ),
            Self::Discord => (
                "https://discord.com/oauth2/authorize",
                "https://discord.com/api/oauth2/token",
                "https://discord.com/api/users/@me",
            ),
            Self::Github => (
                "https://github.com/login/oauth/authorize",
                "https://github.com/login/oauth/access_token",
                "https://api.github.com/user",
            ),
        }
    }

    fn scopes(self) -> &'static [&'static str] {
        match self {
            Self::Twitch => &["user:read:email"],
            Self::Google => &["openid", "email"],
            Self::Discord => &["identify", "email"],
            Self::Github => &["read:user", "user:email"],
        }
    }
}

struct ProviderConfig {
    provider: Provider,
    client_id: String,
    client_secret: String,
    redirect_url: String,
}

impl ProviderConfig {
    fn load(provider: Provider) -> Result<Self> {
        let prefix = format!("OAUTH_{}", provider.name().to_ascii_uppercase());
        let client_id = std::env::var(format!("{prefix}_CLIENT_ID"))
            .with_context(|| format!("{} OAuth is not configured", provider.title()))?;
        let client_secret = std::env::var(format!("{prefix}_CLIENT_SECRET"))
            .with_context(|| format!("{} OAuth is not configured", provider.title()))?;
        let base = std::env::var("OAUTH_REDIRECT_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_owned());
        Ok(Self {
            provider,
            client_id,
            client_secret,
            redirect_url: format!(
                "{}/oauth/{}/callback",
                base.trim_end_matches('/'),
                provider.name()
            ),
        })
    }

    fn client(&self) -> Result<OAuthClient> {
        let (auth_url, token_url, _) = self.provider.endpoints();
        Ok(BasicClient::new(ClientId::new(self.client_id.clone()))
            .set_client_secret(ClientSecret::new(self.client_secret.clone()))
            .set_auth_uri(AuthUrl::new(auth_url.to_owned())?)
            .set_token_uri(TokenUrl::new(token_url.to_owned())?)
            .set_redirect_uri(RedirectUrl::new(self.redirect_url.clone())?))
    }

    async fn authorization_url(&self, accounts: &AccountRepository) -> Result<String> {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let state = accounts
            .begin_oauth(self.provider.name(), verifier.secret())
            .await?;
        let client = self.client()?;
        let mut request = client
            .authorize_url(|| CsrfToken::new(state))
            .set_pkce_challenge(challenge);
        for scope in self.provider.scopes() {
            request = request.add_scope(Scope::new((*scope).to_owned()));
        }
        Ok(request.url().0.to_string())
    }

    async fn identity(
        &self,
        accounts: &AccountRepository,
        http: &reqwest::Client,
        code: String,
        state: String,
    ) -> Result<(String, String)> {
        let verifier = accounts
            .consume_oauth(self.provider.name(), &state)
            .await?
            .context("OAuth state is invalid or expired")?;
        let oauth_http = oauth2::reqwest::ClientBuilder::new()
            .redirect(oauth2::reqwest::redirect::Policy::none())
            .build()?;
        let token = self
            .client()?
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(PkceCodeVerifier::new(verifier))
            .request_async(&oauth_http)
            .await
            .map_err(|error| anyhow::anyhow!("OAuth token exchange failed: {error}"))?;
        fetch_identity(
            self.provider,
            http,
            &self.client_id,
            token.access_token().secret(),
        )
        .await
    }
}

pub fn configured_providers() -> Vec<(&'static str, &'static str)> {
    [
        Provider::Twitch,
        Provider::Google,
        Provider::Discord,
        Provider::Github,
    ]
    .into_iter()
    .filter(|provider| ProviderConfig::load(*provider).is_ok())
    .map(|provider| (provider.name(), provider.title()))
    .collect()
}

#[topcoat::router::path_param]
struct OAuthProvider(str);

#[page("/oauth/{oauth_provider}")]
async fn oauth_start(cx: &Cx) -> topcoat::Result {
    let provider_name = topcoat::router::path_param::<OAuthProvider>(cx);
    let provider =
        Provider::parse(provider_name).ok_or_else(|| bad_request("Unknown OAuth provider"))?;
    let config = ProviderConfig::load(provider).map_err(|error| bad_request(error.to_string()))?;
    let app: &AppHandle = app_context(cx);
    let url = config
        .authorization_url(&app.accounts)
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    Err(topcoat::router::error::redirect(&url).into())
}

#[derive(Deserialize)]
struct OAuthCallbackQuery {
    code: String,
    state: String,
}

#[route(GET "/oauth/{oauth_provider}/callback")]
async fn oauth_callback(cx: &Cx) -> topcoat::Result<topcoat::router::Response> {
    let provider_name = topcoat::router::path_param::<OAuthProvider>(cx);
    let provider =
        Provider::parse(provider_name).ok_or_else(|| bad_request("Unknown OAuth provider"))?;
    let query: OAuthCallbackQuery = parse_query_params(cx)?;
    let config = ProviderConfig::load(provider).map_err(|error| bad_request(error.to_string()))?;
    let app: &AppHandle = app_context(cx);
    let (subject, email) = config
        .identity(&app.accounts, &app.http_client, query.code, query.state)
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    let user = app
        .accounts
        .find_or_create_oauth_user(provider.name(), &subject, &email)
        .await
        .map_err(|error| bad_request(error.to_string()))?;
    let new_session = session::start(cx).await?;
    app.accounts.create_session(&user.id, &new_session).await?;
    super::redirect_to(cx, "/")
}

async fn fetch_identity(
    provider: Provider,
    http: &reqwest::Client,
    client_id: &str,
    access_token: &str,
) -> Result<(String, String)> {
    let (_, _, user_url) = provider.endpoints();
    let mut request = http.get(user_url).bearer_auth(access_token);
    if matches!(provider, Provider::Twitch) {
        request = request.header("Client-Id", client_id);
    }
    if matches!(provider, Provider::Github) {
        request = request.header("User-Agent", "rtmp-manager");
    }
    let profile: serde_json::Value = request.send().await?.error_for_status()?.json().await?;
    match provider {
        Provider::Twitch => {
            let user = profile["data"]
                .as_array()
                .and_then(|users| users.first())
                .context("Twitch did not return a user profile")?;
            identity_fields(user, "id", "email")
        }
        Provider::Google => {
            if profile["email_verified"].as_bool() != Some(true) {
                bail!("Google account email is not verified");
            }
            identity_fields(&profile, "sub", "email")
        }
        Provider::Discord => {
            if profile["verified"].as_bool() != Some(true) {
                bail!("Discord account email is not verified");
            }
            identity_fields(&profile, "id", "email")
        }
        Provider::Github => {
            let subject = profile["id"]
                .as_u64()
                .context("GitHub profile has no ID")?
                .to_string();
            let emails: Vec<serde_json::Value> = http
                .get("https://api.github.com/user/emails")
                .header("User-Agent", "rtmp-manager")
                .bearer_auth(access_token)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let email = emails
                .iter()
                .find(|email| {
                    email["primary"].as_bool() == Some(true)
                        && email["verified"].as_bool() == Some(true)
                })
                .and_then(|email| email["email"].as_str())
                .context("GitHub account has no verified primary email")?;
            Ok((subject, email.to_owned()))
        }
    }
}

fn identity_fields(profile: &serde_json::Value, id: &str, email: &str) -> Result<(String, String)> {
    let subject = profile[id]
        .as_str()
        .context("OAuth profile has no subject")?;
    let email = profile[email]
        .as_str()
        .context("OAuth profile has no email")?;
    Ok((subject.to_owned(), email.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_the_documented_oauth_providers() {
        for provider in ["twitch", "google", "discord", "github"] {
            assert_eq!(Provider::parse(provider).unwrap().name(), provider);
        }
        assert!(Provider::parse("unknown").is_none());
    }
}
