use crate::{ChromeError, Result, chrome::PageProvider, output};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct LabelResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    pub labels: Vec<LabeledElement>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabeledElement {
    pub id: usize,
    pub selector: String,
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub bounds: LabelBounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl output::OutputFormatter for LabelResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut lines = vec![text::success(&format!(
            "Labeled {} interactive elements",
            self.count
        ))];

        if let Some(ref path) = self.screenshot {
            lines.push(text::key_value("Screenshot", path));
        }

        lines.push(String::new());

        for el in &self.labels {
            let mut desc = format!("[{:>2}] <{}>", el.id, el.tag);

            if let Some(ref role) = el.role {
                desc.push_str(&format!(" ({})", role));
            }

            if let Some(ref label) = el.label {
                desc.push_str(&format!(" \"{}\"", output::text::truncate(label, 30)));
            } else if let Some(ref txt) = el.text {
                desc.push_str(&format!(" \"{}\"", output::text::truncate(txt, 30)));
            }

            desc.push_str(&format!(" → {}", el.selector));
            lines.push(desc);
        }

        lines.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

#[derive(Deserialize)]
struct JsLabelResult {
    labels: Vec<JsLabeledElement>,
}

#[derive(Deserialize)]
struct JsLabeledElement {
    id: usize,
    selector: String,
    tag: String,
    role: Option<String>,
    label: Option<String>,
    text: Option<String>,
    bounds: JsBounds,
}

#[derive(Deserialize)]
struct JsBounds {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

pub async fn handle_label(
    provider: &impl PageProvider,
    output_path: Option<&Path>,
    selector: Option<&str>,
) -> Result<LabelResult> {
    let page = provider.get_or_create_page().await?;

    let inject_script = crate::js_templates::label_elements(selector);
    let result = page
        .evaluate(inject_script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let js_result: Option<JsLabelResult> = result.into_value().unwrap_or(None);
    let data = js_result.ok_or_else(|| ChromeError::General("Failed to label elements".into()))?;

    let screenshot_path = if let Some(path) = output_path {
        let screenshot_data = page
            .screenshot(
                chromiumoxide::page::ScreenshotParams::builder()
                    .format(
                        chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png,
                    )
                    .build(),
            )
            .await
            .map_err(|e| ChromeError::General(e.to_string()))?;

        std::fs::write(path, &screenshot_data)?;
        Some(path.to_string_lossy().to_string())
    } else {
        None
    };

    let remove_script = crate::js_templates::remove_labels();
    let _ = page.evaluate(remove_script).await;

    let labels: Vec<LabeledElement> = data
        .labels
        .into_iter()
        .map(|el| LabeledElement {
            id: el.id,
            selector: el.selector,
            tag: el.tag,
            role: el.role,
            label: el.label,
            text: el.text,
            bounds: LabelBounds {
                x: el.bounds.x,
                y: el.bounds.y,
                width: el.bounds.width,
                height: el.bounds.height,
            },
        })
        .collect();

    let count = labels.len();

    Ok(LabelResult {
        screenshot: screenshot_path,
        labels,
        count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputFormatter;

    #[test]
    fn test_label_result_format_text() {
        let result = LabelResult {
            screenshot: Some("labeled.png".to_string()),
            labels: vec![LabeledElement {
                id: 0,
                selector: "#email".to_string(),
                tag: "input".to_string(),
                role: Some("textbox".to_string()),
                label: Some("Email".to_string()),
                text: None,
                bounds: LabelBounds {
                    x: 100,
                    y: 200,
                    width: 200,
                    height: 32,
                },
            }],
            count: 1,
        };

        let text = result.format_text();
        assert!(text.contains("1 interactive elements"));
        assert!(text.contains("labeled.png"));
        assert!(text.contains("<input>"));
        assert!(text.contains("Email"));
    }

    #[test]
    fn test_label_result_json() {
        let result = LabelResult {
            screenshot: None,
            labels: vec![],
            count: 0,
        };

        let json = result.format_json(false).unwrap();
        assert!(json.contains("\"labels\":[]"));
        assert!(json.contains("\"count\":0"));
    }

    #[test]
    fn test_labeled_element_serialization() {
        let el = LabeledElement {
            id: 5,
            selector: "button.submit".to_string(),
            tag: "button".to_string(),
            role: Some("button".to_string()),
            label: None,
            text: Some("Submit".to_string()),
            bounds: LabelBounds {
                x: 50,
                y: 100,
                width: 80,
                height: 40,
            },
        };

        let json = serde_json::to_string(&el).unwrap();
        assert!(json.contains("\"id\":5"));
        assert!(json.contains("\"text\":\"Submit\""));
        assert!(!json.contains("\"label\""));
    }
}
