use crate::{
    ChromeError, Result,
    chrome::models::{PerformanceTrace, TraceEvent, TraceMetadata},
};
use chrono::Utc;
use std::path::Path;

#[derive(Debug, serde::Deserialize)]
struct RawTrace {
    #[serde(rename = "traceEvents")]
    trace_events: Vec<TraceEvent>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

pub fn parse_trace(file_path: &Path) -> Result<PerformanceTrace> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| ChromeError::General(format!("Failed to read trace file: {}", e)))?;

    let raw_trace: RawTrace = serde_json::from_str(&content)
        .map_err(|e| ChromeError::General(format!("Failed to parse trace JSON: {}", e)))?;

    if raw_trace.trace_events.is_empty() {
        return Err(ChromeError::General("Trace file contains no events".into()));
    }

    let first_ts = raw_trace
        .trace_events
        .iter()
        .map(|e| e.timestamp)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);

    let last_ts = raw_trace
        .trace_events
        .iter()
        .map(|e| e.timestamp)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);

    let duration_ms = (last_ts - first_ts) / 1000.0;

    let metadata = TraceMetadata {
        url: extract_url_from_metadata(&raw_trace.metadata),
        start_time: Utc::now(),
        end_time: Some(Utc::now()),
        duration_ms,
    };

    Ok(PerformanceTrace {
        events: raw_trace.trace_events,
        metadata,
    })
}

fn extract_url_from_metadata(metadata: &Option<serde_json::Value>) -> String {
    metadata
        .as_ref()
        .and_then(|m| m.get("url"))
        .and_then(|u| u.as_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_trace() {
        let temp_file = std::env::temp_dir().join("empty_trace.json");
        std::fs::write(&temp_file, r#"{"traceEvents":[]}"#).unwrap();

        let result = parse_trace(&temp_file);
        assert!(result.is_err());

        std::fs::remove_file(temp_file).ok();
    }

    #[test]
    fn test_parse_valid_trace() {
        let temp_file = std::env::temp_dir().join("valid_trace.json");
        let content = r#"{
            "traceEvents": [
                {"name":"test","cat":"blink","ph":"X","ts":1000,"pid":1,"tid":1}
            ]
        }"#;
        std::fs::write(&temp_file, content).unwrap();

        let result = parse_trace(&temp_file);
        assert!(result.is_ok());

        let trace = result.unwrap();
        assert_eq!(trace.events.len(), 1);
        assert_eq!(trace.events[0].name, "test");

        std::fs::remove_file(temp_file).ok();
    }
}
