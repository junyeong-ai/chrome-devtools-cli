use crate::{
    Result,
    chrome::{
        Recording, RecordingDetail, RecordingStatus, SessionStorage,
        collectors::{
            ConsoleLevel, ConsoleMessage, DevToolsIssue, ExtensionEvent, NetworkRequest, PageError,
        },
    },
    output::{self, OutputFormatter},
};
use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Default)]
pub struct TimeFilter {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

impl TimeFilter {
    pub fn new(from: Option<String>, to: Option<String>, last: Option<String>) -> Self {
        if let Some(last) = last
            && let Some(duration) = parse_duration(&last)
        {
            return Self {
                from: Some(Utc::now() - duration),
                to: None,
            };
        }

        Self {
            from: from.and_then(|s| parse_datetime(&s)),
            to: to.and_then(|s| parse_datetime(&s)),
        }
    }

    pub fn matches_utc(&self, timestamp: DateTime<Utc>) -> bool {
        if let Some(from) = self.from
            && timestamp < from
        {
            return false;
        }
        if let Some(to) = self.to
            && timestamp > to
        {
            return false;
        }
        true
    }

    pub fn matches_ms(&self, ts_ms: u64) -> bool {
        let timestamp = DateTime::from_timestamp_millis(ts_ms as i64).unwrap_or_else(Utc::now);
        self.matches_utc(timestamp)
    }

    pub fn is_empty(&self) -> bool {
        self.from.is_none() && self.to.is_none()
    }
}

pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim().to_lowercase();
    let (num_str, unit) = if s.ends_with("ms") {
        (&s[..s.len() - 2], "ms")
    } else if s.ends_with('s') {
        (&s[..s.len() - 1], "s")
    } else if s.ends_with('m') {
        (&s[..s.len() - 1], "m")
    } else if s.ends_with('h') {
        (&s[..s.len() - 1], "h")
    } else if s.ends_with('d') {
        (&s[..s.len() - 1], "d")
    } else {
        return None;
    };

    let num: i64 = num_str.parse().ok()?;
    match unit {
        "ms" => Duration::try_milliseconds(num),
        "s" => Duration::try_seconds(num),
        "m" => Duration::try_minutes(num),
        "h" => Duration::try_hours(num),
        "d" => Duration::try_days(num),
        _ => None,
    }
}

fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }

    if let Ok(time) = NaiveTime::parse_from_str(s, "%H:%M") {
        let today = Local::now().date_naive();
        let local_dt = today.and_time(time);
        return Local
            .from_local_datetime(&local_dt)
            .single()
            .map(|dt| dt.with_timezone(&Utc));
    }

    if let Ok(time) = NaiveTime::parse_from_str(s, "%H:%M:%S") {
        let today = Local::now().date_naive();
        let local_dt = today.and_time(time);
        return Local
            .from_local_datetime(&local_dt)
            .single()
            .map(|dt| dt.with_timezone(&Utc));
    }

    None
}

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
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
    pub limit: Option<usize>,
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

pub fn handle_events(
    session_id: &str,
    event_type: Option<&str>,
    time_filter: TimeFilter,
    recording_id: Option<&str>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<PaginatedResult<ExtensionEvent>> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let all: Vec<ExtensionEvent> = storage.read_all("extension")?;

    // Find recording time range if recording_id is specified
    let recording_range: Option<(u64, Option<u64>)> = recording_id.and_then(|rid| {
        let start_ts = all.iter().find_map(|e| {
            if let ExtensionEvent::RecordingStart(m) = e {
                if m.recording_id == rid {
                    return Some(m.ts);
                }
            }
            None
        })?;

        let end_ts = all.iter().find_map(|e| {
            if let ExtensionEvent::RecordingStop(m) = e {
                if m.recording_id == rid {
                    return Some(m.ts);
                }
            }
            None
        });

        Some((start_ts, end_ts))
    });

    let filtered: Vec<_> = all
        .into_iter()
        .filter(|e| {
            let type_match = event_type.is_none_or(|t| {
                let name = match e {
                    ExtensionEvent::Click(_) => "click",
                    ExtensionEvent::Input(_) => "input",
                    ExtensionEvent::Select(_) => "select",
                    ExtensionEvent::Hover(_) => "hover",
                    ExtensionEvent::Scroll(_) => "scroll",
                    ExtensionEvent::KeyPress(_) => "keypress",
                    ExtensionEvent::Screenshot(_) => "screenshot",
                    ExtensionEvent::Snapshot(_) => "snapshot",
                    ExtensionEvent::Dialog(_) => "dialog",
                    ExtensionEvent::Navigate(_) => "navigate",
                    ExtensionEvent::RecordingStart(_) => "recording_start",
                    ExtensionEvent::RecordingStop(_) => "recording_stop",
                };
                name == t
            });

            let time_match = if time_filter.is_empty() {
                true
            } else {
                e.timestamp_ms().is_none_or(|ts| time_filter.matches_ms(ts))
            };

            let recording_match = recording_range.is_none_or(|(start, end)| {
                e.timestamp_ms().is_some_and(|ts| {
                    ts >= start && end.is_none_or(|end_ts| ts <= end_ts)
                })
            });

            type_match && time_match && recording_match
        })
        .collect();

    paginate(filtered, limit, offset)
}

