pub mod chrome;
pub mod cli;
pub mod client;
pub mod config;
pub mod devices;
pub mod error;
pub mod handlers;
pub mod js_templates;
pub mod output;
pub mod server;
pub mod timeouts;
pub mod trace;
pub mod utils;

pub use config::{Config, DialogBehavior, DialogConfig, ServerConfig};
pub use error::ChromeError;

pub type Result<T> = std::result::Result<T, ChromeError>;
