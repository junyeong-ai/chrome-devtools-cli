use crate::chrome::collectors::{ExtensionEvent, RecordingMarker, TraceStatus};
use crate::chrome::storage::SessionStorage;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use super::session_pool::SessionPool;

pub const DEFAULT_HTTP_PORT: u16 = 9223;

#[derive(Clone)]
struct AppState {
    session_pool: Arc<SessionPool>,
}

#[derive(Serialize)]
struct ApiResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recording_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_status: Option<TraceStatus>,
}

impl ApiResponse {
    fn success() -> Self {
        Self {
            ok: true,
            error: None,
            recording_id: None,
            trace_id: None,
            trace_status: None,
        }
    }

    fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
            recording_id: None,
            trace_id: None,
            trace_status: None,
        }
    }

    fn with_recording_id(recording_id: String) -> Self {
        Self {
            ok: true,
            error: None,
            recording_id: Some(recording_id),
            trace_id: None,
            trace_status: None,
        }
    }

    fn with_trace_id(trace_id: String) -> Self {
        Self {
            ok: true,
            error: None,
            recording_id: None,
            trace_id: Some(trace_id),
            trace_status: None,
        }
    }

    fn with_trace_status(status: TraceStatus) -> Self {
        Self {
            ok: true,
            error: None,
            recording_id: None,
            trace_id: None,
            trace_status: Some(status),
        }
    }
}

#[derive(Serialize)]
struct SessionResponse {
    ok: bool,
    session_id: Option<String>,
    sessions: Vec<SessionInfo>,
}

#[derive(Serialize)]
struct SessionInfo {
    id: String,
    cdp_port: u16,
}

#[derive(Deserialize)]
struct StartRecordingRequest {
    session_id: String,
    fps: u32,
    quality: u8,
}

#[derive(Deserialize)]
struct StopRecordingRequest {
    session_id: String,
    recording_id: String,
    frame_count: u32,
    duration_ms: u64,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct FrameRequest {
    session_id: String,
    recording_id: String,
    index: u32,
    #[allow(dead_code)]
    offset_ms: u64,
    data: String,
}

#[derive(Deserialize)]
struct ScreenshotRequest {
    session_id: String,
    filename: Option<String>,
    data: String,
}

#[derive(Deserialize)]
struct SessionEventRequest {
    session_id: String,
    event: ExtensionEvent,
}

#[derive(Deserialize)]
struct TraceStartRequest {
    session_id: String,
    categories: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct TraceStopRequest {
    session_id: String,
}

#[derive(Deserialize)]
struct TraceStatusRequest {
    session_id: String,
}

pub struct HttpServer {
    port: u16,
    session_pool: Arc<SessionPool>,
}

impl HttpServer {
    pub fn new(port: u16, session_pool: Arc<SessionPool>) -> Self {
        Self { port, session_pool }
    }

    pub async fn run(&self) -> crate::Result<()> {
        let state = AppState {
            session_pool: self.session_pool.clone(),
        };

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let app = Router::new()
            .route("/api/health", get(health))
            .route("/api/session", get(get_session))
            .route("/api/events", post(save_session_event))
            .route("/api/screenshots", post(save_screenshot))
            .route("/api/recording/start", post(start_recording))
            .route("/api/recording/stop", post(stop_recording))
            .route("/api/recording/frame", post(save_frame))
            .route("/api/trace/start", post(start_trace))
            .route("/api/trace/stop", post(stop_trace))
            .route("/api/trace/status", post(trace_status))
            .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024))
            .layer(cors)
            .with_state(state);

        let addr = format!("127.0.0.1:{}", self.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!("HTTP server listening on {}", addr);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn health() -> Json<ApiResponse> {
    Json(ApiResponse::success())
}

async fn get_session(State(state): State<AppState>) -> Json<SessionResponse> {
    let sessions: Vec<SessionInfo> = state
        .session_pool
        .list()
        .await
        .into_iter()
        .map(|s| SessionInfo {
            id: s.id.clone(),
            cdp_port: s.cdp_port,
        })
        .collect();

    let session_id = sessions.first().map(|s| s.id.clone());

    Json(SessionResponse {
        ok: true,
        session_id,
        sessions,
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn start_recording(
    State(state): State<AppState>,
    Json(req): Json<StartRecordingRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let session = match state.session_pool.get(&req.session_id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("session not found")),
            );
        }
    };

    let recording_id = Uuid::new_v4().to_string();

    if let Err(e) = session
        .storage()
        .create_recording(&recording_id, req.fps, req.quality)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        );
    }

