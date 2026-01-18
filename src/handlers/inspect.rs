use crate::{ChromeError, Result, chrome::PageProvider, output};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct InspectResult {
    pub selector: String,
    pub tag_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aria: Option<AriaInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub box_model: Option<BoxModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub styles: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ChildSummary>>,
}

#[derive(Debug, Serialize)]
pub struct AriaInfo {
    pub role: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct BoxModel {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Serialize)]
pub struct ChildSummary {
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl output::OutputFormatter for InspectResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut output = vec![
            text::success(&format!("Element: <{}>", self.tag_name)),
            text::key_value("Selector", &self.selector),
        ];

        if let Some(ref t) = self.text {
            output.push(text::key_value("Text", t));
        }

        if let Some(ref aria) = self.aria {
            output.push(text::key_value("Role", &aria.role));
            output.push(text::key_value("Name", &aria.name));
        }

        if let Some(ref bm) = self.box_model {
            output.push(text::key_value(
                "Box",
                &format!("{}x{} at ({}, {})", bm.width, bm.height, bm.x, bm.y),
            ));
        }

        if let Some(ref attrs) = self.attributes {
            output.push("\nAttributes:".to_string());
            for (k, v) in attrs {
                output.push(format!("  {}: {}", k, v));
            }
        }

        if let Some(ref styles) = self.styles {
            output.push("\nStyles:".to_string());
            for (k, v) in styles {
                output.push(format!("  {}: {}", k, v));
            }
        }

        if let Some(ref children) = self.children {
            output.push(format!("\nChildren ({}):", children.len()));
            for child in children.iter().take(10) {
                let label = child
                    .text
                    .as_ref()
                    .map(|t| format!("<{}> \"{}\"", child.tag, output::text::truncate(t, 30)))
                    .unwrap_or_else(|| format!("<{}>", child.tag));
                output.push(format!("  {}", label));
            }
        }

        output.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

#[derive(Debug, Serialize)]
pub struct ListenersResult {
    pub selector: String,
    pub listeners: Vec<EventListener>,
}

#[derive(Debug, Serialize)]
pub struct EventListener {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub use_capture: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub passive: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub once: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
}

impl output::OutputFormatter for ListenersResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut output = vec![text::success(&format!(
            "Event Listeners for: {}",
            self.selector
        ))];

        if self.listeners.is_empty() {
            output.push("  No event listeners found".to_string());
        } else {
            for listener in &self.listeners {
                let mut flags = vec![];
                if listener.use_capture {
                    flags.push("capture");
                }
                if listener.passive {
                    flags.push("passive");
                }
                if listener.once {
                    flags.push("once");
                }
                let flag_str = if flags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", flags.join(", "))
                };
                output.push(format!("  {}{}", listener.event_type, flag_str));
                if let Some(ref h) = listener.handler {
                    output.push(format!("    {}", output::text::truncate(h, 60)));
                }
            }
        }

        output.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub selector: String,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elements: Option<Vec<ElementSummary>>,
}

#[derive(Debug, Serialize)]
pub struct ElementSummary {
    pub index: usize,
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl output::OutputFormatter for QueryResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut output = vec![text::key_value(
            "Matches",
            &format!("{} elements", self.count),
        )];

        if let Some(ref elements) = self.elements {
            for el in elements {
                let mut desc = format!("[{}] <{}>", el.index, el.tag);
                if let Some(ref id) = el.id {
                    desc.push_str(&format!(" #{}", id));
                }
                if let Some(ref class) = el.class {
                    desc.push_str(&format!(" .{}", class.replace(' ', ".")));
                }
                if let Some(ref t) = el.text {
                    desc.push_str(&format!(" \"{}\"", output::text::truncate(t, 20)));
                }
                output.push(format!("  {}", desc));
            }
        }

