use crate::{ChromeError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default)]
    pub browser: BrowserConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
    #[serde(default)]
    pub emulation: EmulationConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub dialog: DialogConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub filters: FilterConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub socket_path: Option<PathBuf>,
    pub max_sessions: Option<usize>,
    pub session_timeout_secs: Option<u64>,
    #[serde(default = "default_cdp_port_range")]
    pub cdp_port_range: (u16, u16),
    #[serde(default = "default_http_port_range")]
    pub http_port_range: (u16, u16),
    #[serde(default = "default_ws_port_range")]
    pub ws_port_range: (u16, u16),
    pub cdp_port: Option<u16>,
    pub http_port: Option<u16>,
    pub ws_port: Option<u16>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            socket_path: None,
            max_sessions: None,
            session_timeout_secs: None,
            cdp_port_range: default_cdp_port_range(),
            http_port_range: default_http_port_range(),
            ws_port_range: default_ws_port_range(),
            cdp_port: None,
            http_port: None,
            ws_port: None,
        }
    }
}

fn default_cdp_port_range() -> (u16, u16) {
    (9222, 9299)
}

fn default_http_port_range() -> (u16, u16) {
    (9300, 9399)
}

fn default_ws_port_range() -> (u16, u16) {
    (9400, 9499)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FilterConfig {
    #[serde(default = "default_network_exclude_types")]
    pub network_exclude_types: Vec<String>,
    #[serde(default = "default_network_exclude_domains")]
    pub network_exclude_domains: Vec<String>,
    #[serde(default = "default_console_levels")]
    pub console_levels: Vec<String>,
    #[serde(default = "default_network_max_body_size")]
    pub network_max_body_size: usize,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            network_exclude_types: default_network_exclude_types(),
            network_exclude_domains: default_network_exclude_domains(),
            console_levels: default_console_levels(),
            network_max_body_size: default_network_max_body_size(),
        }
    }
}

fn default_network_exclude_types() -> Vec<String> {
    vec![
        "Image".to_string(),
        "Stylesheet".to_string(),
        "Font".to_string(),
        "Media".to_string(),
    ]
}

fn default_network_exclude_domains() -> Vec<String> {
    vec![
        "google-analytics.com".to_string(),
        "googletagmanager.com".to_string(),
        "doubleclick.net".to_string(),
        "facebook.com".to_string(),
        "facebook.net".to_string(),
    ]
}

fn default_console_levels() -> Vec<String> {
    vec!["error".to_string(), "warn".to_string()]
}

