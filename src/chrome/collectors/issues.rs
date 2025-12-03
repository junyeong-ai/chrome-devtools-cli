use crate::{ChromeError, Result};
use chromiumoxide::{Page, cdp::browser_protocol::audits::EventIssueAdded};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::super::storage::SessionStorage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevToolsIssue {
    pub code: String,
    pub severity: String,
    pub details: Option<String>,
    pub url: Option<String>,
    pub timestamp: DateTime<Utc>,
}

pub struct IssuesCollector {
    storage: Arc<SessionStorage>,
}

impl IssuesCollector {
    pub fn new(storage: Arc<SessionStorage>) -> Self {
        Self { storage }
    }

    pub async fn attach(&self, page: &Arc<Page>) -> Result<()> {
        let storage = self.storage.clone();

        let mut stream = page
            .event_listener::<EventIssueAdded>()
            .await
            .map_err(|e| {
                ChromeError::General(format!("Failed to attach issues listener: {}", e))
            })?;

        tokio::spawn(async move {
            while let Some(event) = stream.next().await {
                let issue = &event.issue;
                let code = format!("{:?}", issue.code);
                let details = format!("{:?}", issue.details);

                let devtools_issue = DevToolsIssue {
                    code,
                    severity: "warning".to_string(),
                    details: Some(details),
                    url: None,
                    timestamp: Utc::now(),
                };

                storage.append("issues", &devtools_issue).ok();
            }
        });

        Ok(())
    }

    pub fn get_issues(&self) -> Result<Vec<DevToolsIssue>> {
        self.storage.read_all("issues")
    }

    pub fn count(&self) -> usize {
        self.storage.count("issues")
    }
}
