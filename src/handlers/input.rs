use crate::{
    ChromeError, Result,
    chrome::{
        PageProvider,
        action_executor::{ActionConfig, ActionExecutor},
    },
    js_templates, output,
    timeouts::ms,
};
use chromiumoxide::{element::Element, page::Page};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InteractionMode {
    #[default]
    Auto,
    Cdp,
    JavaScript,
}

impl std::str::FromStr for InteractionMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "cdp" => Ok(Self::Cdp),
            "js" | "javascript" => Ok(Self::JavaScript),
            _ => Err(format!("Invalid mode: {}", s)),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ClickResult {
    pub clicked: String,
}

impl output::OutputFormatter for ClickResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        text::success(&format!("Clicked: {}", self.clicked))
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

#[derive(Debug, Serialize)]
pub struct FillResult {
    pub filled: String,
    pub value: String,
}

impl output::OutputFormatter for FillResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}",
            text::success(&format!("Filled: {}", self.filled)),
            text::key_value("Value", &self.value)
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

#[derive(Debug, Serialize)]
pub struct TypeResult {
    pub typed: String,
    pub value: String,
    #[serde(skip_serializing_if = "is_default_delay")]
    pub delay_ms: u64,
}

fn is_default_delay(ms: &u64) -> bool {
    *ms == 100
}

impl output::OutputFormatter for TypeResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}",
            text::success(&format!("Typed into: {}", self.typed)),
            text::key_value("Value", &self.value)
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_click(
    provider: &impl PageProvider,
    selector: &str,
    mode: InteractionMode,
) -> Result<ClickResult> {
    let page = provider.get_or_create_page().await?;

    match mode {
        InteractionMode::JavaScript => click_via_js(&page, selector).await,
        InteractionMode::Cdp => click_via_cdp(&page, selector).await,
        InteractionMode::Auto => {
            let cdp_result = tokio::time::timeout(
                Duration::from_millis(ms::CDP_ACTION),
                click_via_cdp(&page, selector),
            )
            .await;

            match cdp_result {
                Ok(Ok(result)) => Ok(result),
                Ok(Err(e)) if e.to_string().contains("No node") => {
                    Err(ChromeError::ElementNotFound {
                        selector: selector.to_string(),
                    })
                }
                _ => {
                    tracing::debug!("CDP click failed/timeout, falling back to JS");
                    click_via_js(&page, selector).await
                }
            }
        }
    }
}

async fn click_via_cdp(page: &Arc<Page>, selector: &str) -> Result<ClickResult> {
    let executor = ActionExecutor::new(page.clone(), ActionConfig::default());

    executor
        .execute(|| async { click_element(page, selector).await })
        .await?;

    Ok(ClickResult {
        clicked: selector.to_string(),
    })
}

async fn click_via_js(page: &Arc<Page>, selector: &str) -> Result<ClickResult> {
    let script = js_templates::click_element(selector);

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::General(format!("JS click failed: {}", e)))?;

    let value: serde_json::Value = result
        .into_value()
        .map_err(|e| ChromeError::General(format!("Failed to parse result: {}", e)))?;

    let found = value
        .get("found")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if found {
        Ok(ClickResult {
            clicked: selector.to_string(),
        })
    } else {
        Err(ChromeError::ElementNotFound {
            selector: selector.to_string(),
        })
    }
}

pub async fn handle_fill(
    provider: &impl PageProvider,
    selector: &str,
    text: &str,
    mode: InteractionMode,
) -> Result<FillResult> {
    let page = provider.get_or_create_page().await?;

    match mode {
        InteractionMode::JavaScript => fill_via_js(&page, selector, text).await,
        InteractionMode::Cdp => fill_via_cdp(&page, selector, text).await,
        InteractionMode::Auto => {
            let cdp_result = tokio::time::timeout(
                Duration::from_millis(ms::CDP_ACTION),
                fill_via_cdp(&page, selector, text),
            )
            .await;

            match cdp_result {
                Ok(Ok(result)) => Ok(result),
                Ok(Err(e)) if e.to_string().contains("No node") => {
                    Err(ChromeError::ElementNotFound {
                        selector: selector.to_string(),
                    })
                }
                _ => {
                    tracing::debug!("CDP fill failed/timeout, falling back to JS");
                    fill_via_js(&page, selector, text).await
                }
            }
        }
    }
}

