use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct ServerSettings {
    #[serde(default = "default_listen")]
    pub listen: SocketAddr,

    #[serde(default = "default_health_listen")]
    pub health_listen: SocketAddr,

    #[serde(default = "default_api_listen")]
    pub api_listen: SocketAddr,
}

fn default_listen() -> SocketAddr {
    "0.0.0.0:1935".parse().unwrap()
}

fn default_health_listen() -> SocketAddr {
    "127.0.0.1:8080".parse().unwrap()
}

fn default_api_listen() -> SocketAddr {
    "0.0.0.0:3000".parse().unwrap()
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            health_listen: default_health_listen(),
            api_listen: default_api_listen(),
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
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
pub struct WebAuthSettings {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct ChatSettings {
    #[serde(default)]
    pub ingest_token: Option<String>,
    #[serde(default = "default_chat_queue_capacity")]
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
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read configuration file: {:?}", path.as_ref()))?;
        let config: AppConfig = toml::from_str(&content).with_context(|| {
            format!(
                "Failed to parse TOML configuration from {:?}",
                path.as_ref()
            )
        })?;
        Ok(config)
    }

    /// Validate enabled target URLs (allows 0 enabled targets for ingest-only mode)
    pub fn validate(&self) -> Result<()> {
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
        if self.chat.queue_capacity == 0 {
            bail!("Chat queue capacity must be positive");
        }
        if self.chat.youtube_min_poll_interval_secs == 0 {
            bail!("YouTube minimum poll interval must be positive");
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

/// SQLite-backed configuration storage.
///
/// A `.toml` path is treated as a legacy bootstrap file. It is imported only
/// when the sibling `.sqlite3` database has no configuration yet.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    const STORAGE_VERSION: &'static str = "1";

    pub fn open<P: AsRef<Path>>(config_path: P) -> Result<(Self, AppConfig)> {
        let config_path = config_path.as_ref();
        let is_toml = config_path.extension().is_some_and(|ext| ext == "toml");
        let database_path = if is_toml {
            config_path.with_extension("sqlite3")
        } else {
            config_path.to_path_buf()
        };

        if is_toml && !config_path.exists() && !database_path.exists() {
            bail!(
                "Neither legacy configuration '{}' nor database '{}' exists",
                config_path.display(),
                database_path.display()
            );
        }

        let store = Self {
            path: database_path,
        };
        store.initialize()?;

        // Databases created by the broken form decoder have no storage-version
        // marker. Re-import the legacy TOML once when upgrading those databases,
        // then leave TOML untouched and use SQLite exclusively.
        let storage_version = store.metadata("storage_version")?;
        let needs_legacy_import = is_toml
            && config_path.exists()
            && storage_version.as_deref() != Some(Self::STORAGE_VERSION);

        let config = if needs_legacy_import {
            let config = AppConfig::load_from_file(config_path)?;
            config.validate()?;
            store.save(&config)?;
            config
        } else {
            match store.load()? {
                Some(config) => config,
                None if is_toml && config_path.exists() => {
                    let config = AppConfig::load_from_file(config_path)?;
                    config.validate()?;
                    store.save(&config)?;
                    config
                }
                None => {
                    let config = AppConfig::default();
                    store.save(&config)?;
                    config
                }
            }
        };
        if storage_version.as_deref() != Some(Self::STORAGE_VERSION) {
            store.set_metadata("storage_version", Self::STORAGE_VERSION)?;
        }

        Ok((store, config))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self) -> Result<Connection> {
        let database_exists = self.path.exists();
        let connection = Connection::open(&self.path)
            .with_context(|| format!("Failed to open config database '{}'", self.path.display()))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        #[cfg(unix)]
        if !database_exists {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600)).with_context(
                || format!("Failed to secure config database '{}'", self.path.display()),
            )?;
        }
        Ok(connection)
    }

    fn initialize(&self) -> Result<()> {
        let connection = self.connect()?;
        connection.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                server_listen TEXT NOT NULL,
                health_listen TEXT NOT NULL,
                api_listen TEXT NOT NULL,
                discord_webhook TEXT,
                live_message TEXT NOT NULL,
                webhook_url TEXT
            );
            CREATE TABLE IF NOT EXISTS targets (
                id INTEGER PRIMARY KEY,
                position INTEGER NOT NULL UNIQUE,
                name TEXT NOT NULL,
                url TEXT NOT NULL,
                stream_key TEXT NOT NULL,
                public_url TEXT,
                enabled INTEGER NOT NULL CHECK (enabled IN (0, 1))
            );
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS web_auth (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                username TEXT NOT NULL,
                password TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chat_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                ingest_token TEXT,
                queue_capacity INTEGER NOT NULL,
                twitch_channel TEXT,
                youtube_api_key TEXT,
                youtube_live_chat_id TEXT,
                youtube_video_id TEXT,
                youtube_channel_id TEXT,
                youtube_min_poll_interval_secs INTEGER NOT NULL,
                youtube_adaptive_polling INTEGER NOT NULL CHECK (youtube_adaptive_polling IN (0, 1))
            );
            ",
        )?;
        let has_twitch_channel: bool = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('chat_settings')
                WHERE name = 'twitch_channel'
             )",
            [],
            |row| row.get(0),
        )?;
        if !has_twitch_channel {
            connection.execute(
                "ALTER TABLE chat_settings ADD COLUMN twitch_channel TEXT",
                [],
            )?;
        }
        let has_eventsub_secret: bool = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('chat_settings')
                WHERE name = 'twitch_eventsub_secret'
             )",
            [],
            |row| row.get(0),
        )?;
        if has_eventsub_secret {
            connection.execute(
                "ALTER TABLE chat_settings DROP COLUMN twitch_eventsub_secret",
                [],
            )?;
        }
        Ok(())
    }

    fn metadata(&self, key: &str) -> Result<Option<String>> {
        let connection = self.connect()?;
        Ok(connection
            .query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO metadata (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn load(&self) -> Result<Option<AppConfig>> {
        let connection = self.connect()?;
        let settings = connection
            .query_row(
                "SELECT server_listen, health_listen, api_listen, discord_webhook,
                        live_message, webhook_url
                 FROM settings WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            server_listen,
            health_listen,
            api_listen,
            discord_webhook,
            live_message,
            webhook_url,
        )) = settings
        else {
            return Ok(None);
        };

        let mut statement = connection.prepare(
            "SELECT name, url, stream_key, public_url, enabled
             FROM targets ORDER BY position",
        )?;
        let targets = statement
            .query_map([], |row| {
                Ok(TargetConfig {
                    name: row.get(0)?,
                    url: row.get(1)?,
                    stream_key: row.get(2)?,
                    public_url: row.get(3)?,
                    enabled: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let web_auth = connection
            .query_row(
                "SELECT username, password FROM web_auth WHERE id = 1",
                [],
                |row| {
                    Ok(WebAuthSettings {
                        username: row.get(0)?,
                        password: row.get(1)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default();
        let chat = connection
            .query_row(
                "SELECT ingest_token, queue_capacity, twitch_channel,
                        youtube_api_key, youtube_live_chat_id, youtube_video_id,
                        youtube_channel_id, youtube_min_poll_interval_secs,
                        youtube_adaptive_polling
                 FROM chat_settings WHERE id = 1",
                [],
                |row| {
                    Ok(ChatSettings {
                        ingest_token: row.get(0)?,
                        queue_capacity: row.get::<_, i64>(1)? as usize,
                        twitch_channel: row.get(2)?,
                        youtube_api_key: row.get(3)?,
                        youtube_live_chat_id: row.get(4)?,
                        youtube_video_id: row.get(5)?,
                        youtube_channel_id: row.get(6)?,
                        youtube_min_poll_interval_secs: row.get::<_, i64>(7)? as u64,
                        youtube_adaptive_polling: row.get(8)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default();

        let config = AppConfig {
            server: ServerSettings {
                listen: server_listen
                    .parse()
                    .context("Invalid server listen address")?,
                health_listen: health_listen
                    .parse()
                    .context("Invalid health listen address")?,
                api_listen: api_listen.parse().context("Invalid API listen address")?,
            },
            notifications: NotificationSettings {
                discord_webhook,
                live_message,
                webhook_url,
            },
            targets,
            web_auth,
            chat,
        };
        Ok(Some(config))
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        config.validate()?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO settings (
                id, server_listen, health_listen, api_listen, discord_webhook,
                live_message, webhook_url
             ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                server_listen = excluded.server_listen,
                health_listen = excluded.health_listen,
                api_listen = excluded.api_listen,
                discord_webhook = excluded.discord_webhook,
                live_message = excluded.live_message,
                webhook_url = excluded.webhook_url",
            params![
                config.server.listen.to_string(),
                config.server.health_listen.to_string(),
                config.server.api_listen.to_string(),
                config.notifications.discord_webhook,
                config.notifications.live_message,
                config.notifications.webhook_url,
            ],
        )?;
        transaction.execute("DELETE FROM targets", [])?;
        for (position, target) in config.targets.iter().enumerate() {
            transaction.execute(
                "INSERT INTO targets (
                    position, name, url, stream_key, public_url, enabled
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    position as i64,
                    target.name,
                    target.url,
                    target.stream_key,
                    target.public_url,
                    target.enabled,
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO web_auth (id, username, password) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                username = excluded.username,
                password = excluded.password",
            params![config.web_auth.username, config.web_auth.password],
        )?;
        transaction.execute(
            "INSERT INTO chat_settings (
                id, ingest_token, queue_capacity, twitch_channel,
                youtube_api_key, youtube_live_chat_id, youtube_video_id,
                youtube_channel_id, youtube_min_poll_interval_secs,
                youtube_adaptive_polling
             ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                ingest_token = excluded.ingest_token,
                queue_capacity = excluded.queue_capacity,
                twitch_channel = excluded.twitch_channel,
                youtube_api_key = excluded.youtube_api_key,
                youtube_live_chat_id = excluded.youtube_live_chat_id,
                youtube_video_id = excluded.youtube_video_id,
                youtube_channel_id = excluded.youtube_channel_id,
                youtube_min_poll_interval_secs = excluded.youtube_min_poll_interval_secs,
                youtube_adaptive_polling = excluded.youtube_adaptive_polling",
            params![
                config.chat.ingest_token,
                config.chat.queue_capacity as i64,
                config.chat.twitch_channel,
                config.chat.youtube_api_key,
                config.chat.youtube_live_chat_id,
                config.chat.youtube_video_id,
                config.chat.youtube_channel_id,
                config.chat.youtube_min_poll_interval_secs as i64,
                config.chat.youtube_adaptive_polling,
            ],
        )?;
        transaction.commit()?;
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
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("at least 16"));

        config.chat.ingest_token = None;
        config.chat.twitch_channel = Some("invalid-channel!".into());
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("letters, numbers, or underscores"));
    }

    #[test]
    fn imports_legacy_toml_once_and_round_trips_sqlite() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("rtmp-proxy-config-{unique}"));
        fs::create_dir(&directory).unwrap();
        let toml_path = directory.join("config.toml");
        fs::write(
            &toml_path,
            r#"
                [server]
                api_listen = "127.0.0.1:3001"
                [notifications]
                live_message = "first"
                [[targets]]
                name = "Twitch"
                url = "rtmps://example.test/app"
                enabled = true
            "#,
        )
        .unwrap();

        let (store, mut config) = ConfigStore::open(&toml_path).unwrap();
        assert_eq!(config.server.api_listen.port(), 3001);
        assert_eq!(config.targets[0].stream_key, "");
        config.notifications.live_message = "saved in sqlite".into();
        config.web_auth = WebAuthSettings {
            username: "operator".into(),
            password: "correct horse battery staple".into(),
        };
        config.chat.ingest_token = Some("chat-token-long-enough".into());
        config.chat.youtube_video_id = Some("video-id".into());
        config.chat.youtube_api_key = Some("api-key".into());
        store.save(&config).unwrap();

        fs::write(
            &toml_path,
            "[notifications]\nlive_message = \"changed toml\"",
        )
        .unwrap();
        let (_, reloaded) = ConfigStore::open(&toml_path).unwrap();
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
