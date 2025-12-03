use crate::chrome::collectors::extension::{ExtensionEvent, TargetInfo};
use crate::chrome::storage::SessionStorage;
use crate::output::OutputFormatter;
use crate::{ChromeError, Result};
use serde::Serialize;
use std::fs;

#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub session_id: String,
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
    let events: Vec<ExtensionEvent> = storage.read_all("extension")?;

    if events.is_empty() {
        return Err(ChromeError::General(
            "No extension events recorded in this session".to_string(),
        ));
    }

    let script = generate_playwright_script(&events);

    if let Some(ref path) = output {
        fs::write(path, &script)
            .map_err(|e| ChromeError::General(format!("Failed to write file: {}", e)))?;
    }

    Ok(ExportResult {
        session_id: session_id.to_string(),
        format: format.to_string(),
        events_processed: events.len(),
        output,
        script,
    })
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
        ExtensionEvent::Click(click) => {
            let locator = target_to_locator(click);
            Some(format!("await {}.click();", locator))
        }
        ExtensionEvent::Input(input) => {
            let locator = target_to_locator(&input.target);
            let value = input.value.as_deref().unwrap_or("");
            Some(format!(
                "await {}.fill('{}');",
                locator,
                escape_string(value)
            ))
        }
        ExtensionEvent::Hover(hover) => {
            let locator = target_to_locator(hover);
            Some(format!("await {}.hover();", locator))
        }
        ExtensionEvent::Scroll(scroll) => {
            if let Some(ref target) = scroll.target {
                let locator = aria_to_locator(target);
                Some(format!(
                    "await {}.evaluate(el => el.scrollBy({}, {}));",
                    locator, scroll.x, scroll.y
                ))
            } else {
                Some(format!(
                    "await page.mouse.wheel({}, {});",
                    scroll.x, scroll.y
                ))
            }
        }
        ExtensionEvent::KeyPress(key_data) => Some(format!(
            "await page.keyboard.press('{}');",
            escape_string(&key_data.key)
        )),
        ExtensionEvent::Select(select) => {
            if let Some(ref url) = select.url
                && last_url.as_ref() != Some(url)
            {
                *last_url = Some(url.clone());
                return Some(format!("await page.goto('{}');", escape_string(url)));
            }
            None
        }
        ExtensionEvent::Navigate(nav) => {
            if last_url.as_ref() != Some(&nav.url) {
                *last_url = Some(nav.url.clone());
                Some(format!("await page.goto('{}');", escape_string(&nav.url)))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn target_to_locator(target: &TargetInfo) -> String {
    // Priority: data-testid > ARIA role+name > text > CSS
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

fn aria_to_locator(aria: &[String]) -> String {
    if aria.len() >= 2 && !aria[0].is_empty() && !aria[1].is_empty() {
        format!(
            "page.getByRole('{}', {{ name: '{}', exact: true }})",
            escape_string(&aria[0]),
            escape_string(&aria[1])
        )
    } else {
        "page.locator('body')".to_string()
    }
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::collectors::extension::{InputData, KeyPressData};

    #[test]
    fn test_target_to_locator_testid() {
        let target = TargetInfo {
            aria: vec![],
            css: None,
            xpath: None,
            testid: Some("submit-btn".to_string()),
            text: None,
            rect: None,
            url: None,
            ts: None,
        };
        assert_eq!(target_to_locator(&target), "page.getByTestId('submit-btn')");
    }

    #[test]
    fn test_target_to_locator_aria() {
        let target = TargetInfo {
            aria: vec!["button".to_string(), "Submit".to_string()],
            css: Some("#btn".to_string()),
            xpath: None,
            testid: None,
            text: Some("Submit".to_string()),
            rect: None,
            url: None,
            ts: None,
        };
        assert_eq!(
            target_to_locator(&target),
            "page.getByRole('button', { name: 'Submit', exact: true })"
        );
    }

    #[test]
    fn test_target_to_locator_text() {
        let target = TargetInfo {
            aria: vec![],
            css: Some("#btn".to_string()),
            xpath: None,
            testid: None,
            text: Some("Click me".to_string()),
            rect: None,
            url: None,
            ts: None,
        };
        assert_eq!(target_to_locator(&target), "page.getByText('Click me')");
    }

    #[test]
    fn test_target_to_locator_css() {
        let target = TargetInfo {
            aria: vec![],
            css: Some("#submit-btn".to_string()),
            xpath: None,
            testid: None,
            text: None,
            rect: None,
            url: None,
            ts: None,
        };
        assert_eq!(target_to_locator(&target), "page.locator('#submit-btn')");
    }

    #[test]
    fn test_generate_playwright_script() {
        let events = vec![
            ExtensionEvent::Click(TargetInfo {
                aria: vec!["button".to_string(), "Submit".to_string()],
                css: None,
                xpath: None,
                testid: None,
                text: None,
                rect: None,
                url: None,
                ts: None,
            }),
            ExtensionEvent::Input(InputData {
                target: TargetInfo {
                    aria: vec!["textbox".to_string(), "Email".to_string()],
                    css: None,
                    xpath: None,
                    testid: None,
                    text: None,
                    rect: None,
                    url: None,
                    ts: None,
                },
                value: Some("test@example.com".to_string()),
            }),
            ExtensionEvent::KeyPress(KeyPressData {
                key: "Enter".to_string(),
                modifiers: None,
                ts: None,
            }),
        ];

        let script = generate_playwright_script(&events);
        assert!(script.contains("import { test, expect }"));
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
