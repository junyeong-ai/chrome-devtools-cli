use crate::{ChromeError, Result, chrome::PageProvider, output};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct A11yResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    pub tree: Vec<A11yNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct A11yNode {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub focusable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<A11yNode>,
}

impl output::OutputFormatter for A11yResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut output = vec![text::success(
            if let Some(ref sel) = self.selector {
                format!("Accessibility tree for: {}", sel)
            } else {
                "Accessibility tree (page)".to_string()
            }
            .as_str(),
        )];

        for node in &self.tree {
            format_a11y_node(node, 0, &mut output);
        }

        output.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

fn format_a11y_node(node: &A11yNode, depth: usize, output: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    let mut line = format!("{}{}", indent, node.role);

    if let Some(ref name) = node.name {
        line.push_str(&format!(": \"{}\"", truncate(name, 40)));
    }

    if node.focusable {
        line.push_str(" [focusable]");
    }

    if let Some(ref value) = node.value {
        line.push_str(&format!(" (value: {})", truncate(value, 20)));
    }

    output.push(line);

    for child in &node.children {
        format_a11y_node(child, depth + 1, output);
    }
}

fn truncate(s: &str, max: usize) -> String {
    crate::output::text::truncate(s, max + 3)
}

#[derive(Deserialize)]
struct JsA11yNode {
    role: String,
    name: Option<String>,
    focusable: bool,
    value: Option<String>,
    description: Option<String>,
    children: Vec<JsA11yNode>,
}

pub async fn handle_a11y(
    provider: &impl PageProvider,
    selector: Option<&str>,
    depth: u32,
    interactable_only: bool,
) -> Result<A11yResult> {
    let page = provider.get_or_create_page().await?;

    let selector_code = selector
        .map(|s| {
            format!(
                "document.querySelector('{}')",
                crate::js_templates::escape_selector(s)
            )
        })
        .unwrap_or_else(|| "document.body".to_string());

    let interactive_roles = if interactable_only {
        r#"["button","checkbox","combobox","link","listbox","menu","menubar","menuitem","menuitemcheckbox","menuitemradio","option","radio","searchbox","slider","spinbutton","switch","tab","textbox","treeitem"]"#
    } else {
        "null"
    };

    let script = format!(
        r#"(function(){{
            const interactiveRoles = {};
            function getA11yProps(el) {{
                const role = el.getAttribute('role') || getImplicitRole(el);
                const name = el.getAttribute('aria-label') || el.getAttribute('alt') || el.textContent?.trim()?.substring(0, 100) || null;
                const focusable = el.tabIndex >= 0 || ['A','BUTTON','INPUT','SELECT','TEXTAREA'].includes(el.tagName);
                const value = el.value || el.getAttribute('aria-valuenow') || null;
                const description = el.getAttribute('aria-description') || null;
                return {{ role, name, focusable, value, description }};
            }}
            function getImplicitRole(el) {{
                const tag = el.tagName?.toLowerCase();
                const type = el.getAttribute('type');
                const roleMap = {{
                    'a': el.hasAttribute('href') ? 'link' : null,
                    'article': 'article',
                    'aside': 'complementary',
                    'button': 'button',
                    'dialog': 'dialog',
                    'footer': 'contentinfo',
                    'form': 'form',
                    'h1': 'heading', 'h2': 'heading', 'h3': 'heading', 'h4': 'heading', 'h5': 'heading', 'h6': 'heading',
                    'header': 'banner',
                    'img': 'img',
                    'input': type === 'checkbox' ? 'checkbox' : type === 'radio' ? 'radio' : type === 'range' ? 'slider' : type === 'search' ? 'searchbox' : 'textbox',
                    'li': 'listitem',
                    'main': 'main',
                    'nav': 'navigation',
                    'ol': 'list',
                    'option': 'option',
                    'progress': 'progressbar',
                    'section': 'region',
                    'select': 'combobox',
                    'table': 'table',
                    'tbody': 'rowgroup',
                    'td': 'cell',
                    'textarea': 'textbox',
                    'th': 'columnheader',
                    'tr': 'row',
                    'ul': 'list'
                }};
                return roleMap[tag] || 'generic';
            }}
            function traverse(el, d, maxDepth) {{
                if (!el || d > maxDepth) return [];
                const props = getA11yProps(el);
                if (interactiveRoles && !interactiveRoles.includes(props.role)) {{
                    const children = [];
                    for (const child of el.children || []) {{
                        children.push(...traverse(child, d, maxDepth));
                    }}
                    return children;
                }}
                const children = [];
                if (d < maxDepth) {{
                    for (const child of el.children || []) {{
                        children.push(...traverse(child, d + 1, maxDepth));
                    }}
                }}
                return [{{ ...props, children }}];
            }}
            const root = {};
            if (!root) return null;
            return traverse(root, 0, {});
        }})()"#,
        interactive_roles, selector_code, depth
    );

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let js_nodes: Option<Vec<JsA11yNode>> = result.into_value().unwrap_or(None);

    let nodes = js_nodes.ok_or_else(|| {
        ChromeError::General(
            selector
                .map(|s| format!("Element not found: {}", s))
                .unwrap_or_else(|| "Failed to get accessibility tree".to_string()),
        )
    })?;

    Ok(A11yResult {
        selector: selector.map(|s| s.to_string()),
        tree: nodes.into_iter().map(convert_a11y_node).collect(),
    })
}

