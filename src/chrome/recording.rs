use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordingStatus {
    Recording,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: u64,
    pub fps: u32,
    pub quality: u8,
    pub width: u32,
    pub height: u32,
    pub frame_count: u32,
    pub status: RecordingStatus,
}

impl Recording {
    pub fn new(id: String, session_id: String, fps: u32, quality: u8) -> Self {
        Self {
            id,
            session_id,
            started_at: Utc::now(),
            ended_at: None,
            duration_ms: 0,
            fps,
            quality,
            width: 0,
            height: 0,
            frame_count: 0,
            status: RecordingStatus::Recording,
        }
    }

    pub fn complete(&mut self, frame_count: u32, duration_ms: u64, width: u32, height: u32) {
        self.ended_at = Some(Utc::now());
        self.frame_count = frame_count;
        self.duration_ms = duration_ms;
        self.width = width;
        self.height = height;
        self.status = RecordingStatus::Completed;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameInfo {
    pub index: u32,
    pub offset_ms: u64,
    pub size_bytes: u64,
}

pub struct RecordingStorage {
    base_dir: PathBuf,
    recording_id: String,
}

impl RecordingStorage {
    pub fn new(session_dir: &Path, recording_id: &str) -> crate::Result<Self> {
        let base_dir = session_dir.join("recordings").join(recording_id);
        fs::create_dir_all(&base_dir)?;
        fs::create_dir_all(base_dir.join("frames"))?;
        Ok(Self {
            base_dir,
            recording_id: recording_id.to_string(),
        })
    }

    pub fn from_existing(session_dir: &Path, recording_id: &str) -> crate::Result<Self> {
        let base_dir = session_dir.join("recordings").join(recording_id);
        if !base_dir.exists() {
            return Err(crate::ChromeError::General(format!(
                "Recording not found: {}",
                recording_id
            )));
        }
        Ok(Self {
            base_dir,
            recording_id: recording_id.to_string(),
        })
    }

    pub fn recording_id(&self) -> &str {
        &self.recording_id
    }

    pub fn frames_dir(&self) -> PathBuf {
        self.base_dir.join("frames")
    }

    pub fn save_metadata(&self, recording: &Recording) -> crate::Result<()> {
        let path = self.base_dir.join("metadata.json");
        let file = File::create(&path)?;
        serde_json::to_writer_pretty(file, recording)?;
        Ok(())
    }

    pub fn load_metadata(&self) -> crate::Result<Recording> {
        let path = self.base_dir.join("metadata.json");
        let file = File::open(&path)?;
        let recording = serde_json::from_reader(file)?;
        Ok(recording)
    }

    pub fn save_frame(&self, index: u32, data: &[u8]) -> crate::Result<PathBuf> {
        let filename = format!("{:06}.jpg", index);
        let path = self.frames_dir().join(&filename);
        fs::write(&path, data)?;
        Ok(path)
    }

    pub fn list_frames(&self) -> crate::Result<Vec<FrameInfo>> {
        let frames_dir = self.frames_dir();
        let mut frames = Vec::new();

        if let Ok(entries) = fs::read_dir(&frames_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "jpg")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                    && let Ok(index) = stem.parse::<u32>()
                {
                    let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    frames.push(FrameInfo {
                        index,
                        offset_ms: 0,
                        size_bytes,
                    });
                }
            }
        }

        frames.sort_by_key(|f| f.index);
        Ok(frames)
    }

    pub fn frame_path(&self, index: u32) -> PathBuf {
        self.frames_dir().join(format!("{:06}.jpg", index))
    }
}

pub fn list_recordings(session_dir: &Path) -> crate::Result<Vec<Recording>> {
    let recordings_dir = session_dir.join("recordings");
    let mut recordings = Vec::new();

    if let Ok(entries) = fs::read_dir(&recordings_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let metadata_path = entry.path().join("metadata.json");
                if metadata_path.exists()
                    && let Ok(file) = File::open(&metadata_path)
                    && let Ok(recording) = serde_json::from_reader::<_, Recording>(file)
                {
                    recordings.push(recording);
                }
            }
        }
    }

    recordings.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(recordings)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingDetail {
    pub recording: Recording,
    pub frames_dir: PathBuf,
}
