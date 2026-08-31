use crate::util::non_empty;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, watch};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Validate)]
pub struct ServerSettings {
    #[serde(default = "default_listen")]
    pub listen: SocketAddr,

    #[serde(default = "default_api_listen")]
    pub api_listen: SocketAddr,

    #[serde(default = "default_test_stream_duration_secs")]
    #[validate(minimum = 1)]
    #[validate(maximum = 86_400)]
    pub test_stream_duration_secs: u64,

    #[serde(default)]
    #[validate(max_length = 256)]
    #[validate(
        pattern = r"^[A-Za-z0-9_-]*$",
        message = "Ingest stream key must contain only ASCII letters, numbers, hyphens, or underscores"
    )]
    pub ingest_stream_key: String,
}

fn default_listen() -> SocketAddr {
    "0.0.0.0:1935".parse().unwrap()
}

fn default_api_listen() -> SocketAddr {
    "0.0.0.0:3000".parse().unwrap()
}

fn default_test_stream_duration_secs() -> u64 {
    15
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            api_listen: default_api_listen(),
            test_stream_duration_secs: default_test_stream_duration_secs(),
            ingest_stream_key: String::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct NotificationSettings {
    pub discord_webhook: Option<String>,
    #[serde(default = "default_live_message")]
    pub live_message: String,
    pub webhook_url: Option<String>,
}

fn default_live_message() -> String {
    "Stream is LIVE".to_string()
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            discord_webhook: None,
            live_message: default_live_message(),
            webhook_url: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct TargetConfig {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub stream_key: String,
    #[serde(default)]
    pub public_url: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
pub struct WebAuthSettings {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Validate)]
pub struct ChatSettings {
    #[serde(default = "default_chat_queue_capacity")]
    #[validate(minimum = 1)]
    pub queue_capacity: usize,
    #[serde(default)]
    pub twitch_channel: Option<String>,
    #[serde(default)]
    pub youtube_api_key: Option<String>,
    #[serde(default)]
    pub youtube_live_chat_id: Option<String>,
    #[serde(default)]
    pub youtube_video_id: Option<String>,
    #[serde(default)]
    pub youtube_channel_id: Option<String>,
    #[serde(default = "default_youtube_min_poll_interval_secs")]
    #[validate(minimum = 1)]
    pub youtube_min_poll_interval_secs: u64,
    #[serde(default = "default_true")]
    pub youtube_adaptive_polling: bool,
    #[serde(default)]
    pub youtube_polling_enabled: bool,
    #[serde(default)]
    pub x_api_key: Option<String>,
    #[serde(default)]
    pub x_api_secret: Option<String>,
    #[serde(default)]
    pub x_client_id: Option<String>,
    #[serde(default)]
    pub x_client_secret: Option<String>,
    #[serde(default)]
    pub x_webhook_enabled: bool,
    #[serde(default)]
    pub kick_client_id: Option<String>,
    #[serde(default)]
    pub kick_client_secret: Option<String>,
    #[serde(default)]
    pub kick_channel: Option<String>,
    #[serde(default)]
    pub kick_webhook_enabled: bool,
}

fn default_chat_queue_capacity() -> usize {
    500
}

fn default_youtube_min_poll_interval_secs() -> u64 {
    5
}

fn default_true() -> bool {
    true
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            queue_capacity: default_chat_queue_capacity(),
            twitch_channel: None,
            youtube_api_key: None,
            youtube_live_chat_id: None,
            youtube_video_id: None,
            youtube_channel_id: None,
            youtube_min_poll_interval_secs: default_youtube_min_poll_interval_secs(),
            youtube_adaptive_polling: true,
            youtube_polling_enabled: false,
            x_api_key: None,
            x_api_secret: None,
            x_client_id: None,
            x_client_secret: None,
            x_webhook_enabled: false,
            kick_client_id: None,
            kick_client_secret: None,
            kick_channel: None,
            kick_webhook_enabled: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default)]
    pub initialized: bool,

    #[serde(default)]
    pub server: ServerSettings,

    #[serde(default)]
    pub notifications: NotificationSettings,

    #[serde(default)]
    pub targets: Vec<TargetConfig>,

    #[serde(default)]
    pub web_auth: WebAuthSettings,

    #[serde(default)]
    pub chat: ChatSettings,
}

