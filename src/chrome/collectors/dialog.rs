//! Dialog Collector - Auto-handles JavaScript dialogs within the same CDP session.
//!
//! # Architecture Decision
//!
//! Chrome DevTools Protocol (CDP) dialog handling is **session-specific**.
//! Each CLI invocation creates a new CDP session, so cross-session dialog
//! handling is fundamentally impossible.
//!
//! This collector follows **Playwright's pattern**:
//! > "When no page.on('dialog') listeners are present, all dialogs are automatically dismissed."
//!
//! ## How it works:
//! 1. Dialog event received (EventJavascriptDialogOpening)
//! 2. Dialog info saved to file (for inspection via `dialog` command)
//! 3. Dialog immediately auto-handled within the SAME session
//! 4. Result saved to file (dialog_result.ndjson)
//!
//! ## Configuration:
//! - `dialog.behavior = "dismiss"` (default): Auto-dismiss all dialogs
//! - `dialog.behavior = "accept"`: Auto-accept all dialogs
//! - `dialog.behavior = "none"`: Do not auto-handle (will stall page execution)
//! - `dialog.prompt_text`: Text to enter for prompt dialogs when auto-accepting

use crate::chrome::event_store::EventMetadata;
use crate::config::DialogBehavior;
use crate::{ChromeError, Result};
use chromiumoxide::Page;
use chromiumoxide::cdp::browser_protocol::page::{
    DialogType as CdpDialogType, EventJavascriptDialogOpening, HandleJavaScriptDialogParams,
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;

use super::super::storage::SessionStorage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DialogType {
    Alert,
    Confirm,
    Prompt,
    BeforeUnload,
}

impl std::fmt::Display for DialogType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DialogType::Alert => write!(f, "alert"),
            DialogType::Confirm => write!(f, "confirm"),
            DialogType::Prompt => write!(f, "prompt"),
            DialogType::BeforeUnload => write!(f, "beforeunload"),
        }
    }
}

/// Represents a captured dialog event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dialog {
    pub dialog_type: DialogType,
    pub message: String,
    pub default_prompt: Option<String>,
    pub url: String,
    pub timestamp: DateTime<Utc>,
}

impl EventMetadata for Dialog {
    fn event_type(&self) -> &'static str {
        "dialog"
    }
    fn timestamp_ms(&self) -> Option<u64> {
        Some(self.timestamp.timestamp_millis() as u64)
    }
}

/// Represents the result of auto-handling a dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogResult {
    pub dialog_type: DialogType,
    pub message: String,
    pub action: String,
    pub prompt_text: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl EventMetadata for DialogResult {
    fn event_type(&self) -> &'static str {
        "dialog_result"
    }
    fn timestamp_ms(&self) -> Option<u64> {
        Some(self.timestamp.timestamp_millis() as u64)
    }
}

pub struct DialogCollector {
    storage: Arc<SessionStorage>,
    behavior: DialogBehavior,
    prompt_text: Option<String>,
}

impl DialogCollector {
    const DIALOG_COLLECTION: &'static str = "dialog";
    const RESULT_COLLECTION: &'static str = "dialog_result";

    pub fn new(
        storage: Arc<SessionStorage>,
        behavior: DialogBehavior,
        prompt_text: Option<String>,
    ) -> Self {
        Self {
            storage,
            behavior,
            prompt_text,
        }
    }

