use crate::{ChromeError, Result, timeouts::ms};
use chromiumoxide::Page;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ActionConfig {
    pub wait_for_navigation: bool,
    pub wait_for_stable_dom: bool,
    pub cpu_timeout_multiplier: f64,
    pub network_timeout_multiplier: f64,
}

impl Default for ActionConfig {
    fn default() -> Self {
        Self {
            wait_for_navigation: true,
            wait_for_stable_dom: true,
            cpu_timeout_multiplier: 1.0,
            network_timeout_multiplier: 1.0,
        }
    }
}

pub struct ActionExecutor {
    page: Arc<Page>,
    config: ActionConfig,
}

impl ActionExecutor {
    fn stable_dom_timeout(&self) -> Duration {
        Duration::from_millis((ms::CDP_ACTION as f64 * self.config.cpu_timeout_multiplier) as u64)
    }

    fn stable_dom_for(&self) -> Duration {
        Duration::from_millis(
            (ms::POLL_INTERVAL as f64 * self.config.cpu_timeout_multiplier) as u64,
        )
    }

    fn navigation_timeout(&self) -> Duration {
        Duration::from_millis(
            (ms::SELECTOR_TIMEOUT as f64 * 2.0 * self.config.network_timeout_multiplier) as u64,
        )
    }
}

impl ActionExecutor {
    pub fn new(page: Arc<Page>, config: ActionConfig) -> Self {
        Self { page, config }
    }

    pub fn with_multipliers(page: Arc<Page>, cpu_multiplier: f64, network_multiplier: f64) -> Self {
        let config = ActionConfig {
            cpu_timeout_multiplier: cpu_multiplier,
            network_timeout_multiplier: network_multiplier,
            ..Default::default()
        };
        Self { page, config }
    }

    pub async fn execute<F, Fut, T>(&self, action: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let nav_watcher = if self.config.wait_for_navigation {
            Some(NavigationWatcher::new(self.page.clone()))
        } else {
            None
        };

        if let Some(ref watcher) = nav_watcher {
            watcher.start_watching().await?;
        }

        let result = action().await?;

        if let Some(watcher) = nav_watcher
            && watcher.was_triggered().await
        {
            self.wait_for_navigation().await?;
        }

        if self.config.wait_for_stable_dom {
            self.wait_for_stable_dom().await?;
        }

        Ok(result)
    }

    async fn wait_for_navigation(&self) -> Result<()> {
        let timeout = self.navigation_timeout();

        tokio::time::timeout(timeout, async {
            self.page
                .wait_for_navigation()
                .await
                .map_err(|e| ChromeError::General(format!("Navigation wait failed: {}", e)))
        })
        .await
        .map_err(|_| ChromeError::NavigationTimeout((timeout.as_millis() / 1000) as u64))??;

        Ok(())
    }

    async fn wait_for_stable_dom(&self) -> Result<()> {
        let timeout = self.stable_dom_timeout();
        let check_interval = Duration::from_millis(ms::POLL_INTERVAL);
        let stability_duration = self.stable_dom_for();

        let start = tokio::time::Instant::now();
        let mut last_mutation_time = tokio::time::Instant::now();
        let mut last_mutation_count: i64 = 0;

        tokio::time::timeout(timeout, async {
            loop {
                let mutation_count = self.get_mutation_count().await.unwrap_or(0);

                if mutation_count != last_mutation_count {
                    last_mutation_time = tokio::time::Instant::now();
                    last_mutation_count = mutation_count;
                }

                let stable_duration = tokio::time::Instant::now() - last_mutation_time;
                if stable_duration >= stability_duration {
                    break;
                }

                if start.elapsed() >= timeout {
                    break;
                }

                tokio::time::sleep(check_interval).await;
            }
            Ok::<(), ChromeError>(())
        })
        .await
        .map_err(|_| ChromeError::General("DOM stability wait timeout".to_string()))??;

        Ok(())
    }

    async fn get_mutation_count(&self) -> Result<i64> {
        let script = r#"
            (function() {
                if (!window.__mutationCount) {
                    window.__mutationCount = 0;
                    const observer = new MutationObserver(() => {
                        window.__mutationCount++;
                    });
                    observer.observe(document.body || document.documentElement, {
                        childList: true,
                        subtree: true,
                        attributes: true,
                        characterData: true
                    });
                }
                return window.__mutationCount;
            })()
        "#;

        let result = self
            .page
            .evaluate(script)
            .await
            .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

        result.into_value::<i64>().map_err(|e| {
            ChromeError::EvaluationError(format!("Failed to parse mutation count: {}", e))
        })
    }
}

struct NavigationWatcher {
    page: Arc<Page>,
}

impl NavigationWatcher {
    fn new(page: Arc<Page>) -> Self {
        Self { page }
    }

    async fn start_watching(&self) -> Result<()> {
        let script = r#"
            (function() {
                if (!window.__navigationWatcher) {
                    window.__navigationWatcher = {
                        triggered: false,
                        originalPushState: history.pushState,
                        originalReplaceState: history.replaceState
                    };

                    history.pushState = function() {
                        window.__navigationWatcher.triggered = true;
                        return window.__navigationWatcher.originalPushState.apply(history, arguments);
                    };

                    history.replaceState = function() {
                        window.__navigationWatcher.triggered = true;
                        return window.__navigationWatcher.originalReplaceState.apply(history, arguments);
                    };

                    window.addEventListener('beforeunload', () => {
                        window.__navigationWatcher.triggered = true;
                    });

                    window.addEventListener('popstate', () => {
                        window.__navigationWatcher.triggered = true;
                    });
                }
            })()
        "#;

        self.page
            .evaluate(script)
            .await
            .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

        Ok(())
    }

    async fn was_triggered(&self) -> bool {
        let script = r#"
            (function() {
                return window.__navigationWatcher ? window.__navigationWatcher.triggered : false;
            })()
        "#;

        let result = self.page.evaluate(script).await;

        result
            .ok()
            .and_then(|r| r.into_value::<serde_json::Value>().ok())
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_config_default() {
        let config = ActionConfig::default();
        assert!(config.wait_for_navigation);
        assert!(config.wait_for_stable_dom);
        assert_eq!(config.cpu_timeout_multiplier, 1.0);
        assert_eq!(config.network_timeout_multiplier, 1.0);
    }

    #[test]
    fn test_timeout_multipliers() {
        let config = ActionConfig {
            cpu_timeout_multiplier: 2.0,
            network_timeout_multiplier: 10.0,
            ..Default::default()
        };

        assert_eq!((3000.0 * config.cpu_timeout_multiplier) as u64, 6000);
        assert_eq!((100.0 * config.cpu_timeout_multiplier) as u64, 200);
        assert_eq!((10000.0 * config.network_timeout_multiplier) as u64, 100000);
    }
}
