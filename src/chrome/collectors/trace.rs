use crate::chrome::event_store::EventMetadata;
use crate::chrome::storage::SessionStorage;
use crate::{ChromeError, Result};
use chromiumoxide::Page;
use chromiumoxide::cdp::browser_protocol::tracing::{
    EndParams, EventDataCollected, EventTracingComplete, StartParams, TraceConfig,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::Mutex;

const DEFAULT_CATEGORIES: &[&str] = &[
    "-*",
    "devtools.timeline",
    "v8.execute",
    "v8",
    "blink.console",
    "blink.user_timing",
    "loading",
    "latencyInfo",
    "disabled-by-default-devtools.timeline",
    "disabled-by-default-devtools.timeline.frame",
    "disabled-by-default-devtools.timeline.stack",
    "disabled-by-default-v8.cpu_profiler",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub name: String,
    pub cat: Option<String>,
    pub ph: Option<String>,
    pub ts: Option<f64>,
    pub dur: Option<f64>,
    pub pid: Option<i64>,
    pub tid: Option<i64>,
    #[serde(default)]
    pub args: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceData {
    pub trace_id: String,
    pub url: Option<String>,
    pub start_time: u64,
    pub end_time: u64,
    pub duration_ms: u64,
    pub event_count: usize,
    pub events: Vec<serde_json::Value>,
}

impl EventMetadata for TraceData {
    fn event_type(&self) -> &'static str {
        "trace"
    }
    fn timestamp_ms(&self) -> Option<u64> {
        Some(self.start_time)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceStatus {
    pub is_active: bool,
    pub trace_id: Option<String>,
    pub start_time: Option<u64>,
    pub elapsed_ms: Option<u64>,
}

pub struct TraceCollector {
    storage: Arc<SessionStorage>,
    is_active: AtomicBool,
    trace_id: Mutex<Option<String>>,
    start_instant: Mutex<Option<Instant>>,
    start_time: AtomicU64,
    events: Arc<Mutex<Vec<serde_json::Value>>>,
    current_url: Mutex<Option<String>>,
}

impl TraceCollector {
    pub fn new(storage: Arc<SessionStorage>) -> Self {
        Self {
            storage,
            is_active: AtomicBool::new(false),
            trace_id: Mutex::new(None),
            start_instant: Mutex::new(None),
            start_time: AtomicU64::new(0),
            events: Arc::new(Mutex::new(Vec::new())),
            current_url: Mutex::new(None),
        }
    }

    pub async fn start(&self, page: &Arc<Page>, categories: Option<Vec<String>>) -> Result<String> {
        if self.is_active.swap(true, Ordering::SeqCst) {
            return Err(ChromeError::General("Trace already active".into()));
        }

        let trace_id = uuid::Uuid::new_v4().to_string();
        let categories = categories
            .unwrap_or_else(|| DEFAULT_CATEGORIES.iter().map(|s| s.to_string()).collect());

        self.events.lock().await.clear();

        let events_clone = self.events.clone();
        let mut data_stream = page
            .event_listener::<EventDataCollected>()
            .await
            .map_err(|e| {
                ChromeError::General(format!("Failed to subscribe to trace events: {e}"))
            })?;

        tokio::spawn(async move {
            while let Some(event) = data_stream.next().await {
                let event = Arc::try_unwrap(event).unwrap_or_else(|arc| (*arc).clone());
                if let Ok(mut events) = events_clone.try_lock() {
                    events.extend(event.value);
                }
            }
        });

        let trace_config = TraceConfig::builder()
            .included_categories(categories)
            .build();

        page.execute(StartParams::builder().trace_config(trace_config).build())
            .await
            .map_err(|e| ChromeError::General(format!("Failed to start trace: {e}")))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        *self.trace_id.lock().await = Some(trace_id.clone());
        *self.start_instant.lock().await = Some(Instant::now());
        self.start_time.store(now, Ordering::SeqCst);

        if let Ok(url) = page.url().await {
            *self.current_url.lock().await = url.map(|u| u.to_string());
        }

        tracing::info!(trace_id = %trace_id, "Trace started");
        Ok(trace_id)
    }

    pub async fn stop(&self, page: &Arc<Page>) -> Result<TraceData> {
        if !self.is_active.swap(false, Ordering::SeqCst) {
            return Err(ChromeError::General("No active trace".into()));
        }

        let mut complete_stream = page
            .event_listener::<EventTracingComplete>()
            .await
            .map_err(|e| {
                ChromeError::General(format!("Failed to subscribe to trace complete: {e}"))
            })?;

        page.execute(EndParams::default())
            .await
            .map_err(|e| ChromeError::General(format!("Failed to stop trace: {e}")))?;

        let _ =
            tokio::time::timeout(std::time::Duration::from_secs(10), complete_stream.next()).await;

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let trace_id = self
            .trace_id
            .lock()
            .await
            .take()
            .unwrap_or_else(|| "unknown".to_string());
        let start_time = self.start_time.load(Ordering::SeqCst);
        let duration_ms = self
            .start_instant
            .lock()
            .await
            .take()
            .map(|i| i.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let events = std::mem::take(&mut *self.events.lock().await);
        let url = self.current_url.lock().await.take();

        let trace_data = TraceData {
            trace_id: trace_id.clone(),
            url,
            start_time,
            end_time: now,
            duration_ms,
            event_count: events.len(),
            events,
        };

        if let Err(e) = self.storage.append("trace", &trace_data) {
            tracing::warn!("Failed to save trace data: {}", e);
        }

        tracing::info!(
            trace_id = %trace_id,
            events = trace_data.event_count,
            duration_ms = duration_ms,
            "Trace completed"
        );

        Ok(trace_data)
    }

    pub async fn status(&self) -> TraceStatus {
        let is_active = self.is_active.load(Ordering::SeqCst);
        let trace_id = self.trace_id.lock().await.clone();
        let start_time = self.start_time.load(Ordering::SeqCst);

        let elapsed_ms = if is_active {
            self.start_instant
                .lock()
                .await
                .as_ref()
                .map(|i| i.elapsed().as_millis() as u64)
        } else {
            None
        };

        TraceStatus {
            is_active,
            trace_id,
            start_time: if start_time > 0 {
                Some(start_time)
            } else {
                None
            },
            elapsed_ms,
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }
}
