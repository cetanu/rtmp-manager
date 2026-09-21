//! Shared SQLite connection and migration management.
//!
//! Configuration and chat state use the same model registry and database file.
//! Startup applies embedded migrations and bridges databases created by older
//! releases before the migration ledger existed.

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

/// Applies pending embedded migrations, including the legacy database bridge.
pub async fn run_migrations(db: &toasty::Db) -> Result<toasty::migration::MigrationReport> {
    let mut conn = db.connection().await?;
    // Best-effort: avoid `database is locked` when both stores open the same
    // file with separate pools in quick succession.
    let _ = toasty::sql::statement("PRAGMA busy_timeout = 5000")
        .exec(&mut conn)
        .await;

    match MIGRATIONS.apply(db).await {
        Ok(report) => {
            tracing::info!(
                applied = report.applied(),
                skipped = report.skipped(),
                "Database migrations applied"
            );
            Ok(report)
        }
        Err(error) if is_already_exists_error(&error) => {
            tracing::info!(
                "Database predates the migration ledger; running one-time legacy bridge"
            );
            tracing::debug!("Baseline migration skipped on legacy database: {error:#}");
            bridge_legacy_database(db).await?;
            let report = MIGRATIONS.apply(db).await?;
            tracing::info!(
                applied = report.applied(),
                skipped = report.skipped(),
                "Database migrations applied after legacy bridge"
            );
            Ok(report)
        }
        Err(error) => Err(error).context("Failed to apply database migrations"),
    }
}

/// Brings a pre-migration database up to the embedded baseline.
async fn bridge_legacy_database(db: &toasty::Db) -> Result<()> {
    let mut conn = db.connection().await?;

    // Evolved columns that `CREATE TABLE IF NOT EXISTS` cannot backfill on
    // tables created by older releases.
    add_column_if_missing(&mut conn, "chat_messages", "emoji_data TEXT").await?;

    // Safety net for partial databases (e.g. a store that never ran).
    for ddl in LEGACY_TABLE_DDL {
        toasty::sql::statement(*ddl)
            .exec(&mut conn)
            .await
            .with_context(|| format!("Failed legacy bridge DDL: {ddl}"))?;
    }

    // The schema now matches the embedded baseline; record it as applied.
    for migration in MIGRATIONS.migrations() {
        toasty::sql::statement(
            "INSERT OR IGNORE INTO __toasty_migrations (id, name, applied_at) VALUES (?1, ?2, datetime('now'))",
        )
        .bind(migration.id() as i64)
        .bind(migration.name())
        .exec(&mut conn)
        .await
        .with_context(|| format!("Failed to stamp migration {}", migration.name()))?;
    }

    Ok(())
}

/// Full current schema, used only as a safety net for partial legacy files.
/// Fresh databases are created by the embedded migrations, not by this.
const LEGACY_TABLE_DDL: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS "app_config" (
        "id" BIGINT NOT NULL,
        "data" TEXT NOT NULL,
        PRIMARY KEY ("id")
    )"#,
    r#"CREATE TABLE IF NOT EXISTS "chat_messages" (
        "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
        "source" TEXT NOT NULL,
        "external_id" TEXT NOT NULL,
        "author" TEXT NOT NULL,
        "text" TEXT NOT NULL,
        "emoji_data" TEXT,
        "avatar_url" TEXT,
        "sent_at" TEXT,
        "received_at_unix_ms" INTEGER NOT NULL
    )"#,
    r#"CREATE TABLE IF NOT EXISTS "chat_seen" (
        "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
        "source" TEXT NOT NULL,
        "external_id" TEXT NOT NULL
    )"#,
    r#"CREATE TABLE IF NOT EXISTS "chat_state" (
        "id" INTEGER NOT NULL,
        "dropped" INTEGER NOT NULL,
        PRIMARY KEY ("id")
    )"#,
    // Incremental migrations are stamped as applied by the legacy bridge
    // without running, so every table they create needs a safety-net DDL here.
    r#"CREATE TABLE IF NOT EXISTS "chat_pomodoro" (
        "id" INTEGER NOT NULL,
        "message" TEXT NOT NULL,
        "started_at_unix_ms" INTEGER NOT NULL,
        "ends_at_unix_ms" INTEGER NOT NULL,
        PRIMARY KEY ("id")
    )"#,
    r#"CREATE INDEX IF NOT EXISTS "index_chat_seen_by_source_and_external_id" ON "chat_seen" ("source", "external_id")"#,
];

async fn add_column_if_missing(
    executor: &mut dyn toasty::Executor,
    table: &str,
    column_ddl: &str,
) -> Result<()> {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column_ddl}");
    match toasty::sql::statement(&sql).exec(executor).await {
        Ok(_) => {
            tracing::info!(table, column_ddl, "Added missing column via legacy bridge");
            Ok(())
        }
        Err(error) if is_duplicate_column_error(&error) => Ok(()),
        Err(error) => Err(error).with_context(|| format!("Failed legacy bridge: {sql}")),
    }
}

