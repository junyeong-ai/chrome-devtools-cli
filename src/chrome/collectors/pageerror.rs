use crate::{ChromeError, Result};
use chromiumoxide::{Page, cdp::js_protocol::runtime::EventExceptionThrown};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::super::storage::SessionStorage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageError {
    pub message: String,
    pub url: Option<String>,
    pub line: i64,
    pub column: i64,
    pub stack_trace: Option<String>,
    pub timestamp: DateTime<Utc>,
}

pub struct PageErrorCollector {
    storage: Arc<SessionStorage>,
}

impl PageErrorCollector {
    pub fn new(storage: Arc<SessionStorage>) -> Self {
        Self { storage }
    }

    pub async fn attach(&self, page: &Arc<Page>) -> Result<()> {
        let storage = self.storage.clone();

        let mut stream = page
            .event_listener::<EventExceptionThrown>()
            .await
            .map_err(|e| {
                ChromeError::General(format!("Failed to attach pageerror listener: {}", e))
            })?;

        tokio::spawn(async move {
            while let Some(event) = stream.next().await {
                let details = &event.exception_details;

                let stack_trace = details.stack_trace.as_ref().map(|st| {
                    st.call_frames
                        .iter()
                        .map(|f| {
                            format!(
                                "  at {} ({}:{}:{})",
                                f.function_name, f.url, f.line_number, f.column_number
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                });

                let error = PageError {
                    message: details
                        .exception
                        .as_ref()
                        .and_then(|e| e.description.clone())
                        .unwrap_or_else(|| details.text.clone()),
                    url: details.url.clone(),
                    line: details.line_number,
                    column: details.column_number,
                    stack_trace,
                    timestamp: Utc::now(),
                };

                storage.append("pageerror", &error).ok();
            }
        });

        Ok(())
    }

    pub fn get_errors(&self) -> Result<Vec<PageError>> {
        self.storage.read_all("pageerror")
    }

    pub fn count(&self) -> usize {
        self.storage.count("pageerror")
    }
}
