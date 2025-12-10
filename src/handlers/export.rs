use crate::chrome::collectors::{ExtensionEvent, TargetInfo};
use crate::chrome::storage::SessionStorage;
use crate::output::OutputFormatter;
use crate::{ChromeError, Result};
use serde::Serialize;
use std::fs;

pub use crate::chrome::collectors::extension::TargetInfo as ElementTarget;

#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub session_id: String,
    pub recording_id: Option<String>,
    pub format: String,
    pub events_processed: usize,
    pub output: Option<String>,
    pub script: String,
}

impl OutputFormatter for ExportResult {
    fn format_text(&self) -> String {
        if let Some(ref path) = self.output {
            format!(
                "Exported {} events to {} ({})",
                self.events_processed, path, self.format
            )
        } else {
            self.script.clone()
        }
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        if pretty {
            serde_json::to_string_pretty(self).map_err(|e| ChromeError::General(e.to_string()))
        } else {
            serde_json::to_string(self).map_err(|e| ChromeError::General(e.to_string()))
        }
    }
}

pub fn handle_export(
    session_id: &str,
    recording_id: Option<&str>,
    format: &str,
    output: Option<String>,
) -> Result<ExportResult> {
    if format != "playwright" {
        return Err(ChromeError::General(format!(
            "Unsupported format: {}. Supported: playwright",
            format
        )));
    }

    let storage = SessionStorage::from_session_id(session_id)?;
    let all_events: Vec<ExtensionEvent> = storage.read_all("extension")?;

    let (events, rec_id) = if let Some(rid) = recording_id {
        (filter_by_recording(&all_events, rid), Some(rid.to_string()))
    } else {
        let rec_id = find_latest_recording_id(&all_events);
        if let Some(ref rid) = rec_id {
            (filter_by_recording(&all_events, rid), rec_id)
        } else {
            (all_events, None)
        }
    };

    if events.is_empty() {
        return Err(ChromeError::General(
            "No events found for export".to_string(),
        ));
    }

    let script = generate_playwright_script(&events);

    if let Some(ref path) = output {
        fs::write(path, &script)
            .map_err(|e| ChromeError::General(format!("Failed to write file: {e}")))?;
    }

    Ok(ExportResult {
        session_id: session_id.to_string(),
        recording_id: rec_id,
        format: format.to_string(),
        events_processed: events.len(),
        output,
        script,
    })
}

fn find_latest_recording_id(events: &[ExtensionEvent]) -> Option<String> {
    events.iter().rev().find_map(|e| {
        if let ExtensionEvent::RecordingStart(m) | ExtensionEvent::RecordingStop(m) = e {
            Some(m.recording_id.clone())
        } else {
            None
        }
    })
}

fn filter_by_recording(events: &[ExtensionEvent], recording_id: &str) -> Vec<ExtensionEvent> {
    let mut start_ts: Option<u64> = None;
    let mut end_ts: Option<u64> = None;

    for event in events {
        match event {
            ExtensionEvent::RecordingStart(m) if m.recording_id == recording_id => {
                start_ts = Some(m.ts);
            }
            ExtensionEvent::RecordingStop(m) if m.recording_id == recording_id => {
                end_ts = Some(m.ts);
            }
            _ => {}
        }
    }

    let (start, end) = match (start_ts, end_ts) {
        (Some(s), Some(e)) => (s, e),
        (Some(s), None) => (s, u64::MAX),
        _ => return Vec::new(),
    };

    events
        .iter()
        .filter(|e| {
            if let Some(ts) = e.timestamp_ms() {
                ts >= start && ts <= end
            } else {
                false
            }
        })
        .cloned()
        .collect()
}

fn generate_playwright_script(events: &[ExtensionEvent]) -> String {
    let mut lines = vec![
        "import { test, expect } from '@playwright/test';".to_string(),
        String::new(),
        "test('recorded session', async ({ page }) => {".to_string(),
    ];

    let mut last_url: Option<String> = None;

    for event in events {
        if let Some(code) = event_to_playwright(event, &mut last_url) {
            lines.push(format!("  {}", code));
        }
    }

    lines.push("});".to_string());
    lines.join("\n")
}