pub fn handle_network(
    session_id: &str,
    domain: Option<&str>,
    status: Option<u16>,
    time_filter: TimeFilter,
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
            let time_match = time_filter.matches_utc(r.timestamp);
            domain_match && status_match && time_match
        })
        .collect();

    paginate(filtered, limit, offset)
}

pub fn handle_console(
    session_id: &str,
    level: Option<&str>,
    time_filter: TimeFilter,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<PaginatedResult<ConsoleMessage>> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let all: Vec<ConsoleMessage> = storage.read_all("console")?;

    let level_filter: Option<ConsoleLevel> = level.and_then(|l| l.parse().ok());

    let filtered: Vec<_> = all
        .into_iter()
        .filter(|m| {
            let level_match = level_filter.map(|l| m.level == l).unwrap_or(true);
            let time_match = time_filter.matches_utc(m.timestamp);
            level_match && time_match
        })
        .collect();

    paginate(filtered, limit, offset)
}

pub fn handle_errors(
    session_id: &str,
    time_filter: TimeFilter,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<PaginatedResult<PageError>> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let all: Vec<PageError> = storage.read_all("pageerror")?;

    let filtered: Vec<_> = if time_filter.is_empty() {
        all
    } else {
        all.into_iter()
            .filter(|e| time_filter.matches_utc(e.timestamp))
            .collect()
    };

    paginate(filtered, limit, offset)
}