    let marker = RecordingMarker {
        recording_id: recording_id.clone(),
        ts: now_ms(),
    };
    let _ = session
        .storage()
        .append("extension", &ExtensionEvent::RecordingStart(marker));

    tracing::info!(recording_id = %recording_id, fps = req.fps, "Recording started");

    (
        StatusCode::OK,
        Json(ApiResponse::with_recording_id(recording_id)),
    )
}

async fn stop_recording(
    State(state): State<AppState>,
    Json(req): Json<StopRecordingRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let session = match state.session_pool.get(&req.session_id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("session not found")),
            );
        }
    };

    let storage = match session.storage().get_recording(&req.recording_id) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    let mut recording = match storage.load_metadata() {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    recording.complete(req.frame_count, req.duration_ms, req.width, req.height);

    if let Err(e) = storage.save_metadata(&recording) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        );
    }

    let marker = RecordingMarker {
        recording_id: req.recording_id.clone(),
        ts: now_ms(),
    };
    let _ = session
        .storage()
        .append("extension", &ExtensionEvent::RecordingStop(marker));

    tracing::info!(
        recording_id = %req.recording_id,
        frames = req.frame_count,
        duration_ms = req.duration_ms,
        "Recording completed"
    );

    (StatusCode::OK, Json(ApiResponse::success()))
}

async fn save_frame(
    State(state): State<AppState>,
    Json(req): Json<FrameRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let session = match state.session_pool.get(&req.session_id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("session not found")),
            );
        }
    };

    let storage = match session.storage().get_recording(&req.recording_id) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    let data = req.data.trim_start_matches("data:image/jpeg;base64,");
    let bytes = match BASE64.decode(data) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    if let Err(e) = storage.save_frame(req.index, &bytes) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        );
    }

    (StatusCode::OK, Json(ApiResponse::success()))
}

async fn save_session_event(
    State(_state): State<AppState>,
    Json(req): Json<SessionEventRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let storage = match SessionStorage::from_session_id(&req.session_id) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    if let Err(e) = storage.append("extension", &req.event) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        );
    }

    (StatusCode::OK, Json(ApiResponse::success()))
}

async fn save_screenshot(
    State(_state): State<AppState>,
    Json(req): Json<ScreenshotRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let storage = match SessionStorage::from_session_id(&req.session_id) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    let data = req.data.trim_start_matches("data:image/png;base64,");
    let bytes = match BASE64.decode(data) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    let screenshots_dir = match storage.screenshots_dir() {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    let filename = req
        .filename
        .unwrap_or_else(|| format!("screenshot_{}.png", chrono::Utc::now().timestamp_millis()));

    if let Err(e) = std::fs::write(screenshots_dir.join(&filename), &bytes) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        );
    }

    (StatusCode::OK, Json(ApiResponse::success()))
}

async fn start_trace(
    State(state): State<AppState>,
    Json(req): Json<TraceStartRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let session = match state.session_pool.get(&req.session_id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("session not found")),
            );
        }
    };

    let page = match session.get_or_create_page().await {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    match session
        .collectors()
        .trace
        .start(&page, req.categories)
        .await
    {
        Ok(trace_id) => {
            tracing::info!(trace_id = %trace_id, "Trace started via HTTP");
            (StatusCode::OK, Json(ApiResponse::with_trace_id(trace_id)))
        }
        Err(e) => (
            StatusCode::CONFLICT,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

async fn stop_trace(
    State(state): State<AppState>,
    Json(req): Json<TraceStopRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let session = match state.session_pool.get(&req.session_id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("session not found")),
            );
        }
    };

    let page = match session.get_or_create_page().await {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    match session.collectors().trace.stop(&page).await {
        Ok(data) => {
            tracing::info!(
                trace_id = %data.trace_id,
                events = data.event_count,
                "Trace stopped via HTTP"
            );
            (
                StatusCode::OK,
                Json(ApiResponse::with_trace_id(data.trace_id)),
            )
        }
        Err(e) => (
            StatusCode::CONFLICT,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

async fn trace_status(
    State(state): State<AppState>,
    Json(req): Json<TraceStatusRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let session = match state.session_pool.get(&req.session_id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("session not found")),
            );
        }
    };

    let status = session.collectors().trace.status().await;
    (StatusCode::OK, Json(ApiResponse::with_trace_status(status)))
}
