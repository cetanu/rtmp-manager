use anyhow::{Context, Result, bail};
use sqlx::{AnyPool, any::AnyPoolOptions};
use std::time::Duration;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Database {
    pool: AnyPool,
}

impl Database {
    pub async fn connect(url: &str) -> Result<Self> {
        if !url.starts_with("sqlite:") && !url.starts_with("postgres:") {
            bail!("DATABASE_URL must use sqlite: or postgres:");
        }
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new()
            .min_connections(1)
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Duration::from_secs(300))
            .connect(url)
            .await
            .with_context(|| "Failed to connect to DATABASE_URL")?;
        MIGRATOR
            .run(&pool)
            .await
            .context("Failed to apply database migrations")?;
        sqlx::query("SELECT 1")
            .execute(&pool)
            .await
            .context("Database health check failed")?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

#[cfg(test)]
pub fn sqlite_url(path: &std::path::Path) -> String {
    format!("sqlite://{}?mode=rwc", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connects_migrates_and_passes_health_check() {
        let path = std::env::temp_dir().join(format!(
            "rtmp-manager-database-{}-{}.sqlite3",
            std::process::id(),
            crate::util::now_unix_ms()
        ));
        let database = Database::connect(&sqlite_url(&path)).await.unwrap();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'app_config'",
        )
        .fetch_one(database.pool())
        .await
        .unwrap();
        assert_eq!(count, 1);
        drop(database);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn postgres_runs_migrations_and_bound_queries_when_configured() {
        let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
            return;
        };
        let database = Database::connect(&url).await.unwrap();
        sqlx::query(
            "INSERT INTO app_config (id, data) VALUES ($1, $2) \
             ON CONFLICT (id) DO UPDATE SET data = EXCLUDED.data",
        )
        .bind(1_i64)
        .bind("{}")
        .execute(database.pool())
        .await
        .unwrap();
        let data: String = sqlx::query_scalar("SELECT data FROM app_config WHERE id = $1")
            .bind(1_i64)
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(data, "{}");

        let repository = crate::tenant::TenantRepository::new(database);
        let tenant_id = crate::tenant::TenantId::new("postgres-test").unwrap();
        repository
            .save(crate::tenant::TenantDefinition {
                id: &tenant_id,
                name: "PostgreSQL test",
                stream_key: "postgres-private-key",
                active: true,
                max_concurrent_streams: 1,
                notifications: &crate::config::NotificationSettings::default(),
                chat: &crate::config::ChatSettings::default(),
                overlay: &crate::config::OverlaySettings::default(),
                targets: &[],
            })
            .await
            .unwrap();
        assert_eq!(
            repository
                .authenticate("postgres-private-key")
                .await
                .unwrap()
                .unwrap()
                .id,
            tenant_id
        );
    }
}