        output.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

#[derive(Debug, Serialize)]
pub struct DomResult {
    pub selector: String,
    pub tree: DomNode,
}

#[derive(Debug, Serialize)]
pub struct DomNode {
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DomNode>,
}

impl output::OutputFormatter for DomResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut output = vec![text::success(&format!("DOM tree for: {}", self.selector))];
        format_dom_node(&self.tree, 0, &mut output);
        output.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

fn format_dom_node(node: &DomNode, depth: usize, output: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    let mut line = format!("{}<{}>", indent, node.tag);
    if let Some(ref id) = node.id {
        line.push_str(&format!(" #{}", id));
    }
    if let Some(ref class) = node.class {
        line.push_str(&format!(
            " .{}",
            class.split_whitespace().next().unwrap_or("")
        ));
    }
    if let Some(ref t) = node.text {
        line.push_str(&format!(" \"{}\"", output::text::truncate(t, 20)));
    }
    output.push(line);

    for child in &node.children {
        format_dom_node(child, depth + 1, output);
    }
}

#[derive(Deserialize)]
struct JsInspectResult {
    tag_name: String,
    text: Option<String>,
    role: Option<String>,
    name: Option<String>,
    attributes: HashMap<String, String>,
    box_model: Option<JsBoxModel>,
    styles: HashMap<String, String>,
    children: Vec<JsChild>,
}

#[derive(Deserialize)]
struct JsBoxModel {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Deserialize)]
struct JsChild {
    tag: String,
    text: Option<String>,
}

pub async fn handle_inspect(
    provider: &impl PageProvider,
    selector: &str,
    show_attributes: bool,
    show_styles: bool,
    show_box: bool,
    show_children: bool,
) -> Result<InspectResult> {
    let page = provider.get_or_create_page().await?;

    let escaped = crate::js_templates::escape_selector(selector);
    let script = format!(
        r#"(function(){{
            const el = document.querySelector('{}');
            if (!el) return null;
            const rect = el.getBoundingClientRect();
            const style = window.getComputedStyle(el);
            const attrs = {{}};
            for (const attr of el.attributes) {{ attrs[attr.name] = attr.value; }}
            const children = [];
            for (const child of el.children) {{
                children.push({{ tag: child.tagName.toLowerCase(), text: child.textContent?.trim()?.substring(0, 50) || null }});
            }}
            return {{
                tag_name: el.tagName.toLowerCase(),
                text: el.textContent?.trim()?.substring(0, 200) || null,
                role: el.getAttribute('role') || el.tagName.toLowerCase(),
                name: el.getAttribute('aria-label') || el.textContent?.trim()?.substring(0, 50) || null,
                attributes: attrs,
                box_model: {{ x: rect.x, y: rect.y, width: rect.width, height: rect.height }},
                styles: {{
                    display: style.display,
                    visibility: style.visibility,
                    opacity: style.opacity,
                    position: style.position,
                    color: style.color,
                    backgroundColor: style.backgroundColor,
                    fontSize: style.fontSize
                }},
                children: children
            }};
        }})()"#,
        escaped
    );

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let js_result: Option<JsInspectResult> = result.into_value().unwrap_or(None);

    let js = js_result
        .ok_or_else(|| ChromeError::General(format!("Element not found: {}", selector)))?;

    Ok(InspectResult {
        selector: selector.to_string(),
        tag_name: js.tag_name,
        text: js.text,
        aria: js.role.map(|role| AriaInfo {
            role,
            name: js.name.unwrap_or_default(),
        }),
        attributes: if show_attributes {
            Some(js.attributes)
        } else {
            None
        },
        box_model: if show_box {
            js.box_model.map(|bm| BoxModel {
                x: bm.x,
                y: bm.y,
                width: bm.width,
                height: bm.height,
            })
        } else {
            None
        },
        styles: if show_styles { Some(js.styles) } else { None },
        children: if show_children {
            Some(
                js.children
                    .into_iter()
                    .map(|c| ChildSummary {
                        tag: c.tag,
                        text: c.text,
                    })
                    .collect(),
            )
        } else {
            None
        },
    })
}

#[derive(Deserialize)]
struct JsListenersResult {
    listeners: Vec<JsListener>,
}

#[derive(Deserialize)]
struct JsListener {
    #[serde(rename = "type")]
    event_type: String,
    use_capture: bool,
    passive: bool,
    once: bool,
    handler: Option<String>,
}