async fn fill_via_cdp(page: &Arc<Page>, selector: &str, text: &str) -> Result<FillResult> {
    let executor = ActionExecutor::new(page.clone(), ActionConfig::default());
    let text_owned = text.to_string();

    executor
        .execute(|| async { fill_element(page, selector, &text_owned).await })
        .await?;

    Ok(FillResult {
        filled: selector.to_string(),
        value: text.to_string(),
    })
}

async fn fill_via_js(page: &Arc<Page>, selector: &str, text: &str) -> Result<FillResult> {
    let script = js_templates::fill_element(selector, text);

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::General(format!("JS fill failed: {}", e)))?;

    let value: serde_json::Value = result
        .into_value()
        .map_err(|e| ChromeError::General(format!("Failed to parse result: {}", e)))?;

    let found = value
        .get("found")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if found {
        Ok(FillResult {
            filled: selector.to_string(),
            value: text.to_string(),
        })
    } else {
        Err(ChromeError::ElementNotFound {
            selector: selector.to_string(),
        })
    }
}

pub async fn handle_type(
    provider: &impl PageProvider,
    selector: &str,
    text: &str,
    delay_ms: Option<u64>,
    mode: InteractionMode,
) -> Result<TypeResult> {
    let page = provider.get_or_create_page().await?;
    let delay = delay_ms.unwrap_or(100);

    match mode {
        InteractionMode::JavaScript => type_via_js(&page, selector, text, delay).await,
        InteractionMode::Cdp => type_via_cdp(&page, selector, text, delay).await,
        InteractionMode::Auto => {
            let cdp_result = tokio::time::timeout(
                Duration::from_millis(ms::CDP_ACTION + delay * text.len() as u64),
                type_via_cdp(&page, selector, text, delay),
            )
            .await;

            match cdp_result {
                Ok(Ok(result)) => Ok(result),
                Ok(Err(e)) if e.to_string().contains("No node") => {
                    Err(ChromeError::ElementNotFound {
                        selector: selector.to_string(),
                    })
                }
                _ => {
                    tracing::debug!("CDP type failed/timeout, falling back to JS");
                    type_via_js(&page, selector, text, delay).await
                }
            }
        }
    }
}

async fn type_via_cdp(
    page: &Arc<Page>,
    selector: &str,
    text: &str,
    delay: u64,
) -> Result<TypeResult> {
    let config = ActionConfig {
        wait_for_navigation: false,
        wait_for_stable_dom: false,
        ..Default::default()
    };
    let executor = ActionExecutor::new(page.clone(), config);
    let text_owned = text.to_string();

    executor
        .execute(|| async { type_element(page, selector, &text_owned, delay).await })
        .await?;

    Ok(TypeResult {
        typed: selector.to_string(),
        value: text.to_string(),
        delay_ms: delay,
    })
}

