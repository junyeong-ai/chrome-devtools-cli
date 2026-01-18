use crate::{ChromeError, Result, chrome::PageProvider, js_templates, output};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct DescribeResult {
    pub page: PageInfo,
    pub elements: Vec<VisibleElement>,
    pub summary: Summary,
}

#[derive(Debug, Serialize)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
    pub viewport: Viewport,
}

#[derive(Debug, Serialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct VisibleElement {
    #[serde(rename = "ref")]
    pub ref_id: String,
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub category: ElementCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<ElementState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ElementCategory {
    Interactive,
    Form,
    Navigation,
    Media,
    Text,
    Container,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ElementState {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub checked: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub selected: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub expanded: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub readonly: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(rename = "inViewport")]
    pub in_viewport: bool,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    #[serde(rename = "totalVisible")]
    pub total_visible: usize,
    pub interactive: usize,
    pub forms: usize,
    pub navigation: usize,
    pub truncated: bool,
}

impl output::OutputFormatter for DescribeResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut lines = vec![
            text::success(&format!("Page: {}", self.page.title)),
            text::key_value("URL", &self.page.url),
            text::key_value(
                "Viewport",
                &format!("{}x{}", self.page.viewport.width, self.page.viewport.height),
            ),
            String::new(),
            format!(
                "Elements: {} visible ({} interactive, {} forms, {} navigation){}",
                self.summary.total_visible,
                self.summary.interactive,
                self.summary.forms,
                self.summary.navigation,
                if self.summary.truncated {
                    " [truncated]"
                } else {
                    ""
                }
            ),
            String::new(),
        ];

        for el in &self.elements {
            let mut desc = format!("[{:>3}] <{}>", el.ref_id, el.tag);

            if let Some(ref role) = el.role {
                desc.push_str(&format!(" ({})", role));
            }

            if let Some(ref label) = el.label {
                desc.push_str(&format!(" \"{}\"", output::text::truncate(label, 40)));
            } else if let Some(ref txt) = el.text {
                desc.push_str(&format!(" \"{}\"", output::text::truncate(txt, 40)));
            }

            if let Some(ref selector) = el.selector {
                desc.push_str(&format!(" → {}", output::text::truncate(selector, 50)));
            }

            lines.push(desc);
        }

        lines.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

#[derive(Deserialize)]
struct JsDescribeResult {
    page: JsPageInfo,
    elements: Vec<JsElement>,
    summary: JsSummary,
}

#[derive(Deserialize)]
struct JsPageInfo {
    url: String,
    title: String,
    viewport: JsViewport,
}

#[derive(Deserialize)]
struct JsViewport {
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct JsElement {
    index: usize,
    tag: String,
    role: Option<String>,
    label: Option<String>,
    text: Option<String>,
    category: String,
    state: Option<JsElementState>,
    selector: Option<String>,
    bounds: Option<JsBounds>,
}

#[derive(Deserialize)]
struct JsElementState {
    disabled: bool,
    checked: bool,
    selected: bool,
    expanded: bool,
    readonly: bool,
    required: bool,
}

#[derive(Deserialize)]
struct JsBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    #[serde(rename = "inViewport")]
    in_viewport: bool,
}

#[derive(Deserialize)]
struct JsSummary {
    #[serde(rename = "totalVisible")]
    total_visible: usize,
    interactive: usize,
    forms: usize,
    navigation: usize,
    truncated: bool,
}

#[derive(Debug, Default)]
pub struct DescribeOptions<'a> {
    pub selector: Option<&'a str>,
    pub interactable: bool,
    pub forms: bool,
    pub navigation: bool,
    pub limit: usize,
    pub with_bounds: bool,
    pub with_selectors: bool,
}

impl<'a> DescribeOptions<'a> {
    pub fn new() -> Self {
        Self {
            limit: 100,
            ..Default::default()
        }
    }
}

pub async fn handle_describe(
    provider: &impl PageProvider,
    options: DescribeOptions<'_>,
) -> Result<DescribeResult> {
    let page = provider.get_or_create_page().await?;

    let script = js_templates::describe_visible_elements(
        options.selector,
        options.interactable,
        options.forms,
        options.navigation,
        options.limit,
        options.with_bounds,
        options.with_selectors,
    );

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let js_result: Option<JsDescribeResult> = result.into_value().unwrap_or(None);

    let data = js_result.ok_or_else(|| {
        ChromeError::General(
            options
                .selector
                .map(|s| format!("Element not found: {}", s))
                .unwrap_or_else(|| "Failed to describe page".to_string()),
        )
    })?;

    Ok(DescribeResult {
        page: PageInfo {
            url: data.page.url,
            title: data.page.title,
            viewport: Viewport {
                width: data.page.viewport.width,
                height: data.page.viewport.height,
            },
        },
        elements: data.elements.into_iter().map(convert_element).collect(),
        summary: Summary {
            total_visible: data.summary.total_visible,
            interactive: data.summary.interactive,
            forms: data.summary.forms,
            navigation: data.summary.navigation,
            truncated: data.summary.truncated,
        },
    })
}