    /// Attaches dialog event listener and auto-handles dialogs within the same session.
    ///
    /// This is the **fundamental solution** to the cross-session dialog problem.
    /// By handling dialogs in the same tokio::spawn that receives the event,
    /// we ensure the dialog is processed within the same CDP session.
    pub async fn attach(&self, page: &Arc<Page>) -> Result<()> {
        let storage = self.storage.clone();
        let behavior = self.behavior;
        let prompt_text = self.prompt_text.clone();
        let page = page.clone(); // Clone Arc<Page> for use in tokio::spawn

        let mut stream = page
            .event_listener::<EventJavascriptDialogOpening>()
            .await
            .map_err(|e| {
                ChromeError::General(format!("Failed to attach dialog listener: {}", e))
            })?;

        tokio::spawn(async move {
            while let Some(event) = stream.next().await {
                let dialog_type = match event.r#type {
                    CdpDialogType::Alert => DialogType::Alert,
                    CdpDialogType::Confirm => DialogType::Confirm,
                    CdpDialogType::Prompt => DialogType::Prompt,
                    CdpDialogType::Beforeunload => DialogType::BeforeUnload,
                };

                let dialog = Dialog {
                    dialog_type,
                    message: event.message.clone(),
                    default_prompt: event.default_prompt.clone(),
                    url: event.url.clone(),
                    timestamp: Utc::now(),
                };

                // Step 1: Clear previous dialog and save new one
                Self::clear_dialog_storage(&storage);
                storage.append(Self::DIALOG_COLLECTION, &dialog).ok();

                // Step 2: Auto-handle dialog based on behavior (SAME SESSION!)
                let (action, text_used) = match behavior {
                    DialogBehavior::Accept => {
                        let text = prompt_text.clone();
                        let result = Self::handle_dialog_internal(&page, true, text.clone()).await;
                        if result.is_ok() {
                            ("accepted".to_string(), text)
                        } else {
                            ("error".to_string(), None)
                        }
                    }
                    DialogBehavior::Dismiss => {
                        let result = Self::handle_dialog_internal(&page, false, None).await;
                        if result.is_ok() {
                            ("dismissed".to_string(), None)
                        } else {
                            ("error".to_string(), None)
                        }
                    }
                    DialogBehavior::None => {
                        // Don't auto-handle - page will stall until manually handled
                        ("pending".to_string(), None)
                    }
                };

                // Step 3: Save result
                let result = DialogResult {
                    dialog_type,
                    message: event.message.clone(),
                    action,
                    prompt_text: text_used,
                    timestamp: Utc::now(),
                };

                Self::clear_result_storage(&storage);
                storage.append(Self::RESULT_COLLECTION, &result).ok();

                // Step 4: Clear dialog storage after handling (unless pending)
                if behavior != DialogBehavior::None {
                    Self::clear_dialog_storage(&storage);
                }
            }
        });

        Ok(())
    }

    /// Internal dialog handling - called within the same CDP session.
    async fn handle_dialog_internal(
        page: &Page,
        accept: bool,
        prompt_text: Option<String>,
    ) -> std::result::Result<(), String> {
        let mut builder = HandleJavaScriptDialogParams::builder();
        builder = builder.accept(accept);

        if let Some(text) = prompt_text {
            builder = builder.prompt_text(text);
        }

        let params = builder.build().map_err(|e| e.to_string())?;

        page.execute(params).await.map_err(|e| e.to_string())?;

        Ok(())
    }

    fn clear_dialog_storage(storage: &SessionStorage) {
        let path = storage
            .session_dir()
            .join(format!("{}.ndjson", Self::DIALOG_COLLECTION));
        fs::remove_file(&path).ok();
    }

    fn clear_result_storage(storage: &SessionStorage) {
        let path = storage
            .session_dir()
            .join(format!("{}.ndjson", Self::RESULT_COLLECTION));
        fs::remove_file(&path).ok();
    }

    /// Get the most recent dialog (for inspection).
    /// Returns None if dialog was already auto-handled.
    pub fn get(&self) -> Option<Dialog> {
        self.storage
            .read_all::<Dialog>(Self::DIALOG_COLLECTION)
            .ok()
            .and_then(|dialogs| dialogs.into_iter().last())
    }

    /// Get the most recent dialog result.
    pub fn get_result(&self) -> Option<DialogResult> {
        self.storage
            .read_all::<DialogResult>(Self::RESULT_COLLECTION)
            .ok()
            .and_then(|results| results.into_iter().last())
    }

    /// Clear dialog storage.
    pub fn clear(&self) {
        Self::clear_dialog_storage(&self.storage);
    }

    /// Clear result storage.
    pub fn clear_result(&self) {
        Self::clear_result_storage(&self.storage);
    }

    /// Manually handle a dialog (only works if behavior is "none").
    /// Note: This is a fallback for manual mode - will likely fail due to
    /// cross-session limitations unless called from the same session.
    pub async fn handle(
        &self,
        page: &Arc<Page>,
        accept: bool,
        prompt_text: Option<String>,
    ) -> Result<()> {
        let mut builder = HandleJavaScriptDialogParams::builder();
        builder = builder.accept(accept);

        if let Some(text) = prompt_text {
            builder = builder.prompt_text(text);
        }

        let params = builder
            .build()
            .map_err(|e| ChromeError::General(format!("Failed to build dialog params: {}", e)))?;

        page.execute(params)
            .await
            .map_err(|e| ChromeError::General(format!("Failed to handle dialog: {}", e)))?;

        self.clear();
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.storage.count(Self::DIALOG_COLLECTION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_type_display() {
        assert_eq!(DialogType::Alert.to_string(), "alert");
        assert_eq!(DialogType::Confirm.to_string(), "confirm");
        assert_eq!(DialogType::Prompt.to_string(), "prompt");
        assert_eq!(DialogType::BeforeUnload.to_string(), "beforeunload");
    }
}
