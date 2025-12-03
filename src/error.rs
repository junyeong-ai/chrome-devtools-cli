use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChromeError {
    #[error("Failed to launch Chrome: {0}")]
    LaunchFailed(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Chrome connection lost")]
    ConnectionLost,

    #[error("Navigation timeout after {0}s")]
    NavigationTimeout(u64),

    #[error("Element not found: {selector}")]
    ElementNotFound { selector: String },

    #[error("Screenshot failed: {0}")]
    ScreenshotFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("JavaScript evaluation failed: {0}")]
    EvaluationError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Storage operation failed: {0}")]
    StorageError(String),

    #[error("Tracing error: {0}")]
    TracingError(String),

    #[error("Device profile not found: {0}")]
    DeviceNotFound(String),

    #[error("File I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDeError(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerError(#[from] toml::ser::Error),

    #[error("Invalid port: {0}")]
    InvalidPort(u16),

    #[error("Session not found")]
    SessionNotFound,

    #[error("General error: {0}")]
    General(String),
}

impl ChromeError {
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            Self::LaunchFailed(_) => vec![
                "Ensure Chrome/Chromium is installed".into(),
                "Check if another Chrome instance is using the debugging port".into(),
                "Try specifying Chrome path with --chrome-path".into(),
            ],
            Self::ConnectionLost => vec![
                "Check if Chrome was closed manually".into(),
                "Verify network connectivity if using remote debugging".into(),
                "Try restarting the session".into(),
            ],
            Self::NavigationTimeout(timeout) => vec![
                format!("Increase timeout with --timeout {}", timeout + 30),
                "Check network connectivity".into(),
                "Verify URL is accessible".into(),
            ],
            Self::ElementNotFound { selector } => vec![
                "Verify the selector syntax is correct".into(),
                "Wait for page to fully load with --wait-for load".into(),
                format!("Check if element '{}' exists on the page", selector),
            ],
            Self::ScreenshotFailed(_) => vec![
                "Ensure output directory exists and is writable".into(),
                "Check if page is fully loaded".into(),
                "Try with --wait-for load option".into(),
            ],
            Self::ConfigError(_) => vec![
                "Check configuration file syntax".into(),
                "Run with --verbose to see detailed error".into(),
                "Use --config to specify a different config file".into(),
            ],
            Self::InvalidUrl(_) => vec![
                "Ensure URL includes protocol (http:// or https://)".into(),
                "Check for typos in the URL".into(),
            ],
            Self::EvaluationError(_) => vec![
                "Check JavaScript syntax".into(),
                "Ensure the script is compatible with the page context".into(),
                "Use console.log for debugging".into(),
            ],
            Self::NetworkError(_) => vec![
                "Check network connectivity".into(),
                "Verify proxy settings if using a proxy".into(),
                "Check if URL is accessible".into(),
            ],
            Self::StorageError(_) => vec![
                "Check if cookies/localStorage are enabled".into(),
                "Verify domain matches the current page".into(),
            ],
            Self::TracingError(_) => vec![
                "Ensure sufficient disk space for trace files".into(),
                "Check write permissions for output directory".into(),
            ],
            Self::DeviceNotFound(name) => vec![
                "List available devices with: chrome-devtools-cli devices list".into(),
                format!("Check if '{}' is spelled correctly", name),
                "Use a custom device profile with --device-file".into(),
            ],
            Self::InvalidPort(port) => vec![
                format!("Port {} is out of valid range (1024-65535)", port),
                "Use --port to specify a different port".into(),
            ],
            Self::SessionNotFound => vec![
                "Start a new session by running a command without existing session".into(),
                "Check if Chrome was closed manually".into(),
            ],
            _ => vec![
                "Run with --verbose for more details".into(),
                "Check the documentation for help".into(),
            ],
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::LaunchFailed(_) | Self::ConnectionLost | Self::SessionNotFound => 3,
            Self::NavigationTimeout(_) => 4,
            Self::ElementNotFound { .. } => 5,
            Self::IoError(_) | Self::ScreenshotFailed(_) | Self::StorageError(_) => 6,
            Self::ConfigError(_)
            | Self::TomlDeError(_)
            | Self::TomlSerError(_)
            | Self::InvalidPort(_) => 7,
            Self::InvalidUrl(_) => 2,
            _ => 1,
        }
    }
}
