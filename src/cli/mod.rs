pub mod commands;
pub mod dispatch;

use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "chrome-devtools-cli")]
#[command(version, about = "Chrome DevTools Protocol CLI tool")]
#[command(
    long_about = "High-performance Chrome DevTools Protocol CLI for browser automation and testing"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<commands::Command>,

    #[arg(long, global = true, help = "Output in JSON format")]
    pub json: bool,

    #[arg(short, long, global = true, help = "Enable verbose output")]
    pub verbose: bool,

    #[arg(long, global = true, help = "Path to config file")]
    pub config: Option<PathBuf>,

    #[arg(long, global = true, help = "Run Chrome in headless mode")]
    pub headless: Option<bool>,

    #[arg(long, global = true, help = "Chrome debugging port")]
    pub port: Option<u16>,

    #[arg(long, global = true, help = "Path to Chrome executable")]
    pub chrome_path: Option<PathBuf>,

    #[arg(long, global = true, help = "Navigation timeout in seconds")]
    pub timeout: Option<u64>,

    #[arg(short = 's', long, global = true, help = "Session ID (daemon mode)")]
    pub session: Option<String>,

    #[arg(
        long,
        global = true,
        help = "Use user profile session (auto-creates/joins)"
    )]
    pub user_profile: bool,
}

impl Cli {
    pub fn with_env_context(mut self) -> Self {
        if self.session.is_none() {
            self.session = std::env::var("CHROME_SESSION").ok();
        }

        if !self.user_profile {
            self.user_profile = std::env::var("CHROME_USER_PROFILE")
                .map(|v| !v.is_empty() && v != "0" && v.to_lowercase() != "false")
                .unwrap_or(false);
        }

        if self.headless.is_none() {
            self.headless = std::env::var("CHROME_HEADLESS")
                .ok()
                .map(|v| v != "0" && v.to_lowercase() != "false");
        }

        self
    }
}

pub async fn run() -> crate::Result<()> {
    let cli = Cli::parse().with_env_context();

    let config = if let Some(config_path) = &cli.config {
        let content = std::fs::read_to_string(config_path)?;
        toml::from_str(&content)?
    } else {
        crate::config::Config::load()?
    };

    let overrides = crate::config::ConfigOverrides {
        headless: cli.headless,
        port: cli.port,
        json: Some(cli.json),
        chrome_path: cli.chrome_path.clone(),
        timeout: cli.timeout,
    };

    let config = Arc::new(config.load_with_overrides(overrides));
    config.validate()?;

    dispatch::dispatch(cli, config).await
}
