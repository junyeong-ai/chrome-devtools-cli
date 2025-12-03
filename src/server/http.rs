use crate::chrome::collectors::extension::{ExtensionEvent, RecordingData, ScreenshotData};
use crate::chrome::storage::SessionStorage;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use super::session_pool::SessionPool;

pub const DEFAULT_HTTP_PORT: u16 = 9223;

#[derive(Clone)]
struct AppState {
    session_pool: Arc<SessionPool>,
}

#[derive(Deserialize)]
struct EventPayload {
    session_id: String,
    event: serde_json::Value,
}

#[derive(Deserialize)]
struct FramePayload {
    session_id: String,
    index: u32,
    data: String,
}

#[derive(Deserialize)]
struct ScreenshotPayload {
    session_id: String,
    filename: Option<String>,
    data: String,
}

#[derive(Serialize)]
struct ApiResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl ApiResponse {
    fn success() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
        }
    }
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
            .route("/api/events", post(handle_event))
            .route("/api/frames", post(handle_frame))
            .route("/api/screenshots", post(handle_screenshot))
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

async fn handle_event(
    State(state): State<AppState>,
    Json(payload): Json<EventPayload>,
) -> (StatusCode, Json<ApiResponse>) {
    let session = match state.session_pool.get(&payload.session_id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("session not found")),
            );
        }
    };

    let storage = session.storage();

    if let Some(screenshot) = payload.event.get("screenshot") {
        if let Some(data) = screenshot.get("data").and_then(|d| d.as_str()) {
            let data = data.trim_start_matches("data:image/png;base64,");
            if let Ok(bytes) = BASE64.decode(data) {
                let ts = Utc::now().timestamp_millis();
                let filename = format!("screenshot_{}.png", ts);
                if let Ok(dir) = storage.screenshots_dir()
                    && std::fs::write(dir.join(&filename), &bytes).is_ok()
                {
                    let target = screenshot.get("target").cloned();
                    let stored = ExtensionEvent::Screenshot(ScreenshotData {
                        target: target.and_then(|t| serde_json::from_value(t).ok()),
                        data: None,
                        file: Some(format!("screenshots/{}", filename)),
                        ts: Some(ts as u64),
                    });
                    let _ = storage.append("extension", &stored);
                }
            }
        }
        return (StatusCode::OK, Json(ApiResponse::success()));
    }

    if let Some(recording) = payload.event.get("recording") {
        if let Some("frame") = recording.get("type").and_then(|t| t.as_str()) {
            if let (Some(i), Some(data)) = (
                recording.get("i").and_then(|v| v.as_u64()),
                recording.get("data").and_then(|v| v.as_str()),
            ) {
                let data = data.trim_start_matches("data:image/jpeg;base64,");
                if let Ok(bytes) = BASE64.decode(data)
                    && let Ok(dir) = storage.frames_dir()
                {
                    let _ = std::fs::write(dir.join(format!("frame_{:04}.jpg", i)), &bytes);
                }
            }
            return (StatusCode::OK, Json(ApiResponse::success()));
        }

        if let Ok(rec_data) = serde_json::from_value::<RecordingData>(recording.clone()) {
            let event = ExtensionEvent::Recording(rec_data);
            let _ = storage.append("extension", &event);
        }
        return (StatusCode::OK, Json(ApiResponse::success()));
    }

    if let Ok(event) = serde_json::from_value::<ExtensionEvent>(payload.event)
        && let Err(e) = storage.append("extension", &event)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        );
    }

    (StatusCode::OK, Json(ApiResponse::success()))
}

async fn handle_frame(
    State(_state): State<AppState>,
    Json(payload): Json<FramePayload>,
) -> (StatusCode, Json<ApiResponse>) {
    let storage = match SessionStorage::from_session_id(&payload.session_id) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    let data = payload.data.trim_start_matches("data:image/jpeg;base64,");
    let bytes = match BASE64.decode(data) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    let frames_dir = match storage.frames_dir() {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    let filename = format!("frame_{:04}.jpg", payload.index);
    if let Err(e) = std::fs::write(frames_dir.join(&filename), &bytes) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        );
    }

    (StatusCode::OK, Json(ApiResponse::success()))
}

async fn handle_screenshot(
    State(_state): State<AppState>,
    Json(payload): Json<ScreenshotPayload>,
) -> (StatusCode, Json<ApiResponse>) {
    let storage = match SessionStorage::from_session_id(&payload.session_id) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(e.to_string())),
            );
        }
    };

    let data = payload.data.trim_start_matches("data:image/png;base64,");
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

    let filename = payload
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
