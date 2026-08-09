use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

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
    #[serde(default)]
    pub ingest_token: Option<String>,
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
            ingest_token: None,
            queue_capacity: default_chat_queue_capacity(),
            twitch_channel: None,
            youtube_api_key: None,
            youtube_live_chat_id: None,
            youtube_video_id: None,
            youtube_channel_id: None,
            youtube_min_poll_interval_secs: default_youtube_min_poll_interval_secs(),
            youtube_adaptive_polling: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
pub struct AppConfig {
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
    /// Validate enabled target URLs (allows 0 enabled targets for ingest-only mode)
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
        if self
            .chat
            .ingest_token
            .as_ref()
            .is_some_and(|token| !token.trim().is_empty() && token.len() < 16)
        {
            bail!("Chat ingest token must be at least 16 characters");
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
        if youtube_selectors > 0
            && self
                .chat
                .youtube_api_key
                .as_ref()
                .is_none_or(|key| key.trim().is_empty())
        {
            bail!("A YouTube API key is required when a YouTube chat selector is configured");
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
}

#[derive(Debug, toasty::Model)]
#[table = "app_config"]
struct StoredConfig {
    #[key]
    id: i64,
    data: String,
}

/// Toasty-backed SQLite configuration storage.
///
/// A `.json` path maps to a `.sqlite3` database path; the JSON file is not used
/// as storage.
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
        if is_json && !config_path.exists() && !database_path.exists() {
            bail!(
                "Configuration database '{}' (from '{}') does not exist",
                database_path.display(),
                config_path.display()
            );
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_unsafe_web_and_chat_credentials() {
        let mut config = AppConfig::default();
        config.web_auth.username = "operator:name".into();
        config.web_auth.password = "correct horse battery staple".into();
        assert!(config.validate().unwrap_err().to_string().contains("':'"));

        config.web_auth.username = "operator".into();
        config.chat.ingest_token = Some("too-short".into());
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("at least 16")
        );

        config.chat.ingest_token = None;
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
        config.chat.ingest_token = Some("chat-token-long-enough".into());
        config.chat.youtube_video_id = Some("video-id".into());
        config.chat.youtube_api_key = Some("api-key".into());
        store.save(&config).await.unwrap();

        let (_, reloaded) = ConfigStore::open(&database_path).await.unwrap();
        assert_eq!(reloaded.server.test_stream_duration_secs, 30);
        assert_eq!(reloaded.server.ingest_stream_key, "new-ingest-key");
        assert_eq!(reloaded.notifications.live_message, "saved in sqlite");
        assert_eq!(reloaded.web_auth.username, "operator");
        assert_eq!(reloaded.web_auth.password, "correct horse battery staple");
        assert_eq!(
            reloaded.chat.ingest_token.as_deref(),
            Some("chat-token-long-enough")
        );
        assert_eq!(reloaded.chat.youtube_video_id.as_deref(), Some("video-id"));

        fs::remove_dir_all(directory).unwrap();
    }
}
