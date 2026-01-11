use crate::chrome::collectors::{ExtensionEvent, TargetInfo};
use crate::chrome::event_store::EventMetadata;
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
        match &self.output {
            Some(path) => format!(
                "Exported {} events to {} ({})",
                self.events_processed, path, self.format
            ),
            None => self.script.clone(),
        }
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        let result = if pretty {
            serde_json::to_string_pretty(self)
        } else {
            serde_json::to_string(self)
        };
        result.map_err(|e| ChromeError::General(e.to_string()))
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

    let (events, rec_id) = match recording_id {
        Some(rid) => (filter_by_recording(&all_events, rid), Some(rid.to_string())),
        None => {
            let rid = find_latest_recording_id(&all_events);
            match &rid {
                Some(r) => (filter_by_recording(&all_events, r), rid),
                None => (all_events, None),
            }
        }
    };

    if events.is_empty() {
        return Err(ChromeError::General("No events found for export".into()));
    }

    let script = PlaywrightGenerator::generate(&events);

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
    events.iter().rev().find_map(|e| match e {
        ExtensionEvent::RecordingStart(m) | ExtensionEvent::RecordingStop(m) => {
            Some(m.recording_id.clone())
        }
        _ => None,
    })
}

fn filter_by_recording(events: &[ExtensionEvent], recording_id: &str) -> Vec<ExtensionEvent> {
    let (start_ts, end_ts) = events
        .iter()
        .fold((None, None), |(start, end), event| match event {
            ExtensionEvent::RecordingStart(m) if m.recording_id == recording_id => {
                (Some(m.ts), end)
            }
            ExtensionEvent::RecordingStop(m) if m.recording_id == recording_id => {
                (start, Some(m.ts))
            }
            _ => (start, end),
        });

    let (start, end) = match (start_ts, end_ts) {
        (Some(s), Some(e)) => (s, e),
        (Some(s), None) => (s, u64::MAX),
        _ => return Vec::new(),
    };

    events
        .iter()
        .filter(|e| e.timestamp_ms().is_some_and(|ts| ts >= start && ts <= end))
        .cloned()
        .collect()
}

struct PlaywrightGenerator {
    lines: Vec<String>,
    last_url: Option<String>,
}

impl PlaywrightGenerator {
    fn generate(events: &[ExtensionEvent]) -> String {
        let mut ctx = Self {
            lines: Vec::with_capacity(events.len() * 3),
            last_url: None,
        };

        let test_name = ctx.infer_test_name(events);
        ctx.lines
            .push("import { test, expect } from '@playwright/test';".into());
        ctx.lines.push(String::new());
        ctx.lines
            .push(format!("test('{test_name}', async ({{ page }}) => {{"));

        let merged = Self::merge_events(events);
        for (i, event) in merged.iter().enumerate() {
            let next = merged.get(i + 1);
            ctx.emit_event(event, next);
        }

        ctx.lines.push("});".into());
        ctx.lines.join("\n")
    }

    fn merge_events(events: &[ExtensionEvent]) -> Vec<ExtensionEvent> {
        let mut result = Vec::with_capacity(events.len());
        let mut i = 0;

        while i < events.len() {
            let current = &events[i];
            let next = events.get(i + 1);

            // Skip recording markers
            if matches!(
                current,
                ExtensionEvent::RecordingStart(_) | ExtensionEvent::RecordingStop(_)
            ) {
                i += 1;
                continue;
            }

            // Skip duplicate clicks on same element within 500ms
            if let ExtensionEvent::Click(target) = current
                && let Some(ExtensionEvent::Click(next_target)) = next
                && Self::is_same_element(target, next_target)
                && Self::within_threshold(target.ts, next_target.ts, 500)
            {
                i += 1;
                continue;
            }

            // Skip click if followed by input on same element (fill() auto-focuses)
            if let ExtensionEvent::Click(click_target) = current
                && let Some(ExtensionEvent::Input(input_data)) = next
                && Self::is_same_element(click_target, &input_data.target)
            {
                i += 1;
                continue;
            }

            // Skip navigate after keypress Enter (form already submitted)
            if let ExtensionEvent::KeyPress(kp) = current
                && kp.key == "Enter"
                && let Some(ExtensionEvent::Navigate(_)) = next
            {
                result.push(current.clone());
                i += 2; // Skip both current and navigate
                continue;
            }

            result.push(current.clone());
            i += 1;
        }

        result
    }

    fn is_same_element(a: &TargetInfo, b: &TargetInfo) -> bool {
        (a.css.is_some() && a.css == b.css) || (a.xpath.is_some() && a.xpath == b.xpath)
    }

