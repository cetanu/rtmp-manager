use crate::config::{ChatSettings, NotificationSettings, OverlaySettings, TargetConfig};
use crate::database::Database;
use crate::util::stream_key_digest;
use anyhow::{Context, Result, bail};
use sqlx::Row;

pub const DEFAULT_TENANT_ID: &str = "default";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TenantId(String);

impl TenantId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            bail!("Tenant ID must contain between 1 and 128 characters");
        }
        Ok(Self(value))
    }

    pub fn default_tenant() -> Self {
        Self(DEFAULT_TENANT_ID.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct Tenant {
    pub id: TenantId,
    pub name: String,
    pub active: bool,
    pub max_concurrent_streams: usize,
    pub notifications: NotificationSettings,
    pub chat: ChatSettings,
    pub overlay: OverlaySettings,
    pub targets: Vec<TargetConfig>,
}

pub struct TenantDefinition<'a> {
    pub id: &'a TenantId,
    pub name: &'a str,
    pub stream_key: &'a str,
    pub active: bool,
    pub max_concurrent_streams: usize,
    pub notifications: &'a NotificationSettings,
    pub chat: &'a ChatSettings,
    pub overlay: &'a OverlaySettings,
    pub targets: &'a [TargetConfig],
}

#[derive(Clone)]
pub struct TenantRepository {
    database: Database,
}

impl TenantRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn authenticate(&self, stream_key: &str) -> Result<Option<Tenant>> {
        if stream_key.is_empty() {
            return Ok(None);
        }
        let digest = stream_key_digest(stream_key);
        let tenant_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM tenants \
             WHERE stream_key_digest = $1 AND active = 1",
        )
        .bind(digest)
        .fetch_optional(self.database.pool())
        .await?;
        match tenant_id {
            Some(tenant_id) => self.find(&TenantId::new(tenant_id)?).await,
            None => Ok(None),
        }
    }

    pub async fn authenticate_overlay(&self, overlay_key: &str) -> Result<Option<Tenant>> {
        if overlay_key.is_empty() {
            return Ok(None);
        }
        let tenant_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM tenants WHERE overlay_key_digest = $1 AND active = 1",
        )
        .bind(stream_key_digest(overlay_key))
        .fetch_optional(self.database.pool())
        .await?;
        match tenant_id {
            Some(tenant_id) => self.find(&TenantId::new(tenant_id)?).await,
            None => Ok(None),
        }
    }

    pub async fn find(&self, tenant_id: &TenantId) -> Result<Option<Tenant>> {
        let row = sqlx::query(
            "SELECT name, active, max_concurrent_streams, notifications, chat, overlay \
             FROM tenants WHERE id = $1",
        )
        .bind(tenant_id.as_str())
        .fetch_optional(self.database.pool())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let active: i64 = row.try_get("active")?;
        let max_concurrent_streams: i64 = row.try_get("max_concurrent_streams")?;
        let notifications: String =
            crate::crypto::decrypt(&row.try_get::<String, _>("notifications")?)?;
        let chat: String = crate::crypto::decrypt(&row.try_get::<String, _>("chat")?)?;
        let overlay: String = crate::crypto::decrypt(&row.try_get::<String, _>("overlay")?)?;
        let target_data: Vec<String> = sqlx::query_scalar(
            "SELECT config FROM tenant_targets WHERE tenant_id = $1 ORDER BY position",
        )
        .bind(tenant_id.as_str())
        .fetch_all(self.database.pool())
        .await?;

        Ok(Some(Tenant {
            id: tenant_id.clone(),
            name: row.try_get("name")?,
            active: active != 0,
            max_concurrent_streams: usize::try_from(max_concurrent_streams)
                .context("Tenant concurrency limit is invalid")?,
            notifications: serde_json::from_str(&notifications)
                .context("Tenant notification settings are invalid")?,
            chat: serde_json::from_str(&chat).context("Tenant chat settings are invalid")?,
            overlay: serde_json::from_str(&overlay)
                .context("Tenant overlay settings are invalid")?,
            targets: target_data
                .into_iter()
                .map(|target| {
                    let target = crate::crypto::decrypt(&target)?;
                    serde_json::from_str(&target).context("Tenant target configuration is invalid")
                })
                .collect::<Result<_>>()?,
        }))
    }

    pub async fn save(&self, definition: TenantDefinition<'_>) -> Result<()> {
        if definition.max_concurrent_streams == 0 {
            bail!("Tenant concurrency limit must be positive");
        }
        let ingest_digest =
            (!definition.stream_key.is_empty()).then(|| stream_key_digest(definition.stream_key));
        let notifications =
            crate::crypto::encrypt(&serde_json::to_string(definition.notifications)?)?;
        let mut transaction = self.database.pool().begin().await?;
        sqlx::query(
            "INSERT INTO tenants \
             (id, name, stream_key_digest, active, max_concurrent_streams, notifications, chat, overlay, overlay_key_digest) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT (id) DO UPDATE SET \
                name = excluded.name, \
                stream_key_digest = excluded.stream_key_digest, \
                active = excluded.active, \
                max_concurrent_streams = excluded.max_concurrent_streams, \
                notifications = excluded.notifications, \
                chat = excluded.chat, \
                overlay = excluded.overlay, \
                overlay_key_digest = excluded.overlay_key_digest",
        )
        .bind(definition.id.as_str())
        .bind(definition.name)
        .bind(ingest_digest)
        .bind(i64::from(definition.active))
        .bind(i64::try_from(definition.max_concurrent_streams)?)
        .bind(notifications)
        .bind(crate::crypto::encrypt(&serde_json::to_string(definition.chat)?)?)
        .bind(crate::crypto::encrypt(&serde_json::to_string(definition.overlay)?)?)
        .bind(
            (!definition.overlay.key.is_empty())
                .then(|| stream_key_digest(&definition.overlay.key)),
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM tenant_targets WHERE tenant_id = $1")
            .bind(definition.id.as_str())
            .execute(&mut *transaction)
            .await?;
        for (position, target) in definition.targets.iter().enumerate() {
            let target_id = format!("{}:{position}", definition.id.as_str());
            sqlx::query(
                "INSERT INTO tenant_targets (id, tenant_id, position, config) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(target_id)
            .bind(definition.id.as_str())
            .bind(i64::try_from(position)?)
            .bind(crate::crypto::encrypt(&serde_json::to_string(target)?)?)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn update_configuration(
        &self,
        tenant_id: &TenantId,
        notifications: &NotificationSettings,
        chat: &ChatSettings,
        overlay: &OverlaySettings,
        targets: &[TargetConfig],
    ) -> Result<()> {
        let mut transaction = self.database.pool().begin().await?;
        let updated = sqlx::query(
            "UPDATE tenants SET notifications = $1, chat = $2, overlay = $3, overlay_key_digest = $4 WHERE id = $5",
        )
        .bind(crate::crypto::encrypt(&serde_json::to_string(notifications)?)?)
        .bind(crate::crypto::encrypt(&serde_json::to_string(chat)?)?)
        .bind(crate::crypto::encrypt(&serde_json::to_string(overlay)?)?)
        .bind((!overlay.key.is_empty()).then(|| stream_key_digest(&overlay.key)))
        .bind(tenant_id.as_str())
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            bail!("Tenant does not exist");
        }
        sqlx::query("DELETE FROM tenant_targets WHERE tenant_id = $1")
            .bind(tenant_id.as_str())
            .execute(&mut *transaction)
            .await?;
        for (position, target) in targets.iter().enumerate() {
            sqlx::query(
                "INSERT INTO tenant_targets (id, tenant_id, position, config) VALUES ($1, $2, $3, $4)",
            )
            .bind(format!("{}:{position}", tenant_id.as_str()))
            .bind(tenant_id.as_str())
            .bind(i64::try_from(position)?)
            .bind(crate::crypto::encrypt(&serde_json::to_string(target)?)?)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database() -> (Database, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "rtmp-manager-tenants-{}-{}.sqlite3",
            std::process::id(),
            crate::util::now_unix_ms()
        ));
        let database = Database::connect(&crate::database::sqlite_url(&path))
            .await
            .unwrap();
        (database, path)
    }

    fn target(name: &str, stream_key: &str) -> TargetConfig {
        TargetConfig {
            name: name.to_owned(),
            url: format!("rtmps://{name}.example/live"),
            stream_key: stream_key.to_owned(),
            public_url: None,
            enabled: true,
            encoding: crate::config::EncodingProfile::default(),
        }
    }

    #[tokio::test]
    async fn authenticates_tenants_without_storing_plaintext_ingest_keys() {
        let (database, path) = database().await;
        let repository = TenantRepository::new(database.clone());
        let alpha = TenantId::new("alpha").unwrap();
        let beta = TenantId::new("beta").unwrap();
        repository
            .save(TenantDefinition {
                id: &alpha,
                name: "Alpha",
                stream_key: "alpha-private-key",
                active: true,
                max_concurrent_streams: 1,
                notifications: &NotificationSettings::default(),
                chat: &ChatSettings::default(),
                overlay: &OverlaySettings::default(),
                targets: &[target("alpha", "alpha-destination")],
            })
            .await
            .unwrap();
        repository
            .save(TenantDefinition {
                id: &beta,
                name: "Beta",
                stream_key: "beta-private-key",
                active: true,
                max_concurrent_streams: 1,
                notifications: &NotificationSettings::default(),
                chat: &ChatSettings::default(),
                overlay: &OverlaySettings::default(),
                targets: &[target("beta", "beta-destination")],
            })
            .await
            .unwrap();

        let alpha = repository
            .authenticate("alpha-private-key")
            .await
            .unwrap()
            .unwrap();
        let beta = repository
            .authenticate("beta-private-key")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(alpha.id.as_str(), "alpha");
        assert_eq!(alpha.targets[0].stream_key, "alpha-destination");
        assert_eq!(beta.id.as_str(), "beta");
        assert_eq!(beta.targets[0].stream_key, "beta-destination");
        assert!(
            repository
                .authenticate("wrong-key")
                .await
                .unwrap()
                .is_none()
        );
        let stored: Vec<String> =
            sqlx::query_scalar("SELECT stream_key_digest FROM tenants ORDER BY id")
                .fetch_all(database.pool())
                .await
                .unwrap();
        assert!(!stored.iter().any(|value| value.contains("private-key")));

        drop(repository);
        drop(database);
        std::fs::remove_file(path).unwrap();
    }
}
