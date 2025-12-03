//! Dialog Handler - Provides dialog information to CLI users.
//!
//! # Important
//!
//! Dialogs are **automatically handled** within the same CDP session
//! that receives them (following Playwright's pattern). By default,
//! dialogs are auto-dismissed.
//!
//! This handler allows users to:
//! 1. Inspect the most recent dialog that was handled
//! 2. View the result of auto-handling (accepted/dismissed)
//!
//! The `dialog --accept` and `dialog --dismiss` commands are kept for
//! compatibility but will only work if dialog behavior is set to "none".

use crate::{
    ChromeError, Result,
    chrome::{PageProvider, collectors::DialogType},
    output,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DialogInfo {
    pub dialog_type: String,
    pub message: String,
    pub default_value: Option<String>,
    pub url: Option<String>,
    pub auto_handled: bool,
    pub auto_action: Option<String>,
}

impl output::OutputFormatter for DialogInfo {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut lines = vec![
            text::key_value("Type", &self.dialog_type),
            text::key_value("Message", &self.message),
        ];

        if let Some(ref default_val) = self.default_value {
            lines.push(text::key_value("Default", default_val));
        }

        if let Some(ref url) = self.url {
            lines.push(text::key_value("URL", url));
        }

        if self.auto_handled
            && let Some(ref action) = self.auto_action
        {
            lines.push(text::key_value("Auto-handled", action));
        }

        lines.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

#[derive(Debug, Serialize)]
pub struct DialogResult {
    pub action: String,
    pub status: String,
}

impl output::OutputFormatter for DialogResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}",
            text::success(&format!("Dialog {}", self.action)),
            text::key_value("Status", &self.status)
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_get_dialog(provider: &impl PageProvider) -> Result<DialogInfo> {
    // Ensure browser/page is connected so dialog collector can receive events
    let _ = provider.get_or_create_page().await?;

    // First check if there's a pending dialog (behavior = "none" mode)
    if let Some(dialog) = provider.collectors().dialog.get() {
        let dialog_type = match dialog.dialog_type {
            DialogType::Alert => "alert",
            DialogType::Confirm => "confirm",
            DialogType::Prompt => "prompt",
            DialogType::BeforeUnload => "beforeunload",
        };

        return Ok(DialogInfo {
            dialog_type: dialog_type.to_string(),
            message: dialog.message,
            default_value: dialog.default_prompt,
            url: Some(dialog.url),
            auto_handled: false,
            auto_action: None,
        });
    }

    // Check if there was a recently auto-handled dialog
    if let Some(result) = provider.collectors().dialog.get_result() {
        let dialog_type = match result.dialog_type {
            DialogType::Alert => "alert",
            DialogType::Confirm => "confirm",
            DialogType::Prompt => "prompt",
            DialogType::BeforeUnload => "beforeunload",
        };

        return Ok(DialogInfo {
            dialog_type: dialog_type.to_string(),
            message: result.message,
            default_value: result.prompt_text,
            url: None,
            auto_handled: true,
            auto_action: Some(result.action),
        });
    }

    Err(ChromeError::General("No dialog found. Dialogs are auto-handled by default (Playwright-style). Set dialog.behavior = \"none\" in config to handle manually.".to_string()))
}

pub async fn handle_dialog_action(
    provider: &impl PageProvider,
    accept: bool,
    prompt_text: Option<String>,
) -> Result<DialogResult> {
    // Ensure browser/page is connected
    let page = provider.get_or_create_page().await?;

    // Check if there's a pending dialog
    if provider.collectors().dialog.get().is_none() {
        // Check if it was already auto-handled
        if let Some(result) = provider.collectors().dialog.get_result() {
            return Ok(DialogResult {
                action: result.action.clone(),
                status: format!(
                    "Dialog was already auto-{} (behavior: auto). Set dialog.behavior = \"none\" in config to handle manually.",
                    result.action
                ),
            });
        }

        return Err(ChromeError::General(
            "No open dialog found. Dialogs are auto-handled by default.".to_string(),
        ));
    }

    // Handle the dialog using the collector's method
    provider
        .collectors()
        .dialog
        .handle(&page, accept, prompt_text)
        .await?;

    Ok(DialogResult {
        action: if accept { "accepted" } else { "dismissed" }.to_string(),
        status: format!(
            "Dialog {} successfully",
            if accept { "accepted" } else { "dismissed" }
        ),
    })
}