async fn type_via_js(
    page: &Arc<Page>,
    selector: &str,
    text: &str,
    delay: u64,
) -> Result<TypeResult> {
    let script = js_templates::type_element(selector, text, delay);

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::General(format!("JS type failed: {}", e)))?;

    let value: serde_json::Value = result
        .into_value()
        .map_err(|e| ChromeError::General(format!("Failed to parse result: {}", e)))?;

    let found = value
        .get("found")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if found {
        Ok(TypeResult {
            typed: selector.to_string(),
            value: text.to_string(),
            delay_ms: delay,
        })
    } else {
        Err(ChromeError::ElementNotFound {
            selector: selector.to_string(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct ActionabilityOptions {
    check_visible: bool,
    check_stable: bool,
    check_enabled: bool,
    scroll_into_view: bool,
}

impl Default for ActionabilityOptions {
    fn default() -> Self {
        Self {
            check_visible: true,
            check_stable: true,
            check_enabled: false,
            scroll_into_view: true,
        }
    }
}

impl ActionabilityOptions {
    fn for_click() -> Self {
        Self {
            check_visible: true,
            check_stable: true,
            check_enabled: true,
            scroll_into_view: true,
        }
    }

    fn for_fill() -> Self {
        Self {
            check_visible: true,
            check_stable: false,
            check_enabled: true,
            scroll_into_view: true,
        }
    }

    fn for_hover() -> Self {
        Self {
            check_visible: true,
            check_stable: true,
            check_enabled: false,
            scroll_into_view: true,
        }
    }
}

async fn wait_for_actionable(
    page: &Page,
    selector: &str,
    timeout_ms: u64,
    opts: ActionabilityOptions,
) -> Result<Element> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");

    loop {
        if start.elapsed() >= timeout {
            return Err(ChromeError::ElementNotFound {
                selector: selector.to_string(),
            });
        }

        let element = match page.find_element(selector).await {
            Ok(el) => el,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(ms::POLL_INTERVAL)).await;
                continue;
            }
        };

        if opts.scroll_into_view {
            let scroll_script = format!(
                "document.querySelector('{}')?.scrollIntoView({{block:'center',behavior:'instant'}})",
                escaped
            );
            let _ = page.evaluate(scroll_script).await;
            tokio::time::sleep(Duration::from_millis(ms::VIEWPORT_SETTLE)).await;
        }

        if opts.check_visible {
            let visible_script = format!(
                r#"(function(){{
                    const el=document.querySelector('{}');
                    if(!el)return false;
                    const style=window.getComputedStyle(el);
                    const rect=el.getBoundingClientRect();
                    return style.display!=='none' &&
                           style.visibility!=='hidden' &&
                           parseFloat(style.opacity||'1')>0 &&
                           rect.width>0 && rect.height>0;
                }})()"#,
                escaped
            );
            let is_visible = page
                .evaluate(visible_script)
                .await
                .ok()
                .and_then(|r| r.into_value::<bool>().ok())
                .unwrap_or(false);

            if !is_visible {
                tokio::time::sleep(Duration::from_millis(ms::POLL_INTERVAL)).await;
                continue;
            }
        }

        if opts.check_stable {
            let pos_script = format!(
                r#"(function(){{
                    const el=document.querySelector('{}');
                    if(!el)return null;
                    const r=el.getBoundingClientRect();
                    return {{x:r.x,y:r.y,w:r.width,h:r.height}};
                }})()"#,
                escaped
            );

            let pos1 = page.evaluate(pos_script.clone()).await.ok();
            tokio::time::sleep(Duration::from_millis(ms::VIEWPORT_SETTLE)).await;
            let pos2 = page.evaluate(pos_script).await.ok();

            let is_stable = match (pos1, pos2) {
                (Some(p1), Some(p2)) => {
                    let v1: Option<serde_json::Value> = p1.into_value().ok();
                    let v2: Option<serde_json::Value> = p2.into_value().ok();
                    v1 == v2 && v1.is_some()
                }
                _ => false,
            };

            if !is_stable {
                tokio::time::sleep(Duration::from_millis(ms::POLL_INTERVAL)).await;
                continue;
            }
        }

        if opts.check_enabled {
            let enabled_script = format!(
                r#"(function(){{
                    const el=document.querySelector('{}');
                    if(!el)return false;
                    return !el.disabled && !el.hasAttribute('readonly');
                }})()"#,
                escaped
            );
            let is_enabled = page
                .evaluate(enabled_script)
                .await
                .ok()
                .and_then(|r| r.into_value::<bool>().ok())
                .unwrap_or(true);

            if !is_enabled {
                tokio::time::sleep(Duration::from_millis(ms::POLL_INTERVAL)).await;
                continue;
            }
        }

        return Ok(element);
    }
}

