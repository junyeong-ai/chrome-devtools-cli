pub mod console;
pub mod dialog;
pub mod extension;
pub mod issues;
pub mod network;
pub mod pageerror;

use crate::Result;
use crate::config::{DialogConfig, FilterConfig};
use chromiumoxide::Page;
use std::sync::Arc;

pub use console::{ConsoleCollector, ConsoleLevel, ConsoleMessage};
pub use dialog::{Dialog, DialogCollector, DialogResult, DialogType};
pub use extension::{ExtensionCollector, ExtensionEvent};
pub use issues::{DevToolsIssue, IssuesCollector};
pub use network::{NetworkCollector, NetworkRequest};
pub use pageerror::{PageError, PageErrorCollector};

use super::storage::SessionStorage;

pub struct CollectorSet {
    pub network: NetworkCollector,
    pub console: ConsoleCollector,
    pub pageerror: PageErrorCollector,
    pub issues: IssuesCollector,
    pub dialog: DialogCollector,
    pub extension: ExtensionCollector,
}

impl CollectorSet {
    pub fn new(
        storage: Arc<SessionStorage>,
        dialog_config: DialogConfig,
        filter_config: FilterConfig,
    ) -> Self {
        Self {
            network: NetworkCollector::new(storage.clone(), filter_config.clone()),
            console: ConsoleCollector::new(storage.clone(), filter_config),
            pageerror: PageErrorCollector::new(storage.clone()),
            issues: IssuesCollector::new(storage.clone()),
            dialog: DialogCollector::new(
                storage.clone(),
                dialog_config.behavior,
                dialog_config.prompt_text,
            ),
            extension: ExtensionCollector::new(storage),
        }
    }

    pub async fn attach(&self, page: &Arc<Page>) -> Result<()> {
        self.network.attach(page).await?;
        self.console.attach(page).await?;
        self.pageerror.attach(page).await?;
        self.issues.attach(page).await?;
        self.dialog.attach(page).await?;
        Ok(())
    }

    pub fn network_count(&self) -> usize {
        self.network.count()
    }

    pub fn console_count(&self) -> usize {
        self.console.count()
    }

    pub fn pageerror_count(&self) -> usize {
        self.pageerror.count()
    }

    pub fn issues_count(&self) -> usize {
        self.issues.count()
    }
}