    fn within_threshold(ts1: Option<u64>, ts2: Option<u64>, threshold_ms: u64) -> bool {
        match (ts1, ts2) {
            (Some(t1), Some(t2)) => t2.saturating_sub(t1) < threshold_ms,
            _ => false,
        }
    }

    fn infer_test_name(&self, events: &[ExtensionEvent]) -> &'static str {
        let has_login = events.iter().any(|e| match e {
            ExtensionEvent::Navigate(d) => {
                d.url.contains("login") || d.url.contains("signin") || d.url.contains("auth")
            }
            ExtensionEvent::Input(d) => d
                .target
                .aria
                .iter()
                .any(|a| a.to_lowercase().contains("password")),
            _ => false,
        });

        let has_form = events.iter().any(|e| matches!(e, ExtensionEvent::Input(_)));
        let has_search = events.iter().any(|e| match e {
            ExtensionEvent::Navigate(d) => d.url.contains("search"),
            _ => false,
        });

        if has_login {
            "user authentication flow"
        } else if has_search && has_form {
            "search flow"
        } else if has_form {
            "form submission flow"
        } else {
            "recorded user flow"
        }
    }

    fn emit_event(&mut self, event: &ExtensionEvent, next: Option<&ExtensionEvent>) {
        match event {
            ExtensionEvent::Navigate(data) => self.emit_navigate(&data.url),
            ExtensionEvent::Click(target) => self.emit_click(target, next),
            ExtensionEvent::Input(data) => self.emit_input(&data.target, data.value.as_deref()),
            ExtensionEvent::KeyPress(data) => self.emit_keypress(&data.key, next),
            ExtensionEvent::Scroll(data) => self.emit_scroll(data.x, data.y),
            ExtensionEvent::Select(target) => self.emit_select(target),
            ExtensionEvent::Hover(target) => self.emit_hover(target),
            ExtensionEvent::Screenshot(data) => self.emit_screenshot(&data.filename),
            ExtensionEvent::Dialog(data) => self.emit_dialog(data.ok),
            ExtensionEvent::Snapshot(_) => {}
            ExtensionEvent::RecordingStart(_) | ExtensionEvent::RecordingStop(_) => {}
        }
    }

    fn emit_navigate(&mut self, url: &str) {
        if self.last_url.as_deref() == Some(url) {
            return;
        }
        self.last_url = Some(url.to_string());
        self.add(format!("await page.goto('{}');", escape_string(url)));
        self.add(format!(
            "await expect(page).toHaveURL(/{}/);",
            url_to_pattern(url)
        ));
        self.add_empty();
    }

    fn emit_click(&mut self, target: &TargetInfo, next: Option<&ExtensionEvent>) {
        let locator = to_locator(target);
        self.add(format!("await expect({locator}).toBeVisible();"));
        self.add(format!("await {locator}.click();"));
        if Self::triggers_navigation(target, next) {
            self.add("await page.waitForLoadState('networkidle');");
        }
        self.add_empty();
    }

    fn emit_input(&mut self, target: &TargetInfo, value: Option<&str>) {
        let locator = to_locator(target);
        let val = value.unwrap_or("");
        self.add(format!("await expect({locator}).toBeVisible();"));
        self.add(format!("await {locator}.fill('{}');", escape_string(val)));
        if !val.is_empty() {
            self.add(format!(
                "await expect({locator}).toHaveValue('{}');",
                escape_string(val)
            ));
        }
        self.add_empty();
    }

    fn emit_keypress(&mut self, key: &str, next: Option<&ExtensionEvent>) {
        self.add(format!(
            "await page.keyboard.press('{}');",
            escape_string(key)
        ));
        if key == "Enter" && matches!(next, Some(ExtensionEvent::Navigate(_))) {
            self.add("await page.waitForLoadState('networkidle');");
        }
        self.add_empty();
    }

    fn emit_scroll(&mut self, x: i32, y: i32) {
        self.add(format!("await page.mouse.wheel({x}, {y});"));
    }

    fn emit_select(&mut self, target: &TargetInfo) {
        let locator = to_locator(target);
        self.add(format!("await {locator}.click();"));
    }

    fn emit_hover(&mut self, target: &TargetInfo) {
        let locator = to_locator(target);
        self.add(format!("await {locator}.hover();"));
    }

    fn emit_screenshot(&mut self, filename: &str) {
        self.add(format!(
            "await page.screenshot({{ path: '{}' }});",
            escape_string(filename)
        ));
    }

    fn emit_dialog(&mut self, accept: bool) {
        let action = if accept { "accept" } else { "dismiss" };
        self.add(format!("page.on('dialog', dialog => dialog.{action}());"));
    }

    fn triggers_navigation(target: &TargetInfo, next: Option<&ExtensionEvent>) -> bool {
        let is_submit = target.aria.iter().any(|a| {
            let lower = a.to_lowercase();
            lower.contains("submit")
                || lower.contains("login")
                || lower.contains("sign")
                || lower.contains("search")
        });
        is_submit || matches!(next, Some(ExtensionEvent::Navigate(_)))
    }

    fn add(&mut self, line: impl Into<String>) {
        self.lines.push(format!("  {}", line.into()));
    }

    fn add_empty(&mut self) {
        self.lines.push(String::new());
    }
}

