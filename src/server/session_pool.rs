use crate::chrome::CollectorSet;
use crate::chrome::storage::SessionStorage;
use crate::config::Config;
use crate::utils::find_chrome_executable;
use crate::{ChromeError, Result, timeouts::secs};
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast};

use super::protocol::SessionEvent;

const PORT_RANGE_START: u16 = 9222;
const PORT_RANGE_END: u16 = 9322;
const DEFAULT_MAX_SESSIONS: usize = 5;

/// Check if a Chrome instance is already running on the given port
async fn check_existing_chrome(port: u16) -> Option<u16> {
    let url = format!("http://127.0.0.1:{}/json/version", port);
    match reqwest::get(&url).await {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!("Found existing Chrome on port {}", port);
            Some(port)
        }
        _ => None,
    }
}

/// Find existing Chrome instance in the port range
async fn find_existing_chrome() -> Option<u16> {
    for port in PORT_RANGE_START..=PORT_RANGE_END {
        if let Some(p) = check_existing_chrome(port).await {
            return Some(p);
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub cdp_port: u16,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub page_count: usize,
    pub headless: bool,
    pub uses_user_profile: bool,
}

pub struct Session {
    pub id: String,
    pub cdp_port: u16,
    browser: Arc<Browser>,
    pages: RwLock<Vec<Arc<Page>>>,
    selected_page: RwLock<usize>,
    collectors: Arc<CollectorSet>,
    storage: Arc<SessionStorage>,
    event_tx: broadcast::Sender<SessionEvent>,
    created_at: Instant,
    last_activity: RwLock<Instant>,
    headless: bool,
    uses_user_profile: bool,
}

impl Session {
    pub async fn new(
        id: String,
        cdp_port: u16,
        config: &Config,
        headless: bool,
        profile_directory: Option<String>,
        extension_path: Option<&PathBuf>,
    ) -> Result<Self> {
        let uses_user_profile = profile_directory.is_some();
        let storage = Arc::new(SessionStorage::new(&id)?);

        let chrome_path = config
            .browser
            .chrome_path
            .clone()
            .or_else(|| find_chrome_executable().ok())
            .unwrap_or_else(|| PathBuf::from("google-chrome"));

        let base_ext = extension_path
            .filter(|p| p.exists() && p.join("manifest.json").exists())
            .cloned()
            .or_else(|| {
                crate::config::default_config_dir()
                    .ok()
                    .map(|d| d.join("extension"))
                    .filter(|p| p.exists() && p.join("manifest.json").exists())
            });

        let resolved_ext = base_ext
            .as_ref()
            .and_then(|src| storage.setup_extension(src).ok());

        let has_extension = resolved_ext.is_some();

        let (browser, mut handler) = if has_extension {
            let mut cmd = Command::new(&chrome_path);
            cmd.arg(format!("--remote-debugging-port={}", cdp_port))
                .arg(format!(
                    "--window-size={},{}",
                    config.browser.window_width, config.browser.window_height
                ))
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg("--disable-features=ProfilePickerOnStartup")
                .stdout(Stdio::null())
                .stderr(Stdio::piped());

            if headless {
                cmd.arg("--headless=new");
            }

            if let Some(ref ext_path) = resolved_ext {
                let ext_str = ext_path.display().to_string();
                cmd.arg(format!("--disable-extensions-except={}", ext_str))
                    .arg(format!("--load-extension={}", ext_str));
            }

            if profile_directory.is_some()
                && let Some(ref user_data_dir) = config.browser.user_data_dir
                && user_data_dir.exists()
            {
                cmd.arg(format!("--user-data-dir={}", user_data_dir.display()));
                let profile = profile_directory
                    .filter(|s| !s.is_empty())
                    .or_else(|| config.browser.profile_directory.clone())
                    .unwrap_or_else(|| "Default".to_string());
                cmd.arg(format!("--profile-directory={}", profile));
            } else {
                cmd.arg("--profile-directory=Default");
            }

            cmd.spawn()
                .map_err(|e| ChromeError::LaunchFailed(e.to_string()))?;

            tokio::time::sleep(Duration::from_secs(secs::DAEMON_STARTUP)).await;

            let debug_url = format!("http://127.0.0.1:{}", cdp_port);
            Browser::connect(&debug_url)
                .await
                .map_err(|e| ChromeError::Connection(e.to_string()))?
        } else {
            let mut builder = BrowserConfig::builder()
                .chrome_executable(&chrome_path)
                .port(cdp_port)
                .request_timeout(Duration::from_secs(
                    config.performance.navigation_timeout_seconds,
                ))
                .disable_default_args();

            if !headless {
                builder = builder.with_head();
            }

            let profile_arg = if profile_directory.is_some()
                && let Some(ref user_data_dir) = config.browser.user_data_dir
                && user_data_dir.exists()
            {
                builder = builder.user_data_dir(user_data_dir.clone());
                let profile = profile_directory
                    .filter(|s| !s.is_empty())
                    .or_else(|| config.browser.profile_directory.clone())
                    .unwrap_or_else(|| "Default".to_string());
                format!("--profile-directory={}", profile)
            } else {
                "--profile-directory=Default".to_string()
            };

            builder = builder
                .window_size(config.browser.window_width, config.browser.window_height)
                .viewport(None)
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg("--disable-features=ProfilePickerOnStartup")
                .arg(profile_arg);

            let browser_config = builder
                .build()
                .map_err(|e| ChromeError::General(e.to_string()))?;

            Browser::launch(browser_config)
                .await
                .map_err(|e| ChromeError::Connection(e.to_string()))?
        };

        tokio::spawn(async move { while handler.next().await.is_some() {} });

        let browser = Arc::new(browser);
        let (event_tx, _) = broadcast::channel(1024);
        let collectors = Arc::new(CollectorSet::new(
            storage.clone(),
            config.dialog.clone(),
            config.filters.clone(),
        ));

        let now = Instant::now();

        Ok(Self {
            id,
            cdp_port,
            browser,
            pages: RwLock::new(Vec::new()),
            selected_page: RwLock::new(0),
            collectors,
            storage,
            event_tx,
            created_at: now,
            last_activity: RwLock::new(now),
            headless,
            uses_user_profile,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_tx.subscribe()
    }

    pub async fn touch(&self) {
        *self.last_activity.write().await = Instant::now();
    }

    pub async fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.clone(),
            cdp_port: self.cdp_port,
            created_at: self.created_at,
            last_activity: *self.last_activity.read().await,
            page_count: self.pages.read().await.len(),
            headless: self.headless,
            uses_user_profile: self.uses_user_profile,
        }
    }

    pub async fn get_or_create_page(&self) -> Result<Arc<Page>> {
        self.touch().await;

        let pages = self.pages.read().await;
        let idx = *self.selected_page.read().await;
        if let Some(page) = pages.get(idx) {
            return Ok(page.clone());
        }
        drop(pages);

        self.new_page(None).await
    }

    pub async fn new_page(&self, url: Option<&str>) -> Result<Arc<Page>> {
        self.touch().await;

        let page = self
            .browser
            .new_page(url.unwrap_or("about:blank"))
            .await
            .map_err(|e| ChromeError::General(e.to_string()))?;

        let page = Arc::new(page);
        self.collectors.attach(&page).await?;

        let mut pages = self.pages.write().await;
        pages.push(page.clone());
        *self.selected_page.write().await = pages.len() - 1;

        Ok(page)
    }

    pub async fn is_alive(&self) -> bool {
        let url = format!("http://127.0.0.1:{}/json/version", self.cdp_port);
        match tokio::time::timeout(
            Duration::from_secs(secs::DAEMON_STARTUP),
            reqwest::get(&url),
        )
        .await
        {
            Ok(Ok(resp)) => resp.status().is_success(),
            _ => false,
        }
    }

    pub fn storage(&self) -> &Arc<SessionStorage> {
        &self.storage
    }

    pub fn collectors(&self) -> &Arc<CollectorSet> {
        &self.collectors
    }

    pub async fn list_pages(&self) -> Vec<PageInfo> {
        let pages = self.pages.read().await;
        let selected = *self.selected_page.read().await;
        let mut infos = Vec::with_capacity(pages.len());

        for (idx, page) in pages.iter().enumerate() {
            let url = page.url().await.unwrap_or_default();
            let title = page.get_title().await.ok().flatten();
            infos.push(PageInfo {
                index: idx,
                url: url.map(|u| u.to_string()),
                title,
                active: idx == selected,
            });
        }

        infos
    }

    pub async fn select_page(&self, index: usize) -> Result<Arc<Page>> {
        self.touch().await;
        let pages = self.pages.read().await;

        if index >= pages.len() {
            return Err(ChromeError::General(format!(
                "Page index {} out of range (0-{})",
                index,
                pages.len().saturating_sub(1)
            )));
        }

        drop(pages);
        *self.selected_page.write().await = index;

        self.get_or_create_page().await
    }

    pub async fn close_page(&self, index: usize) -> Result<()> {
        self.touch().await;
        let mut pages = self.pages.write().await;

        if index >= pages.len() {
            return Err(ChromeError::General(format!(
                "Page index {} out of range",
                index
            )));
        }

        pages.remove(index);

        let mut selected = self.selected_page.write().await;
        if *selected >= pages.len() && !pages.is_empty() {
            *selected = pages.len() - 1;
        }

        Ok(())
    }

    pub async fn get_selected_page(&self) -> Result<Arc<Page>> {
        self.get_or_create_page().await
    }
}

#[async_trait::async_trait]
impl crate::chrome::PageProvider for Session {
    async fn get_or_create_page(&self) -> Result<Arc<Page>> {
        self.get_or_create_page().await
    }

    fn storage(&self) -> &Arc<SessionStorage> {
        &self.storage
    }

    fn collectors(&self) -> &Arc<CollectorSet> {
        &self.collectors
    }
}

#[derive(Debug, Clone)]
pub struct PageInfo {
    pub index: usize,
    pub url: Option<String>,
    pub title: Option<String>,
    pub active: bool,
}

pub struct SessionPool {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
    allocated_ports: RwLock<Vec<u16>>,
    config: Arc<Config>,
    max_sessions: usize,
}

impl SessionPool {
    pub fn new(config: Arc<Config>) -> Self {
        let max_sessions = config.server.max_sessions.unwrap_or(DEFAULT_MAX_SESSIONS);

        Self {
            sessions: RwLock::new(HashMap::new()),
            allocated_ports: RwLock::new(Vec::new()),
            config,
            max_sessions,
        }
    }

    async fn create_session_internal(
        &self,
        headless: bool,
        profile_directory: Option<String>,
        extension_path: Option<&PathBuf>,
    ) -> Result<Arc<Session>> {
        let port = self.allocate_port().await?;
        let id = uuid::Uuid::new_v4().to_string();

        let session = Session::new(
            id.clone(),
            port,
            &self.config,
            headless,
            profile_directory,
            extension_path,
        )
        .await?;
        let session = Arc::new(session);

        self.sessions.write().await.insert(id, session.clone());
        Ok(session)
    }

    pub async fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.read().await.get(id).cloned()
    }

    pub async fn destroy(&self, id: &str) -> Result<()> {
        let session = self.sessions.write().await.remove(id);

        if let Some(session) = session {
            self.release_port(session.cdp_port).await;
            session.storage().cleanup().ok();
        }

        Ok(())
    }

    pub async fn list(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let mut infos = Vec::with_capacity(sessions.len());

        for session in sessions.values() {
            infos.push(session.info().await);
        }

        infos
    }

    /// Clean up sessions whose browsers have been terminated
    pub async fn cleanup_dead_browsers(&self) -> usize {
        let mut to_remove = Vec::new();

        {
            let sessions = self.sessions.read().await;
            for (id, session) in sessions.iter() {
                if !session.is_alive().await {
                    to_remove.push(id.clone());
                }
            }
        }

        let count = to_remove.len();
        for id in to_remove {
            tracing::info!("Cleaning up dead session: {}", id);
            self.destroy(&id).await.ok();
        }

        count
    }

    pub async fn get_or_create_user_profile_session(
        &self,
        headless: bool,
        extension_path: Option<&PathBuf>,
    ) -> Result<Arc<Session>> {
        self.cleanup_dead_browsers().await;

        // Check for existing session in our pool
        {
            let sessions = self.sessions.read().await;
            for session in sessions.values() {
                if session.uses_user_profile && session.is_alive().await {
                    tracing::info!("Reusing user-profile session: {}", session.id);
                    session.touch().await;
                    return Ok(session.clone());
                }
            }
        }

        // Check for existing Chrome instance not in our pool (external or orphaned)
        if let Some(port) = find_existing_chrome().await {
            tracing::info!("Found existing Chrome on port {}, attempting to connect", port);
            match self
                .create_session_from_existing(port, extension_path)
                .await
            {
                Ok(session) => {
                    tracing::info!("Connected to existing Chrome session: {}", session.id);
                    return Ok(session);
                }
                Err(e) => {
                    tracing::warn!("Failed to connect to existing Chrome: {}", e);
                    // Fall through to create new session
                }
            }
        }

        self.ensure_capacity(false).await?;

        tracing::info!("Creating user-profile session");
        self.create_session_internal(headless, Some(String::new()), extension_path)
            .await
    }

    /// Create a session by connecting to an existing Chrome instance
    async fn create_session_from_existing(
        &self,
        cdp_port: u16,
        extension_path: Option<&PathBuf>,
    ) -> Result<Arc<Session>> {
        let id = uuid::Uuid::new_v4().to_string();
        let storage = Arc::new(SessionStorage::new(&id)?);

        // Setup extension if available
        let base_ext = extension_path
            .filter(|p| p.exists() && p.join("manifest.json").exists())
            .cloned()
            .or_else(|| {
                crate::config::default_config_dir()
                    .ok()
                    .map(|d| d.join("extension"))
                    .filter(|p| p.exists() && p.join("manifest.json").exists())
            });

        if let Some(ref src) = base_ext {
            storage.setup_extension(src).ok();
        }

        let debug_url = format!("http://127.0.0.1:{}", cdp_port);
        let (browser, mut handler) = Browser::connect(&debug_url)
            .await
            .map_err(|e| ChromeError::Connection(e.to_string()))?;

        tokio::spawn(async move { while handler.next().await.is_some() {} });

        let browser = Arc::new(browser);
        let collectors = Arc::new(CollectorSet::new(
            storage.clone(),
            self.config.dialog.clone(),
            self.config.filters.clone(),
        ));
        let (event_tx, _) = broadcast::channel(1024);
        let now = Instant::now();

        let session = Arc::new(Session {
            id: id.clone(),
            cdp_port,
            browser,
            pages: RwLock::new(Vec::new()),
            selected_page: RwLock::new(0),
            storage,
            collectors,
            event_tx,
            created_at: now,
            last_activity: RwLock::new(now),
            headless: false,
            uses_user_profile: true,
        });

        // Mark this port as allocated
        self.allocated_ports.write().await.push(cdp_port);
        self.sessions.write().await.insert(id, session.clone());

        Ok(session)
    }

    pub async fn create_ephemeral(
        &self,
        headless: bool,
        extension_path: Option<&PathBuf>,
    ) -> Result<Arc<Session>> {
        self.cleanup_dead_browsers().await;
        self.cleanup_expired_ephemeral().await;
        self.ensure_capacity(true).await?;

        let session = self
            .create_session_internal(headless, None, extension_path)
            .await?;
        tracing::info!("Created ephemeral session: {}", session.id);
        Ok(session)
    }

    async fn ensure_capacity(&self, allow_eviction: bool) -> Result<()> {
        let sessions = self.sessions.read().await;
        if sessions.len() < self.max_sessions {
            return Ok(());
        }
        drop(sessions);

        if allow_eviction && self.cleanup_oldest_ephemeral().await > 0 {
            return Ok(());
        }

        Err(ChromeError::General(format!(
            "Maximum sessions ({}) reached",
            self.max_sessions
        )))
    }

    async fn cleanup_oldest_ephemeral(&self) -> usize {
        let mut oldest: Option<(String, Instant)> = None;

        {
            let sessions = self.sessions.read().await;
            for (id, session) in sessions.iter() {
                if !session.uses_user_profile {
                    let activity = *session.last_activity.read().await;
                    if oldest.is_none() || activity < oldest.as_ref().unwrap().1 {
                        oldest = Some((id.clone(), activity));
                    }
                }
            }
        }

        if let Some((id, _)) = oldest {
            tracing::info!("Removing oldest ephemeral session: {}", id);
            self.destroy(&id).await.ok();
            1
        } else {
            0
        }
    }

    pub async fn cleanup_expired_ephemeral(&self) -> usize {
        const EPHEMERAL_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
        let mut to_remove = Vec::new();
        let now = Instant::now();

        {
            let sessions = self.sessions.read().await;
            for (id, session) in sessions.iter() {
                if !session.uses_user_profile {
                    let last_activity = *session.last_activity.read().await;
                    if now.duration_since(last_activity) > EPHEMERAL_IDLE_TIMEOUT {
                        to_remove.push(id.clone());
                    }
                }
            }
        }

        let count = to_remove.len();
        for id in to_remove {
            self.destroy(&id).await.ok();
        }

        count
    }

    pub async fn cleanup_all(&self) -> usize {
        let ids: Vec<String> = self.sessions.read().await.keys().cloned().collect();
        let count = ids.len();

        for id in ids {
            self.destroy(&id).await.ok();
        }

        count
    }

    pub fn cleanup_stale_storage(&self) -> Result<usize> {
        SessionStorage::cleanup_stale(self.config.storage.session_ttl_hours * 3600)
    }

    async fn allocate_port(&self) -> Result<u16> {
        let mut ports = self.allocated_ports.write().await;

        for port in PORT_RANGE_START..=PORT_RANGE_END {
            if !ports.contains(&port) && Self::is_port_available(port).await {
                ports.push(port);
                return Ok(port);
            }
        }

        Err(ChromeError::General("No available ports".to_string()))
    }

    async fn release_port(&self, port: u16) {
        let mut ports = self.allocated_ports.write().await;
        ports.retain(|&p| p != port);
    }

    async fn is_port_available(port: u16) -> bool {
        tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_info() {
        let now = Instant::now();
        let info = SessionInfo {
            id: "test".to_string(),
            cdp_port: 9222,
            created_at: now,
            last_activity: now,
            page_count: 0,
            headless: true,
            uses_user_profile: false,
        };
        assert_eq!(info.id, "test");
        assert_eq!(info.cdp_port, 9222);
    }
}
