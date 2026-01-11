pub mod action_executor;
pub mod collectors;
pub mod event_store;
pub mod models;
pub mod recording;
pub mod session_manager;
pub mod storage;

use crate::Result;
use chromiumoxide::Page;
use std::sync::Arc;

pub use action_executor::{ActionConfig, ActionExecutor};
pub use collectors::{
    CollectorSet, ConsoleLevel, ConsoleMessage, DevToolsIssue, Dialog, DialogCollector, DialogType,
    NetworkRequest, PageError,
};
pub use models::BrowserSession;
pub use recording::{FrameInfo, Recording, RecordingDetail, RecordingStatus, RecordingStorage};
pub use session_manager::{BrowserSessionManager, PageInfo, SessionConfig};
pub use storage::SessionStorage;

#[async_trait::async_trait]
pub trait PageProvider: Send + Sync {
    async fn get_or_create_page(&self) -> Result<Arc<Page>>;
    fn storage(&self) -> &Arc<SessionStorage>;
    fn collectors(&self) -> &Arc<CollectorSet>;

    /// Optional method to update page info for persistence (CLI mode only)
    async fn update_active_page_info(&self) -> Result<()> {
        Ok(())
    }
}
