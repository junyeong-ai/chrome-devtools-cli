use crate::{ChromeError, Result, chrome::PageProvider, js_templates, output, timeouts::ms};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EvalResult {
    pub expression: String,
    pub result: serde_json::Value,
    pub result_type: String,
}

impl output::OutputFormatter for EvalResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let result_str = serde_json::to_string_pretty(&self.result).unwrap_or_default();
        format!(
            "{}\n{}\n{}",
            text::success("Script executed"),
            text::key_value("Type", &self.result_type),
            text::key_value("Result", &result_str)
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

#[derive(Debug, Serialize)]
pub struct WaitResult {
    pub condition: String,
    pub selector: Option<String>,
    pub timeout_ms: u64,
    pub status: String,
}

impl output::OutputFormatter for WaitResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut output = format!(
            "{}\n{}",
            text::success(&format!("Wait completed: {}", self.condition)),
            text::key_value("Timeout", &format!("{}ms", self.timeout_ms))
        );
        if let Some(ref sel) = self.selector {
            output.push_str(&format!("\n{}", text::key_value("Selector", sel)));
        }
        output
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_eval(provider: &impl PageProvider, expression: &str) -> Result<EvalResult> {
    let page = provider.get_or_create_page().await?;

    let result = page
        .evaluate(expression)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let json_value = result
        .into_value::<serde_json::Value>()
        .unwrap_or(serde_json::Value::Null);

    let result_type = match &json_value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
    .to_string();

    Ok(EvalResult {
        expression: expression.to_string(),
        result: json_value,
        result_type,
    })
}

pub async fn handle_wait(
    provider: &impl PageProvider,
    condition: &str,
    selector: Option<&str>,
    timeout_ms: u64,
) -> Result<WaitResult> {
    let page = provider.get_or_create_page().await?;

    let timeout = std::time::Duration::from_millis(timeout_ms);

    match condition {
        "selector" => {
            let sel = selector.ok_or_else(|| {
                ChromeError::General("Selector required for 'selector' condition".to_string())
            })?;

            tokio::time::timeout(timeout, async {
                loop {
                    if page.find_element(sel).await.is_ok() {
                        return Ok::<(), ChromeError>(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(ms::POLL_INTERVAL)).await;
                }
            })
            .await
            .map_err(|_| {
                ChromeError::General(format!("Timeout waiting for selector: {}", sel))
            })??;
        }
        "visible" => {
            let sel = selector.ok_or_else(|| {
                ChromeError::General("Selector required for 'visible' condition".to_string())
            })?;

            let script = js_templates::visibility_check(sel, true);

            tokio::time::timeout(timeout, async {
                loop {
                    if let Ok(result) = page.evaluate(script.as_str()).await
                        && result.into_value::<bool>().unwrap_or(false)
                    {
                        return Ok::<(), ChromeError>(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(ms::POLL_INTERVAL)).await;
                }
            })
            .await
            .map_err(|_| ChromeError::General(format!("Timeout waiting for visible: {}", sel)))??;
        }
        "hidden" => {
            let sel = selector.ok_or_else(|| {
                ChromeError::General("Selector required for 'hidden' condition".to_string())
            })?;

            let script = js_templates::visibility_check(sel, false);

            tokio::time::timeout(timeout, async {
                loop {
                    if let Ok(result) = page.evaluate(script.as_str()).await
                        && result.into_value::<bool>().unwrap_or(false)
                    {
                        return Ok::<(), ChromeError>(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(ms::POLL_INTERVAL)).await;
                }
            })
            .await
            .map_err(|_| ChromeError::General(format!("Timeout waiting for hidden: {}", sel)))??;
        }
        "stable" => {
            let stability_time = std::time::Duration::from_millis(ms::STABILITY_DURATION);
            let check_interval = std::time::Duration::from_millis(ms::POLL_INTERVAL);

            tokio::time::timeout(timeout, async {
                let mut last_count: i64 = -1;
                let mut stable_since = tokio::time::Instant::now();

                loop {
                    if let Ok(result) = page.evaluate(js_templates::MUTATION_OBSERVER).await {
                        let count = result.into_value::<i64>().unwrap_or(0);
                        if count != last_count {
                            last_count = count;
                            stable_since = tokio::time::Instant::now();
                        } else if stable_since.elapsed() >= stability_time {
                            return Ok::<(), ChromeError>(());
                        }
                    }
                    tokio::time::sleep(check_interval).await;
                }
            })
            .await
            .map_err(|_| ChromeError::General("Timeout waiting for DOM stability".to_string()))??;
        }
        _ => {
            return Err(ChromeError::General(format!(
                "Unknown wait condition: {}. Use: selector, visible, hidden, stable",
                condition
            )));
        }
    }

    Ok(WaitResult {
        condition: condition.to_string(),
        selector: selector.map(|s| s.to_string()),
        timeout_ms,
        status: "completed".to_string(),
    })
}
