use crate::{ChromeError, Result};
use r2d2::{HandleError, Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;

#[derive(Debug)]
struct DebugErrorHandler;

impl<E: std::fmt::Debug> HandleError<E> for DebugErrorHandler {
    fn handle_error(&self, error: E) {
        tracing::debug!(
            "Pool error (expected during concurrent access): {:?}",
            error
        );
    }
}

pub trait EventMetadata {
    fn event_type(&self) -> &'static str;
    fn timestamp_ms(&self) -> Option<u64>;
}

type SqlitePool = Pool<SqliteConnectionManager>;

pub struct EventStore {
    pool: SqlitePool,
}

impl EventStore {
    pub fn open(path: &Path) -> Result<Self> {
        let manager = SqliteConnectionManager::file(path).with_init(|conn| {
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                     PRAGMA synchronous = NORMAL;
                     PRAGMA foreign_keys = ON;
                     PRAGMA cache_size = -2000;
                     PRAGMA busy_timeout = 5000;",
            )?;
            Ok(())
        });

        let pool = Pool::builder()
            .max_size(8)
            .error_handler(Box::new(DebugErrorHandler))
            .build(manager)
            .map_err(|e| ChromeError::General(format!("Failed to create pool: {}", e)))?;

        let conn = pool
            .get()
            .map_err(|e| ChromeError::General(format!("Failed to get connection: {}", e)))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                collection TEXT NOT NULL,
                event_type TEXT,
                timestamp_ms INTEGER,
                data TEXT NOT NULL,
                created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
            )",
            [],
        )
        .map_err(|e| ChromeError::General(format!("Failed to create table: {}", e)))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_collection ON events(collection)",
            [],
        )
        .ok();

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(collection, timestamp_ms)",
            [],
        )
        .ok();

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_type ON events(collection, event_type)",
            [],
        )
        .ok();

        Ok(Self { pool })
    }

    fn conn(&self) -> Result<PooledConnection<SqliteConnectionManager>> {
        self.pool
            .get()
            .map_err(|e| ChromeError::General(format!("Pool error: {}", e)))
    }

    pub fn append<T: Serialize + EventMetadata>(&self, collection: &str, item: &T) -> Result<i64> {
        let data = serde_json::to_string(item)
            .map_err(|e| ChromeError::General(format!("Serialization error: {}", e)))?;

        let event_type = item.event_type();
        let timestamp_ms = item.timestamp_ms().map(|t| t as i64);

        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO events (collection, event_type, timestamp_ms, data) VALUES (?1, ?2, ?3, ?4)",
            params![collection, event_type, timestamp_ms, data],
        )
        .map_err(|e| ChromeError::General(format!("Insert error: {}", e)))?;

        Ok(conn.last_insert_rowid())
    }

    pub fn append_raw<T: Serialize>(&self, collection: &str, item: &T) -> Result<i64> {
        let data = serde_json::to_string(item)
            .map_err(|e| ChromeError::General(format!("Serialization error: {}", e)))?;

        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO events (collection, data) VALUES (?1, ?2)",
            params![collection, data],
        )
        .map_err(|e| ChromeError::General(format!("Insert error: {}", e)))?;

        Ok(conn.last_insert_rowid())
    }

    pub fn read_all<T: DeserializeOwned>(&self, collection: &str) -> Result<Vec<T>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT data FROM events WHERE collection = ?1 ORDER BY id ASC")
            .map_err(|e| ChromeError::General(format!("Prepare error: {}", e)))?;

        let rows = stmt
            .query_map([collection], |row| row.get::<_, String>(0))
            .map_err(|e| ChromeError::General(format!("Query error: {}", e)))?;

        let mut items = Vec::new();
        for row in rows {
            match row {
                Ok(data) => match serde_json::from_str(&data) {
                    Ok(item) => items.push(item),
                    Err(e) => tracing::warn!("Failed to deserialize event: {}", e),
                },
                Err(e) => tracing::warn!("Failed to read row: {}", e),
            }
        }

        Ok(items)
    }

    pub fn query_range<T: DeserializeOwned>(
        &self,
        collection: &str,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
        event_type: Option<&str>,
    ) -> Result<Vec<T>> {
        let conn = self.conn()?;

        let (sql, params) = build_range_query(collection, start_ms, end_ms, event_type);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| ChromeError::General(format!("Prepare error: {}", e)))?;

        let rows = stmt
            .query_map(rusqlite::params_from_iter(params), |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| ChromeError::General(format!("Query error: {}", e)))?;

        let mut items = Vec::new();
        for row in rows {
            match row {
                Ok(data) => match serde_json::from_str(&data) {
                    Ok(item) => items.push(item),
                    Err(e) => tracing::warn!("Failed to deserialize event: {}", e),
                },
                Err(e) => tracing::warn!("Failed to read row: {}", e),
            }
        }

        Ok(items)
    }

    pub fn count(&self, collection: &str) -> usize {
        match self.conn() {
            Ok(conn) => conn
                .query_row(
                    "SELECT COUNT(*) FROM events WHERE collection = ?1",
                    [collection],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as usize,
            Err(e) => {
                tracing::warn!("Failed to count events: {}", e);
                0
            }
        }
    }

    pub fn count_by_type(&self, collection: &str, event_type: &str) -> usize {
        match self.conn() {
            Ok(conn) => conn
                .query_row(
                    "SELECT COUNT(*) FROM events WHERE collection = ?1 AND event_type = ?2",
                    params![collection, event_type],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as usize,
            Err(e) => {
                tracing::warn!("Failed to count events by type: {}", e);
                0
            }
        }
    }

    pub fn count_collections(
        &self,
        collections: &[&str],
    ) -> std::collections::HashMap<String, usize> {
        let mut result = std::collections::HashMap::new();
        for &c in collections {
            result.insert(c.to_string(), 0);
        }

        let Ok(conn) = self.conn() else {
            return result;
        };

        let Ok(mut stmt) = conn.prepare(
            "SELECT collection, COUNT(*) FROM events WHERE collection IN (SELECT value FROM json_each(?1)) GROUP BY collection"
        ) else {
            return result;
        };

        let json_array = serde_json::to_string(collections).unwrap_or_default();
        if let Ok(rows) = stmt.query_map([json_array], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }) {
            for row in rows.flatten() {
                result.insert(row.0, row.1 as usize);
            }
        }

        result
    }

    pub fn clear(&self, collection: &str) -> Result<usize> {
        let conn = self.conn()?;
        let deleted = conn
            .execute("DELETE FROM events WHERE collection = ?1", [collection])
            .map_err(|e| ChromeError::General(format!("Delete error: {}", e)))?;

        Ok(deleted)
    }

    pub fn vacuum(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("VACUUM", [])
            .map_err(|e| ChromeError::General(format!("Vacuum error: {}", e)))?;

        Ok(())
    }
}