impl AppConfig {
    /// Validate enabled target URLs and credentials.
    pub fn validate(&self) -> Result<()> {
        self.server
            .validate()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        self.chat
            .validate()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;

        let username_set = !self.web_auth.username.trim().is_empty();
        let password_set = !self.web_auth.password.is_empty();
        if username_set != password_set {
            bail!(
                "Web authentication username and password must either both be set or both be empty"
            );
        }
        if password_set && self.web_auth.password.len() < 12 {
            bail!("Web authentication password must be at least 12 characters");
        }
        if username_set && self.web_auth.username.contains(':') {
            bail!("Web authentication username must not contain ':'");
        }
        if self.chat.twitch_channel.as_ref().is_some_and(|channel| {
            let channel = channel.trim().trim_start_matches('#');
            channel.is_empty()
                || channel.len() > 25
                || !channel
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        }) {
            bail!("Twitch channel must be 1-25 ASCII letters, numbers, or underscores");
        }
        let youtube_selectors = [
            &self.chat.youtube_live_chat_id,
            &self.chat.youtube_video_id,
            &self.chat.youtube_channel_id,
        ]
        .into_iter()
        .filter(|value| value.as_ref().is_some_and(|value| !value.trim().is_empty()))
        .count();
        if youtube_selectors > 1 {
            bail!("Configure only one of YouTube live chat ID, video ID, or channel ID");
        }
        if self.chat.kick_webhook_enabled
            && (self
                .chat
                .kick_client_id
                .as_ref()
                .is_none_or(|value| value.trim().is_empty())
                || self
                    .chat
                    .kick_client_secret
                    .as_ref()
                    .is_none_or(|value| value.trim().is_empty())
                || self
                    .chat
                    .kick_channel
                    .as_ref()
                    .is_none_or(|value| value.trim().is_empty()))
        {
            bail!("Kick webhooks require a client ID, client secret, and channel");
        }
        if self.chat.kick_channel.as_ref().is_some_and(|channel| {
            channel.is_empty()
                || channel.len() > 25
                || !channel
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "_-".contains(character))
        }) {
            bail!("Kick channel must be 1-25 ASCII letters, numbers, hyphens, or underscores");
        }
        for target in &self.targets {
            if target.enabled {
                let url = target.url.trim();
                if url.is_empty() {
                    bail!("Target '{}' has an empty RTMP URL.", target.name);
                }
                if !url.starts_with("rtmp://") && !url.starts_with("rtmps://") {
                    bail!(
                        "Target '{}' has an invalid URL. It must start with rtmp:// or rtmps://",
                        target.name
                    );
                }
            }
        }
        Ok(())
    }

    /// Merges submitted form fields into the current configuration, preserving omitted or secret fields.
    pub fn merge_form(&self, form: ConfigForm) -> Result<Self> {
        let mut config = self.clone();

        if let Some(server) = form.server {
            config.server = ServerSettings {
                listen: parse_address(server.listen, config.server.listen, "RTMP listen")?,
                api_listen: parse_address(
                    server.api_listen,
                    config.server.api_listen,
                    "API listen",
                )?,
                test_stream_duration_secs: server
                    .test_stream_duration_secs
                    .unwrap_or(config.server.test_stream_duration_secs),
                ingest_stream_key: non_empty(server.ingest_stream_key)
                    .unwrap_or(config.server.ingest_stream_key),
            };
        }
        if let Some(notification_fields) = form.notifications {
            config.notifications = NotificationSettings {
                discord_webhook: updated_secret(
                    notification_fields.discord_webhook,
                    notification_fields.clear_discord_webhook,
                    config.notifications.discord_webhook,
                ),
                live_message: notification_fields
                    .live_message
                    .unwrap_or(config.notifications.live_message),
                webhook_url: updated_secret(
                    notification_fields.webhook_url,
                    notification_fields.clear_webhook_url,
                    config.notifications.webhook_url,
                ),
            };
        }
        if let Some(auth_fields) = form.web_auth {
            config.web_auth = WebAuthSettings {
                username: auth_fields
                    .username
                    .unwrap_or(config.web_auth.username)
                    .trim()
                    .to_string(),
                password: non_empty(auth_fields.password).unwrap_or(config.web_auth.password),
            };
        }
        if let Some(chat) = form.chat {
            config.chat = ChatSettings {
                queue_capacity: chat.queue_capacity.unwrap_or(config.chat.queue_capacity),
                twitch_channel: non_empty(chat.twitch_channel)
                    .map(|channel| channel.trim_start_matches('#').to_ascii_lowercase()),
                youtube_api_key: updated_secret(
                    chat.youtube_api_key,
                    chat.clear_youtube_api_key,
                    config.chat.youtube_api_key,
                ),
                youtube_live_chat_id: non_empty(chat.youtube_live_chat_id),
                youtube_video_id: non_empty(chat.youtube_video_id),
                youtube_channel_id: non_empty(chat.youtube_channel_id),
                youtube_min_poll_interval_secs: chat
                    .youtube_min_poll_interval_secs
                    .unwrap_or(config.chat.youtube_min_poll_interval_secs),
                youtube_adaptive_polling: chat.youtube_adaptive_polling,
                youtube_polling_enabled: config.chat.youtube_polling_enabled,
                x_api_key: updated_secret(
                    chat.x_api_key,
                    chat.clear_x_api_key,
                    config.chat.x_api_key,
                ),
                x_api_secret: updated_secret(
                    chat.x_api_secret,
                    chat.clear_x_api_secret,
                    config.chat.x_api_secret,
                ),
                x_client_id: updated_secret(
                    chat.x_client_id,
                    chat.clear_x_client_id,
                    config.chat.x_client_id,
                ),
                x_client_secret: updated_secret(
                    chat.x_client_secret,
                    chat.clear_x_client_secret,
                    config.chat.x_client_secret,
                ),
                x_webhook_enabled: config.chat.x_webhook_enabled,
                kick_client_id: non_empty(chat.kick_client_id),
                kick_client_secret: updated_secret(
                    chat.kick_client_secret,
                    chat.clear_kick_client_secret,
                    config.chat.kick_client_secret,
                ),
                kick_channel: non_empty(chat.kick_channel)
                    .map(|channel| channel.trim().to_ascii_lowercase()),
                kick_webhook_enabled: config.chat.kick_webhook_enabled,
            };
        }
        if let Some(target_fields) = form.targets {
            config.targets = target_fields
                .into_iter()
                .enumerate()
                .map(|(index, target)| TargetConfig {
                    name: target.name,
                    url: target.url,
                    stream_key: non_empty(target.stream_key).unwrap_or_else(|| {
                        config
                            .targets
                            .get(index)
                            .map(|target| target.stream_key.clone())
                            .unwrap_or_default()
                    }),
                    public_url: non_empty(target.public_url),
                    enabled: target.enabled,
                })
                .collect();
        }

        let action = form.action.as_deref().unwrap_or_default();
        if action == "add_target" {
            config.targets.push(TargetConfig {
                name: "New Target".to_string(),
                url: "".to_string(),
                stream_key: "".to_string(),
                public_url: None,
                enabled: false,
            });
        } else if action.starts_with("remove_target:")
            && let Some(idx_str) = action.split(':').nth(1)
            && let Ok(idx) = idx_str.parse::<usize>()
            && idx < config.targets.len()
        {
            config.targets.remove(idx);
        }

        Ok(config)
    }

    /// Parses imported configuration JSON bytes.
    pub fn parse_imported(body: &[u8]) -> Result<Self> {
        let value: serde_json::Value =
            serde_json::from_slice(body).context("Invalid JSON configuration")?;
        let object = value
            .as_object()
            .context("The JSON configuration must be an object")?;

        for field in ["server", "notifications", "targets", "web_auth", "chat"] {
            if !object.contains_key(field) {
                bail!("JSON configuration is missing required field '{field}'");
            }
        }

        let config: AppConfig =
            serde_json::from_value(value).context("Invalid configuration structure")?;
        config.validate()?;
        Ok(config)
    }
}

