use super::collectors::CollectorSet;
use super::models::BrowserSession;
use super::storage::SessionStorage;
use crate::{
    ChromeError, Result,
    config::Config,
    timeouts::{ms, secs},
};
use chromiumoxide::cdp::browser_protocol::target::{
    AttachToTargetParams, CloseTargetParams, TargetId,
};
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Deserialize)]
struct PageTarget {
    id: TargetId,
    url: String,
    #[serde(rename = "type")]
    target_type: String,
}
pub struct SessionConfig {
    pub keep_alive: bool,
    pub headless: bool,
    pub session_id: Option<String>,
}

impl SessionConfig {
    pub fn new(keep_alive: bool, headless: bool, session_id: Option<String>) -> Self {
        Self {
            keep_alive,
            headless,
            session_id,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PageInfo {
    pub index: usize,
    pub url: String,
    pub title: String,
    pub is_selected: bool,
}

pub struct BrowserSessionManager {
    browser: Arc<RwLock<Option<Arc<Browser>>>>,
    pages: Arc<RwLock<Vec<Arc<Page>>>>,
    selected_page: Arc<RwLock<usize>>,
    storage: Arc<SessionStorage>,
    collectors: Arc<CollectorSet>,
    config: Arc<Config>,
    session_config: SessionConfig,
}

impl BrowserSessionManager {
    pub fn new(config: Arc<Config>, session_config: SessionConfig) -> Result<Self> {
        SessionStorage::cleanup_stale(config.storage.session_ttl_hours * 3600)?;

        let storage = Self::resolve_storage(&session_config)?;

        Ok(Self {
            browser: Arc::new(RwLock::new(None)),
            pages: Arc::new(RwLock::new(Vec::new())),
            selected_page: Arc::new(RwLock::new(0)),
            collectors: Arc::new(CollectorSet::new(
                storage.clone(),
                config.dialog.clone(),
                config.filters.clone(),
            )),
            storage,
            config,
            session_config,
        })
    }

    fn resolve_storage(session_config: &SessionConfig) -> Result<Arc<SessionStorage>> {
        if let Some(ref id) = session_config.session_id {
            return Ok(Arc::new(SessionStorage::from_session_id(id)?));
        }

        if session_config.keep_alive
            && let Ok(Some(existing)) = Self::load_existing_session()
        {
            return Ok(Arc::new(SessionStorage::new(&existing.session_id)?));
        }

        Ok(Arc::new(SessionStorage::new(
            &uuid::Uuid::new_v4().to_string(),
        )?))
    }

    fn load_existing_session() -> Result<Option<BrowserSession>> {
        let path = crate::config::default_config_dir()?.join("session.toml");
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(Some(toml::from_str(&content)?))
    }

    pub fn session_id(&self) -> &str {
        self.storage.session_id()
    }

    pub fn storage(&self) -> &Arc<SessionStorage> {
        &self.storage
    }

    pub fn collectors(&self) -> &Arc<CollectorSet> {
        &self.collectors
    }

    pub async fn get_or_create_browser(&self) -> Result<Arc<Browser>> {
        if let Some(browser) = self.browser.read().await.as_ref() {
            return Ok(browser.clone());
        }

        // Always try to connect to existing browser first (Playwright/Puppeteer best practice)
        // This ensures page state is preserved across CLI invocations
        if let Ok(Some(session)) = self.load_session().await
            && let Ok(browser) = self.connect_to_existing(session.debug_port).await
        {
            *self.browser.write().await = Some(browser.clone());
            self.restore_pages(
                &browser,
                session.active_page_target_id.as_deref(),
                session.active_page_url.as_deref(),
                Some(session.selected_page_index),
            )
            .await?;

            if let Ok(page) = self.get_active_page().await {
                let target_id = page.target_id().inner().to_string();
                let url = page.url().await.unwrap_or_default();
                let mut updated_session = session.clone();
                updated_session.active_page_target_id = Some(target_id);
                updated_session.active_page_url = url;
                self.save_session(&updated_session).await.ok();
            }

            return Ok(browser);
        }

        self.launch_browser().await
    }

    pub async fn get_active_page(&self) -> Result<Arc<Page>> {
        let pages = self.pages.read().await;
        let index = *self.selected_page.read().await;

        pages
            .get(index)
            .cloned()
            .ok_or_else(|| ChromeError::General("No active page".to_string()))
    }

    pub async fn get_or_create_page(&self) -> Result<Arc<Page>> {
        if let Ok(page) = self.get_active_page().await {
            return Ok(page);
        }

        let _browser = self.get_or_create_browser().await?;

        if let Ok(page) = self.get_active_page().await {
            return Ok(page);
        }

        self.new_page(None).await
    }

    pub async fn new_page(&self, url: Option<&str>) -> Result<Arc<Page>> {
        let browser = self.get_or_create_browser().await?;

        let page = browser
            .new_page(url.unwrap_or("about:blank"))
            .await
            .map_err(|e| ChromeError::General(e.to_string()))?;

        let page = Arc::new(page);
        self.collectors.attach(&page).await?;

        {
            let mut pages = self.pages.write().await;
            pages.push(page.clone());
            *self.selected_page.write().await = pages.len() - 1;
        }

        self.update_active_page_info().await.ok();

        Ok(page)
    }

    pub async fn select_page(&self, index: usize) -> Result<()> {
        let page_count = if let Ok(http_pages) = self.get_page_list_from_http().await {
            http_pages.len()
        } else {
            self.pages.read().await.len()
        };

        if index >= page_count {
            return Err(ChromeError::General(format!(
                "Invalid page index: {} (total: {})",
                index, page_count
            )));
        }

        *self.selected_page.write().await = index;
        self.update_active_page_info().await.ok();
        Ok(())
    }

    pub async fn list_pages(&self) -> Vec<PageInfo> {
        if let Ok(http_pages) = self.get_page_list_from_http().await
            && !http_pages.is_empty()
        {
            return self.build_page_info_from_http(http_pages).await;
        }

        self.build_page_info_from_memory().await
    }

    async fn build_page_info_from_http(&self, http_pages: Vec<PageTarget>) -> Vec<PageInfo> {
        let session = self.load_session().await.ok().flatten();
        let saved_pages = session
            .as_ref()
            .map(|s| s.pages.clone())
            .unwrap_or_default();

        let selected = if let Some(ref s) = session {
            *self.selected_page.write().await = s.selected_page_index;
            s.selected_page_index
        } else {
            *self.selected_page.read().await
        };

        // Acquire read lock on pages AFTER session data is loaded
        let in_memory_pages = self.pages.read().await;

        // Build a map of target_id -> saved_index for order preservation
        let saved_order: std::collections::HashMap<String, usize> = saved_pages
            .iter()
            .enumerate()
            .map(|(i, p)| (p.target_id.clone(), i))
            .collect();

        // Map of target_id -> in-memory page for title lookup
        let memory_map: std::collections::HashMap<String, &Arc<Page>> = in_memory_pages
            .iter()
            .map(|p| (p.target_id().inner().to_string(), p))
            .collect();

        let mut result: Vec<(usize, PageInfo)> = Vec::with_capacity(http_pages.len());

        for http_page in &http_pages {
            let target_id = http_page.id.inner().to_string();

            // Get title from in-memory page if available (more accurate)
            let title = if let Some(page) = memory_map.get(&target_id) {
                page.get_title()
                    .await
                    .unwrap_or_default()
                    .unwrap_or_default()
            } else {
                String::new()
            };

            // Determine order: use saved order if available, otherwise append at end
            let order = saved_order.get(&target_id).copied().unwrap_or(usize::MAX);

            result.push((
                order,
                PageInfo {
                    index: 0, // Will be reassigned after sorting
                    url: http_page.url.clone(),
                    title,
                    is_selected: false, // Will be set after sorting
                },
            ));
        }

        // Sort by saved order to maintain consistent page indices
        result.sort_by_key(|(order, _)| *order);

        // Reassign indices and set selected flag
        result
            .into_iter()
            .enumerate()
            .map(|(i, (_, mut info))| {
                info.index = i;
                info.is_selected = i == selected;
                info
            })
            .collect()
    }

    /// Build PageInfo list from in-memory pages (fallback)
    async fn build_page_info_from_memory(&self) -> Vec<PageInfo> {
        let pages = self.pages.read().await;
        let selected = *self.selected_page.read().await;

        let mut result = Vec::with_capacity(pages.len());
        for (i, page) in pages.iter().enumerate() {
            let url = page.url().await.unwrap_or_default().unwrap_or_default();
            let title = page
                .get_title()
                .await
                .unwrap_or_default()
                .unwrap_or_default();

            result.push(PageInfo {
                index: i,
                url,
                title,
                is_selected: i == selected,
            });
        }
        result
    }

    /// Update the active page info in the session file for restoration
    /// Best practice (Playwright-style): Use HTTP API as authoritative source for pages,
    /// ensuring persistence across CLI invocations even when in-memory state is empty
    ///
    /// CRITICAL FIX: Always persist session state regardless of keep_alive flag.
    /// This ensures select-page index persists across CLI invocations.
    /// The keep_alive flag controls browser lifecycle, not session data persistence.
    pub async fn update_active_page_info(&self) -> Result<()> {
        // REMOVED: early return for !keep_alive
        // Session data should always be persisted to ensure consistency across CLI invocations
        // The keep_alive flag is for browser lifecycle management, not data persistence

        let selected_idx = *self.selected_page.read().await;

        // Best practice: Use HTTP API as authoritative source for page list
        // This ensures persistence even when in-memory pages is empty (new CLI process)
        let saved_pages = if let Ok(http_pages) = self.get_page_list_from_http().await {
            http_pages
                .into_iter()
                .map(|p| super::models::SavedPageEntry {
                    target_id: p.id.inner().to_string(),
                    url: p.url,
                })
                .collect()
        } else {
            // Fallback to in-memory pages if HTTP API fails
            let pages_guard = self.pages.read().await;
            let mut result = Vec::with_capacity(pages_guard.len());
            for p in pages_guard.iter() {
                let page_url = p.url().await.unwrap_or_default().unwrap_or_default();
                let page_target_id = p.target_id().inner().to_string();
                result.push(super::models::SavedPageEntry {
                    target_id: page_target_id,
                    url: page_url,
                });
            }
            result
        };

        // Get active page info (may fail if no pages exist)
        let (target_id, url) = if let Ok(page) = self.get_active_page().await {
            (
                Some(page.target_id().inner().to_string()),
                page.url().await.unwrap_or_default(),
            )
        } else if !saved_pages.is_empty() && selected_idx < saved_pages.len() {
            // Fallback: use saved pages info
            let entry = &saved_pages[selected_idx];
            (Some(entry.target_id.clone()), Some(entry.url.clone()))
        } else {
            (None, None)
        };

        let mut session =
            BrowserSession::new(self.session_id().to_string(), self.config.browser.port);
        session.active_page_target_id = target_id;
        session.active_page_url = url;
        session.selected_page_index = selected_idx;
        session.pages = saved_pages;

        self.save_session(&session).await
    }

    pub async fn close_page(&self, index: usize) -> Result<()> {
        {
            let mut pages = self.pages.write().await;

            if pages.len() <= 1 {
                return Err(ChromeError::General("Cannot close last page".to_string()));
            }

            if index >= pages.len() {
                return Err(ChromeError::General(format!(
                    "Invalid page index: {}",
                    index
                )));
            }

            // Get the page to close and its target_id
            let page = &pages[index];
            let target_id = page.target_id().clone();

            // Execute CDP Target.closeTarget command to actually close the Chrome tab
            let close_params = CloseTargetParams::new(target_id);
            page.execute(close_params)
                .await
                .map_err(|e| ChromeError::General(format!("Failed to close tab via CDP: {}", e)))?;

            // Remove from internal vector after successful CDP close
            pages.remove(index);

            let mut selected = self.selected_page.write().await;
            if *selected >= pages.len() {
                *selected = pages.len() - 1;
            }
        }

        // Persist session state after page closure (Playwright best practice)
        self.update_active_page_info().await.ok();

        Ok(())
    }

    pub async fn cleanup(&self, keep_data: bool) -> Result<()> {
        if let Some(browser) = self.browser.write().await.take() {
            drop(browser);
        }
        self.pages.write().await.clear();

        if !keep_data {
            self.storage.cleanup()?;
        }

        self.cleanup_session_file().await?;
        Ok(())
    }

    pub fn network_count(&self) -> usize {
        self.collectors.network_count()
    }

    pub fn console_count(&self) -> usize {
        self.collectors.console_count()
    }

    async fn launch_browser(&self) -> Result<Arc<Browser>> {
        let chrome_path = self
            .config
            .browser
            .chrome_path
            .clone()
            .map(|p| p.to_string_lossy().to_string())
            .map(Ok)
            .unwrap_or_else(|| {
                crate::utils::find_chrome_executable().map(|p| p.to_string_lossy().to_string())
            })?;

        let port = self.config.browser.port;

        let has_extension = self.resolve_extension_path().is_some();

        if self.session_config.keep_alive
            || self.session_config.session_id.is_some()
            || has_extension
        {
            return self.launch_persistent(&chrome_path, port).await;
        }

        let mut builder = BrowserConfig::builder()
            .chrome_executable(&chrome_path)
            .port(port)
            .request_timeout(Duration::from_secs(secs::REQUEST));

        if self.session_config.headless {
            builder = builder.arg("--headless");
        }

        if let Some(ref dir) = self.config.browser.user_data_dir {
            builder = builder.user_data_dir(dir);
        }

        if self.config.browser.disable_web_security {
            builder = builder.arg("--disable-web-security");
        }

        let config = builder
            .build()
            .map_err(|e| ChromeError::LaunchFailed(e.to_string()))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| ChromeError::LaunchFailed(e.to_string()))?;

        tokio::spawn(async move { while handler.next().await.is_some() {} });

        let browser = Arc::new(browser);
        *self.browser.write().await = Some(browser.clone());

        Ok(browser)
    }

    async fn launch_persistent(&self, chrome_path: &str, port: u16) -> Result<Arc<Browser>> {
        use std::process::Command;

        let mut cmd = Command::new(chrome_path);
        cmd.arg(format!("--remote-debugging-port={}", port));

        if self.session_config.headless {
            cmd.arg("--headless");
        }

        let user_data = self
            .config
            .browser
            .user_data_dir
            .clone()
            .unwrap_or_else(|| {
                dirs::cache_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                    .join("chrome-devtools-cli")
                    .join("chrome-profile")
            });

        std::fs::create_dir_all(&user_data).ok();

        // Remove stale SingletonLock from previous crash (Playwright/Puppeteer best practice)
        let lock_file = user_data.join("SingletonLock");
        if lock_file.exists() || lock_file.is_symlink() {
            std::fs::remove_file(&lock_file).ok();
        }

        cmd.arg(format!("--user-data-dir={}", user_data.display()));

        if self.config.browser.disable_web_security {
            cmd.arg("--disable-web-security");
        }

        // Load extension using Playwright/Puppeteer best practice pattern
        // --load-extension and --disable-extensions-except enable extension in both headed and headless modes
        if let Some(extension_path) = self.resolve_extension_path()
            && extension_path.exists()
        {
            let ext_str = extension_path.display().to_string();
            cmd.arg(format!("--disable-extensions-except={}", ext_str));
            cmd.arg(format!("--load-extension={}", ext_str));
            tracing::debug!("Loading extension from: {}", ext_str);
        }

        cmd.args([
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-sync",
            "--disable-features=ProfilePickerOnStartup",
            "--profile-directory=Default",
        ]);

        cmd.spawn()
            .map_err(|e| ChromeError::LaunchFailed(format!("Failed to spawn Chrome: {}", e)))?;

        let browser = self.connect_with_retry(port, 10).await?;

        *self.browser.write().await = Some(browser.clone());

        // Wait for Chrome's default tab with retry
        let page = self.wait_for_page_with_retry(&browser, 10).await?;

        // Store the page in our pages array
        {
            let mut pages = self.pages.write().await;
            pages.push(page.clone());
            *self.selected_page.write().await = 0;
        }

        // Save session with TargetId
        let target_id = page.target_id().inner().to_string();
        let url = page.url().await.unwrap_or_default();

        let mut session = BrowserSession::new(self.session_id().to_string(), port);
        session.active_page_target_id = Some(target_id);
        session.active_page_url = url;
        self.save_session(&session).await?;

        Ok(browser)
    }

    async fn connect_to_existing(&self, port: u16) -> Result<Arc<Browser>> {
        use chromiumoxide::handler::HandlerConfig;

        let url = format!("http://127.0.0.1:{}/json/version", port);

        let response: serde_json::Value = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|_| ChromeError::ConnectionLost)?
            .json()
            .await
            .map_err(|_| ChromeError::ConnectionLost)?;

        let ws_url = response
            .get("webSocketDebuggerUrl")
            .and_then(|v| v.as_str())
            .ok_or(ChromeError::ConnectionLost)?;

        let handler_config = HandlerConfig {
            request_timeout: Duration::from_secs(secs::REQUEST),
            ..Default::default()
        };

        let (browser, mut handler) = Browser::connect_with_config(ws_url, handler_config)
            .await
            .map_err(|_| ChromeError::ConnectionLost)?;

        tokio::spawn(async move { while handler.next().await.is_some() {} });

        Ok(Arc::new(browser))
    }

    async fn connect_with_retry(&self, port: u16, retries: u32) -> Result<Arc<Browser>> {
        for attempt in 1..=retries {
            tokio::time::sleep(Duration::from_millis(ms::SESSION_CLEANUP_INTERVAL)).await;

            if let Ok(browser) = self.connect_to_existing(port).await {
                return Ok(browser);
            }

            tracing::debug!("Connection attempt {} failed", attempt);
        }

        Err(ChromeError::ConnectionLost)
    }

    /// Wait for Chrome's first page to be available with retry
    async fn wait_for_page_with_retry(
        &self,
        browser: &Arc<Browser>,
        max_retries: u32,
    ) -> Result<Arc<Page>> {
        for attempt in 1..=max_retries {
            tokio::time::sleep(Duration::from_millis(ms::PAGE_LOAD_SETTLE)).await;

            // Try HTTP /json/list first
            if let Ok(page_infos) = self.get_page_list_from_http().await {
                for info in page_infos {
                    if let Ok(page) = browser.get_page(info.id).await {
                        let page = Arc::new(page);
                        self.collectors.attach(&page).await.ok();
                        return Ok(page);
                    }
                }
            }

            // Fallback to browser.pages()
            if let Ok(pages) = browser.pages().await
                && let Some(page) = pages.into_iter().next()
            {
                let page = Arc::new(page);
                self.collectors.attach(&page).await.ok();
                return Ok(page);
            }

            tracing::debug!("Page wait attempt {} failed", attempt);
        }

        // If no page after retries, create a new one
        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| ChromeError::General(format!("Failed to create page: {}", e)))?;

        let page = Arc::new(page);
        self.collectors.attach(&page).await?;
        Ok(page)
    }