fn paginate<T>(
    items: Vec<T>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<PaginatedResult<T>> {
    let total = items.len();
    let offset = offset.unwrap_or(0);
    let items: Vec<_> = items
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

#[derive(Debug, Serialize)]
pub struct RecordingList {
    pub session_id: String,
    pub recordings: Vec<Recording>,
}

#[derive(Debug, Serialize)]
pub struct RecordingInfo {
    pub recording: Recording,
    pub frames_dir: String,
}

#[derive(Debug, Serialize)]
pub struct FrameList {
    pub recording_id: String,
    pub frames: Vec<FrameDetail>,
}

#[derive(Debug, Serialize)]
pub struct FrameDetail {
    pub index: u32,
    pub path: String,
    pub size_bytes: u64,
}

impl OutputFormatter for RecordingList {
    fn format_text(&self) -> String {
        use crate::output::text;
        if self.recordings.is_empty() {
            return text::info("No recordings found");
        }

        let mut out = text::section(&format!("Recordings ({})", self.session_id));
        for r in &self.recordings {
            let status = match r.status {
                RecordingStatus::Recording => "REC",
                RecordingStatus::Completed => "OK",
                RecordingStatus::Failed => "ERR",
            };
            out.push_str(&format!(
                "\n  [{}] {} ({}ms, {} frames, {}fps)",
                status, r.id, r.duration_ms, r.frame_count, r.fps
            ));
        }
        out
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

impl OutputFormatter for RecordingInfo {
    fn format_text(&self) -> String {
        use crate::output::text;
        let status = match self.recording.status {
            RecordingStatus::Recording => "Recording",
            RecordingStatus::Completed => "Completed",
            RecordingStatus::Failed => "Failed",
        };
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}",
            text::section(&format!("Recording: {}", self.recording.id)),
            text::key_value("Status", status),
            text::key_value("Duration", &format!("{}ms", self.recording.duration_ms)),
            text::key_value("Frames", &self.recording.frame_count.to_string()),
            text::key_value("FPS", &self.recording.fps.to_string()),
            text::key_value("Quality", &self.recording.quality.to_string()),
            text::key_value("Frames Dir", &self.frames_dir),
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

impl OutputFormatter for RecordingDetail {
    fn format_text(&self) -> String {
        use crate::output::text;
        let status = match self.recording.status {
            RecordingStatus::Recording => "Recording",
            RecordingStatus::Completed => "Completed",
            RecordingStatus::Failed => "Failed",
        };
        format!(
            "{}\n{}\n{}\n{}\n{}",
            text::section(&format!("Recording: {}", self.recording.id)),
            text::key_value("Status", status),
            text::key_value("Duration", &format!("{}ms", self.recording.duration_ms)),
            text::key_value("Frames", &self.recording.frame_count.to_string()),
            text::key_value("FPS", &self.recording.fps.to_string()),
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

impl OutputFormatter for FrameList {
    fn format_text(&self) -> String {
        use crate::output::text;
        if self.frames.is_empty() {
            return text::info("No frames found");
        }

        let total_size: u64 = self.frames.iter().map(|f| f.size_bytes).sum();
        let mut out = text::section(&format!(
            "Frames ({} files, {} total)",
            self.frames.len(),
            text::format_bytes(total_size)
        ));
        for f in &self.frames {
            out.push_str(&format!(
                "\n  {:06}.jpg ({})",
                f.index,
                text::format_bytes(f.size_bytes)
            ));
        }
        out
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub fn handle_recordings_list(session_id: &str) -> Result<RecordingList> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let recordings = storage.list_recordings()?;
    Ok(RecordingList {
        session_id: session_id.to_string(),
        recordings,
    })
}

pub fn handle_recording_show(session_id: &str, recording_id: &str) -> Result<RecordingInfo> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let rec_storage = storage.get_recording(recording_id)?;
    let recording = rec_storage.load_metadata()?;

    Ok(RecordingInfo {
        recording,
        frames_dir: rec_storage.frames_dir().display().to_string(),
    })
}

pub fn handle_recording_detail(session_id: &str, recording_id: &str) -> Result<RecordingDetail> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let rec_storage = storage.get_recording(recording_id)?;
    let recording = rec_storage.load_metadata()?;

    Ok(RecordingDetail {
        recording,
        frames_dir: rec_storage.frames_dir(),
    })
}

pub fn handle_recording_frames(session_id: &str, recording_id: &str) -> Result<FrameList> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let rec_storage = storage.get_recording(recording_id)?;
    let frame_infos = rec_storage.list_frames()?;

    let frames: Vec<FrameDetail> = frame_infos
        .into_iter()
        .map(|f| FrameDetail {
            index: f.index,
            path: rec_storage.frame_path(f.index).display().to_string(),
            size_bytes: f.size_bytes,
        })
        .collect();

    Ok(FrameList {
        recording_id: recording_id.to_string(),
        frames,
    })
}

pub fn handle_issues(
    session_id: &str,
    limit: Option<usize>,
) -> Result<PaginatedResult<DevToolsIssue>> {
    let storage = SessionStorage::from_session_id(session_id)?;
    let all: Vec<DevToolsIssue> = storage.read_all("issues")?;
    paginate(all, limit, None)
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

impl OutputFormatter for SessionSummary {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            text::section(&format!("Session: {}", self.session_id)),
            text::key_value("Network", &self.network_count.to_string()),
            text::key_value("Console", &self.console_count.to_string()),
            text::key_value("Errors", &self.pageerror_count.to_string()),
            text::key_value("Issues", &self.issues_count.to_string()),
            text::key_value("Path", &self.path),
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
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

#[derive(Debug, Serialize)]
pub struct CleanResult {
    pub removed: usize,
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

pub fn handle_clean(duration: Option<Duration>) -> Result<CleanResult> {
    let ttl_secs = duration
        .map(|d| d.num_seconds() as u64)
        .unwrap_or(24 * 3600);
    let removed = SessionStorage::cleanup_stale(ttl_secs)?;
    Ok(CleanResult { removed })
}

pub fn handle_delete(session_id: &str) -> Result<()> {
    let storage = SessionStorage::from_session_id(session_id)?;
    storage.cleanup()?;
    Ok(())
}
