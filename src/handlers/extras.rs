use crate::{ChromeError, Result, chrome::PageProvider, output};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ScrollResult {
    pub selector: String,
    pub scrolled: bool,
}

impl output::OutputFormatter for ScrollResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        if self.scrolled {
            text::success(&format!("Scrolled to: {}", self.selector))
        } else {
            text::error(&format!("Element not found: {}", self.selector))
        }
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_scroll(
    provider: &impl PageProvider,
    selector: &str,
    behavior: &str,
    block: &str,
) -> Result<ScrollResult> {
    let page = provider.get_or_create_page().await?;

    let escaped = crate::js_templates::escape_selector(selector);
    let script = format!(
        r#"(function(){{
            const el = document.querySelector('{}');
            if (!el) return false;
            el.scrollIntoView({{ behavior: '{}', block: '{}' }});
            return true;
        }})()"#,
        escaped, behavior, block
    );

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let scrolled: bool = result.into_value().unwrap_or(false);

    Ok(ScrollResult {
        selector: selector.to_string(),
        scrolled,
    })
}

#[derive(Debug, Serialize)]
pub struct SelectResult {
    pub selector: String,
    pub selected_value: Option<String>,
    pub selected_text: Option<String>,
}

impl output::OutputFormatter for SelectResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        if let Some(ref val) = self.selected_value {
            let mut output = text::success(&format!("Selected in: {}", self.selector));
            output.push_str(&format!("\n{}", text::key_value("Value", val)));
            if let Some(ref txt) = self.selected_text {
                output.push_str(&format!("\n{}", text::key_value("Text", txt)));
            }
            output
        } else {
            text::error(&format!("Selection failed for: {}", self.selector))
        }
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_select(
    provider: &impl PageProvider,
    selector: &str,
    value: Option<&str>,
    index: Option<usize>,
    label: Option<&str>,
) -> Result<SelectResult> {
    let page = provider.get_or_create_page().await?;

    let escaped = crate::js_templates::escape_selector(selector);
    let select_code = if let Some(val) = value {
        format!(
            "el.value = '{}';",
            crate::js_templates::escape_selector(val)
        )
    } else if let Some(idx) = index {
        format!("el.selectedIndex = {};", idx)
    } else if let Some(lbl) = label {
        format!(
            "Array.from(el.options).find(o => o.text === '{}')?.selected = true;",
            crate::js_templates::escape_selector(lbl)
        )
    } else {
        return Err(ChromeError::General(
            "Must specify value, index, or label".to_string(),
        ));
    };

    let script = format!(
        r#"(function(){{
            const el = document.querySelector('{}');
            if (!el || el.tagName !== 'SELECT') return null;
            {};
            el.dispatchEvent(new Event('change', {{ bubbles: true }}));
            const opt = el.options[el.selectedIndex];
            return {{ value: el.value, text: opt?.text || null }};
        }})()"#,
        escaped, select_code
    );

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    #[derive(Deserialize)]
    struct JsSelectResult {
        value: String,
        text: Option<String>,
    }

    let js: Option<JsSelectResult> = result.into_value().unwrap_or(None);

    Ok(SelectResult {
        selector: selector.to_string(),
        selected_value: js.as_ref().map(|j| j.value.clone()),
        selected_text: js.and_then(|j| j.text),
    })
}

#[derive(Debug, Serialize)]
pub struct HtmlResult {
    pub selector: Option<String>,
    pub html: String,
    pub length: usize,
}

impl output::OutputFormatter for HtmlResult {
    fn format_text(&self) -> String {
        self.html.clone()
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_html(
    provider: &impl PageProvider,
    selector: Option<&str>,
    inner: bool,
) -> Result<HtmlResult> {
    let page = provider.get_or_create_page().await?;

    let (escaped, is_doc) = selector
        .map(|s| (crate::js_templates::escape_selector(s), false))
        .unwrap_or_else(|| ("html".to_string(), true));

    let prop = if inner { "innerHTML" } else { "outerHTML" };

    let script = if is_doc && !inner {
        "document.documentElement.outerHTML".to_string()
    } else {
        format!(
            r#"(function(){{
                const el = document.querySelector('{}');
                return el ? el.{} : null;
            }})()"#,
            escaped, prop
        )
    };

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let html: Option<String> = result.into_value().unwrap_or(None);
    let html = html.ok_or_else(|| {
        ChromeError::General(
            selector
                .map(|s| format!("Element not found: {}", s))
                .unwrap_or_else(|| "Failed to get HTML".to_string()),
        )
    })?;

    Ok(HtmlResult {
        selector: selector.map(|s| s.to_string()),
        length: html.len(),
        html,
    })
}