fn convert_a11y_node(node: JsA11yNode) -> A11yNode {
    A11yNode {
        role: node.role,
        name: node.name,
        focusable: node.focusable,
        value: node.value,
        description: node.description,
        children: node.children.into_iter().map(convert_a11y_node).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputFormatter;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a long string", 5), "a lon...");
    }

    #[test]
    fn test_a11y_node_serialization() {
        let node = A11yNode {
            role: "button".to_string(),
            name: Some("Submit".to_string()),
            focusable: true,
            value: None,
            description: None,
            children: vec![],
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"role\":\"button\""));
        assert!(json.contains("\"focusable\":true"));
    }

    #[test]
    fn test_a11y_node_with_value() {
        let node = A11yNode {
            role: "slider".to_string(),
            name: Some("Volume".to_string()),
            focusable: true,
            value: Some("50".to_string()),
            description: Some("Adjust volume level".to_string()),
            children: vec![],
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"value\":\"50\""));
        assert!(json.contains("\"description\""));
    }

    #[test]
    fn test_a11y_result_format() {
        let result = A11yResult {
            selector: Some("#form".to_string()),
            tree: vec![A11yNode {
                role: "form".to_string(),
                name: Some("Login".to_string()),
                focusable: false,
                value: None,
                description: None,
                children: vec![A11yNode {
                    role: "textbox".to_string(),
                    name: Some("Email".to_string()),
                    focusable: true,
                    value: None,
                    description: None,
                    children: vec![],
                }],
            }],
        };
        let text = result.format_text();
        assert!(text.contains("#form"));
        assert!(text.contains("form"));
        assert!(text.contains("textbox"));
    }

    #[test]
    fn test_a11y_result_page_level() {
        let result = A11yResult {
            selector: None,
            tree: vec![A11yNode {
                role: "main".to_string(),
                name: None,
                focusable: false,
                value: None,
                description: None,
                children: vec![],
            }],
        };
        let text = result.format_text();
        assert!(text.contains("(page)"));
    }

    #[test]
    fn test_a11y_result_json() {
        let result = A11yResult {
            selector: Some("body".to_string()),
            tree: vec![A11yNode {
                role: "generic".to_string(),
                name: None,
                focusable: false,
                value: None,
                description: None,
                children: vec![],
            }],
        };
        let json = result.format_json(false).unwrap();
        assert!(json.contains("\"selector\":\"body\""));
        assert!(json.contains("\"role\":\"generic\""));
    }

    #[test]
    fn test_a11y_node_focusable_flag() {
        let focusable = A11yNode {
            role: "link".to_string(),
            name: Some("Home".to_string()),
            focusable: true,
            value: None,
            description: None,
            children: vec![],
        };
        let non_focusable = A11yNode {
            role: "heading".to_string(),
            name: Some("Title".to_string()),
            focusable: false,
            value: None,
            description: None,
            children: vec![],
        };

        let json1 = serde_json::to_string(&focusable).unwrap();
        let json2 = serde_json::to_string(&non_focusable).unwrap();

        assert!(json1.contains("\"focusable\":true"));
        assert!(!json2.contains("focusable"));
    }
}