fn build_range_query(
    collection: &str,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    event_type: Option<&str>,
) -> (String, Vec<String>) {
    let mut sql = String::from("SELECT data FROM events WHERE collection = ?");
    let mut params: Vec<String> = vec![collection.to_string()];

    if let Some(start) = start_ms {
        sql.push_str(" AND timestamp_ms >= ?");
        params.push(start.to_string());
    }

    if let Some(end) = end_ms {
        sql.push_str(" AND timestamp_ms <= ?");
        params.push(end.to_string());
    }

    if let Some(etype) = event_type {
        sql.push_str(" AND event_type = ?");
        params.push(etype.to_string());
    }

    sql.push_str(" ORDER BY timestamp_ms ASC, id ASC");

    (sql, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestEvent {
        event_type: String,
        ts: u64,
        data: String,
    }

    impl EventMetadata for TestEvent {
        fn event_type(&self) -> &'static str {
            "test"
        }
        fn timestamp_ms(&self) -> Option<u64> {
            Some(self.ts)
        }
    }

    fn create_test_store() -> (EventStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("events.db");
        let store = EventStore::open(&db_path).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_append_and_read() {
        let (store, _temp) = create_test_store();

        let event = TestEvent {
            event_type: "click".to_string(),
            ts: 1000,
            data: "test".to_string(),
        };

        store.append("test", &event).unwrap();

        let events: Vec<TestEvent> = store.read_all("test").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], event);
    }

    #[test]
    fn test_count() {
        let (store, _temp) = create_test_store();

        for i in 0..10 {
            store
                .append(
                    "test",
                    &TestEvent {
                        event_type: "click".to_string(),
                        ts: i * 100,
                        data: format!("event {}", i),
                    },
                )
                .unwrap();
        }

        assert_eq!(store.count("test"), 10);
        assert_eq!(store.count("nonexistent"), 0);
    }

    #[test]
    fn test_clear() {
        let (store, _temp) = create_test_store();

        for i in 0..5 {
            store
                .append(
                    "test",
                    &TestEvent {
                        event_type: "click".to_string(),
                        ts: i * 100,
                        data: format!("event {}", i),
                    },
                )
                .unwrap();
        }

        assert_eq!(store.count("test"), 5);
        store.clear("test").unwrap();
        assert_eq!(store.count("test"), 0);
    }

    #[test]
    fn test_query_range() {
        let (store, _temp) = create_test_store();

        for i in 0..10 {
            store
                .append(
                    "test",
                    &TestEvent {
                        event_type: "click".to_string(),
                        ts: i * 100,
                        data: format!("event {}", i),
                    },
                )
                .unwrap();
        }

        let events: Vec<TestEvent> = store
            .query_range("test", Some(300), Some(600), None)
            .unwrap();

        assert_eq!(events.len(), 4);
        assert_eq!(events[0].ts, 300);
        assert_eq!(events[3].ts, 600);
    }

    #[test]
    fn test_concurrent_access() {
        let (store, _temp) = create_test_store();

        std::thread::scope(|s| {
            for t in 0..4 {
                let store = &store;
                s.spawn(move || {
                    for i in 0..25 {
                        store
                            .append(
                                "test",
                                &TestEvent {
                                    event_type: "click".to_string(),
                                    ts: (t * 100 + i) as u64,
                                    data: format!("thread {} event {}", t, i),
                                },
                            )
                            .unwrap();
                    }
                });
            }
        });

        assert_eq!(store.count("test"), 100);
    }
}
