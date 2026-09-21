//! Project-local migration CLI built from `toasty-cli`.
//!
//! The tool must live in this crate (not as a global command) because diffing
//! the schema needs the compiled model types. Run it from the crate root so it
//! finds `Toasty.toml`:
//!
//! ```sh
//! cargo run --bin migrate -- migration generate --name describe_change
//! cargo run --bin migrate -- migration apply
//! ```
//!
//! `generate` diffs the models against the stored snapshot and writes
//! incremental SQL under `toasty/`; `apply` runs pending files against the
//! database and records them in `__toasty_migrations`. The server binary
//! embeds the same files via `toasty::embed_migrations!` and applies them on
//! startup, so commit everything under `toasty/` alongside code changes.
//!
//! Which database the CLI touches is resolved exactly like the server does:
//! `CONFIG_PATH` (default `config.json`), where a `.json` suffix maps to the
//! sibling `.sqlite3` file.

use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Reads Toasty.toml ([migration] path = "toasty").
    let config = toasty_cli::Config::load()?;

    let config_path = std::env::var_os("CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.json"));
    let db_path = if config_path.extension().is_some_and(|ext| ext == "json") {
        config_path.with_extension("sqlite3")
    } else {
        config_path
    };

    // Same models and connection as the server, via the shared library, so
    // generation and runtime can never disagree on the schema.
    let db = rtmp_proxy::db::connect(&db_path).await?;

    toasty_cli::ToastyCli::with_config(db, config)
        .parse_and_run()
        .await?;

    Ok(())
}