fn updated_secret(
    submitted: Option<String>,
    clear: bool,
    current: Option<String>,
) -> Option<String> {
    if clear {
        None
    } else {
        non_empty(submitted).or(current)
    }
}

fn parse_address(
    submitted: Option<String>,
    current: SocketAddr,
    field: &str,
) -> Result<SocketAddr> {
    submitted
        .map(|value| {
            value
                .parse()
                .with_context(|| format!("Invalid {field} address"))
        })
        .transpose()
        .map(|value| value.unwrap_or(current))
}

#[derive(Debug, Default, Deserialize)]
pub struct ServerForm {
    pub listen: Option<String>,
    pub api_listen: Option<String>,
    pub test_stream_duration_secs: Option<u64>,
    pub ingest_stream_key: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct NotificationsForm {
    pub discord_webhook: Option<String>,
    #[serde(default)]
    pub clear_discord_webhook: bool,
    pub live_message: Option<String>,
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub clear_webhook_url: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct WebAuthForm {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ChatForm {
    pub queue_capacity: Option<usize>,
    pub twitch_channel: Option<String>,
    pub youtube_api_key: Option<String>,
    #[serde(default)]
    pub clear_youtube_api_key: bool,
    pub youtube_live_chat_id: Option<String>,
    pub youtube_video_id: Option<String>,
    pub youtube_channel_id: Option<String>,
    pub youtube_min_poll_interval_secs: Option<u64>,
    #[serde(default)]
    pub youtube_adaptive_polling: bool,
    pub x_api_key: Option<String>,
    #[serde(default)]
    pub clear_x_api_key: bool,
    pub x_api_secret: Option<String>,
    #[serde(default)]
    pub clear_x_api_secret: bool,
    pub x_client_id: Option<String>,
    #[serde(default)]
    pub clear_x_client_id: bool,
    pub x_client_secret: Option<String>,
    #[serde(default)]
    pub clear_x_client_secret: bool,
    pub kick_client_id: Option<String>,
    pub kick_client_secret: Option<String>,
    #[serde(default)]
    pub clear_kick_client_secret: bool,
    pub kick_channel: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TargetForm {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub stream_key: Option<String>,
    pub public_url: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct ConfigForm {
    pub server: Option<ServerForm>,
    pub web_auth: Option<WebAuthForm>,
    pub chat: Option<ChatForm>,
    pub notifications: Option<NotificationsForm>,
    pub targets: Option<Vec<TargetForm>>,
    pub action: Option<String>,
    pub return_to: Option<String>,
}

impl ConfigForm {
    pub fn is_empty(&self) -> bool {
        self.server.is_none()
            && self.web_auth.is_none()
            && self.chat.is_none()
            && self.notifications.is_none()
            && self.targets.is_none()
    }
}

#[derive(Debug, toasty::Model)]
#[table = "app_config"]
struct StoredConfig {
    #[key]
    id: i64,
    data: String,
}

/// Toasty-backed SQLite configuration storage.
#[derive(Clone)]
pub struct ConfigStore {
    path: PathBuf,
    database: toasty::Db,
}

impl ConfigStore {
    pub async fn open<P: AsRef<Path>>(config_path: P) -> Result<(Self, AppConfig)> {
        let config_path = config_path.as_ref();
        let is_json = config_path.extension().is_some_and(|ext| ext == "json");
        let database_path = if is_json {
            config_path.with_extension("sqlite3")
        } else {
            config_path.to_path_buf()
        };
        let database_exists = database_path.exists();
        let database = toasty::Db::builder()
            .models(toasty::models!(StoredConfig))
            .connect(&format!("sqlite:{}", database_path.display()))
            .await
            .with_context(|| {
                format!(
                    "Failed to open config database '{}'",
                    database_path.display()
                )
            })?;
        #[cfg(unix)]
        if !database_exists {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&database_path, fs::Permissions::from_mode(0o600)).with_context(
                || {
                    format!(
                        "Failed to secure config database '{}'",
                        database_path.display()
                    )
                },
            )?;
        }
        let store = Self {
            path: database_path,
            database,
        };
        if !database_exists {
            store.database.push_schema().await?;
        }
        let config = match store.load().await? {
            Some(config) => config,
            None => {
                let config = AppConfig::default();
                store.save(&config).await?;
                config
            }
        };

        Ok((store, config))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn load(&self) -> Result<Option<AppConfig>> {
        let mut database = self.database.clone();
        let record = StoredConfig::filter(StoredConfig::fields().id().eq(1_i64))
            .first()
            .exec(&mut database)
            .await?;
        record
            .map(|record| {
                serde_json::from_str(&record.data)
                    .context("Failed to deserialize configuration from Toasty")
            })
            .transpose()
    }

    pub async fn save(&self, config: &AppConfig) -> Result<()> {
        config.validate()?;
        let data = serde_json::to_string(config)?;
        let mut database = self.database.clone();
        if let Some(mut record) = StoredConfig::filter(StoredConfig::fields().id().eq(1_i64))
            .first()
            .exec(&mut database)
            .await?
        {
            record.update().data(data).exec(&mut database).await?;
        } else {
            toasty::create!(StoredConfig { id: 1, data })
                .exec(&mut database)
                .await?;
        }
        Ok(())
    }
}

/// Lightweight, cloneable handle managing live configuration broadcasts and persistent SQLite storage.
#[derive(Clone)]
pub struct ConfigHandle {
    store: ConfigStore,
    current: watch::Sender<Arc<AppConfig>>,
    update_lock: Arc<Mutex<()>>,
}

impl ConfigHandle {
    pub async fn open<P: AsRef<Path>>(config_path: P) -> Result<(Self, Arc<AppConfig>)> {
        let (store, config) = ConfigStore::open(config_path).await?;
        config.validate()?;
        let config_arc = Arc::new(config);
        let (current, _) = watch::channel(Arc::clone(&config_arc));
        Ok((
            Self {
                store,
                current,
                update_lock: Arc::new(Mutex::new(())),
            },
            config_arc,
        ))
    }

    /// Returns the current configuration snapshot with zero lock contention.
    pub fn get(&self) -> Arc<AppConfig> {
        Arc::clone(&self.current.borrow())
    }

    /// Subscribes to live configuration change events.
    pub fn subscribe(&self) -> watch::Receiver<Arc<AppConfig>> {
        self.current.subscribe()
    }

    /// Merges form updates, validates, persists to SQLite, and broadcasts the updated config.
    /// Returns `(new_config, changed, chat_changed)`.
    pub async fn save_form(&self, form: ConfigForm) -> Result<(Arc<AppConfig>, bool, bool)> {
        let _guard = self.update_lock.lock().await;
        let current_config = self.get();
        let updated = current_config.merge_form(form)?;
        self.save_updated(current_config, updated).await
    }

    pub async fn complete_setup(&self, updated: AppConfig) -> Result<Arc<AppConfig>> {
        let _guard = self.update_lock.lock().await;
        let current_config = self.get();
        let (config, _, _) = self.save_updated(current_config, updated).await?;
        Ok(config)
    }

    pub async fn set_youtube_polling(&self, enabled: bool) -> Result<(Arc<AppConfig>, bool, bool)> {
        let _guard = self.update_lock.lock().await;
        let current_config = self.get();
        let mut updated = (*current_config).clone();
        updated.chat.youtube_polling_enabled = enabled;
        self.save_updated(current_config, updated).await
    }

    pub async fn set_x_webhook(&self, enabled: bool) -> Result<(Arc<AppConfig>, bool, bool)> {
        let _guard = self.update_lock.lock().await;
        let current_config = self.get();
        let mut updated = (*current_config).clone();
        updated.chat.x_webhook_enabled = enabled;
        self.save_updated(current_config, updated).await
    }

    pub async fn set_kick_webhook(&self, enabled: bool) -> Result<(Arc<AppConfig>, bool, bool)> {
        let _guard = self.update_lock.lock().await;
        let current_config = self.get();
        let mut updated = (*current_config).clone();
        updated.chat.kick_webhook_enabled = enabled;
        self.save_updated(current_config, updated).await
    }

    async fn save_updated(
        &self,
        current_config: Arc<AppConfig>,
        updated: AppConfig,
    ) -> Result<(Arc<AppConfig>, bool, bool)> {
        if let Err(error) = updated.validate() {
            bail!(error);
        }

        let changed = updated != *current_config;
        let chat_changed = updated.chat != current_config.chat;

        if changed {
            self.store.save(&updated).await?;
            let new_arc = Arc::new(updated);
            self.current.send_replace(Arc::clone(&new_arc));
            Ok((new_arc, true, chat_changed))
        } else {
            Ok((current_config, false, false))
        }
    }

    /// Parses imported JSON configuration, persists it, and broadcasts the new config.
    pub async fn import(&self, body: &[u8]) -> Result<(Arc<AppConfig>, bool, bool)> {
        let _guard = self.update_lock.lock().await;
        let imported = AppConfig::parse_imported(body)?;
        let current_config = self.get();
        self.save_updated(current_config, imported).await
    }

    pub fn path(&self) -> &Path {
        self.store.path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn populated_config() -> AppConfig {
        AppConfig {
            initialized: true,
            server: ServerSettings {
                listen: "0.0.0.0:1935".parse().unwrap(),
                api_listen: "10.0.0.1:3000".parse().unwrap(),
                test_stream_duration_secs: 15,
                ingest_stream_key: "existing-ingest-key".into(),
            },
            notifications: NotificationSettings {
                discord_webhook: Some("https://discord.test/hook".into()),
                live_message: "Still live".into(),
                webhook_url: Some("https://example.test/hook".into()),
            },
            targets: vec![TargetConfig {
                name: "Twitch".into(),
                url: "rtmps://example.test/app".into(),
                stream_key: "secret".into(),
                public_url: Some("https://example.test/watch".into()),
                enabled: true,
            }],
            web_auth: WebAuthSettings {
                username: "operator".into(),
                password: "correct horse battery staple".into(),
            },
            chat: ChatSettings {
                twitch_channel: Some("streamer".into()),
                youtube_api_key: Some("youtube-api-key".into()),
                youtube_polling_enabled: true,
                x_api_key: Some("x-api-key".into()),
                x_api_secret: Some("x-api-secret".into()),
                x_client_id: Some("x-client-id".into()),
                x_client_secret: Some("x-client-secret".into()),
                kick_client_id: Some("kick-client-id".into()),
                kick_client_secret: Some("kick-client-secret".into()),
                kick_channel: Some("streamer".into()),
                ..ChatSettings::default()
            },
        }
    }

    #[test]
    fn rejects_unsafe_web_and_chat_credentials() {
        let mut config = AppConfig::default();
        config.web_auth.username = "operator:name".into();
        config.web_auth.password = "correct horse battery staple".into();
        assert!(config.validate().unwrap_err().to_string().contains("':'"));

        config.web_auth.username = "operator".into();
        config.chat.twitch_channel = Some("invalid-channel!".into());
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("letters, numbers, or underscores")
        );

        config.chat.twitch_channel = None;
        config.server.ingest_stream_key = "unsafe/key".into();
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("letters, numbers, hyphens, or underscores")
        );
    }

    #[tokio::test]
    async fn stores_and_round_trips_config_with_toasty() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("rtmp-proxy-config-{unique}"));
        fs::create_dir(&directory).unwrap();
        let database_path = directory.join("config.sqlite3");
        let (store, mut config) = ConfigStore::open(&database_path).await.unwrap();
        config.server.test_stream_duration_secs = 30;
        config.server.ingest_stream_key = "new-ingest-key".into();
        config.notifications.live_message = "saved in sqlite".into();
        config.web_auth = WebAuthSettings {
            username: "operator".into(),
            password: "correct horse battery staple".into(),
        };
        config.chat.youtube_video_id = Some("video-id".into());
        config.chat.youtube_api_key = Some("api-key".into());
        store.save(&config).await.unwrap();

        let (_, reloaded) = ConfigStore::open(&database_path).await.unwrap();
        assert_eq!(reloaded.server.test_stream_duration_secs, 30);
        assert_eq!(reloaded.server.ingest_stream_key, "new-ingest-key");
        assert_eq!(reloaded.notifications.live_message, "saved in sqlite");
        assert_eq!(reloaded.web_auth.username, "operator");
        assert_eq!(reloaded.web_auth.password, "correct horse battery staple");
        assert_eq!(reloaded.chat.youtube_video_id.as_deref(), Some("video-id"));

        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn config_handle_broadcasts_live_updates() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("rtmp-proxy-handle-{unique}"));
        fs::create_dir(&directory).unwrap();
        let database_path = directory.join("config.sqlite3");
        let (handle, _) = ConfigHandle::open(&database_path).await.unwrap();
        let mut rx = handle.subscribe();

        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("server%5Btest_stream_duration_secs%5D=42&action=save")
            .unwrap();
        handle.save_form(form).await.unwrap();

        rx.changed().await.unwrap();
        assert_eq!(rx.borrow().server.test_stream_duration_secs, 42);
        assert_eq!(handle.get().server.test_stream_duration_secs, 42);

        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn concurrent_ingest_updates_do_not_overwrite_each_other() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("rtmp-proxy-polling-{unique}"));
        fs::create_dir(&directory).unwrap();
        let database_path = directory.join("config.sqlite3");
        let (handle, _) = ConfigHandle::open(&database_path).await.unwrap();
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "chat%5Bkick_client_id%5D=client-id&\
                 chat%5Bkick_client_secret%5D=client-secret&\
                 chat%5Bkick_channel%5D=streamer&action=save",
            )
            .unwrap();
        handle.save_form(form).await.unwrap();

