use crate::database::Database;
use anyhow::Result;
use anyhow::bail;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;

type BillingHmac = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize)]
pub struct UsageSnapshot {
    pub plan: String,
    pub stream_seconds: i64,
    pub active_streams: i64,
    pub limit_seconds: Option<i64>,
}

#[derive(Clone)]
pub struct UsageRepository {
    database: Database,
}

impl UsageRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// Reserve a stream against the tenant's monthly allowance.
    pub async fn begin_stream(&self, tenant_id: &str, stream_id: &str, now: i64) -> Result<bool> {
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
        sqlx::query("INSERT INTO tenant_active_streams (tenant_id, stream_id, started_at) VALUES ($1, $2, $3) ON CONFLICT (stream_id) DO NOTHING")
            .bind(tenant_id).bind(stream_id).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn record_seconds(
        &self,
        tenant_id: &str,
        stream_id: &str,
        started: i64,
        ended: i64,
    ) -> Result<()> {
        let period = started - started.rem_euclid(30 * 24 * 60 * 60);
        let seconds = ended.saturating_sub(started);
        let mut tx = self.database.pool().begin().await?;
        sqlx::query("DELETE FROM tenant_active_streams WHERE tenant_id = $1 AND stream_id = $2")
            .bind(tenant_id)
            .bind(stream_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE tenant_usage SET stream_seconds = stream_seconds + $1 WHERE tenant_id = $2 AND period_start = $3")
            .bind(seconds).bind(tenant_id).bind(period).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_plan(&self, tenant_id: &str, plan: &str, now: i64) -> Result<()> {
        if !matches!(plan, "free" | "pro" | "enterprise") {
            bail!("Unsupported subscription plan");
        }
        let period = now - now.rem_euclid(30 * 24 * 60 * 60);
        sqlx::query("INSERT INTO tenant_usage (tenant_id, period_start, plan) VALUES ($1, $2, $3) ON CONFLICT (tenant_id, period_start) DO UPDATE SET plan = excluded.plan")
            .bind(tenant_id).bind(period).bind(plan).execute(self.database.pool()).await?;
        Ok(())
    }

    pub async fn current_usage(&self, tenant_id: &str, now: i64) -> Result<UsageSnapshot> {
        let period = now - now.rem_euclid(30 * 24 * 60 * 60);
        let row = sqlx::query_as::<_, (String, i64)>(
            "SELECT plan, stream_seconds FROM tenant_usage WHERE tenant_id = $1 AND period_start = $2",
        )
        .bind(tenant_id)
        .bind(period)
        .fetch_optional(self.database.pool())
        .await?;
        let (plan, stream_seconds) = row.unwrap_or_else(|| ("free".to_owned(), 0));
        let active_streams = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM tenant_active_streams WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(self.database.pool())
        .await?;
        let limit_seconds = match plan.as_str() {
            "pro" => Some(100 * 60 * 60),
            "enterprise" => None,
            _ => Some(10 * 60 * 60),
        };
        Ok(UsageSnapshot {
            plan,
            stream_seconds,
            active_streams,
            limit_seconds,
        })
    }

    pub fn verify_webhook(body: &[u8], signature: &str, secret: &str) -> bool {
        let Some(signature) = signature.strip_prefix("sha256=") else {
            return false;
        };
        let Ok(mut mac) = BillingHmac::new_from_slice(secret.as_bytes()) else {
            return false;
        };
        mac.update(body);
        let expected = hex::encode(mac.finalize().into_bytes());
        subtle::ConstantTimeEq::ct_eq(expected.as_bytes(), signature.as_bytes()).unwrap_u8() == 1
    }

    pub fn verify_stripe_signature(body: &[u8], header: &str, secret: &str, now: i64) -> bool {
        let mut timestamp = None;
        let mut signatures = Vec::new();
        for part in header.split(',') {
            let Some((key, value)) = part.trim().split_once('=') else {
                continue;
            };
            if key == "t" {
                timestamp = value.parse::<i64>().ok();
            }
            if key == "v1" {
                signatures.push(value);
            }
        }
        let Some(timestamp) = timestamp else {
            return false;
        };
        if (now - timestamp).abs() > 300 {
            return false;
        }
        let Ok(mut mac) = BillingHmac::new_from_slice(secret.as_bytes()) else {
            return false;
        };
        mac.update(format!("{timestamp}.").as_bytes());
        mac.update(body);
        let expected = hex::encode(mac.finalize().into_bytes());
        signatures.iter().any(|signature| {
            subtle::ConstantTimeEq::ct_eq(expected.as_bytes(), signature.as_bytes()).unwrap_u8()
                == 1
        })
    }

    pub fn verify_hex_signature(body: &[u8], signature: &str, secret: &str) -> bool {
        let Ok(mut mac) = BillingHmac::new_from_slice(secret.as_bytes()) else {
            return false;
        };
        mac.update(body);
        let expected = hex::encode(mac.finalize().into_bytes());
        subtle::ConstantTimeEq::ct_eq(expected.as_bytes(), signature.trim().as_bytes()).unwrap_u8()
            == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_provider_signatures_without_accepting_modified_payloads() {
        let body = br#"{"tenant_id":"tenant-a","plan":"pro"}"#;
        let mut mac = BillingHmac::new_from_slice(b"secret").unwrap();
        mac.update(body);
        let signature = hex::encode(mac.finalize().into_bytes());
        assert!(UsageRepository::verify_hex_signature(
            body, &signature, "secret"
        ));
        assert!(!UsageRepository::verify_hex_signature(
            b"tampered",
            &signature,
            "secret"
        ));
    }

    #[test]
    fn stripe_signatures_require_a_fresh_timestamp() {
        let body = br#"{"type":"customer.subscription.updated"}"#;
        let now = 1_700_000_000_i64;
        let timestamp = now - 30;
        let mut mac = BillingHmac::new_from_slice(b"stripe-secret").unwrap();
        mac.update(format!("{timestamp}.").as_bytes());
        mac.update(body);
        let header = format!(
            "t={timestamp},v1={}",
            hex::encode(mac.finalize().into_bytes())
        );
        assert!(UsageRepository::verify_stripe_signature(
            body,
            &header,
            "stripe-secret",
            now
        ));
        assert!(!UsageRepository::verify_stripe_signature(
            body,
            &header,
            "stripe-secret",
            now + 301
        ));
    }
}
