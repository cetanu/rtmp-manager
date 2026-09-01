use crate::database::Database;
use anyhow::Result;

#[derive(Clone)]
pub struct UsageRepository {
    database: Database,
}

impl UsageRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// Reserve a stream against the tenant's monthly allowance.
    pub async fn begin_stream(&self, tenant_id: &str, now: i64) -> Result<bool> {
        let period = now - now.rem_euclid(30 * 24 * 60 * 60);
        let mut tx = self.database.pool().begin().await?;
        sqlx::query("INSERT INTO tenant_usage (tenant_id, period_start) VALUES ($1, $2) ON CONFLICT (tenant_id, period_start) DO NOTHING")
            .bind(tenant_id).bind(period).execute(&mut *tx).await?;
        let used: i64 = sqlx::query_scalar(
            "SELECT stream_seconds FROM tenant_usage WHERE tenant_id = $1 AND period_start = $2",
        )
        .bind(tenant_id)
        .bind(period)
        .fetch_one(&mut *tx)
        .await?;
        let plan: String = sqlx::query_scalar(
            "SELECT plan FROM tenant_usage WHERE tenant_id = $1 AND period_start = $2",
        )
        .bind(tenant_id)
        .bind(period)
        .fetch_one(&mut *tx)
        .await?;
        let limit = match plan.as_str() {
            "pro" => 100 * 60 * 60,
            "enterprise" => i64::MAX,
            _ => 10 * 60 * 60,
        };
        if used >= limit {
            tx.rollback().await?;
            return Ok(false);
        }
        tx.commit().await?;
        Ok(true)
    }

    pub async fn record_seconds(&self, tenant_id: &str, started: i64, ended: i64) -> Result<()> {
        let period = started - started.rem_euclid(30 * 24 * 60 * 60);
        let seconds = ended.saturating_sub(started);
        sqlx::query("UPDATE tenant_usage SET stream_seconds = stream_seconds + $1 WHERE tenant_id = $2 AND period_start = $3")
            .bind(seconds).bind(tenant_id).bind(period).execute(self.database.pool()).await?;
        Ok(())
    }
}
