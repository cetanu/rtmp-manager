//! Project-local migration CLI using the crate's compiled model types.
//!
//! ```sh
//! cargo run --bin migrate -- migration generate --name describe_change
//! cargo run --bin migrate -- migration apply
//! ```
//!
//! `CONFIG_PATH` selects the SQLite database.

use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = toasty_cli::Config::load()?;

    let db_path = std::env::var_os("CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.sqlite3"));
    let db_path = rtmp_proxy::config::ConfigStore::database_path(db_path)?;

    let db = rtmp_proxy::db::connect(&db_path).await?;

    toasty_cli::ToastyCli::with_config(db, config)
        .parse_and_run()
        .await?;

    Ok(())
}