fn default_network_max_body_size() -> usize {
    10000
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BrowserConfig {
    pub chrome_path: Option<PathBuf>,
    #[serde(default = "default_headless")]
    pub headless: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    pub user_data_dir: Option<PathBuf>,
    pub profile_directory: Option<String>,
    pub extension_path: Option<PathBuf>,
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    #[serde(default = "default_window_height")]
    pub window_height: u32,
    #[serde(default)]
    pub disable_web_security: bool,
    #[serde(default)]
    pub reuse_browser: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PerformanceConfig {
    #[serde(default = "default_trace_categories")]
    pub trace_categories: Vec<String>,
    #[serde(default = "default_navigation_timeout")]
    pub navigation_timeout_seconds: u64,
    #[serde(default = "default_network_idle_timeout")]
    pub network_idle_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmulationConfig {
    #[serde(default = "default_device")]
    pub default_device: String,
    pub custom_devices_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct NetworkConfig {
    pub proxy: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputConfig {
    #[serde(default = "default_screenshot_format")]
    pub default_screenshot_format: String,
    #[serde(default = "default_screenshot_quality")]
    pub screenshot_quality: u8,
    #[serde(default = "default_json_pretty")]
    pub json_pretty: bool,
}

/// Dialog handling configuration.
///
/// By default, dialogs are auto-dismissed (same as Playwright's behavior).
/// This is necessary because CDP dialog handling is session-specific,
/// meaning dialogs must be handled within the same session that receives them.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DialogConfig {
    /// How to automatically handle JavaScript dialogs (alert, confirm, prompt, beforeunload).
    /// - "dismiss": Auto-dismiss all dialogs (default, like Playwright)
    /// - "accept": Auto-accept all dialogs
    /// - "none": Do not auto-handle (will cause page to stall on dialog)
    #[serde(default = "default_dialog_behavior")]
    pub behavior: DialogBehavior,

    /// Default text to enter for prompt dialogs when auto-accepting.
    #[serde(default)]
    pub prompt_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DialogBehavior {
    /// Auto-dismiss all dialogs (default, matches Playwright behavior)
    #[default]
    Dismiss,
    /// Auto-accept all dialogs
    Accept,
    /// Do not auto-handle dialogs (will stall page execution)
    None,
}

fn default_dialog_behavior() -> DialogBehavior {
    DialogBehavior::Dismiss
}

impl Default for DialogConfig {
    fn default() -> Self {
        Self {
            behavior: default_dialog_behavior(),
            prompt_text: None,
        }
    }
}

fn default_headless() -> bool {
    true
}
fn default_port() -> u16 {
    9222
}
fn default_navigation_timeout() -> u64 {
    30
}
fn default_network_idle_timeout() -> u64 {
    2000
}
fn default_device() -> String {
    "Desktop".to_string()
}
fn default_screenshot_format() -> String {
    "png".to_string()
}
fn default_screenshot_quality() -> u8 {
    90
}
fn default_json_pretty() -> bool {
    false
}
fn default_window_width() -> u32 {
    1280
}
fn default_window_height() -> u32 {
    800
}

fn default_trace_categories() -> Vec<String> {
    vec![
        "loading".to_string(),
        "devtools.timeline".to_string(),
        "blink.user_timing".to_string(),
    ]
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            chrome_path: None,
            headless: default_headless(),
            port: default_port(),
            user_data_dir: None,
            profile_directory: None,
            extension_path: None,
            window_width: default_window_width(),
            window_height: default_window_height(),
            disable_web_security: false,
            reuse_browser: false,
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            trace_categories: default_trace_categories(),
            navigation_timeout_seconds: default_navigation_timeout(),
            network_idle_timeout_ms: default_network_idle_timeout(),
        }
    }
}

impl Default for EmulationConfig {
    fn default() -> Self {
        Self {
            default_device: default_device(),
            custom_devices_path: None,
        }
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            default_screenshot_format: default_screenshot_format(),
            screenshot_quality: default_screenshot_quality(),
            json_pretty: default_json_pretty(),
        }
    }
}

pub fn default_config_path() -> Result<PathBuf> {
    default_config_dir().map(|p| p.join("config.toml"))
}

pub fn default_config_dir() -> Result<PathBuf> {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .map(|p| p.join("chrome-devtools-cli"))
        .ok_or_else(|| ChromeError::ConfigError("Could not determine config directory".into()))
}

impl Config {
    pub fn load() -> Result<Self> {
        let mut config = Self::default();

        let global_path = default_config_path()?;
        if global_path.exists() {
            let content = std::fs::read_to_string(&global_path)?;
            config = toml::from_str(&content)?;
        }

        let project_path = PathBuf::from(".chrome-devtools.toml");
        if project_path.exists() {
            let content = std::fs::read_to_string(&project_path)?;
            let project_config: Config = toml::from_str(&content)?;
            config = config.merge(project_config);
        }

        config.load_from_env();

        Ok(config)
    }

    pub fn load_with_overrides(&self, cli_overrides: ConfigOverrides) -> Self {
        let mut config = self.clone();

        if let Some(headless) = cli_overrides.headless {
            config.browser.headless = headless;
        }
        if let Some(port) = cli_overrides.port {
            config.browser.port = port;
        }
        if let Some(json) = cli_overrides.json {
            config.output.json_pretty = json;
        }
        if let Some(chrome_path) = cli_overrides.chrome_path {
            config.browser.chrome_path = Some(chrome_path);
        }
        if let Some(timeout) = cli_overrides.timeout {
            config.performance.navigation_timeout_seconds = timeout;
        }

        config
    }

    fn merge(mut self, other: Config) -> Self {
        if other.browser.chrome_path.is_some() {
            self.browser.chrome_path = other.browser.chrome_path;
        }
        if other.browser.user_data_dir.is_some() {
            self.browser.user_data_dir = other.browser.user_data_dir;
        }
        if other.network.proxy.is_some() {
            self.network.proxy = other.network.proxy;
        }
        if other.network.user_agent.is_some() {
            self.network.user_agent = other.network.user_agent;
        }
        if other.emulation.custom_devices_path.is_some() {
            self.emulation.custom_devices_path = other.emulation.custom_devices_path;
        }
        self
    }

    fn load_from_env(&mut self) {
        if let Ok(port) = std::env::var("CHROME_DEBUG_PORT")
            && let Ok(port) = port.parse()
        {
            self.browser.port = port;
        }
        if let Ok(headless) = std::env::var("CHROME_HEADLESS") {
            self.browser.headless = headless == "true" || headless == "1";
        }
        if let Ok(path) = std::env::var("CHROME_PATH") {
            self.browser.chrome_path = Some(PathBuf::from(path));
        }
        if let Ok(timeout) = std::env::var("CHROME_TIMEOUT")
            && let Ok(timeout) = timeout.parse()
        {
            self.performance.navigation_timeout_seconds = timeout;
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.browser.port < 1024 {
            return Err(ChromeError::InvalidPort(self.browser.port));
        }

        if self.performance.navigation_timeout_seconds == 0 {
            return Err(ChromeError::ConfigError(
                "navigation_timeout_seconds must be greater than 0".into(),
            ));
        }

        if self.output.screenshot_quality < 1 || self.output.screenshot_quality > 100 {
            return Err(ChromeError::ConfigError(
                "screenshot_quality must be between 1 and 100".into(),
            ));
        }

        if let Some(ref path) = self.browser.chrome_path
            && !path.exists()
        {
            return Err(ChromeError::ConfigError(format!(
                "Chrome path does not exist: {}",
                path.display()
            )));
        }

        Ok(())
    }

    pub fn show_masked(&self) -> String {
        let ext_status = self.resolve_extension_status();
        format!(
            r#"Browser:
  Chrome Path: {}
  Headless: {}
  Port: {}
  User Data Dir: {}
  Extension: {}

Performance:
  Navigation Timeout: {}s

Emulation:
  Default Device: {}

Network:
  Proxy: {}

Output:
  Screenshot Format: {}

Dialog:
  Behavior: {:?}
"#,
            self.browser
                .chrome_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "auto-detect".into()),
            self.browser.headless,
            self.browser.port,
            self.browser
                .user_data_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "default".into()),
            ext_status,
            self.performance.navigation_timeout_seconds,
            self.emulation.default_device,
            self.network.proxy.as_ref().unwrap_or(&"none".into()),
            self.output.default_screenshot_format,
            self.dialog.behavior,
        )
    }

    fn resolve_extension_status(&self) -> String {
        if let Some(ref path) = self.browser.extension_path
            && path.exists()
            && path.join("manifest.json").exists()
        {
            return format!("{} (config)", path.display());
        }
        if let Ok(config_dir) = default_config_dir() {
            let default_ext = config_dir.join("extension");
            if default_ext.exists() && default_ext.join("manifest.json").exists() {
                return format!("{} (default)", default_ext.display());
            }
        }
        "not installed".into()
    }
}

#[derive(Debug, Default)]
pub struct ConfigOverrides {
    pub headless: Option<bool>,
    pub port: Option<u16>,
    pub json: Option<bool>,
    pub chrome_path: Option<PathBuf>,
    pub timeout: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.browser.headless);
        assert_eq!(config.browser.port, 9222);
        assert_eq!(config.performance.navigation_timeout_seconds, 30);
        assert_eq!(config.output.screenshot_quality, 90);
    }

    #[test]
    fn test_browser_config_default() {
        let config = BrowserConfig::default();
        assert!(config.chrome_path.is_none());
        assert!(config.headless);
        assert_eq!(config.port, 9222);
        assert!(!config.disable_web_security);
    }

    #[test]
    fn test_config_validate_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_invalid_port() {
        let mut config = Config::default();
        config.browser.port = 80;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_timeout() {
        let mut config = Config::default();
        config.performance.navigation_timeout_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_quality() {
        let mut config = Config::default();
        config.output.screenshot_quality = 0;
        assert!(config.validate().is_err());

        config.output.screenshot_quality = 101;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_load_with_overrides() {
        let config = Config::default();
        let overrides = ConfigOverrides {
            headless: Some(false),
            port: Some(9333),
            json: Some(true),
            chrome_path: None,
            timeout: Some(60),
        };

        let result = config.load_with_overrides(overrides);
        assert!(!result.browser.headless);
        assert_eq!(result.browser.port, 9333);
        assert!(result.output.json_pretty);
        assert_eq!(result.performance.navigation_timeout_seconds, 60);
    }

    #[test]
    fn test_config_merge() {
        let base = Config::default();
        let mut other = Config::default();
        other.browser.chrome_path = Some(PathBuf::from("/usr/bin/chrome"));
        other.network.proxy = Some("http://proxy:8080".to_string());

        let merged = base.merge(other);
        assert_eq!(
            merged.browser.chrome_path,
            Some(PathBuf::from("/usr/bin/chrome"))
        );
        assert_eq!(merged.network.proxy, Some("http://proxy:8080".to_string()));
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("[browser]"));
        assert!(toml_str.contains("[performance]"));

        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.browser.port, config.browser.port);
    }

    #[test]
    fn test_performance_config_default() {
        let config = PerformanceConfig::default();
        assert_eq!(config.navigation_timeout_seconds, 30);
        assert_eq!(config.network_idle_timeout_ms, 2000);
        assert!(config.trace_categories.contains(&"loading".to_string()));
    }

    #[test]
    fn test_output_config_default() {
        let config = OutputConfig::default();
        assert_eq!(config.default_screenshot_format, "png");
        assert_eq!(config.screenshot_quality, 90);
        assert!(!config.json_pretty);
    }
}