pub async fn handle_listeners(
    provider: &impl PageProvider,
    selector: &str,
) -> Result<ListenersResult> {
    let page = provider.get_or_create_page().await?;

    let escaped = crate::js_templates::escape_selector(selector);
    let script = format!(
        r#"(function(){{
            const el = document.querySelector('{}');
            if (!el) return null;
            const listeners = [];
            if (typeof getEventListeners === 'function') {{
                const evts = getEventListeners(el);
                for (const [type, handlers] of Object.entries(evts)) {{
                    for (const h of handlers) {{
                        listeners.push({{
                            type,
                            use_capture: h.useCapture || false,
                            passive: h.passive || false,
                            once: h.once || false,
                            handler: h.listener?.toString()?.substring(0, 200) || null
                        }});
                    }}
                }}
            }}
            return {{ listeners }};
        }})()"#,
        escaped
    );

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let js_result: Option<JsListenersResult> = result.into_value().unwrap_or(None);

    let js = js_result
        .ok_or_else(|| ChromeError::General(format!("Element not found: {}", selector)))?;

    Ok(ListenersResult {
        selector: selector.to_string(),
        listeners: js
            .listeners
            .into_iter()
            .map(|l| EventListener {
                event_type: l.event_type,
                use_capture: l.use_capture,
                passive: l.passive,
                once: l.once,
                handler: l.handler,
            })
            .collect(),
    })
}

#[derive(Deserialize)]
struct JsQueryResult {
    count: usize,
    elements: Vec<JsElementSummary>,
}

#[derive(Deserialize)]
struct JsElementSummary {
    index: usize,
    tag: String,
    id: Option<String>,
    class: Option<String>,
    text: Option<String>,
}

pub async fn handle_query(
    provider: &impl PageProvider,
    selector: &str,
    count_only: bool,
    limit: Option<usize>,
) -> Result<QueryResult> {
    let page = provider.get_or_create_page().await?;
    let limit = limit.unwrap_or(20);

    let escaped = crate::js_templates::escape_selector(selector);
    let script = format!(
        r#"(function(){{
            const els = document.querySelectorAll('{}');
            const count = els.length;
            const elements = [];
            const limit = {};
            for (let i = 0; i < Math.min(count, limit); i++) {{
                const el = els[i];
                elements.push({{
                    index: i,
                    tag: el.tagName.toLowerCase(),
                    id: el.id || null,
                    class: el.className || null,
                    text: el.textContent?.trim()?.substring(0, 50) || null
                }});
            }}
            return {{ count, elements }};
        }})()"#,
        escaped, limit
    );

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let js_result: JsQueryResult = result
        .into_value()
        .map_err(|_| ChromeError::General("Failed to parse query result".to_string()))?;

    Ok(QueryResult {
        selector: selector.to_string(),
        count: js_result.count,
        elements: if count_only {
            None
        } else {
            Some(
                js_result
                    .elements
                    .into_iter()
                    .map(|e| ElementSummary {
                        index: e.index,
                        tag: e.tag,
                        id: e.id,
                        class: e.class,
                        text: e.text,
                    })
                    .collect(),
            )
        },
    })
}

#[derive(Deserialize)]
struct JsDomNode {
    tag: String,
    id: Option<String>,
    class: Option<String>,
    text: Option<String>,
    attributes: HashMap<String, String>,
    children: Vec<JsDomNode>,
}

pub async fn handle_dom(
    provider: &impl PageProvider,
    selector: &str,
    depth: u32,
) -> Result<DomResult> {
    let page = provider.get_or_create_page().await?;

    let escaped = crate::js_templates::escape_selector(selector);
    let script = format!(
        r#"(function(){{
            function traverse(el, d, maxDepth) {{
                if (!el || d > maxDepth) return null;
                const attrs = {{}};
                for (const attr of el.attributes || []) {{ attrs[attr.name] = attr.value; }}
                const children = [];
                if (d < maxDepth) {{
                    for (const child of el.children || []) {{
                        const c = traverse(child, d + 1, maxDepth);
                        if (c) children.push(c);
                    }}
                }}
                return {{
                    tag: el.tagName?.toLowerCase() || 'unknown',
                    id: el.id || null,
                    class: el.className || null,
                    text: el.children?.length === 0 ? (el.textContent?.trim()?.substring(0, 100) || null) : null,
                    attributes: attrs,
                    children
                }};
            }}
            const el = document.querySelector('{}');
            return el ? traverse(el, 0, {}) : null;
        }})()"#,
        escaped, depth
    );

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let js_node: Option<JsDomNode> = result.into_value().unwrap_or(None);

    let node =
        js_node.ok_or_else(|| ChromeError::General(format!("Element not found: {}", selector)))?;

    Ok(DomResult {
        selector: selector.to_string(),
        tree: convert_dom_node(node),
    })
}

