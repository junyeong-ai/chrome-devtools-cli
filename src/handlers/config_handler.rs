use crate::{Result, config::Config, output};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct ConfigInfo {
    pub path: PathBuf,
    pub exists: bool,
}

#[derive(Debug, Serialize)]
pub struct ConfigShowResult {
    pub config: Config,
}

impl output::OutputFormatter for ConfigInfo {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}",
            text::key_value("Config Path", &self.path.display().to_string()),
            text::key_value("Exists", &self.exists.to_string())
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

impl output::OutputFormatter for ConfigShowResult {
    fn format_text(&self) -> String {
        self.config.show_masked()
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(&self.config, pretty)
    }
}

pub fn handle_config_init() -> Result<ConfigInfo> {
    let config_path = crate::config::default_config_path()?;
    let config_dir = config_path
        .parent()
        .ok_or_else(|| crate::ChromeError::ConfigError("Invalid config path".into()))?;

    std::fs::create_dir_all(config_dir)?;

    if config_path.exists() {
        return Err(crate::ChromeError::ConfigError(format!(
            "Config file already exists at {}",
            config_path.display()
        )));
    }

    let default_config = Config::default();
    let toml_content = toml::to_string_pretty(&default_config)?;

    std::fs::write(&config_path, toml_content)?;

    Ok(ConfigInfo {
        path: config_path,
        exists: true,
    })
}

pub fn handle_config_show(config: &Config) -> Result<ConfigShowResult> {
    Ok(ConfigShowResult {
        config: config.clone(),
    })
}

pub fn handle_config_edit() -> Result<ConfigInfo> {
    let config_path = crate::config::default_config_path()?;

    if !config_path.exists() {
        return Err(crate::ChromeError::ConfigError(
            "Config file not found. Run 'config init' first.".to_string(),
        ));
    }

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "macos") {
                "open".to_string()
            } else if cfg!(target_os = "windows") {
                "notepad".to_string()
            } else {
                "nano".to_string()
            }
        });

    std::process::Command::new(&editor)
        .arg(&config_path)
        .status()
        .map_err(|e| crate::ChromeError::ConfigError(format!("Failed to open editor: {}", e)))?;

    Ok(ConfigInfo {
        path: config_path,
        exists: true,
    })
}

pub fn handle_config_path() -> Result<ConfigInfo> {
    let config_path = crate::config::default_config_path()?;
    let exists = config_path.exists();

    Ok(ConfigInfo {
        path: config_path,
        exists,
    })
}
