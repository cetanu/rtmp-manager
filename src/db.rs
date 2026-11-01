//! Shared SQLite connection and migration management.
//!
//! Configuration and chat state use the same model registry and database file.
//! Startup applies embedded schema migrations.

use anyhow::{Context, Result};
use std::path::Path;

static MIGRATIONS: toasty::migration::MigrationSet = toasty::embed_migrations!();

/// Opens the shared SQLite database with the full model registry.
pub async fn connect(db_path: &Path) -> Result<toasty::Db> {
    toasty::Db::builder()
        .models(toasty::models!(crate::*))
        .connect(&format!("sqlite:{}", db_path.display()))
        .await
        .with_context(|| format!("Failed to open database '{}'", db_path.display()))
}

/// Applies pending embedded schema migrations.
pub async fn run_migrations(db: &toasty::Db) -> Result<toasty::migration::MigrationReport> {
    let mut conn = db.connection().await?;
    // Best-effort: avoid `database is locked` when both stores open the same
    // file with separate pools in quick succession.
    let _ = toasty::sql::statement("PRAGMA busy_timeout = 5000")
        .exec(&mut conn)
        .await;

    let report = MIGRATIONS
        .apply(db)
        .await
        .context("Failed to apply database migrations")?;
    tracing::info!(
        applied = report.applied(),
        skipped = report.skipped(),
        "Database migrations applied"
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_dir(prefix: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "rtmp-proxy-{prefix}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[tokio::test]
    async fn fresh_database_is_created_by_embedded_migrations() {
        let directory = unique_dir("migrations");
        let path = directory.join("fresh.sqlite3");
        let db = connect(&path).await.unwrap();

        let report = run_migrations(&db).await.unwrap();
        assert_eq!(report.applied(), MIGRATIONS.migrations().len());
        assert_eq!(report.skipped(), 0);

        let mut conn = db.connection().await.unwrap();
        let tables =
            toasty::sql::query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
                .exec(&mut conn)
                .await
                .unwrap();
        let tables = format!("{tables:?}");
        for expected in [
            "app_config",
            "chat_messages",
            "chat_seen",
            "chat_state",
            "chat_pomodoro",
            "chat_poll",
            "chat_poll_votes",
            "chat_automations",
            "chat_automation_data",
            "chat_automation_runs",
            "__toasty_migrations",
        ] {
            assert!(
                tables.contains(expected),
                "missing table {expected}: {tables}"
            );
        }

        let columns = toasty::sql::query("SELECT name FROM pragma_table_info('chat_messages')")
            .exec(&mut conn)
            .await
            .unwrap();
        assert!(
            format!("{columns:?}").contains("emoji_data"),
            "emoji_data column missing: {columns:?}"
        );

        let report = run_migrations(&db).await.unwrap();
        assert_eq!(report.applied(), 0);
        assert_eq!(report.skipped(), MIGRATIONS.migrations().len());

        std::fs::remove_dir_all(directory).unwrap();
    }
}