async fn click_element(page: &Page, selector: &str) -> Result<()> {
    let element = wait_for_actionable(
        page,
        selector,
        ms::SELECTOR_TIMEOUT,
        ActionabilityOptions::for_click(),
    )
    .await?;
    element
        .click()
        .await
        .map_err(|e| ChromeError::General(format!("Click failed: {}", e)))?;
    Ok(())
}

async fn fill_element(page: &Page, selector: &str, text: &str) -> Result<()> {
    let element = wait_for_actionable(
        page,
        selector,
        ms::SELECTOR_TIMEOUT,
        ActionabilityOptions::for_fill(),
    )
    .await?;

    element
        .click()
        .await
        .map_err(|e| ChromeError::General(format!("Focus failed: {}", e)))?;

    let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
    page.evaluate(format!("document.querySelector('{}').value = ''", escaped))
        .await
        .map_err(|e| ChromeError::General(format!("Clear failed: {}", e)))?;

    element
        .type_str(text)
        .await
        .map_err(|e| ChromeError::General(format!("Type failed: {}", e)))?;

    Ok(())
}

async fn type_element(page: &Page, selector: &str, text: &str, delay_ms: u64) -> Result<()> {
    let element = wait_for_actionable(
        page,
        selector,
        ms::SELECTOR_TIMEOUT,
        ActionabilityOptions::for_fill(),
    )
    .await?;

    element
        .click()
        .await
        .map_err(|e| ChromeError::General(format!("Focus failed: {}", e)))?;

    if delay_ms > 0 {
        for ch in text.chars() {
            element
                .type_str(&ch.to_string())
                .await
                .map_err(|e| ChromeError::General(format!("Failed to type character: {}", e)))?;

            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
        }
    } else {
        element
            .type_str(text)
            .await
            .map_err(|e| ChromeError::General(format!("Failed to type text: {}", e)))?;
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct HoverResult {
    pub hovered: String,
}

impl output::OutputFormatter for HoverResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        text::success(&format!("Hovered: {}", self.hovered))
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

#[derive(Debug, Serialize)]
pub struct PressResult {
    pub pressed: String,
}

impl output::OutputFormatter for PressResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        text::success(&format!("Pressed: {}", self.pressed))
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_hover(provider: &impl PageProvider, selector: &str) -> Result<HoverResult> {
    let page = provider.get_or_create_page().await?;

    let element = wait_for_actionable(
        &page,
        selector,
        ms::SELECTOR_TIMEOUT,
        ActionabilityOptions::for_hover(),
    )
    .await?;

    element
        .hover()
        .await
        .map_err(|e| ChromeError::General(format!("Failed to hover element: {}", e)))?;

    Ok(HoverResult {
        hovered: selector.to_string(),
    })
}

pub async fn handle_press(provider: &impl PageProvider, key: &str) -> Result<PressResult> {
    let page = provider.get_or_create_page().await?;

    use chromiumoxide::cdp::browser_protocol::input::{
        DispatchKeyEventParams, DispatchKeyEventType,
    };

    let key_down = DispatchKeyEventParams::builder()
        .r#type(DispatchKeyEventType::KeyDown)
        .key(key.to_string())
        .build()
        .map_err(|e| ChromeError::General(format!("Failed to build key down params: {}", e)))?;

    page.execute(key_down)
        .await
        .map_err(|e| ChromeError::General(format!("Failed to dispatch key down: {}", e)))?;

    let key_up = DispatchKeyEventParams::builder()
        .r#type(DispatchKeyEventType::KeyUp)
        .key(key.to_string())
        .build()
        .map_err(|e| ChromeError::General(format!("Failed to build key up params: {}", e)))?;

    page.execute(key_up)
        .await
        .map_err(|e| ChromeError::General(format!("Failed to dispatch key up: {}", e)))?;

    Ok(PressResult {
        pressed: key.to_string(),
    })
}