        let youtube = {
            let handle = handle.clone();
            tokio::spawn(async move { handle.set_youtube_polling(true).await.unwrap() })
        };
        let x = {
            let handle = handle.clone();
            tokio::spawn(async move { handle.set_x_webhook(true).await.unwrap() })
        };
        let kick = {
            let handle = handle.clone();
            tokio::spawn(async move { handle.set_kick_webhook(true).await.unwrap() })
        };
        youtube.await.unwrap();
        x.await.unwrap();
        kick.await.unwrap();

        let config = handle.get();
        assert!(config.chat.youtube_polling_enabled);
        assert!(config.chat.x_webhook_enabled);
        assert!(config.chat.kick_webhook_enabled);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn partial_form_update_preserves_every_omitted_field() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("server%5Blisten%5D=127.0.0.1%3A1936&action=save")
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert_eq!(updated.server.listen, "127.0.0.1:1936".parse().unwrap());
        assert_eq!(updated.server.api_listen, "10.0.0.1:3000".parse().unwrap());
        assert_eq!(updated.server.test_stream_duration_secs, 15);
        assert_eq!(updated.server.ingest_stream_key, "existing-ingest-key");
        assert_eq!(updated.notifications.live_message, "Still live");
        assert_eq!(updated.targets.len(), 1);
        assert_eq!(updated.targets[0].stream_key, "secret");
        assert_eq!(updated.web_auth.password, "correct horse battery staple");
    }

    #[test]
    fn test_stream_duration_is_configurable() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("server%5Btest_stream_duration_secs%5D=30&action=save")
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert_eq!(updated.server.test_stream_duration_secs, 30);
        updated.validate().unwrap();
    }

    #[test]
    fn blank_ingest_stream_key_preserves_existing_key() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("server%5Bingest_stream_key%5D=")
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert_eq!(updated.server.ingest_stream_key, "existing-ingest-key");
    }

    #[test]
    fn ingest_stream_key_is_configurable() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("server%5Bingest_stream_key%5D=new-private-key")
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert_eq!(updated.server.ingest_stream_key, "new-private-key");
    }

    #[test]
    fn unchecked_target_decodes_as_disabled() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "targets%5B0%5D%5Bname%5D=Twitch&\
                 targets%5B0%5D%5Burl%5D=rtmps%3A%2F%2Fexample.test%2Fapp&\
                 targets%5B0%5D%5Bstream_key%5D=secret&action=save",
            )
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert!(!updated.targets[0].enabled);
    }

    #[test]
    fn checked_target_decodes_as_enabled() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "targets%5B0%5D%5Bname%5D=Twitch&\
                 targets%5B0%5D%5Burl%5D=rtmps%3A%2F%2Fexample.test%2Fapp&\
                 targets%5B0%5D%5Bstream_key%5D=secret&\
                 targets%5B0%5D%5Benabled%5D=true&action=save",
            )
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert!(updated.targets[0].enabled);
        assert_eq!(updated.targets[0].stream_key, "secret");
    }

    #[test]
    fn blank_secret_fields_preserve_existing_secrets() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "notifications%5Blive_message%5D=Updated&\
                 notifications%5Bdiscord_webhook%5D=&\
                 notifications%5Bwebhook_url%5D=&\
                 targets%5B0%5D%5Bname%5D=Twitch&\
                 targets%5B0%5D%5Burl%5D=rtmps%3A%2F%2Fexample.test%2Fapp&\
                 targets%5B0%5D%5Bstream_key%5D=&action=save",
            )
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert_eq!(updated.targets[0].stream_key, "secret");
        assert_eq!(
            updated.notifications.discord_webhook.as_deref(),
            Some("https://discord.test/hook")
        );
        assert_eq!(
            updated.notifications.webhook_url.as_deref(),
            Some("https://example.test/hook")
        );
    }

    #[test]
    fn explicit_clear_removes_webhook_credentials() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "notifications%5Blive_message%5D=Updated&\
                 notifications%5Bclear_discord_webhook%5D=true&\
                 notifications%5Bclear_webhook_url%5D=true&action=save",
            )
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert!(updated.notifications.discord_webhook.is_none());
        assert!(updated.notifications.webhook_url.is_none());
    }

    #[test]
    fn explicit_clear_removes_x_credentials() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "chat%5Bclear_x_api_key%5D=true&\
                 chat%5Bclear_x_api_secret%5D=true&\
                 chat%5Bclear_x_client_id%5D=true&\
                 chat%5Bclear_x_client_secret%5D=true&action=save",
            )
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert!(updated.chat.x_api_key.is_none());
        assert!(updated.chat.x_api_secret.is_none());
        assert!(updated.chat.x_client_id.is_none());
        assert!(updated.chat.x_client_secret.is_none());
    }

    #[test]
    fn explicit_clear_removes_kick_client_secret() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("chat%5Bclear_kick_client_secret%5D=true&action=save")
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert!(updated.chat.kick_client_secret.is_none());
    }

    #[test]
    fn enabled_kick_webhooks_require_app_credentials_and_channel() {
        let mut config = AppConfig::default();
        config.chat.kick_webhook_enabled = true;
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("Kick webhooks require")
        );

        config.chat.kick_client_id = Some("client-id".into());
        config.chat.kick_client_secret = Some("client-secret".into());
        config.chat.kick_channel = Some("streamer".into());
        config.validate().unwrap();
    }

    #[test]
    fn kick_channel_is_normalized_from_the_settings_form() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("chat%5Bkick_channel%5D=Streamer_Name&action=save")
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert_eq!(updated.chat.kick_channel.as_deref(), Some("streamer_name"));
    }

    #[test]
    fn blank_web_and_chat_secrets_preserve_existing_credentials() {
        let form: ConfigForm = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(
                "web_auth%5Busername%5D=operator&web_auth%5Bpassword%5D=&\
                 chat%5Btwitch_channel%5D=streamer&\
                 chat%5Byoutube_api_key%5D=&chat%5Bqueue_capacity%5D=250&action=save",
            )
            .unwrap();
        let updated = populated_config().merge_form(form).unwrap();

        assert_eq!(updated.web_auth.password, "correct horse battery staple");
        assert_eq!(updated.chat.twitch_channel.as_deref(), Some("streamer"));
        assert_eq!(
            updated.chat.youtube_api_key.as_deref(),
            Some("youtube-api-key")
        );
        assert_eq!(updated.chat.x_api_secret.as_deref(), Some("x-api-secret"));
        assert_eq!(updated.chat.queue_capacity, 250);
    }

    #[test]
    fn query_mode_does_not_decode_browser_form_keys() {
        let form: ConfigForm =
            serde_qs::from_str("server%5Blisten%5D=127.0.0.1%3A1936&action=save").unwrap();

        assert!(form.server.is_none());
    }

    #[test]
    fn exported_config_json_can_be_imported_without_losing_secrets() {
        let original = populated_config();
        let json = serde_json::to_vec_pretty(&original).unwrap();
        let imported = AppConfig::parse_imported(&json).unwrap();

        assert_eq!(imported.server.api_listen, original.server.api_listen);
        assert_eq!(
            imported.notifications.discord_webhook,
            original.notifications.discord_webhook
        );
        assert_eq!(imported.targets[0].stream_key, "secret");
        assert!(imported.targets[0].enabled);
    }

    #[test]
    fn import_rejects_incomplete_or_invalid_configs() {
        let incomplete = br#"{"server": {}, "targets": []}"#;
        assert!(
            AppConfig::parse_imported(incomplete)
                .unwrap_err()
                .to_string()
                .contains("notifications")
        );

        let legacy_export = br#"{"server": {}, "notifications": {}, "targets": []}"#;
        assert!(
            AppConfig::parse_imported(legacy_export)
                .unwrap_err()
                .to_string()
                .contains("web_auth")
        );

        let invalid_target = br#"{
            "server": {},
            "notifications": {},
            "web_auth": {},
            "chat": {},
            "targets": [{
                "name": "Twitch",
                "url": "https://example.test/app",
                "stream_key": "secret",
                "public_url": null,
                "enabled": true
            }]
        }"#;
        assert!(
            AppConfig::parse_imported(invalid_target)
                .unwrap_err()
                .to_string()
                .contains("invalid URL")
        );
    }
}
