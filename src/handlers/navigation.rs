use crate::{
    ChromeError, Result,
    chrome::PageProvider,
    output,
    timeouts::{ms, secs},
};
use chromiumoxide::cdp::browser_protocol::page::{
    GetNavigationHistoryParams, NavigateParams, NavigateToHistoryEntryParams,
};
use chromiumoxide::page::Page;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct NavigationResult {
    pub url: String,
    pub title: String,
    pub ms: u64,
}

impl output::OutputFormatter for NavigationResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}\n{}",
            text::success(&format!("Navigated to: {}", self.url)),
            text::key_value("Title", &self.title),
            text::key_value("Load Time", &format!("{}ms", self.ms))
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_navigate(
    provider: &impl PageProvider,
    url: &str,
    wait_for: Option<&str>,
    timeout_secs: u64,
) -> Result<NavigationResult> {
    let start = std::time::Instant::now();

    let page = provider.get_or_create_page().await?;
    let timeout = Duration::from_secs(timeout_secs);

    let nav_params = NavigateParams::builder()
        .url(url)
        .build()
        .map_err(|e| ChromeError::General(format!("Failed to build navigate params: {}", e)))?;

    tokio::time::timeout(timeout, page.execute(nav_params))
        .await
        .map_err(|_| ChromeError::NavigationTimeout(timeout_secs))?
        .map_err(|e| ChromeError::General(format!("Navigation failed: {}", e)))?;

    let wait_result = match wait_for {
        Some("load") => wait_for_load(&page, timeout).await,
        Some("domcontentloaded") => wait_for_dom_content_loaded(&page, timeout).await,
        Some("networkidle") => wait_for_network_idle(&page, timeout).await,
        _ => wait_for_load(&page, timeout).await,
    };

    wait_result?;

    let final_url = page
        .url()
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| url.to_string());
    let title = page
        .evaluate("document.title")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
        .unwrap_or_default();

    provider.update_active_page_info().await.ok();

    Ok(NavigationResult {
        url: final_url,
        title,
        ms: start.elapsed().as_millis() as u64,
    })
}

pub async fn handle_reload(provider: &impl PageProvider, hard: bool) -> Result<NavigationResult> {
    use chromiumoxide::cdp::browser_protocol::page::ReloadParams;

    let start = std::time::Instant::now();
    let page = provider.get_or_create_page().await?;

    let params = ReloadParams::builder().ignore_cache(hard).build();

    page.execute(params)
        .await
        .map_err(|e| ChromeError::General(format!("Reload failed: {}", e)))?;

    wait_for_load(&page, Duration::from_secs(secs::NAVIGATION)).await?;

    let url = page.url().await.unwrap_or_default().unwrap_or_default();
    let title = page
        .evaluate("document.title")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
        .unwrap_or_default();

    Ok(NavigationResult {
        url,
        title,
        ms: start.elapsed().as_millis() as u64,
    })
}

pub async fn handle_back(provider: &impl PageProvider) -> Result<NavigationResult> {
    let start = std::time::Instant::now();
    let page = provider.get_or_create_page().await?;

    let history = page
        .execute(GetNavigationHistoryParams::default())
        .await
        .map_err(|e| ChromeError::General(format!("Failed to get navigation history: {}", e)))?;

    let current_index = history.current_index;
    if current_index <= 0 {
        return Err(ChromeError::General(
            "No history to navigate back".to_string(),
        ));
    }

    let target_index = (current_index - 1) as usize;
    let target_entry = history
        .entries
        .get(target_index)
        .ok_or_else(|| ChromeError::General("Invalid history entry".to_string()))?;

    page.execute(NavigateToHistoryEntryParams::new(target_entry.id))
        .await
        .map_err(|e| ChromeError::General(format!("Failed to navigate to history entry: {}", e)))?;

    wait_for_history_navigation(&page, Duration::from_secs(secs::READY_STATE)).await?;

    let url = page.url().await.unwrap_or_default().unwrap_or_default();
    let title = page
        .evaluate("document.title")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
        .unwrap_or_default();

    provider.update_active_page_info().await.ok();

    Ok(NavigationResult {
        url,
        title,
        ms: start.elapsed().as_millis() as u64,
    })
}