#[derive(Debug, Serialize)]
pub struct PdfResult {
    pub path: String,
    pub size: u64,
}

impl output::OutputFormatter for PdfResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}",
            text::success(&format!("PDF saved to: {}", self.path)),
            text::key_value("Size", &crate::output::text::format_bytes(self.size))
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_pdf(
    provider: &impl PageProvider,
    output_path: &std::path::Path,
    _format: &str,
    landscape: bool,
    print_background: bool,
) -> Result<PdfResult> {
    use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;

    let page = provider.get_or_create_page().await?;

    let params = PrintToPdfParams::builder()
        .landscape(landscape)
        .print_background(print_background)
        .build();

    let pdf_data = page
        .pdf(params)
        .await
        .map_err(|e| ChromeError::General(format!("PDF generation failed: {}", e)))?;

    std::fs::write(output_path, &pdf_data)
        .map_err(|e| ChromeError::General(format!("Failed to write PDF: {}", e)))?;

    Ok(PdfResult {
        path: output_path.display().to_string(),
        size: pdf_data.len() as u64,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CookieInfo {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub secure: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub http_only: bool,
}

#[derive(Debug, Serialize)]
pub struct CookiesResult {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookies: Option<Vec<CookieInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookie: Option<CookieInfo>,
}

impl output::OutputFormatter for CookiesResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut output = vec![text::success(&self.action)];

        if let Some(ref cookies) = self.cookies {
            for c in cookies {
                output.push(format!("  {} = {}", c.name, c.value));
            }
        }

        if let Some(ref c) = self.cookie {
            output.push(text::key_value("Name", &c.name));
            output.push(text::key_value("Value", &c.value));
            if let Some(ref d) = c.domain {
                output.push(text::key_value("Domain", d));
            }
        }

        output.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_cookies_list(provider: &impl PageProvider) -> Result<CookiesResult> {
    let page = provider.get_or_create_page().await?;

    let script = r#"(function(){
        return document.cookie.split(';').map(c => {
            const [name, ...rest] = c.trim().split('=');
            return { name, value: rest.join('='), domain: null, path: null, secure: false, http_only: false };
        }).filter(c => c.name);
    })()"#;

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let cookies: Vec<CookieInfo> = result.into_value().unwrap_or_default();

    Ok(CookiesResult {
        action: format!("Found {} cookies", cookies.len()),
        cookies: Some(cookies),
        cookie: None,
    })
}

pub async fn handle_cookies_get(provider: &impl PageProvider, name: &str) -> Result<CookiesResult> {
    let page = provider.get_or_create_page().await?;

    let escaped = crate::js_templates::escape_selector(name);
    let script = format!(
        r#"(function(){{
            const cookies = document.cookie.split(';').map(c => {{
                const [n, ...rest] = c.trim().split('=');
                return {{ name: n, value: rest.join('=') }};
            }});
            return cookies.find(c => c.name === '{}') || null;
        }})()"#,
        escaped
    );

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    #[derive(Deserialize)]
    struct JsCookie {
        name: String,
        value: String,
    }

    let cookie: Option<JsCookie> = result.into_value().unwrap_or(None);
    let cookie =
        cookie.ok_or_else(|| ChromeError::General(format!("Cookie not found: {}", name)))?;

    Ok(CookiesResult {
        action: "Cookie found".to_string(),
        cookies: None,
        cookie: Some(CookieInfo {
            name: cookie.name,
            value: cookie.value,
            domain: None,
            path: None,
            secure: false,
            http_only: false,
        }),
    })
}

