use crate::{
    ChromeError, Result,
    chrome::{models::PerformanceAnalysis, session_manager::BrowserSessionManager},
    output,
    timeouts::{ms, secs},
    trace,
};
use chromiumoxide::cdp::browser_protocol::tracing::{
    EndParams, EventDataCollected, EventTracingComplete, StartParams, TraceConfig,
};
use futures::StreamExt;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Serialize)]
pub struct TraceResult {
    pub file_path: PathBuf,
    pub file_size_bytes: u64,
    pub duration_ms: f64,
    pub status: String,
}

impl output::OutputFormatter for TraceResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}\n{}\n{}",
            text::success(&format!("Trace captured: {}", self.file_path.display())),
            text::key_value("File Size", &format!("{} bytes", self.file_size_bytes)),
            text::key_value("Duration", &format!("{:.2}ms", self.duration_ms)),
            text::key_value("Status", &self.status)
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

impl output::OutputFormatter for PerformanceAnalysis {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut output = String::new();

        output.push_str(&text::section("Performance Analysis"));
        output.push_str(&format!("\n{}\n\n", text::key_value("URL", &self.url)));

        output.push_str(&text::subsection("Core Web Vitals"));
        if let Some(lcp) = self.core_web_vitals.lcp_ms {
            output.push_str(&format!(
                "\n  {} {:.0}ms [{:?}]",
                text::key_value("LCP", ""),
                lcp,
                self.core_web_vitals.lcp_rating
            ));
        }
        if let Some(fid) = self.core_web_vitals.fid_ms {
            output.push_str(&format!(
                "\n  {} {:.0}ms [{:?}]",
                text::key_value("FID", ""),
                fid,
                self.core_web_vitals.fid_rating
            ));
        }
        if let Some(cls) = self.core_web_vitals.cls {
            output.push_str(&format!(
                "\n  {} {:.3} [{:?}]",
                text::key_value("CLS", ""),
                cls,
                self.core_web_vitals.cls_rating
            ));
        }
        if let Some(ttfb) = self.core_web_vitals.ttfb_ms {
            output.push_str(&format!(
                "\n  {} {:.0}ms [{:?}]",
                text::key_value("TTFB", ""),
                ttfb,
                self.core_web_vitals.ttfb_rating
            ));
        }

        output.push_str(&format!("\n\n{}", text::subsection("Page Load Metrics")));
        output.push_str(&format!(
            "\n  {}",
            text::key_value(
                "DOM Content Loaded",
                &format!("{:.0}ms", self.page_load_metrics.dom_content_loaded_ms)
            )
        ));
        output.push_str(&format!(
            "\n  {}",
            text::key_value(
                "Load Complete",
                &format!("{:.0}ms", self.page_load_metrics.load_complete_ms)
            )
        ));
        if let Some(fp) = self.page_load_metrics.first_paint_ms {
            output.push_str(&format!(
                "\n  {}",
                text::key_value("First Paint", &format!("{:.0}ms", fp))
            ));
        }
        if let Some(fcp) = self.page_load_metrics.first_contentful_paint_ms {
            output.push_str(&format!(
                "\n  {}",
                text::key_value("First Contentful Paint", &format!("{:.0}ms", fcp))
            ));
        }

        output.push_str(&format!("\n\n{}", text::subsection("Main Thread")));
        output.push_str(&format!(
            "\n  {}",
            text::key_value(
                "Total Blocking Time",
                &format!("{:.0}ms", self.main_thread_metrics.total_blocking_time_ms)
            )
        ));
        output.push_str(&format!(
            "\n  {}",
            text::key_value(
                "Long Tasks",
                &self.main_thread_metrics.long_tasks_count.to_string()
            )
        ));
        output.push_str(&format!(
            "\n  {}",
            text::key_value(
                "Script Duration",
                &format!("{:.0}ms", self.main_thread_metrics.script_duration_ms)
            )
        ));