fn convert_element(el: JsElement) -> VisibleElement {
    let category = parse_category(&el.category);
    let ref_id = generate_ref(&category, el.index);

    VisibleElement {
        ref_id,
        tag: el.tag,
        role: el.role,
        label: el.label,
        text: el.text,
        category,
        state: el.state.map(|s| ElementState {
            disabled: s.disabled,
            checked: s.checked,
            selected: s.selected,
            expanded: s.expanded,
            readonly: s.readonly,
            required: s.required,
        }),
        selector: el.selector,
        bounds: el.bounds.map(|b| Bounds {
            x: b.x,
            y: b.y,
            width: b.width,
            height: b.height,
            in_viewport: b.in_viewport,
        }),
    }
}

fn generate_ref(category: &ElementCategory, index: usize) -> String {
    let prefix = match category {
        ElementCategory::Interactive => "i",
        ElementCategory::Form => "f",
        ElementCategory::Navigation => "n",
        ElementCategory::Media => "m",
        ElementCategory::Text => "t",
        ElementCategory::Container => "c",
    };
    format!("{}{}", prefix, index)
}

fn parse_category(s: &str) -> ElementCategory {
    match s {
        "interactive" => ElementCategory::Interactive,
        "form" => ElementCategory::Form,
        "navigation" => ElementCategory::Navigation,
        "media" => ElementCategory::Media,
        "text" => ElementCategory::Text,
        _ => ElementCategory::Container,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputFormatter;

    #[test]
    fn test_element_category_serialization() {
        assert_eq!(
            serde_json::to_string(&ElementCategory::Interactive).unwrap(),
            "\"interactive\""
        );
        assert_eq!(
            serde_json::to_string(&ElementCategory::Form).unwrap(),
            "\"form\""
        );
    }

    #[test]
    fn test_element_state_skip_false() {
        let state = ElementState {
            disabled: false,
            checked: true,
            selected: false,
            expanded: false,
            readonly: false,
            required: true,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"checked\":true"));
        assert!(json.contains("\"required\":true"));
        assert!(!json.contains("disabled"));
    }

    #[test]
    fn test_describe_result_format_text() {
        let result = DescribeResult {
            page: PageInfo {
                url: "https://example.com".to_string(),
                title: "Example".to_string(),
                viewport: Viewport {
                    width: 1920,
                    height: 1080,
                },
            },
            elements: vec![VisibleElement {
                ref_id: "i0".to_string(),
                tag: "button".to_string(),
                role: Some("button".to_string()),
                label: Some("Submit".to_string()),
                text: None,
                category: ElementCategory::Interactive,
                state: None,
                selector: Some("#submit-btn".to_string()),
                bounds: None,
            }],
            summary: Summary {
                total_visible: 1,
                interactive: 1,
                forms: 0,
                navigation: 0,
                truncated: false,
            },
        };

        let text = result.format_text();
        assert!(text.contains("Example"));
        assert!(text.contains("<button>"));
        assert!(text.contains("Submit"));
        assert!(text.contains("i0"));
    }

    #[test]
    fn test_generate_ref() {
        assert_eq!(generate_ref(&ElementCategory::Interactive, 5), "i5");
        assert_eq!(generate_ref(&ElementCategory::Form, 0), "f0");
        assert_eq!(generate_ref(&ElementCategory::Navigation, 12), "n12");
    }

    #[test]
    fn test_describe_result_json() {
        let result = DescribeResult {
            page: PageInfo {
                url: "https://example.com".to_string(),
                title: "Test".to_string(),
                viewport: Viewport {
                    width: 800,
                    height: 600,
                },
            },
            elements: vec![],
            summary: Summary {
                total_visible: 0,
                interactive: 0,
                forms: 0,
                navigation: 0,
                truncated: false,
            },
        };

        let json = result.format_json(false).unwrap();
        assert!(json.contains("\"url\":\"https://example.com\""));
        assert!(json.contains("\"totalVisible\":0"));
    }

    #[test]
    fn test_bounds_serialization() {
        let bounds = Bounds {
            x: 10.5,
            y: 20.0,
            width: 100.0,
            height: 50.0,
            in_viewport: true,
        };
        let json = serde_json::to_string(&bounds).unwrap();
        assert!(json.contains("\"inViewport\":true"));
    }
}