fn convert_dom_node(node: JsDomNode) -> DomNode {
    DomNode {
        tag: node.tag,
        id: node.id,
        class: node.class,
        text: node.text,
        attributes: if node.attributes.is_empty() {
            None
        } else {
            Some(node.attributes)
        },
        children: node.children.into_iter().map(convert_dom_node).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputFormatter;

    #[test]
    fn test_inspect_result_format_text() {
        let result = InspectResult {
            selector: "#btn".to_string(),
            tag_name: "button".to_string(),
            text: Some("Click".to_string()),
            aria: Some(AriaInfo {
                role: "button".to_string(),
                name: "Click".to_string(),
            }),
            attributes: None,
            box_model: Some(BoxModel {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 40.0,
            }),
            styles: None,
            children: None,
        };
        let text = result.format_text();
        assert!(text.contains("<button>"));
        assert!(text.contains("#btn"));
        assert!(text.contains("Role"));
    }

    #[test]
    fn test_inspect_result_json() {
        let result = InspectResult {
            selector: "#test".to_string(),
            tag_name: "div".to_string(),
            text: None,
            aria: None,
            attributes: Some(HashMap::from([(
                "class".to_string(),
                "container".to_string(),
            )])),
            box_model: None,
            styles: None,
            children: None,
        };
        let json = result.format_json(false).unwrap();
        assert!(json.contains("\"tag_name\":\"div\""));
        assert!(json.contains("\"attributes\""));
    }

    #[test]
    fn test_query_result_format() {
        let result = QueryResult {
            selector: "button".to_string(),
            count: 3,
            elements: Some(vec![ElementSummary {
                index: 0,
                tag: "button".to_string(),
                id: Some("submit".to_string()),
                class: Some("btn primary".to_string()),
                text: Some("Submit".to_string()),
            }]),
        };
        let text = result.format_text();
        assert!(text.contains("3 elements"));
        assert!(text.contains("#submit"));
    }

    #[test]
    fn test_query_result_count_only() {
        let result = QueryResult {
            selector: "div".to_string(),
            count: 10,
            elements: None,
        };
        let text = result.format_text();
        assert!(text.contains("10 elements"));
    }

    #[test]
    fn test_listeners_result_format() {
        let result = ListenersResult {
            selector: "#btn".to_string(),
            listeners: vec![
                EventListener {
                    event_type: "click".to_string(),
                    use_capture: false,
                    passive: false,
                    once: false,
                    handler: Some("function() {}".to_string()),
                },
                EventListener {
                    event_type: "mouseenter".to_string(),
                    use_capture: true,
                    passive: true,
                    once: false,
                    handler: None,
                },
            ],
        };
        let text = result.format_text();
        assert!(text.contains("click"));
        assert!(text.contains("mouseenter"));
        assert!(text.contains("[capture, passive]"));
    }

    #[test]
    fn test_listeners_result_empty() {
        let result = ListenersResult {
            selector: "#empty".to_string(),
            listeners: vec![],
        };
        let text = result.format_text();
        assert!(text.contains("No event listeners found"));
    }

    #[test]
    fn test_dom_result_format() {
        let result = DomResult {
            selector: "#root".to_string(),
            tree: DomNode {
                tag: "div".to_string(),
                id: Some("root".to_string()),
                class: Some("container".to_string()),
                text: None,
                attributes: None,
                children: vec![DomNode {
                    tag: "span".to_string(),
                    id: None,
                    class: None,
                    text: Some("Hello".to_string()),
                    attributes: None,
                    children: vec![],
                }],
            },
        };
        let text = result.format_text();
        assert!(text.contains("<div>"));
        assert!(text.contains("#root"));
        assert!(text.contains("<span>"));
    }

    #[test]
    fn test_dom_node_serialization() {
        let node = DomNode {
            tag: "input".to_string(),
            id: Some("email".to_string()),
            class: None,
            text: None,
            attributes: Some(HashMap::from([("type".to_string(), "email".to_string())])),
            children: vec![],
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"tag\":\"input\""));
        assert!(json.contains("\"id\":\"email\""));
    }
}
