use crate::{
    Result,
    chrome::{
        SessionStorage,
        collectors::{
            ConsoleLevel, ConsoleMessage, DevToolsIssue, ExtensionEvent, NetworkRequest, PageError,
        },
    },
    output::{self, OutputFormatter},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SessionList {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub network_count: usize,
    pub console_count: usize,
    pub pageerror_count: usize,
    pub issues_count: usize,
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub network_count: usize,
    pub console_count: usize,
    pub pageerror_count: usize,
    pub issues_count: usize,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct CleanResult {
    pub removed: usize,
}

impl OutputFormatter for SessionList {
    fn format_text(&self) -> String {
        use crate::output::text;
        if self.sessions.is_empty() {
            return text::info("No sessions found");
        }

        let mut out = text::section("Sessions");
        for s in &self.sessions {
            out.push_str(&format!(
                "\n  {} (net:{} con:{} err:{} iss:{})",
                s.session_id, s.network_count, s.console_count, s.pageerror_count, s.issues_count
            ));
        }
        out
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

impl OutputFormatter for SessionSummary {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            text::section(&format!("Session: {}", self.session_id)),
            text::key_value("Network Requests", &self.network_count.to_string()),
            text::key_value("Console Messages", &self.console_count.to_string()),
            text::key_value("Page Errors", &self.pageerror_count.to_string()),
            text::key_value("DevTools Issues", &self.issues_count.to_string()),
            text::key_value("Path", &self.path),
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

impl<T: Serialize> OutputFormatter for PaginatedResult<T> {
    fn format_text(&self) -> String {
        use crate::output::text;
        let showing = if let Some(limit) = self.limit {
            format!(
                "{}-{}",
                self.offset + 1,
                (self.offset + limit).min(self.total)
            )
        } else {
            format!("{}-{}", self.offset + 1, self.total)
        };
        text::info(&format!("Showing {} of {} items", showing, self.total))
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

impl OutputFormatter for CleanResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        text::success(&format!("Removed {} sessions", self.removed))
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub fn handle_list() -> Result<SessionList> {
    let sessions = SessionStorage::list_sessions()?;
    let mut infos = Vec::new();

    for session_id in sessions {
        if let Ok(storage) = SessionStorage::from_session_id(&session_id) {
            infos.push(SessionInfo {
                session_id,
                network_count: storage.count("network"),
                console_count: storage.count("console"),
                pageerror_count: storage.count("pageerror"),
                issues_count: storage.count("issues"),
            });
        }
    }

    Ok(SessionList { sessions: infos })
}

pub fn handle_show(session_id: &str) -> Result<SessionSummary> {
    let storage = SessionStorage::from_session_id(session_id)?;

    Ok(SessionSummary {
        session_id: session_id.to_string(),
        network_count: storage.count("network"),
        console_count: storage.count("console"),
        pageerror_count: storage.count("pageerror"),
        issues_count: storage.count("issues"),
        path: storage.session_dir().display().to_string(),
    })
}

pub fn handle_network(
    session_id: &str,
    domain: Option<&str>,
    status: Option<u16>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<PaginatedResult<NetworkRequest>> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let all: Vec<NetworkRequest> = storage.read_all("network")?;

    let filtered: Vec<_> = all
        .into_iter()
        .filter(|r| {
            let domain_match = domain.map(|d| r.url.contains(d)).unwrap_or(true);
            let status_match = status.map(|s| r.status == Some(s)).unwrap_or(true);
            domain_match && status_match
        })
        .collect();

    let total = filtered.len();
    let offset = offset.unwrap_or(0);
    let items: Vec<_> = filtered
        .into_iter()
        .skip(offset)
        .take(limit.unwrap_or(usize::MAX))
        .collect();

    Ok(PaginatedResult {
        items,
        total,
        offset,
        limit,
    })
}

pub fn handle_console(
    session_id: &str,
    level: Option<&str>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<PaginatedResult<ConsoleMessage>> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let all: Vec<ConsoleMessage> = storage.read_all("console")?;

    let level_filter: Option<ConsoleLevel> = level.and_then(|l| l.parse().ok());

    let filtered: Vec<_> = all
        .into_iter()
        .filter(|m| level_filter.map(|l| m.level == l).unwrap_or(true))
        .collect();

    let total = filtered.len();
    let offset = offset.unwrap_or(0);
    let items: Vec<_> = filtered
        .into_iter()
        .skip(offset)
        .take(limit.unwrap_or(usize::MAX))
        .collect();

    Ok(PaginatedResult {
        items,
        total,
        offset,
        limit,
    })
}

pub fn handle_errors(session_id: &str, limit: Option<usize>) -> Result<PaginatedResult<PageError>> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let all: Vec<PageError> = storage.read_all("pageerror")?;

    let total = all.len();
    let items: Vec<_> = all.into_iter().take(limit.unwrap_or(usize::MAX)).collect();

    Ok(PaginatedResult {
        items,
        total,
        offset: 0,
        limit,
    })
}

pub fn handle_issues(
    session_id: &str,
    limit: Option<usize>,
) -> Result<PaginatedResult<DevToolsIssue>> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let all: Vec<DevToolsIssue> = storage.read_all("issues")?;

    let total = all.len();
    let items: Vec<_> = all.into_iter().take(limit.unwrap_or(usize::MAX)).collect();

    Ok(PaginatedResult {
        items,
        total,
        offset: 0,
        limit,
    })
}

pub fn handle_delete(session_id: &str) -> Result<()> {
    let storage = SessionStorage::from_session_id(session_id)?;
    storage.cleanup()?;
    Ok(())
}

pub fn handle_clean(older_than: u64) -> Result<CleanResult> {
    let removed = SessionStorage::cleanup_stale(older_than)?;
    Ok(CleanResult { removed })
}

pub fn handle_extension(
    session_id: &str,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<PaginatedResult<ExtensionEvent>> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let all: Vec<ExtensionEvent> = storage.read_all("extension")?;

    let total = all.len();
    let offset = offset.unwrap_or(0);
    let items: Vec<_> = all
        .into_iter()
        .skip(offset)
        .take(limit.unwrap_or(usize::MAX))
        .collect();

    Ok(PaginatedResult {
        items,
        total,
        offset,
        limit,
    })
}
