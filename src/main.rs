mod accounts;
mod billing;
mod chat;
mod config;
mod crypto;
mod database;
mod embedded_assets;
mod log_buffer;
mod metrics;
mod notifications;
mod server;
mod tenant;
mod util;
mod web;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::ConfigHandle;
use metrics::Metrics;
use server::relay::run_redis_worker;
use server::{run_rtmp_server, state::AppHandle};
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
    /// SQLite or PostgreSQL connection URL
    #[arg(
        long,
        env = "DATABASE_URL",
        default_value = "sqlite://rtmp-manager.sqlite3?mode=rwc"
    )]
    database_url: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    InstallSystemd {
        #[arg(long, default_value = "/opt/rtmp-proxy")]
        work_dir: PathBuf,
    },
    RelayWorker {
        #[arg(long, env = "RELAY_BROKER_URL")]
        broker_url: String,
    },
}

fn install_systemd(work_dir: &Path) -> Result<()> {
    let current_exe =
        std::env::current_exe().context("Failed to determine path of current executable")?;

    let unit_content = SYSTEMD_UNIT_TEMPLATE
        .replace("{WORK_DIR}", &work_dir.display().to_string())
        .replace("{EXEC_START}", &current_exe.display().to_string())
        .replace("{STATE_DIR}", &work_dir.display().to_string());

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

    if let Some(Commands::InstallSystemd { work_dir }) = cli.command {
        return install_systemd(&work_dir);
    }
    if let Some(Commands::RelayWorker { broker_url }) = cli.command {
        return run_redis_worker(&broker_url).await;
    }

    embedded_assets::install(web::TAILWIND_STYLESHEET)
        .context("Failed to install embedded web assets")?;

    info!("Connecting to application database");
    let database = database::Database::connect(&cli.database_url).await?;
    let (config_handle, config) = ConfigHandle::open(database.clone()).await?;

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
    let rtmp_listen = config.server.listen;
    let listen_port = config.server.listen.port();
    let srt_listen = config.server.srt_listen;
    let srt_enabled = config.server.srt_enabled;

    let app = AppHandle::new(metrics, database, config_handle, http_client, listen_port).await?;

    // Spawn Web Server
    let web_app = app.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::web::run_web_server(web_app, web_addr).await {
            warn!("Web interface server error: {:#}", e);
        }
    });

    if srt_enabled {
        let srt_app = app.clone();
        tokio::spawn(async move {
            if let Err(error) = crate::server::srt::run_srt_server(srt_listen, srt_app).await {
                warn!("SRT ingest server error: {error:#}");
            }
        });
    }

    // Run RTMP Server
    run_rtmp_server(rtmp_listen, app).await
}
