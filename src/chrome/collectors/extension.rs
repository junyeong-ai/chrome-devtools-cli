use crate::chrome::storage::SessionStorage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionEvent {
    Click(TargetInfo),
    Input(InputData),
    Select(TargetInfo),
    Hover(TargetInfo),
    Scroll(ScrollData),
    KeyPress(KeyPressData),
    Screenshot(ScreenshotData),
    Recording(RecordingData),
    Snapshot(SnapshotData),
    Dialog(DialogData),
    Navigate(NavigateData),
}

pub type AriaTarget = Vec<String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetInfo {
    pub aria: AriaTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub css: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xpath: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub testid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rect: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<u64>,
}

impl TargetInfo {
    pub fn from_aria(aria: AriaTarget) -> Self {
        Self {
            aria,
            css: None,
            xpath: None,
            testid: None,
            text: None,
            rect: None,
            url: None,
            ts: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputData {
    #[serde(flatten)]
    pub target: TargetInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollData {
    pub x: i32,
    pub y: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<AriaTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPressData {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigateData {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(rename = "type")]
    pub nav_type: String,
    pub ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<AriaTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecordingData {
    Start { fps: u32 },
    Stop { frames: u32, ms: u64 },
    Frame { i: u32, data: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotData {
    pub url: String,
    pub title: String,
    pub w: u32,
    pub h: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a11y: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogData {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
}

pub struct ExtensionCollector {
    storage: Arc<SessionStorage>,
    sender: broadcast::Sender<ExtensionEvent>,
    count: AtomicUsize,
}

impl ExtensionCollector {
    pub fn new(storage: Arc<SessionStorage>) -> Self {
        let (sender, _) = broadcast::channel(100);
        Self {
            storage,
            sender,
            count: AtomicUsize::new(0),
        }
    }

    pub fn handle_event(&self, event: &ExtensionEvent) {
        let _ = self.storage.append("extension", event);
        self.count.fetch_add(1, Ordering::Relaxed);
        let _ = self.sender.send(event.clone());
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ExtensionEvent> {
        self.sender.subscribe()
    }

    pub fn count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
}