fn event_to_playwright(event: &ExtensionEvent, last_url: &mut Option<String>) -> Option<String> {
    match event {
        ExtensionEvent::Navigate(data) => {
            if last_url.as_ref() != Some(&data.url) {
                *last_url = Some(data.url.clone());
                Some(format!("await page.goto('{}');", escape_string(&data.url)))
            } else {
                None
            }
        }
        ExtensionEvent::Click(target) => {
            let locator = target_to_locator(target);
            Some(format!("await {}.click();", locator))
        }
        ExtensionEvent::Input(data) => {
            let locator = target_to_locator(&data.target);
            let value = data.value.as_deref().unwrap_or("");
            Some(format!(
                "await {}.fill('{}');",
                locator,
                escape_string(value)
            ))
        }
        ExtensionEvent::Scroll(data) => {
            Some(format!("await page.mouse.wheel({}, {});", data.x, data.y))
        }
        ExtensionEvent::KeyPress(data) => Some(format!(
            "await page.keyboard.press('{}');",
            escape_string(&data.key)
        )),
        ExtensionEvent::Select(target) => {
            let locator = target_to_locator(target);
            Some(format!("await {}.click();", locator))
        }
        ExtensionEvent::Hover(_)
        | ExtensionEvent::Screenshot(_)
        | ExtensionEvent::Snapshot(_)
        | ExtensionEvent::Dialog(_)
        | ExtensionEvent::RecordingStart(_)
        | ExtensionEvent::RecordingStop(_) => None,
    }
}

fn target_to_locator(target: &TargetInfo) -> String {
    if let Some(ref testid) = target.testid {
        return format!("page.getByTestId('{}')", escape_string(testid));
    }

    if target.aria.len() >= 2 {
        let role = &target.aria[0];
        let name = &target.aria[1];
        if !role.is_empty() && !name.is_empty() {
            return format!(
                "page.getByRole('{}', {{ name: '{}', exact: true }})",
                escape_string(role),
                escape_string(name)
            );
        }
    }

    if let Some(ref text) = target.text
        && !text.is_empty()
    {
        return format!("page.getByText('{}')", escape_string(text));
    }

    if let Some(ref css) = target.css {
        return format!("page.locator('{}')", escape_string(css));
    }

    "page.locator('body')".to_string()
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::collectors::extension::{InputData, KeyPressData, NavigateData};

    fn make_target(
        aria: Vec<&str>,
        testid: Option<&str>,
        text: Option<&str>,
        css: Option<&str>,
    ) -> TargetInfo {
        TargetInfo {
            aria: aria.into_iter().map(String::from).collect(),
            css: css.map(String::from),
            xpath: None,
            testid: testid.map(String::from),
            text: text.map(String::from),
            rect: None,
            url: None,
            ts: None,
        }
    }

    #[test]
    fn test_target_to_locator_testid() {
        let target = make_target(vec![], Some("submit-btn"), None, None);
        assert_eq!(target_to_locator(&target), "page.getByTestId('submit-btn')");
    }

    #[test]
    fn test_target_to_locator_aria() {
        let target = make_target(vec!["button", "Submit"], None, Some("Submit"), Some("#btn"));
        assert_eq!(
            target_to_locator(&target),
            "page.getByRole('button', { name: 'Submit', exact: true })"
        );
    }

    #[test]
    fn test_target_to_locator_text() {
        let target = make_target(vec![], None, Some("Click me"), Some("#btn"));
        assert_eq!(target_to_locator(&target), "page.getByText('Click me')");
    }

    #[test]
    fn test_target_to_locator_css() {
        let target = make_target(vec![], None, None, Some("#submit-btn"));
        assert_eq!(target_to_locator(&target), "page.locator('#submit-btn')");
    }

    #[test]
    fn test_generate_playwright_script() {
        let events = vec![
            ExtensionEvent::Navigate(NavigateData {
                url: "https://example.com".to_string(),
                from: None,
                nav_type: "link".to_string(),
                ts: 0,
            }),
            ExtensionEvent::Click(make_target(vec!["button", "Submit"], None, None, None)),
            ExtensionEvent::Input(InputData {
                target: make_target(vec!["textbox", "Email"], None, None, None),
                value: Some("test@example.com".to_string()),
            }),
            ExtensionEvent::KeyPress(KeyPressData {
                key: "Enter".to_string(),
                aria: None,
                css: None,
                xpath: None,
                testid: None,
                url: None,
                ts: Some(300),
            }),
        ];

        let script = generate_playwright_script(&events);
        assert!(script.contains("import { test, expect }"));
        assert!(script.contains("page.goto('https://example.com')"));
        assert!(
            script.contains("page.getByRole('button', { name: 'Submit', exact: true }).click()")
        );
        assert!(script.contains(
            "page.getByRole('textbox', { name: 'Email', exact: true }).fill('test@example.com')"
        ));
        assert!(script.contains("page.keyboard.press('Enter')"));
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string("hello"), "hello");
        assert_eq!(escape_string("it's"), "it\\'s");
        assert_eq!(escape_string("path\\to"), "path\\\\to");
    }
}