pub async fn handle_cookies_set(
    provider: &impl PageProvider,
    name: &str,
    value: &str,
    domain: Option<&str>,
    path: Option<&str>,
    secure: bool,
    http_only: bool,
) -> Result<CookiesResult> {
    use chromiumoxide::cdp::browser_protocol::network::SetCookieParams;

    let page = provider.get_or_create_page().await?;

    let url = page
        .url()
        .await
        .map_err(|e| ChromeError::General(e.to_string()))?
        .unwrap_or_default();

    let mut params = SetCookieParams::builder()
        .name(name)
        .value(value)
        .url(&url)
        .secure(secure)
        .http_only(http_only);

    if let Some(d) = domain {
        params = params.domain(d);
    }
    if let Some(p) = path {
        params = params.path(p);
    }

    let cmd = params
        .build()
        .map_err(|e| ChromeError::General(format!("Invalid cookie params: {}", e)))?;

    page.execute(cmd)
        .await
        .map_err(|e| ChromeError::General(format!("Failed to set cookie: {}", e)))?;

    Ok(CookiesResult {
        action: format!("Cookie '{}' set", name),
        cookies: None,
        cookie: Some(CookieInfo {
            name: name.to_string(),
            value: value.to_string(),
            domain: domain.map(|s| s.to_string()),
            path: path.map(|s| s.to_string()),
            secure,
            http_only,
        }),
    })
}

pub async fn handle_cookies_delete(
    provider: &impl PageProvider,
    name: &str,
) -> Result<CookiesResult> {
    use chromiumoxide::cdp::browser_protocol::network::DeleteCookiesParams;

    let page = provider.get_or_create_page().await?;

    let cmd = DeleteCookiesParams::builder()
        .name(name)
        .build()
        .map_err(|e| ChromeError::General(format!("Invalid params: {}", e)))?;

    page.execute(cmd)
        .await
        .map_err(|e| ChromeError::General(format!("Failed to delete cookie: {}", e)))?;

    Ok(CookiesResult {
        action: format!("Cookie '{}' deleted", name),
        cookies: None,
        cookie: None,
    })
}

pub async fn handle_cookies_clear(provider: &impl PageProvider) -> Result<CookiesResult> {
    use chromiumoxide::cdp::browser_protocol::network::ClearBrowserCookiesParams;

    let page = provider.get_or_create_page().await?;

    page.execute(ClearBrowserCookiesParams::default())
        .await
        .map_err(|e| ChromeError::General(format!("Failed to clear cookies: {}", e)))?;

    Ok(CookiesResult {
        action: "All cookies cleared".to_string(),
        cookies: None,
        cookie: None,
    })
}

#[derive(Debug, Serialize)]
pub struct StorageResult {
    pub storage_type: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl output::OutputFormatter for StorageResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut output = vec![text::success(&format!(
            "{} ({})",
            self.action, self.storage_type
        ))];

        if let Some(ref keys) = self.keys {
            for k in keys {
                output.push(format!("  {}", k));
            }
        }

        if let Some(ref key) = self.key {
            output.push(text::key_value("Key", key));
        }
        if let Some(ref val) = self.value {
            output.push(text::key_value("Value", val));
        }

        output.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

fn storage_type_name(session: bool) -> &'static str {
    if session {
        "sessionStorage"
    } else {
        "localStorage"
    }
}

pub async fn handle_storage_list(
    provider: &impl PageProvider,
    session: bool,
) -> Result<StorageResult> {
    let page = provider.get_or_create_page().await?;
    let storage = storage_type_name(session);

    let script = format!("Object.keys({})", storage);

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let keys: Vec<String> = result.into_value().unwrap_or_default();

    Ok(StorageResult {
        storage_type: storage.to_string(),
        action: format!("Found {} keys", keys.len()),
        keys: Some(keys),
        key: None,
        value: None,
    })
}

pub async fn handle_storage_get(
    provider: &impl PageProvider,
    key: &str,
    session: bool,
) -> Result<StorageResult> {
    let page = provider.get_or_create_page().await?;
    let storage = storage_type_name(session);

    let escaped = crate::js_templates::escape_selector(key);
    let script = format!("{}.getItem('{}')", storage, escaped);

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let value: Option<String> = result.into_value().unwrap_or(None);

    Ok(StorageResult {
        storage_type: storage.to_string(),
        action: if value.is_some() {
            "Value found"
        } else {
            "Key not found"
        }
        .to_string(),
        keys: None,
        key: Some(key.to_string()),
        value,
    })
}

pub async fn handle_storage_set(
    provider: &impl PageProvider,
    key: &str,
    value: &str,
    session: bool,
) -> Result<StorageResult> {
    let page = provider.get_or_create_page().await?;
    let storage = storage_type_name(session);

    let escaped_key = crate::js_templates::escape_selector(key);
    let escaped_val = crate::js_templates::escape_selector(value);
    let script = format!(
        "{}.setItem('{}', '{}'); true",
        storage, escaped_key, escaped_val
    );

    page.evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    Ok(StorageResult {
        storage_type: storage.to_string(),
        action: "Value set".to_string(),
        keys: None,
        key: Some(key.to_string()),
        value: Some(value.to_string()),
    })
}

