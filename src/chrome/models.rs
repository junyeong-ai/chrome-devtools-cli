use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Represents a saved page entry for session restoration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPageEntry {
    pub target_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSession {
    pub session_id: String,
    pub debug_port: u16,
    pub created_at: DateTime<Utc>,
    /// TargetId of the active page for reliable restoration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_page_target_id: Option<String>,
    /// URL of the active page (fallback for restoration)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_page_url: Option<String>,
    /// Selected page index for multi-tab persistence
    #[serde(default)]
    pub selected_page_index: usize,
    /// Full list of pages with their target_ids for order-preserving restoration
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<SavedPageEntry>,
}

impl BrowserSession {
    pub fn new(session_id: String, debug_port: u16) -> Self {
        Self {
            session_id,
            debug_port,
            created_at: Utc::now(),
            active_page_target_id: None,
            active_page_url: None,
            selected_page_index: 0,
            pages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rating {
    Good,
    NeedsImprovement,
    Poor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreWebVitals {
    pub lcp_ms: Option<f64>,
    pub fid_ms: Option<f64>,
    pub cls: Option<f64>,
    pub ttfb_ms: Option<f64>,
    pub lcp_rating: Rating,
    pub fid_rating: Rating,
    pub cls_rating: Rating,
    pub ttfb_rating: Rating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserActivity {
    pub timestamp: DateTime<Utc>,
    pub activity_type: ActivityType,
    pub description: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityType {
    Navigation,
    PageLoad,
    Click,
    Input,
    NetworkRequest,
    ConsoleLog,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub pixel_ratio: f64,
    pub is_mobile: bool,
    pub has_touch: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            pixel_ratio: 1.0,
            is_mobile: false,
            has_touch: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotCapture {
    pub file_path: PathBuf,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub full_page: bool,
    pub url: String,
    pub captured_at: DateTime<Utc>,
    pub file_size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTrace {
    pub events: Vec<TraceEvent>,
    pub metadata: TraceMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceMetadata {
    pub url: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub name: String,
    #[serde(rename = "cat")]
    pub category: String,
    #[serde(rename = "ph")]
    pub phase: String,
    #[serde(rename = "ts")]
    pub timestamp: f64,
    pub pid: u32,
    pub tid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dur: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAnalysis {
    pub url: String,
    pub core_web_vitals: CoreWebVitals,
    pub page_load_metrics: PageLoadMetrics,
    pub main_thread_metrics: MainThreadMetrics,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLoadMetrics {
    pub dom_content_loaded_ms: f64,
    pub load_complete_ms: f64,
    pub first_paint_ms: Option<f64>,
    pub first_contentful_paint_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainThreadMetrics {
    pub total_blocking_time_ms: f64,
    pub long_tasks_count: usize,
    pub script_duration_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub category: String,
    pub severity: Severity,
    pub message: String,
    pub metric_value: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoRecording {
    pub file_path: PathBuf,
    pub format: VideoFormat,
    pub duration_ms: u64,
    pub frame_count: usize,
    pub fps: u32,
    pub resolution: (u32, u32),
    pub file_size_bytes: u64,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoFormat {
    Mp4,
    Frames,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSession {
    pub recording: VideoRecording,
    pub activities: Vec<BrowserActivity>,
    pub summary: ActivitySummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySummary {
    pub total_activities: usize,
    pub pages_visited: Vec<String>,
    pub navigation_count: usize,
    pub interaction_count: usize,
    pub error_count: usize,
    pub duration_seconds: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_session_new() {
        let session = BrowserSession::new("test-id".to_string(), 9222);
        assert_eq!(session.session_id, "test-id");
        assert_eq!(session.debug_port, 9222);
        assert!(session.active_page_target_id.is_none());
        assert!(session.active_page_url.is_none());
    }

    #[test]
    fn test_browser_session_active_page_fields() {
        let mut session = BrowserSession::new("test-id".to_string(), 9222);
        session.active_page_target_id = Some("ABC123".to_string());
        session.active_page_url = Some("https://example.com".to_string());

        assert_eq!(session.active_page_target_id, Some("ABC123".to_string()));
        assert_eq!(
            session.active_page_url,
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn test_browser_session_serialization() {
        let session = BrowserSession::new("test-id".to_string(), 9222);
        let toml_str = toml::to_string(&session).unwrap();
        assert!(toml_str.contains("session_id"));
        assert!(toml_str.contains("debug_port"));

        let parsed: BrowserSession = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.session_id, session.session_id);
        assert_eq!(parsed.debug_port, session.debug_port);
    }

    #[test]
    fn test_viewport_default() {
        let viewport = Viewport::default();
        assert_eq!(viewport.width, 1920);
        assert_eq!(viewport.height, 1080);
        assert_eq!(viewport.pixel_ratio, 1.0);
        assert!(!viewport.is_mobile);
        assert!(!viewport.has_touch);
    }

    #[test]
    fn test_image_format_serialization() {
        let format = ImageFormat::Png;
        let json = serde_json::to_string(&format).unwrap();
        assert_eq!(json, "\"png\"");

        let parsed: ImageFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ImageFormat::Png);
    }

    #[test]
    fn test_rating_serialization() {
        let rating = Rating::Good;
        let json = serde_json::to_string(&rating).unwrap();
        assert_eq!(json, "\"good\"");

        let parsed: Rating = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Rating::Good);
    }

    #[test]
    fn test_activity_type_serialization() {
        let activity = ActivityType::Navigation;
        let json = serde_json::to_string(&activity).unwrap();
        assert_eq!(json, "\"navigation\"");
    }

    #[test]
    fn test_severity_serialization() {
        let severity = Severity::High;
        let json = serde_json::to_string(&severity).unwrap();
        assert_eq!(json, "\"high\"");
    }

    #[test]
    fn test_video_format_serialization() {
        let format = VideoFormat::Mp4;
        let json = serde_json::to_string(&format).unwrap();
        assert_eq!(json, "\"mp4\"");
    }
}
