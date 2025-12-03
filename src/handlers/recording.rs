use crate::{
    ChromeError, Result,
    chrome::{
        PageProvider,
        collectors::{ConsoleLevel, ConsoleMessage, NetworkRequest, PageError},
        models::{
            ActivitySummary, ActivityType, BrowserActivity, RecordingSession, VideoFormat,
            VideoRecording,
        },
    },
    output,
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams, EventFrameNavigated, EventLoadEventFired,
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

impl output::OutputFormatter for VideoRecording {
    fn format_text(&self) -> String {
        use crate::output::text;
        let format_str = match self.format {
            VideoFormat::Mp4 => "MP4",
            VideoFormat::Frames => "JPEG Frames",
        };
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            text::success(&format!("Video recorded: {}", self.file_path.display())),
            text::key_value(
                "Duration",
                &format!("{:.2}s", self.duration_ms as f64 / 1000.0)
            ),
            text::key_value(
                "Frames",
                &format!("{} ({} fps)", self.frame_count, self.fps)
            ),
            text::key_value(
                "Resolution",
                &format!("{}x{}", self.resolution.0, self.resolution.1)
            ),
            text::key_value("Format", format_str),
            text::key_value("File Size", &text::format_bytes(self.file_size_bytes))
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

impl output::OutputFormatter for RecordingSession {
    fn format_text(&self) -> String {
        use crate::output::text;
        let format_str = match self.recording.format {
            VideoFormat::Mp4 => "MP4",
            VideoFormat::Frames => "JPEG Frames",
        };

        let mut output = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n\n{}",
            text::success(&format!(
                "Recording session completed: {}",
                self.recording.file_path.display()
            )),
            text::key_value(
                "Duration",
                &format!("{:.2}s", self.summary.duration_seconds)
            ),
            text::key_value(
                "Frames",
                &format!(
                    "{} ({} fps)",
                    self.recording.frame_count, self.recording.fps
                )
            ),
            text::key_value(
                "Resolution",
                &format!(
                    "{}x{}",
                    self.recording.resolution.0, self.recording.resolution.1
                )
            ),
            text::key_value("Format", format_str),
            text::key_value(
                "File Size",
                &text::format_bytes(self.recording.file_size_bytes)
            ),
            text::section("Activity Summary")
        );

        output.push_str(&format!(
            "\n{}\n{}\n{}",
            text::key_value(
                "Total Activities",
                &self.summary.total_activities.to_string()
            ),
            text::key_value("Navigations", &self.summary.navigation_count.to_string()),
            text::key_value("Errors", &self.summary.error_count.to_string()),
        ));

        if !self.summary.pages_visited.is_empty() {
            output.push_str(&format!("\n{}", text::section("Pages Visited")));
            for (i, page) in self.summary.pages_visited.iter().enumerate() {
                output.push_str(&format!("\n  {}. {}", i + 1, page));
            }
        }

        if !self.activities.is_empty() {
            output.push_str(&format!("\n{}", text::section("Activity Timeline")));
            for activity in &self.activities {
                let type_str = match activity.activity_type {
                    ActivityType::Navigation => "🔗 Navigate",
                    ActivityType::PageLoad => "📄 Load",
                    ActivityType::Click => "👆 Click",
                    ActivityType::Input => "⌨️ Input",
                    ActivityType::NetworkRequest => "🌐 Request",
                    ActivityType::ConsoleLog => "📝 Console",
                    ActivityType::Error => "❌ Error",
                };
                output.push_str(&format!("\n  {} {}", type_str, activity.description));
            }
        }

        output
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_record(
    provider: &impl PageProvider,
    output: &Path,
    duration: u64,
    fps: u32,
    quality: u8,
    encode_mp4: bool,
) -> Result<VideoRecording> {
    let page = provider.get_or_create_page().await?;

    let temp_dir = output
        .parent()
        .unwrap_or(Path::new("/tmp"))
        .join(format!("recording_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir)?;

    // Use screenshot-based recording (more reliable than screencast)
    let frame_interval = Duration::from_millis(1000 / fps as u64);
    let start_time = Instant::now();
    let target_duration = Duration::from_secs(duration);
    let mut frame_count = 0usize;
    let mut actual_width = 0u32;
    let mut actual_height = 0u32;

    while start_time.elapsed() < target_duration {
        let frame_start = Instant::now();

        // Capture screenshot
        let params = CaptureScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Jpeg)
            .quality(quality as i64)
            .build();

        if let Ok(screenshot) = page.execute(params).await {
            let frame_path = temp_dir.join(format!("frame_{:04}.jpg", frame_count));
            if let Ok(data) = BASE64_STANDARD.decode(&screenshot.data)
                && let Ok(()) = std::fs::write(&frame_path, &data)
            {
                // Get dimensions from first frame
                if frame_count == 0
                    && let Ok(img) = image::load_from_memory(&data)
                {
                    actual_width = img.width();
                    actual_height = img.height();
                }
                frame_count += 1;
            }
        }

        // Maintain frame rate
        let elapsed = frame_start.elapsed();
        if elapsed < frame_interval {
            tokio::time::sleep(frame_interval - elapsed).await;
        }
    }

    if frame_count == 0 {
        std::fs::remove_dir_all(&temp_dir)?;
        return Err(ChromeError::General(
            "No frames were captured during recording".into(),
        ));
    }

    let (final_path, format) = if encode_mp4 && has_ffmpeg() {
        match encode_to_mp4(&temp_dir, output, fps) {
            Ok(path) => (path, VideoFormat::Mp4),
            Err(e) => {
                tracing::warn!("ffmpeg encoding failed: {}, keeping frame sequence", e);
                (temp_dir.clone(), VideoFormat::Frames)
            }
        }
    } else {
        let frames_dir = output.with_extension("frames");
        std::fs::rename(&temp_dir, &frames_dir)?;
        (frames_dir, VideoFormat::Frames)
    };

    let file_size = get_total_size(&final_path)?;

    Ok(VideoRecording {
        file_path: final_path.to_path_buf(),
        format,
        duration_ms: start_time.elapsed().as_millis() as u64,
        frame_count,
        fps,
        resolution: (actual_width, actual_height),
        file_size_bytes: file_size,
        recorded_at: Utc::now(),
    })
}

fn encode_to_mp4(frames_dir: &Path, output: &Path, fps: u32) -> Result<PathBuf> {
    let fps_str = fps.to_string();
    let input = format!("{}/frame_%04d.jpg", frames_dir.display());
    let output_str = output
        .to_str()
        .ok_or_else(|| ChromeError::General("Invalid UTF-8 in output path".into()))?;

    let status = std::process::Command::new("ffmpeg")
        .args([
            "-r", &fps_str, "-i", &input, "-c:v", "libx264", "-crf", "20", "-preset", "fast",
            "-pix_fmt", "yuv420p", "-g", "1", "-an", "-y", output_str,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| ChromeError::General(format!("Failed to run ffmpeg: {}", e)))?;

    if !status.success() {
        return Err(ChromeError::General("ffmpeg encoding failed".into()));
    }

    std::fs::remove_dir_all(frames_dir)?;
    Ok(output.to_path_buf())
}

fn has_ffmpeg() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

fn get_total_size(path: &Path) -> Result<u64> {
    if path.is_file() {
        Ok(std::fs::metadata(path)?.len())
    } else if path.is_dir() {
        let mut total = 0u64;
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                total += entry.metadata()?.len();
            }
        }
        Ok(total)
    } else {
        Ok(0)
    }
}

pub async fn handle_interactive_record(
    provider: &impl PageProvider,
    output: &Path,
    duration: Option<u64>,
    fps: u32,
    quality: u8,
    encode_mp4: bool,
) -> Result<RecordingSession> {
    let page = provider.get_or_create_page().await?;

    let temp_dir = output
        .parent()
        .unwrap_or(Path::new("/tmp"))
        .join(format!("recording_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir)?;

    // Setup event listeners for activity tracking
    let mut nav_stream = page
        .event_listener::<EventFrameNavigated>()
        .await
        .map_err(|e| ChromeError::General(format!("Failed to create nav listener: {}", e)))?;

    let mut load_stream = page
        .event_listener::<EventLoadEventFired>()
        .await
        .map_err(|e| ChromeError::General(format!("Failed to create load listener: {}", e)))?;

    // Setup Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            r.store(false, Ordering::SeqCst);
        }
    });

    // Print interactive mode message
    if duration.is_none() {
        use crate::output::text;
        println!(
            "{}",
            text::info("Interactive recording started. Press Ctrl+C to stop...")
        );
        println!(
            "{}",
            text::info("Navigate the browser - all activities will be tracked.")
        );
        println!();
    }

    let frame_interval = Duration::from_millis(1000 / fps as u64);
    let start_time = Instant::now();
    let recording_start_ts = Utc::now(); // Record start timestamp for filtering collector data
    let target_duration = duration.map(Duration::from_secs);
    let mut frame_count = 0usize;
    let mut actual_width = 0u32;
    let mut actual_height = 0u32;
    let mut activities: Vec<BrowserActivity> = Vec::new();
    let mut pages_visited: HashSet<String> = HashSet::new();
    let mut last_frame_time = Instant::now();

    loop {
        // Check termination conditions
        if !running.load(Ordering::SeqCst) {
            break;
        }
        if let Some(target) = target_duration
            && start_time.elapsed() >= target
        {
            break;
        }

        // Capture frame using screenshot (more reliable than screencast)
        if last_frame_time.elapsed() >= frame_interval {
            let params = CaptureScreenshotParams::builder()
                .format(CaptureScreenshotFormat::Jpeg)
                .quality(quality as i64)
                .build();

            if let Ok(screenshot) = page.execute(params).await {
                let frame_path = temp_dir.join(format!("frame_{:04}.jpg", frame_count));
                if let Ok(data) = BASE64_STANDARD.decode(&screenshot.data)
                    && let Ok(()) = std::fs::write(&frame_path, &data)
                {
                    if frame_count == 0
                        && let Ok(img) = image::load_from_memory(&data)
                    {
                        actual_width = img.width();
                        actual_height = img.height();
                    }
                    frame_count += 1;
                }
            }
            last_frame_time = Instant::now();
        }

        // Check for navigation events (non-blocking)
        tokio::select! {
            Some(nav_event) = nav_stream.next() => {
                let url = nav_event.frame.url.clone();
                if !url.is_empty() && url != "about:blank" {
                    pages_visited.insert(url.clone());
                    activities.push(BrowserActivity {
                        timestamp: Utc::now(),
                        activity_type: ActivityType::Navigation,
                        description: format!("Navigated to {}", truncate_url(&url, 60)),
                        url: Some(url),
                    });

                    use crate::output::text;
                    println!("  {} {}", text::info("🔗"), truncate_url(&nav_event.frame.url, 70));
                }
            }
            Some(_load_event) = load_stream.next() => {
                activities.push(BrowserActivity {
                    timestamp: Utc::now(),
                    activity_type: ActivityType::PageLoad,
                    description: "Page load complete".to_string(),
                    url: None,
                });
            }
            _ = tokio::time::sleep(Duration::from_millis(10)) => {
                // Continue loop
            }
        }
    }

    let duration_secs = start_time.elapsed().as_secs_f64();

    // Collect activities from storage (network, console, pageerror)
    let collector_activities =
        collect_activities_from_storage(provider.storage(), recording_start_ts);
    activities.extend(collector_activities);

    // Sort activities by timestamp
    activities.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    // Handle no frames case
    if frame_count == 0 {
        std::fs::remove_dir_all(&temp_dir)?;
        return Err(ChromeError::General(
            "No frames were captured during recording".into(),
        ));
    }

    // Encode video or keep frames
    let (final_path, format) = if encode_mp4 && has_ffmpeg() {
        match encode_to_mp4(&temp_dir, output, fps) {
            Ok(path) => (path, VideoFormat::Mp4),
            Err(e) => {
                tracing::warn!("ffmpeg encoding failed: {}, keeping frame sequence", e);
                (temp_dir.clone(), VideoFormat::Frames)
            }
        }
    } else {
        let frames_dir = output.with_extension("frames");
        std::fs::rename(&temp_dir, &frames_dir)?;
        (frames_dir, VideoFormat::Frames)
    };

    let file_size = get_total_size(&final_path)?;

    // Build summary
    let navigation_count = activities
        .iter()
        .filter(|a| a.activity_type == ActivityType::Navigation)
        .count();
    let error_count = activities
        .iter()
        .filter(|a| a.activity_type == ActivityType::Error)
        .count();

    let summary = ActivitySummary {
        total_activities: activities.len(),
        pages_visited: pages_visited.into_iter().collect(),
        navigation_count,
        interaction_count: 0,
        error_count,
        duration_seconds: duration_secs,
    };

    let recording = VideoRecording {
        file_path: final_path.to_path_buf(),
        format,
        duration_ms: (duration_secs * 1000.0) as u64,
        frame_count,
        fps,
        resolution: (actual_width, actual_height),
        file_size_bytes: file_size,
        recorded_at: Utc::now(),
    };

    Ok(RecordingSession {
        recording,
        activities,
        summary,
    })
}

fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        url.to_string()
    } else {
        format!("{}...", &url[..max_len - 3])
    }
}

/// Collect activities from NDJSON storage files (network, console, pageerror)
/// Filters by recording start timestamp to only include relevant activities
fn collect_activities_from_storage(
    storage: &crate::chrome::storage::SessionStorage,
    start_ts: DateTime<Utc>,
) -> Vec<BrowserActivity> {
    let mut activities = Vec::new();

    // Collect network requests (only errors/failures - status >= 400)
    if let Ok(requests) = storage.read_all::<NetworkRequest>("network") {
        for req in requests {
            if req.timestamp >= start_ts {
                // Only include failed requests (HTTP 4xx/5xx errors)
                if let Some(status) = req.status
                    && status >= 400
                {
                    let status_text = req
                        .status_text
                        .as_ref()
                        .map(|t| format!(": {}", t))
                        .unwrap_or_default();

                    activities.push(BrowserActivity {
                        timestamp: req.timestamp,
                        activity_type: ActivityType::Error,
                        description: format!(
                            "HTTP {} {}{} - {}",
                            status,
                            req.method,
                            status_text,
                            truncate_url(&req.url, 40)
                        ),
                        url: Some(req.url),
                    });
                }
            }
        }
    }

    // Collect console errors and warnings
    if let Ok(messages) = storage.read_all::<ConsoleMessage>("console") {
        for msg in messages {
            if msg.timestamp >= start_ts {
                let (activity_type, prefix) = match msg.level {
                    ConsoleLevel::Error => (ActivityType::Error, "Console error"),
                    ConsoleLevel::Warning => (ActivityType::ConsoleLog, "Console warning"),
                    _ => continue, // Skip info/log/debug
                };

                activities.push(BrowserActivity {
                    timestamp: msg.timestamp,
                    activity_type,
                    description: format!("{}: {}", prefix, truncate_url(&msg.text, 60)),
                    url: msg.url,
                });
            }
        }
    }

    // Collect page errors (JavaScript exceptions)
    if let Ok(errors) = storage.read_all::<PageError>("pageerror") {
        for err in errors {
            if err.timestamp >= start_ts {
                activities.push(BrowserActivity {
                    timestamp: err.timestamp,
                    activity_type: ActivityType::Error,
                    description: format!("JS error: {}", truncate_url(&err.message, 60)),
                    url: err.url,
                });
            }
        }
    }

    activities
}
