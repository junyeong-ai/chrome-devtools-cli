use crate::{ChromeError, Result, chrome::session_manager::BrowserSessionManager, output};
use chromiumoxide::Browser;
use futures::StreamExt;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub data_dir: String,
    pub pages_count: usize,
    pub network_requests: usize,
    pub console_messages: usize,
    pub status: String,
}

impl output::OutputFormatter for SessionInfo {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            text::key_value("Session ID", &self.session_id),
            text::key_value("Data Dir", &self.data_dir),
            text::key_value("Pages", &self.pages_count.to_string()),
            text::key_value("Network Requests", &self.network_requests.to_string()),
            text::key_value("Console Messages", &self.console_messages.to_string()),
            text::key_value("Status", &self.status)
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_session_info(manager: &BrowserSessionManager) -> Result<SessionInfo> {
    manager.get_or_create_browser().await?;
    let pages = manager.list_pages().await;

    Ok(SessionInfo {
        session_id: manager.session_id().to_string(),
        data_dir: manager.storage().session_dir().display().to_string(),
        pages_count: pages.len(),
        network_requests: manager.network_count(),
        console_messages: manager.console_count(),
        status: "active".to_string(),
    })
}

#[derive(Debug, Serialize)]
pub struct StopResult {
    pub status: String,
    pub browser_closed: bool,
    pub session_cleaned: bool,
}

impl output::OutputFormatter for StopResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        if self.browser_closed {
            text::success("Browser session stopped")
        } else {
            text::warning("Session files cleaned (browser was not running)")
        }
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_stop() -> Result<StopResult> {
    let session_path = crate::config::default_config_dir()?.join("session.toml");
    let mut browser_closed = false;

    if session_path.exists() {
        let content = std::fs::read_to_string(&session_path)?;
        if let Ok(session) = toml::from_str::<crate::chrome::models::BrowserSession>(&content)
            && let Ok(mut browser) = connect_and_close(session.debug_port).await
        {
            browser.close().await.ok();
            browser_closed = true;
        }
    }

    for session_id in crate::chrome::SessionStorage::list_sessions()? {
        if let Ok(storage) = crate::chrome::SessionStorage::from_session_id(&session_id) {
            storage.cleanup()?;
        }
    }

    std::fs::remove_file(&session_path).ok();

    Ok(StopResult {
        status: "stopped".to_string(),
        browser_closed,
        session_cleaned: true,
    })
}

async fn connect_and_close(port: u16) -> Result<Browser> {
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

    let (browser, mut handler) = Browser::connect(ws_url)
        .await
        .map_err(|_| ChromeError::ConnectionLost)?;

    tokio::spawn(async move { while handler.next().await.is_some() {} });

    Ok(browser)
}