pub async fn handle_storage_delete(
    provider: &impl PageProvider,
    key: &str,
    session: bool,
) -> Result<StorageResult> {
    let page = provider.get_or_create_page().await?;
    let storage = storage_type_name(session);

    let escaped = crate::js_templates::escape_selector(key);
    let script = format!("{}.removeItem('{}'); true", storage, escaped);

    page.evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    Ok(StorageResult {
        storage_type: storage.to_string(),
        action: "Key deleted".to_string(),
        keys: None,
        key: Some(key.to_string()),
        value: None,
    })
}

pub async fn handle_storage_clear(
    provider: &impl PageProvider,
    session: bool,
) -> Result<StorageResult> {
    let page = provider.get_or_create_page().await?;
    let storage = storage_type_name(session);

    let script = format!("{}.clear(); true", storage);

    page.evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    Ok(StorageResult {
        storage_type: storage.to_string(),
        action: "Storage cleared".to_string(),
        keys: None,
        key: None,
        value: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputFormatter;

    #[test]
    fn test_scroll_result_success() {
        let result = ScrollResult {
            selector: "#btn".to_string(),
            scrolled: true,
        };
        let text = result.format_text();
        assert!(text.contains("Scrolled to"));
        assert!(text.contains("#btn"));
    }

    #[test]
    fn test_scroll_result_failure() {
        let result = ScrollResult {
            selector: "#missing".to_string(),
            scrolled: false,
        };
        let text = result.format_text();
        assert!(text.contains("not found"));
    }

    #[test]
    fn test_select_result_format() {
        let result = SelectResult {
            selector: "#dropdown".to_string(),
            selected_value: Some("opt1".to_string()),
            selected_text: Some("Option 1".to_string()),
        };
        let text = result.format_text();
        assert!(text.contains("#dropdown"));
        assert!(text.contains("opt1"));
    }

    #[test]
    fn test_html_result_format() {
        let result = HtmlResult {
            selector: Some("#div".to_string()),
            html: "<div>Test</div>".to_string(),
            length: 15,
        };
        assert_eq!(result.format_text(), "<div>Test</div>");
    }

    #[test]
    fn test_html_result_json() {
        let result = HtmlResult {
            selector: None,
            html: "<html></html>".to_string(),
            length: 13,
        };
        let json = result.format_json(false).unwrap();
        assert!(json.contains("\"length\":13"));
    }

    #[test]
    fn test_pdf_result_format() {
        let result = PdfResult {
            path: "/tmp/test.pdf".to_string(),
            size: 1024,
        };
        let text = result.format_text();
        assert!(text.contains("/tmp/test.pdf"));
        assert!(text.contains("1.00 KB"));
    }

    #[test]
    fn test_cookies_result_list() {
        let result = CookiesResult {
            action: "Found 2 cookies".to_string(),
            cookies: Some(vec![
                CookieInfo {
                    name: "session".to_string(),
                    value: "abc123".to_string(),
                    domain: None,
                    path: None,
                    secure: false,
                    http_only: false,
                },
                CookieInfo {
                    name: "user".to_string(),
                    value: "john".to_string(),
                    domain: Some("example.com".to_string()),
                    path: Some("/".to_string()),
                    secure: true,
                    http_only: true,
                },
            ]),
            cookie: None,
        };
        let text = result.format_text();
        assert!(text.contains("session"));
        assert!(text.contains("abc123"));
    }

    #[test]
    fn test_storage_result_format() {
        let result = StorageResult {
            storage_type: "localStorage".to_string(),
            action: "Value found".to_string(),
            keys: None,
            key: Some("user".to_string()),
            value: Some("john".to_string()),
        };
        let text = result.format_text();
        assert!(text.contains("localStorage"));
        assert!(text.contains("user"));
    }

    #[test]
    fn test_storage_result_list_keys() {
        let result = StorageResult {
            storage_type: "sessionStorage".to_string(),
            action: "Found 3 keys".to_string(),
            keys: Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]),
            key: None,
            value: None,
        };
        let text = result.format_text();
        assert!(text.contains("sessionStorage"));
        assert!(text.contains("a"));
        assert!(text.contains("b"));
    }

    #[test]
    fn test_storage_type_name() {
        assert_eq!(storage_type_name(false), "localStorage");
        assert_eq!(storage_type_name(true), "sessionStorage");
    }
}
