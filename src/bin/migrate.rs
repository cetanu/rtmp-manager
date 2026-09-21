//! Project-local migration CLI using the crate's compiled model types.
//!
//! ```sh
//! cargo run --bin migrate -- migration generate --name describe_change
//! cargo run --bin migrate -- migration apply
//! ```
//!
//! `CONFIG_PATH` selects the database, with `.json` paths mapped to a sibling
//! `.sqlite3` file.

use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = toasty_cli::Config::load()?;

    let config_path = std::env::var_os("CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.json"));
    let db_path = if config_path.extension().is_some_and(|ext| ext == "json") {
        config_path.with_extension("sqlite3")
    } else {
        config_path
    };

    let db = rtmp_proxy::db::connect(&db_path).await?;

    toasty_cli::ToastyCli::with_config(db, config)
        .parse_and_run()
        .await?;

    Ok(())
}