        if !self.recommendations.is_empty() {
            output.push_str(&format!("\n\n{}", text::subsection("Recommendations")));
            for rec in &self.recommendations {
                let severity_icon = match rec.severity {
                    crate::chrome::models::Severity::High => "🔴",
                    crate::chrome::models::Severity::Medium => "🟡",
                    crate::chrome::models::Severity::Low => "🟢",
                };
                output.push_str(&format!("\n  {} {}", severity_icon, rec.message));
            }
        }

        output
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_trace(
    manager: &BrowserSessionManager,
    url: &str,
    output_path: &Path,
    categories: Option<Vec<String>>,
) -> Result<TraceResult> {
    let browser = manager.get_or_create_browser().await?;
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| ChromeError::General(format!("Failed to create page: {}", e)))?;

    let default_categories = vec![
        "-*".to_string(),
        "devtools.timeline".to_string(),
        "v8.execute".to_string(),
        "v8".to_string(),
        "blink.console".to_string(),
        "blink.user_timing".to_string(),
        "loading".to_string(),
        "latencyInfo".to_string(),
        "disabled-by-default-devtools.timeline".to_string(),
        "disabled-by-default-devtools.timeline.frame".to_string(),
        "disabled-by-default-devtools.timeline.stack".to_string(),
        "disabled-by-default-v8.cpu_profiler".to_string(),
    ];

    let trace_categories = categories.unwrap_or(default_categories);

    let trace_events: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
    let trace_events_clone = trace_events.clone();

    let mut data_stream = page
        .event_listener::<EventDataCollected>()
        .await
        .map_err(|e| ChromeError::General(format!("Failed to subscribe to trace events: {}", e)))?;

    let mut complete_stream = page
        .event_listener::<EventTracingComplete>()
        .await
        .map_err(|e| {
            ChromeError::General(format!("Failed to subscribe to trace complete: {}", e))
        })?;

    let collector_handle = tokio::spawn(async move {
        while let Some(event) = data_stream.next().await {
            let event = std::sync::Arc::try_unwrap(event).unwrap_or_else(|arc| (*arc).clone());
            if let Ok(mut events) = trace_events_clone.lock() {
                events.extend(event.value);
            }
        }
    });

    let trace_config = TraceConfig::builder()
        .included_categories(trace_categories)
        .build();

    let start_params = StartParams::builder().trace_config(trace_config).build();

    let start_time = std::time::Instant::now();

    page.execute(start_params)
        .await
        .map_err(|e| ChromeError::General(format!("Failed to start trace: {}", e)))?;

    page.goto(url)
        .await
        .map_err(|e| ChromeError::General(format!("Failed to navigate: {}", e)))?;

    tokio::time::sleep(tokio::time::Duration::from_secs(secs::PERFORMANCE_WAIT)).await;

    let end_params = EndParams::default();
    page.execute(end_params)
        .await
        .map_err(|e| ChromeError::General(format!("Failed to stop trace: {}", e)))?;

    let _ = tokio::time::timeout(
        tokio::time::Duration::from_secs(secs::PERFORMANCE_TIMEOUT),
        complete_stream.next(),
    )
    .await;

    tokio::time::sleep(tokio::time::Duration::from_millis(ms::NETWORK_IDLE)).await;
    collector_handle.abort();

    let duration = start_time.elapsed();

    let collected_events = trace_events
        .lock()
        .map_err(|e| ChromeError::General(format!("Failed to lock trace events: {}", e)))?
        .clone();

    let event_count = collected_events.len();

    let trace_data = serde_json::json!({
        "traceEvents": collected_events,
        "metadata": {
            "url": url,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    });

    std::fs::write(output_path, serde_json::to_string_pretty(&trace_data)?)?;

    let file_size = std::fs::metadata(output_path)?.len();

    let status = if event_count > 0 {
        format!("captured {} events", event_count)
    } else {
        "completed (no events captured)".to_string()
    };

    Ok(TraceResult {
        file_path: output_path.to_path_buf(),
        file_size_bytes: file_size,
        duration_ms: duration.as_millis() as f64,
        status,
    })
}

pub fn handle_analyze(trace_file: &Path) -> Result<PerformanceAnalysis> {
    let trace = trace::parser::parse_trace(trace_file)?;

    let url = trace.metadata.url.clone();

    Ok(trace::analyzer::analyze_trace(&trace, url))
}