    async fn restore_pages(
        &self,
        browser: &Arc<Browser>,
        target_id: Option<&str>,
        fallback_url: Option<&str>,
        saved_selected_index: Option<usize>,
    ) -> Result<()> {
        const MAX_RETRIES: u32 = 15;

        let saved_pages = if let Ok(Some(session)) = self.load_session().await {
            session.pages
        } else {
            Vec::new()
        };

        for attempt in 1..=MAX_RETRIES {
            let http_page_infos = match self.get_page_list_from_http().await {
                Ok(infos) if !infos.is_empty() => infos,
                _ => {
                    tokio::time::sleep(Duration::from_millis(ms::RETRY_DELAY)).await;
                    tracing::debug!("HTTP /json/list empty, attempt {}/{}", attempt, MAX_RETRIES);
                    continue;
                }
            };

            let http_map: std::collections::HashMap<String, &PageTarget> = http_page_infos
                .iter()
                .map(|p| (p.id.inner().to_string(), p))
                .collect();

            for info in &http_page_infos {
                if info.target_type == "page" {
                    let attach_params = AttachToTargetParams::builder()
                        .target_id(info.id.clone())
                        .flatten(true)
                        .build()
                        .unwrap();

                    if let Err(e) = browser.execute(attach_params).await {
                        tracing::debug!("Failed to attach to target {}: {}", info.id.inner(), e);
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(ms::PAGE_CLOSE_SETTLE)).await;

            let mut pages = self.pages.write().await;
            let mut restored_count: usize = 0;

            if !saved_pages.is_empty() {
                for saved in &saved_pages {
                    if http_map.contains_key(&saved.target_id)
                        && let Ok(page) = browser
                            .get_page(TargetId::from(saved.target_id.clone()))
                            .await
                    {
                        let page = Arc::new(page);
                        self.collectors.attach(&page).await.ok();
                        pages.push(page);
                        restored_count += 1;
                    }
                }

                if restored_count > 0 {
                    let final_idx = saved_selected_index
                        .filter(|&idx| idx < restored_count)
                        .unwrap_or(0);
                    *self.selected_page.write().await = final_idx;
                    tracing::debug!(
                        "Restored {} pages in saved order, selected index {}",
                        restored_count,
                        final_idx
                    );
                    return Ok(());
                }
            }

            let mut selected_idx: usize = 0;
            let mut matched_by_target_id = false;
            let mut matched_by_url = false;

            for info in &http_page_infos {
                match browser.get_page(info.id.clone()).await {
                    Ok(page) => {
                        let page_target_id = page.target_id().inner().to_string();
                        let page = Arc::new(page);
                        self.collectors.attach(&page).await.ok();

                        let current_idx = pages.len();
                        pages.push(page);
                        restored_count += 1;

                        if !matched_by_target_id
                            && let Some(tid) = target_id
                            && page_target_id == tid
                        {
                            selected_idx = current_idx;
                            matched_by_target_id = true;
                        }

                        if !matched_by_target_id
                            && !matched_by_url
                            && let Some(url) = fallback_url
                            && (info.url.contains(url) || url.contains(&info.url))
                        {
                            selected_idx = current_idx;
                            matched_by_url = true;
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Failed to get page {}: {}", info.id.inner(), e);
                    }
                }
            }

            if restored_count > 0 {
                let final_selected_idx = if matched_by_target_id || matched_by_url {
                    selected_idx
                } else {
                    restored_count.saturating_sub(1)
                };

                *self.selected_page.write().await = final_selected_idx;
                tracing::debug!(
                    "Restored {} pages (fallback mode), selected index {} (matched: tid={}, url={})",
                    restored_count,
                    final_selected_idx,
                    matched_by_target_id,
                    matched_by_url
                );
                return Ok(());
            }

            drop(pages);
            tokio::time::sleep(Duration::from_millis(ms::RETRY_DELAY)).await;
            tracing::debug!(
                "Page restoration attempt {}/{} - no pages restored",
                attempt,
                MAX_RETRIES
            );
        }

        // All retries failed, create fallback page with URL if available
        self.create_fallback_page(browser, fallback_url).await
    }

    async fn create_fallback_page(
        &self,
        browser: &Arc<Browser>,
        fallback_url: Option<&str>,
    ) -> Result<()> {
        let page_url = fallback_url.unwrap_or("about:blank");
        let page = Arc::new(
            browser
                .new_page(page_url)
                .await
                .map_err(|e| ChromeError::General(e.to_string()))?,
        );

        if page_url != "about:blank" {
            tokio::time::sleep(Duration::from_millis(ms::SESSION_CLEANUP_INTERVAL)).await;
        }

        self.collectors.attach(&page).await?;

        let mut pages = self.pages.write().await;
        pages.push(page);
        drop(pages);

        *self.selected_page.write().await = 0;

        Ok(())
    }

    async fn get_page_list_from_http(&self) -> Result<Vec<PageTarget>> {
        let url = format!("http://127.0.0.1:{}/json/list", self.config.browser.port);

        let response: Vec<PageTarget> = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|_| ChromeError::ConnectionLost)?
            .json()
            .await
            .map_err(|_| ChromeError::ConnectionLost)?;

        Ok(response
            .into_iter()
            .filter(|t| t.target_type == "page")
            .collect())
    }

    async fn save_session(&self, session: &BrowserSession) -> Result<()> {
        let dir = crate::config::default_config_dir()?;
        std::fs::create_dir_all(&dir)?;

        let path = dir.join("session.toml");
        std::fs::write(path, toml::to_string(session)?)?;

        Ok(())
    }

    async fn load_session(&self) -> Result<Option<BrowserSession>> {
        let path = crate::config::default_config_dir()?.join("session.toml");

        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)?;
        Ok(Some(toml::from_str(&content)?))
    }

    async fn cleanup_session_file(&self) -> Result<()> {
        let path = crate::config::default_config_dir()?.join("session.toml");
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Resolve extension path with fallback to default installation location.
    /// Priority:
    /// 1. Config file extension_path
    /// 2. Default installation: ~/.config/chrome-devtools-cli/extension/
    fn resolve_extension_path(&self) -> Option<std::path::PathBuf> {
        // First check config
        if let Some(ref path) = self.config.browser.extension_path
            && path.exists()
            && path.join("manifest.json").exists()
        {
            return Some(path.clone());
        }

        // Fallback to default installation location (set by install.sh)
        if let Ok(config_dir) = crate::config::default_config_dir() {
            let default_ext = config_dir.join("extension");
            if default_ext.exists() && default_ext.join("manifest.json").exists() {
                return Some(default_ext);
            }
        }

        None
    }
}

#[async_trait::async_trait]
impl super::PageProvider for BrowserSessionManager {
    async fn get_or_create_page(&self) -> Result<Arc<Page>> {
        self.get_or_create_page().await
    }

    fn storage(&self) -> &Arc<SessionStorage> {
        &self.storage
    }

    fn collectors(&self) -> &Arc<CollectorSet> {
        &self.collectors
    }

    async fn update_active_page_info(&self) -> Result<()> {
        BrowserSessionManager::update_active_page_info(self).await
    }
}