pub async fn handle_forward(provider: &impl PageProvider) -> Result<NavigationResult> {
    let start = std::time::Instant::now();
    let page = provider.get_or_create_page().await?;

    let history = page
        .execute(GetNavigationHistoryParams::default())
        .await
        .map_err(|e| ChromeError::General(format!("Failed to get navigation history: {}", e)))?;

    let current_index = history.current_index as usize;
    let entries_len = history.entries.len();

    if current_index >= entries_len.saturating_sub(1) {
        return Err(ChromeError::General(
            "No forward history available".to_string(),
        ));
    }

    let target_index = current_index + 1;
    let target_entry = history
        .entries
        .get(target_index)
        .ok_or_else(|| ChromeError::General("Invalid history entry".to_string()))?;

    page.execute(NavigateToHistoryEntryParams::new(target_entry.id))
        .await
        .map_err(|e| ChromeError::General(format!("Failed to navigate to history entry: {}", e)))?;

    wait_for_history_navigation(&page, Duration::from_secs(secs::READY_STATE)).await?;

    let url = page.url().await.unwrap_or_default().unwrap_or_default();
    let title = page
        .evaluate("document.title")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
        .unwrap_or_default();

    provider.update_active_page_info().await.ok();

    Ok(NavigationResult {
        url,
        title,
        ms: start.elapsed().as_millis() as u64,
    })
}

async fn wait_for_load(page: &Page, timeout: Duration) -> Result<()> {
    tokio::time::timeout(timeout, async {
        const MAX_POLLS: usize = 600;
        let mut stable_count = 0;

        for _ in 0..MAX_POLLS {
            match tokio::time::timeout(
                Duration::from_secs(secs::READY_STATE),
                page.evaluate("document.readyState"),
            )
            .await
            {
                Ok(Ok(result)) => {
                    if let Ok(state) = result.into_value::<String>() {
                        if state == "complete" {
                            stable_count += 1;
                            if stable_count >= 2 {
                                return Ok::<(), ChromeError>(());
                            }
                        } else {
                            stable_count = 0;
                        }
                    }
                }
                Ok(Err(_)) | Err(_) => {
                    stable_count = 0;
                }
            }
            tokio::time::sleep(Duration::from_millis(ms::VIEWPORT_SETTLE)).await;
        }

        Ok::<(), ChromeError>(())
    })
    .await
    .map_err(|_| ChromeError::NavigationTimeout(timeout.as_secs()))?
}

async fn wait_for_dom_content_loaded(page: &Page, timeout: Duration) -> Result<()> {
    tokio::time::timeout(timeout, async {
        const MAX_POLLS: usize = 300;

        for _ in 0..MAX_POLLS {
            if let Ok(Ok(result)) = tokio::time::timeout(
                Duration::from_secs(secs::READY_STATE),
                page.evaluate("document.readyState"),
            )
            .await
                && let Ok(ready_state) = result.into_value::<String>()
                && (ready_state == "interactive" || ready_state == "complete")
            {
                return Ok::<(), ChromeError>(());
            }
            tokio::time::sleep(Duration::from_millis(ms::POLL_INTERVAL)).await;
        }
        Ok::<(), ChromeError>(())
    })
    .await
    .map_err(|_| ChromeError::NavigationTimeout(timeout.as_secs()))?
}

async fn wait_for_network_idle(page: &Page, timeout: Duration) -> Result<()> {
    tokio::time::timeout(timeout, async {
        const MAX_POLLS: usize = 600;

        for _ in 0..MAX_POLLS {
            if let Ok(Ok(result)) = tokio::time::timeout(
                Duration::from_secs(secs::READY_STATE),
                page.evaluate("document.readyState"),
            )
            .await
                && let Ok(state) = result.into_value::<String>()
                && state == "complete"
            {
                tokio::time::sleep(Duration::from_millis(ms::NETWORK_IDLE)).await;
                return Ok::<(), ChromeError>(());
            }
            tokio::time::sleep(Duration::from_millis(ms::POLL_INTERVAL)).await;
        }
        Ok::<(), ChromeError>(())
    })
    .await
    .map_err(|_| ChromeError::NavigationTimeout(timeout.as_secs()))?
}

async fn wait_for_history_navigation(page: &Page, timeout: Duration) -> Result<()> {
    tokio::time::timeout(timeout, async {
        tokio::time::sleep(Duration::from_millis(ms::POLL_INTERVAL)).await;

        for _ in 0..50 {
            let ready_state: String = page
                .evaluate("document.readyState")
                .await
                .map_err(|e| ChromeError::EvaluationError(e.to_string()))?
                .into_value()
                .unwrap_or_default();

            if ready_state == "complete" {
                return Ok::<(), ChromeError>(());
            }

            tokio::time::sleep(Duration::from_millis(ms::POLL_INTERVAL)).await;
        }

        Ok::<(), ChromeError>(())
    })
    .await
    .map_err(|_| ChromeError::NavigationTimeout(timeout.as_secs()))?
}
