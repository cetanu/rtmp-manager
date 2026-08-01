mod chat;
mod config;
mod embedded_assets;
mod metrics;
mod notifications;
mod server;

mod web;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::ConfigStore;
use metrics::{run_health_server, Metrics};
use server::{run_rtmp_server, state::ProxyState};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

// Embed standalone systemd unit template at compile time
const SYSTEMD_UNIT_TEMPLATE: &str = include_str!("../systemd/rtmp-proxy.service");

#[derive(Parser, Debug)]
#[command(author, version, about = "RTMP Stream Multiplexer powered by rtmp-rs", long_about = None)]
struct CliArgs {
    /// Path to the SQLite database, or a legacy TOML file to import once
    #[arg(short, long, env = "CONFIG_PATH", default_value = "config.toml")]
    config: PathBuf,

    /// Initial web username, persisted only when SQLite has no web credentials
    #[arg(long, env = "WEB_AUTH_BOOTSTRAP_USERNAME", hide_env_values = true)]
    web_auth_bootstrap_username: Option<String>,

    /// Initial web password, persisted only when SQLite has no web credentials
    #[arg(long, env = "WEB_AUTH_BOOTSTRAP_PASSWORD", hide_env_values = true)]
    web_auth_bootstrap_password: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Install systemd unit service for self-managed background execution
    InstallSystemd {
        /// Working directory for systemd service
        #[arg(long, default_value = "/opt/rtmp-proxy")]
        work_dir: PathBuf,

        /// Path to config file for systemd service
        #[arg(long, default_value = "/opt/rtmp-proxy/config.toml")]
        config_path: PathBuf,
    },
}

fn install_systemd(work_dir: &Path, config_path: &Path) -> Result<()> {
    let current_exe =
        std::env::current_exe().context("Failed to determine path of current executable")?;

    embedded_assets::install(&current_exe, web::TAILWIND_STYLESHEET)
        .context("Failed to install embedded web assets")?;

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

    if let Ok(s) = status {
        if s.success() {
            println!("Executed systemctl daemon-reload");
        }
    }

    let enable_status = std::process::Command::new("systemctl")
        .args(["enable", "--now", "rtmp-proxy"])
        .status();

    if let Ok(s) = enable_status {
        if s.success() {
            println!("Successfully enabled and started rtmp-proxy systemd service!");
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rtmp_proxy=info,rtmp_rs=info".into()),
        )
        .init();

    let cli = CliArgs::parse();

    if let Some(Commands::InstallSystemd {
        work_dir,
        config_path,
    }) = cli.command
    {
        return install_systemd(&work_dir, &config_path);
    }

    info!(path = ?cli.config, "Loading configuration");
    let (config_store, mut config) = ConfigStore::open(&cli.config)?;
    if config.web_auth.username.is_empty() && config.web_auth.password.is_empty() {
        match (
            cli.web_auth_bootstrap_username
                .filter(|value| !value.trim().is_empty()),
            cli.web_auth_bootstrap_password
                .filter(|value| !value.is_empty()),
        ) {
            (Some(username), Some(password)) => {
                config.web_auth.username = username.trim().to_string();
                config.web_auth.password = password;
                config.validate()?;
                config_store.save(&config)?;
                info!("Persisted initial web authentication credentials to SQLite");
            }
            (None, None) => warn!(
                "Web authentication is disabled; configure web_auth or provide both bootstrap credentials"
            ),
            _ => anyhow::bail!(
                "Both WEB_AUTH_BOOTSTRAP_USERNAME and WEB_AUTH_BOOTSTRAP_PASSWORD are required together"
            ),
        }
    }
    config.validate()?;
    info!(path = %config_store.path().display(), "Using SQLite configuration database");

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
    let state = Arc::new(ProxyState::new(
        Arc::clone(&metrics),
        config,
        http_client,
        listen_port,
        config_store,
    )?);
    state.apply_chat_config().await?;

    // Spawn Web Server
    let web_state = Arc::clone(&state);
    tokio::spawn(async move {
        if let Err(e) = crate::web::run_web_server(web_state, web_addr).await {
            warn!("Web interface server error: {:#}", e);
        }
    });

    // Spawn Health Check HTTP Server on localhost
    let health_addr = state.config.read().await.server.health_listen;
    let health_metrics = Arc::clone(&metrics);
    tokio::spawn(async move {
        if let Err(e) = run_health_server(health_addr, health_metrics).await {
            warn!("Health check server error: {:#}", e);
        }
    });

    // Run RTMP Server
    let rtmp_listen = state.config.read().await.server.listen;
    run_rtmp_server(rtmp_listen, state).await
}
