mod chat;
mod config;
mod embedded_assets;
mod log_buffer;
mod metrics;
mod notifications;
mod server;
mod util;
mod web;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::ConfigStore;
use metrics::Metrics;
use server::{run_rtmp_server, state::ProxyState};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::prelude::*;

// Embed standalone systemd unit template at compile time
const SYSTEMD_UNIT_TEMPLATE: &str = include_str!("../systemd/rtmp-proxy.service");

#[derive(Parser, Debug)]
#[command(author, version, about = "RTMP Stream Multiplexer powered by rtmp-rs", long_about = None)]
struct CliArgs {
    /// Path to the SQLite database; a `.json` suffix maps to a `.sqlite3` path
    #[arg(short, long, env = "CONFIG_PATH", default_value = "config.json")]
    config: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    InstallSystemd {
        #[arg(long, default_value = "/opt/rtmp-proxy")]
        work_dir: PathBuf,

        #[arg(long, default_value = "/opt/rtmp-proxy/config.json")]
        config_path: PathBuf,
    },
}

fn install_systemd(work_dir: &Path, config_path: &Path) -> Result<()> {
    let current_exe =
        std::env::current_exe().context("Failed to determine path of current executable")?;

    let state_dir = config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(work_dir);
    let unit_content = SYSTEMD_UNIT_TEMPLATE
        .replace("{WORK_DIR}", &work_dir.display().to_string())
        .replace("{EXEC_START}", &current_exe.display().to_string())
        .replace("{CONFIG_PATH}", &config_path.display().to_string())
        .replace("{STATE_DIR}", &state_dir.display().to_string());

    let unit_path = Path::new("/etc/systemd/system/rtmp-proxy.service");
    fs::write(unit_path, unit_content)
        .with_context(|| format!("Failed to write systemd unit to {}", unit_path.display()))?;

    println!("Wrote systemd unit file to {}", unit_path.display());

    let status = std::process::Command::new("systemctl")
        .args(["daemon-reload"])
        .status();

    if let Ok(s) = status
        && s.success()
    {
        println!("Executed systemctl daemon-reload");
    }

    let enable_status = std::process::Command::new("systemctl")
        .args(["enable", "--now", "rtmp-proxy"])
        .status();

    if let Ok(s) = enable_status
        && s.success()
    {
        println!("Successfully enabled and started rtmp-proxy systemd service!");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let (_, log_layer) = log_buffer::init();
    let log_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "rtmp_proxy=info,rtmp_rs=off".into())
        .add_directive("rtmp_rs=off".parse().expect("valid log directive"));
    tracing_subscriber::registry()
        .with(log_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(log_layer)
        .init();

    let cli = CliArgs::parse();

    if let Some(Commands::InstallSystemd {
        work_dir,
        config_path,
    }) = cli.command
    {
        return install_systemd(&work_dir, &config_path);
    }

    embedded_assets::install(web::TAILWIND_STYLESHEET)
        .context("Failed to install embedded web assets")?;

    info!(path = ?cli.config, "Loading configuration");
    let (config_store, config) = ConfigStore::open(&cli.config).await?;
    config.validate()?;

    let metrics = Arc::new(Metrics::default());
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    info!(
        "Starting RTMP Stream Multiplexer v{}",
        env!("CARGO_PKG_VERSION")
    );
    info!(
        "Listening for RTMP stream ingest on {}",
        config.server.listen
    );

    for t in &config.targets {
        info!(name = %t.name, enabled = t.enabled, "Target configured");
    }

    let web_addr = config.server.api_listen;
    let listen_port = config.server.listen.port();
    let state =
        Arc::new(ProxyState::new(metrics, config, http_client, listen_port, config_store).await?);
    state.apply_chat_config().await?;

    // Spawn Web Server
    let web_state = Arc::clone(&state);
    tokio::spawn(async move {
        if let Err(e) = crate::web::run_web_server(web_state, web_addr).await {
            warn!("Web interface server error: {:#}", e);
        }
    });

    // Run RTMP Server
    let rtmp_listen = state.config.read().await.server.listen;
    run_rtmp_server(rtmp_listen, state).await
}
