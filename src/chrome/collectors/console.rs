use crate::{ChromeError, Result, config::FilterConfig};
use chromiumoxide::{
    Page,
    cdp::js_protocol::runtime::{EnableParams as RuntimeEnableParams, EventConsoleApiCalled},
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::super::storage::SessionStorage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleLevel {
    Log,
    Debug,
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for ConsoleLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsoleLevel::Log => write!(f, "log"),
            ConsoleLevel::Debug => write!(f, "debug"),
            ConsoleLevel::Info => write!(f, "info"),
            ConsoleLevel::Warning => write!(f, "warning"),
            ConsoleLevel::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for ConsoleLevel {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "log" => Ok(ConsoleLevel::Log),
            "debug" | "verbose" => Ok(ConsoleLevel::Debug),
            "info" => Ok(ConsoleLevel::Info),
            "warn" | "warning" => Ok(ConsoleLevel::Warning),
            "error" => Ok(ConsoleLevel::Error),
            _ => Err(format!("Unknown console level: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleMessage {
    pub level: ConsoleLevel,
    pub text: String,
    pub args: Option<Vec<String>>,
    pub timestamp: DateTime<Utc>,
    pub url: Option<String>,
    pub line: Option<i64>,
}

pub struct ConsoleCollector {
    storage: Arc<SessionStorage>,
    filter_config: FilterConfig,
}

impl ConsoleCollector {
    pub fn new(storage: Arc<SessionStorage>, filter_config: FilterConfig) -> Self {
        Self {
            storage,
            filter_config,
        }
    }

    pub async fn attach(&self, page: &Arc<Page>) -> Result<()> {
        page.execute(RuntimeEnableParams::default())
            .await
            .map_err(|e| ChromeError::General(format!("Failed to enable Runtime domain: {}", e)))?;

        let storage = self.storage.clone();
        let allowed_levels = self.filter_config.console_levels.clone();

        let mut stream = page
            .event_listener::<EventConsoleApiCalled>()
            .await
            .map_err(|e| {
                ChromeError::General(format!("Failed to attach console listener: {}", e))
            })?;

        tokio::spawn(async move {
            while let Some(event) = stream.next().await {
                use chromiumoxide::cdp::js_protocol::runtime::ConsoleApiCalledType;

                let level = match event.r#type {
                    ConsoleApiCalledType::Log => ConsoleLevel::Log,
                    ConsoleApiCalledType::Debug => ConsoleLevel::Debug,
                    ConsoleApiCalledType::Info => ConsoleLevel::Info,
                    ConsoleApiCalledType::Warning => ConsoleLevel::Warning,
                    ConsoleApiCalledType::Error => ConsoleLevel::Error,
                    _ => ConsoleLevel::Log,
                };

                if !should_collect_level(level, &allowed_levels) {
                    continue;
                }

                let text = event
                    .args
                    .first()
                    .and_then(|arg| arg.value.as_ref())
                    .map(|v| v.to_string())
                    .unwrap_or_default();

                let args: Vec<String> = event
                    .args
                    .iter()
                    .filter_map(|arg| arg.value.as_ref().map(|v| v.to_string()))
                    .collect();

                let (url, line) = event
                    .stack_trace
                    .as_ref()
                    .and_then(|st| st.call_frames.first())
                    .map(|f| (Some(f.url.clone()), Some(f.line_number)))
                    .unwrap_or((None, None));

                if !should_include_message(&text, url.as_deref()) {
                    continue;
                }

                let message = ConsoleMessage {
                    level,
                    text,
                    args: if args.is_empty() { None } else { Some(args) },
                    timestamp: Utc::now(),
                    url,
                    line,
                };

                storage.append("console", &message).ok();
            }
        });

        Ok(())
    }

    pub fn get_messages(&self) -> Result<Vec<ConsoleMessage>> {
        self.storage.read_all("console")
    }

    pub fn get_messages_filtered(
        &self,
        level: Option<ConsoleLevel>,
    ) -> Result<Vec<ConsoleMessage>> {
        let messages: Vec<ConsoleMessage> = self.storage.read_all("console")?;

        Ok(match level {
            Some(filter) => messages.into_iter().filter(|m| m.level == filter).collect(),
            None => messages,
        })
    }

    pub fn count(&self) -> usize {
        self.storage.count("console")
    }
}

fn should_collect_level(level: ConsoleLevel, allowed_levels: &[String]) -> bool {
    if allowed_levels.is_empty() {
        return true;
    }
    let level_str = level.to_string();
    allowed_levels
        .iter()
        .any(|l| l.eq_ignore_ascii_case(&level_str))
}

fn should_include_message(text: &str, url: Option<&str>) -> bool {
    if text.contains("[cdtcli-bridge]") {
        return false;
    }
    if let Some(u) = url
        && u.starts_with("chrome-extension://")
    {
        return false;
    }
    true
}