fn is_already_exists_error(error: &toasty::Error) -> bool {
    error.to_string().to_lowercase().contains("already exists")
}

fn is_duplicate_column_error(error: &toasty::Error) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("duplicate column name") || message.contains("already exists")
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

    async fn open_raw(db_path: &Path) -> toasty::Db {
        toasty::Db::builder()
            .connect(&format!("sqlite:{}", db_path.display()))
            .await
            .unwrap()
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

    #[tokio::test]
    async fn pre_migration_database_is_bridged_without_data_loss() {
        let directory = unique_dir("migrations-legacy");
        let path = directory.join("legacy.sqlite3");

        {
            let raw = open_raw(&path).await;
            let mut conn = raw.connection().await.unwrap();
            for ddl in [
                r#"CREATE TABLE "app_config" (
                    "id" BIGINT NOT NULL,
                    "data" TEXT NOT NULL,
                    PRIMARY KEY ("id")
                )"#,
                r#"CREATE TABLE "chat_messages" (
                    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    "source" TEXT NOT NULL,
                    "external_id" TEXT NOT NULL,
                    "author" TEXT NOT NULL,
                    "text" TEXT NOT NULL,
                    "avatar_url" TEXT,
                    "sent_at" TEXT,
                    "received_at_unix_ms" INTEGER NOT NULL
                )"#,
                r#"CREATE TABLE "chat_seen" (
                    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    "source" TEXT NOT NULL,
                    "external_id" TEXT NOT NULL
                )"#,
                r#"CREATE TABLE "chat_state" (
                    "id" INTEGER NOT NULL,
                    "dropped" INTEGER NOT NULL,
                    PRIMARY KEY ("id")
                )"#,
            ] {
                toasty::sql::statement(ddl).exec(&mut conn).await.unwrap();
            }
            toasty::sql::statement(
                "INSERT INTO chat_messages (source, external_id, author, text, received_at_unix_ms) VALUES ('twitch', 'legacy-1', 'Viewer', 'hello', 1)",
            )
            .exec(&mut conn)
            .await
            .unwrap();
        }

        {
            let db = connect(&path).await.unwrap();
            run_migrations(&db).await.unwrap();
            let report = run_migrations(&db).await.unwrap();
            assert_eq!(report.applied(), 0);

            let mut conn = db.connection().await.unwrap();
            let rows = toasty::sql::query(
                "SELECT text, emoji_data FROM chat_messages WHERE external_id = 'legacy-1'",
            )
            .exec(&mut conn)
            .await
            .unwrap();
            assert_eq!(rows.len(), 1, "legacy chat row was lost: {rows:?}");

            let ledger = toasty::sql::query("SELECT id FROM __toasty_migrations")
                .exec(&mut conn)
                .await
                .unwrap();
            assert_eq!(ledger.len(), MIGRATIONS.migrations().len());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn legacy_shared_database_opens_through_both_stores() {
        let directory = unique_dir("migrations-shared");
        let path = directory.join("shared.sqlite3");

        {
            let raw = open_raw(&path).await;
            let mut conn = raw.connection().await.unwrap();
            for ddl in [
                r#"CREATE TABLE "app_config" (
                    "id" BIGINT NOT NULL,
                    "data" TEXT NOT NULL,
                    PRIMARY KEY ("id")
                )"#,
                r#"CREATE TABLE "chat_messages" (
                    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    "source" TEXT NOT NULL,
                    "external_id" TEXT NOT NULL,
                    "author" TEXT NOT NULL,
                    "text" TEXT NOT NULL,
                    "avatar_url" TEXT,
                    "sent_at" TEXT,
                    "received_at_unix_ms" INTEGER NOT NULL
                )"#,
                r#"CREATE TABLE "chat_seen" (
                    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    "source" TEXT NOT NULL,
                    "external_id" TEXT NOT NULL
                )"#,
                r#"CREATE TABLE "chat_state" (
                    "id" INTEGER NOT NULL,
                    "dropped" INTEGER NOT NULL,
                    PRIMARY KEY ("id")
                )"#,
            ] {
                toasty::sql::statement(ddl).exec(&mut conn).await.unwrap();
            }
        }

        let (store, _) = crate::config::ConfigStore::open(&path).await.unwrap();
        let mut inbox = crate::chat::ChatInbox::open(&path, 5).await.unwrap();
        inbox
            .enqueue(crate::chat::types::IncomingChatMessage {
                source: crate::chat::types::Source::Twitch,
                external_id: "shared-legacy-1".into(),
                author: "Viewer".into(),
                text: "hello".into(),
                parts: Vec::new(),
                avatar_url: None,
                sent_at: None,
            })
            .await
            .unwrap();
        assert_eq!(inbox.snapshot().await.unwrap().queued, 1);

        drop(inbox);
        let inbox = crate::chat::ChatInbox::open(&path, 5).await.unwrap();
        let (_, _) = crate::config::ConfigStore::open(&path).await.unwrap();
        assert_eq!(inbox.snapshot().await.unwrap().queued, 1);
        drop(store);

        std::fs::remove_dir_all(directory).unwrap();
    }
}