fn to_locator(target: &TargetInfo) -> String {
    if let Some(ref testid) = target.testid {
        return format!("page.getByTestId('{}')", escape_string(testid));
    }

    if target.aria.len() >= 2 {
        let (role, name) = (&target.aria[0], &target.aria[1]);
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

    "page.locator('body')".into()
}

fn url_to_pattern(url: &str) -> String {
    let Ok(parsed) = url::Url::parse(url) else {
        return escape_regex(url);
    };

    let host = parsed.host_str().unwrap_or("");
    let path = parsed.path();

    if path.len() > 1 {
        let segment = path.split('/').find(|s| !s.is_empty()).unwrap_or("");
        if !segment.is_empty() {
            return format!("{}.*{}", escape_regex(host), escape_regex(segment));
        }
    }

    escape_regex(host)
}

fn escape_regex(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '[' | ']' | '|' | '(' | ')' | '\\'
            | '/' => format!("\\{c}"),
            _ => c.to_string(),
        })
        .collect()
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::collectors::extension::{InputData, KeyPressData, NavigateData};

    fn make_target(aria: &[&str], testid: Option<&str>, css: Option<&str>) -> TargetInfo {
        TargetInfo {
            aria: aria.iter().map(|s| s.to_string()).collect(),
            css: css.map(String::from),
            xpath: None,
            testid: testid.map(String::from),
            text: None,
            rect: None,
            url: None,
            ts: None,
        }
    }

    #[test]
    fn test_locator_priority() {
        let testid = make_target(&[], Some("submit"), None);
        assert_eq!(to_locator(&testid), "page.getByTestId('submit')");

        let aria = make_target(&["button", "Submit"], None, Some("#btn"));
        assert_eq!(
            to_locator(&aria),
            "page.getByRole('button', { name: 'Submit', exact: true })"
        );

        let css = make_target(&[], None, Some("#submit-btn"));
        assert_eq!(to_locator(&css), "page.locator('#submit-btn')");
    }

    #[test]
    fn test_url_pattern() {
        assert_eq!(url_to_pattern("https://example.com"), "example\\.com");
        assert_eq!(
            url_to_pattern("https://google.com/search?q=test"),
            "google\\.com.*search"
        );
        assert_eq!(
            url_to_pattern("https://42dot.ai/careers"),
            "42dot\\.ai.*careers"
        );
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string("hello"), "hello");
        assert_eq!(escape_string("it's"), "it\\'s");
        assert_eq!(escape_string("path\\to"), "path\\\\to");
    }

    #[test]
    fn test_escape_regex() {
        assert_eq!(escape_regex("test.com"), "test\\.com");
        assert_eq!(escape_regex("a/b/c"), "a\\/b\\/c");
    }

    #[test]
    fn test_generate_script() {
        let events = vec![
            ExtensionEvent::Navigate(NavigateData {
                url: "https://example.com".into(),
                from: None,
                nav_type: "link".into(),
                ts: 0,
            }),
            ExtensionEvent::Click(make_target(&["button", "Submit"], None, None)),
            ExtensionEvent::Input(InputData {
                target: make_target(&["textbox", "Email"], None, None),
                value: Some("test@example.com".into()),
            }),
            ExtensionEvent::KeyPress(KeyPressData {
                key: "Enter".into(),
                aria: None,
                css: None,
                xpath: None,
                testid: None,
                url: None,
                ts: Some(300),
            }),
        ];

        let script = PlaywrightGenerator::generate(&events);
        assert!(script.contains("import { test, expect }"));
        assert!(script.contains("page.goto('https://example.com')"));
        assert!(script.contains("getByRole('button', { name: 'Submit', exact: true }).click()"));
        assert!(script.contains(
            "getByRole('textbox', { name: 'Email', exact: true }).fill('test@example.com')"
        ));
        assert!(script.contains("page.keyboard.press('Enter')"));
    }
}
