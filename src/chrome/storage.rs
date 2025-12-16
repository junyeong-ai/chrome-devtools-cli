use crate::Result;
use crate::chrome::recording::{Recording, RecordingStorage, list_recordings};
use serde::{Serialize, de::DeserializeOwned};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

pub struct SessionStorage {
    session_id: String,
    base_dir: PathBuf,
}

impl SessionStorage {
    pub fn new(session_id: &str) -> Result<Self> {
        let base_dir = Self::sessions_dir()?.join(session_id);
        fs::create_dir_all(&base_dir)?;

        Ok(Self {
            session_id: session_id.to_string(),
            base_dir,
        })
    }

    pub fn from_session_id(session_id: &str) -> Result<Self> {
        let base_dir = Self::sessions_dir()?.join(session_id);
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir)?;
        }

        Ok(Self {
            session_id: session_id.to_string(),
            base_dir,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn session_dir(&self) -> &PathBuf {
        &self.base_dir
    }

    fn sessions_dir() -> Result<PathBuf> {
        let dir = crate::config::default_config_dir()?.join("sessions");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn append<T: Serialize>(&self, collection: &str, item: &T) -> Result<()> {
        let path = self.base_dir.join(format!("{}.ndjson", collection));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;

        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, item)?;
        writeln!(writer)?;
        Ok(())
    }

    pub fn read_all<T: DeserializeOwned>(&self, collection: &str) -> Result<Vec<T>> {
        let path = self.base_dir.join(format!("{}.ndjson", collection));
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut items = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if !line.is_empty()
                && let Ok(item) = serde_json::from_str(&line)
            {
                items.push(item);
            }
        }

        Ok(items)
    }

    pub fn count(&self, collection: &str) -> usize {
        let path = self.base_dir.join(format!("{}.ndjson", collection));
        if !path.exists() {
            return 0;
        }

        File::open(&path)
            .map(|f| BufReader::new(f).lines().count())
            .unwrap_or(0)
    }

    pub fn screenshots_dir(&self) -> Result<PathBuf> {
        let dir = self.base_dir.join("screenshots");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn extension_dir(&self) -> Result<PathBuf> {
        let dir = self.base_dir.join("extension");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn setup_extension(&self, source_dir: &PathBuf) -> Result<PathBuf> {
        let ext_dir = self.extension_dir()?;
        copy_dir_recursive(source_dir, &ext_dir)?;
        Ok(ext_dir)
    }

    pub fn create_recording(
        &self,
        recording_id: &str,
        fps: u32,
        quality: u8,
    ) -> Result<RecordingStorage> {
        let storage = RecordingStorage::new(&self.base_dir, recording_id)?;
        let recording = Recording::new(
            recording_id.to_string(),
            self.session_id.clone(),
            fps,
            quality,
        );
        storage.save_metadata(&recording)?;
        Ok(storage)
    }

    pub fn get_recording(&self, recording_id: &str) -> Result<RecordingStorage> {
        RecordingStorage::from_existing(&self.base_dir, recording_id)
    }

    pub fn list_recordings(&self) -> Result<Vec<Recording>> {
        list_recordings(&self.base_dir)
    }

    pub fn cleanup(&self) -> Result<()> {
        if self.base_dir.exists() {
            fs::remove_dir_all(&self.base_dir)?;
        }
        Ok(())
    }

    pub fn list_sessions() -> Result<Vec<String>> {
        let dir = Self::sessions_dir()?;
        let mut sessions = Vec::new();

        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir()
                    && let Some(name) = entry.file_name().to_str()
                {
                    sessions.push(name.to_string());
                }
            }
        }

        Ok(sessions)
    }

    pub fn cleanup_stale(max_age_secs: u64) -> Result<usize> {
        let dir = Self::sessions_dir()?;
        let mut removed = 0;

        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir()
                    && let Ok(metadata) = fs::metadata(&path)
                    && let Ok(modified) = metadata.modified()
                    && let Ok(age) = modified.elapsed()
                    && age.as_secs() > max_age_secs
                {
                    fs::remove_dir_all(&path).ok();
                    removed += 1;
                }
            }
        }

        Ok(removed)
    }
}

fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestItem {
        id: u32,
        name: String,
    }

    fn create_test_storage() -> (SessionStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let session_id = "test-session";
        let base_dir = temp_dir.path().join(session_id);
        fs::create_dir_all(&base_dir).unwrap();

        let storage = SessionStorage {
            session_id: session_id.to_string(),
            base_dir,
        };
        (storage, temp_dir)
    }

    #[test]
    fn test_append_and_read_all() {
        let (storage, _temp) = create_test_storage();

        let item1 = TestItem {
            id: 1,
            name: "First".to_string(),
        };
        let item2 = TestItem {
            id: 2,
            name: "Second".to_string(),
        };

        storage.append("test_collection", &item1).unwrap();
        storage.append("test_collection", &item2).unwrap();

        let items: Vec<TestItem> = storage.read_all("test_collection").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], item1);
        assert_eq!(items[1], item2);
    }

    #[test]
    fn test_read_all_empty_collection() {
        let (storage, _temp) = create_test_storage();
        let items: Vec<TestItem> = storage.read_all("nonexistent").unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn test_count() {
        let (storage, _temp) = create_test_storage();

        assert_eq!(storage.count("test_collection"), 0);

        for i in 0..5 {
            storage
                .append(
                    "test_collection",
                    &TestItem {
                        id: i,
                        name: format!("Item {}", i),
                    },
                )
                .unwrap();
        }

        assert_eq!(storage.count("test_collection"), 5);
    }

    #[test]
    fn test_cleanup() {
        let (storage, _temp) = create_test_storage();

        storage
            .append(
                "test",
                &TestItem {
                    id: 1,
                    name: "Test".to_string(),
                },
            )
            .unwrap();

        assert!(storage.session_dir().exists());
        storage.cleanup().unwrap();
        assert!(!storage.session_dir().exists());
    }
}
