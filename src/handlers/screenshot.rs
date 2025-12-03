use crate::{
    ChromeError, Result,
    chrome::{
        PageProvider,
        models::{ImageFormat, ScreenshotCapture},
    },
    output,
    timeouts::ms,
};
use base64::Engine;
use chromiumoxide::Page;
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams, GetLayoutMetricsParams, Viewport,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

impl output::OutputFormatter for ScreenshotCapture {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}\n{}\n{}\n{}",
            text::success(&format!("Screenshot saved: {}", self.file_path.display())),
            text::key_value("URL", &self.url),
            text::key_value("Size", &format!("{}x{}", self.width, self.height)),
            text::key_value("Format", &format!("{:?}", self.format)),
            text::key_value("File Size", &text::format_bytes(self.file_size_bytes))
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_screenshot(
    provider: &impl PageProvider,
    output_path: &str,
    full_page: bool,
    selector: Option<&str>,
    format: Option<&str>,
    quality: Option<u8>,
) -> Result<ScreenshotCapture> {
    let page = provider.get_or_create_page().await?;

    let current_url = page.url().await.unwrap_or_default().unwrap_or_default();
    if current_url == "about:blank" || current_url.is_empty() {
        tracing::warn!("Page is blank. Use 'navigate' first to load content.");
    }

    if current_url != "about:blank" && !current_url.is_empty() {
        wait_for_render_complete(&page).await;
    }

    let format_enum = parse_format(format)?;
    let screenshot_format = match format_enum {
        ImageFormat::Png => CaptureScreenshotFormat::Png,
        ImageFormat::Jpeg => CaptureScreenshotFormat::Jpeg,
        ImageFormat::Webp => CaptureScreenshotFormat::Webp,
    };

    let screenshot_data = if let Some(sel) = selector {
        let element = page
            .find_element(sel)
            .await
            .map_err(|e| ChromeError::General(format!("Element not found: {}", e)))?;

        element
            .screenshot(screenshot_format)
            .await
            .map_err(|e| ChromeError::ScreenshotFailed(e.to_string()))?
    } else {
        let mut params = CaptureScreenshotParams::builder()
            .format(screenshot_format)
            .build();

        if let Some(q) = quality
            && format_enum != ImageFormat::Png
        {
            params.quality = Some(q as i64);
        }

        if full_page {
            params.capture_beyond_viewport = Some(true);
        } else if let Ok(metrics) = page.execute(GetLayoutMetricsParams::default()).await {
            let css = &metrics.css_layout_viewport;
            params.clip = Some(Viewport {
                x: 0.0,
                y: 0.0,
                width: css.client_width as f64,
                height: css.client_height as f64,
                scale: 1.0,
            });
        }

        page.screenshot(params)
            .await
            .map_err(|e| ChromeError::ScreenshotFailed(e.to_string()))?
    };

    let output_pathbuf = PathBuf::from(output_path);

    if let Some(parent) = output_pathbuf.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&output_pathbuf, &screenshot_data)?;

    let url = page
        .url()
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "about:blank".to_string());

    let image_data = image::load_from_memory(&screenshot_data)
        .map_err(|e| ChromeError::ScreenshotFailed(format!("Failed to load image: {}", e)))?;

    let file_size = std::fs::metadata(&output_pathbuf)?.len();

    let base64_data = base64::engine::general_purpose::STANDARD.encode(&screenshot_data);

    Ok(ScreenshotCapture {
        file_path: output_pathbuf,
        format: format_enum,
        width: image_data.width(),
        height: image_data.height(),
        full_page,
        url,
        captured_at: chrono::Utc::now(),
        file_size_bytes: file_size,
        data: Some(base64_data),
    })
}

fn parse_format(format: Option<&str>) -> Result<ImageFormat> {
    match format {
        None | Some("png") => Ok(ImageFormat::Png),
        Some("jpeg") | Some("jpg") => Ok(ImageFormat::Jpeg),
        Some("webp") => Ok(ImageFormat::Webp),
        Some(other) => Err(ChromeError::ConfigError(format!(
            "Unsupported image format: {}. Use png, jpeg, or webp",
            other
        ))),
    }
}

async fn wait_for_render_complete(page: &Arc<Page>) {
    let start = std::time::Instant::now();

    // Phase 1: Wait for document.readyState === 'complete' and images
    while start.elapsed() < Duration::from_millis(ms::SCREENSHOT_WAIT) {
        let render_check: bool = page
            .evaluate(
                r#"(function() {
                    if (document.readyState !== 'complete') return false;
                    const body = document.body;
                    if (!body) return false;
                    const rect = body.getBoundingClientRect();
                    if (rect.height < 100) return false;
                    const imgs = document.querySelectorAll('img');
                    for (const img of imgs) {
                        if (!img.complete && img.src) return false;
                    }
                    return true;
                })()"#,
            )
            .await
            .ok()
            .and_then(|v| v.into_value().ok())
            .unwrap_or(false);

        if render_check {
            break;
        }

        tokio::time::sleep(Duration::from_millis(ms::POLL_INTERVAL)).await;
    }

    // Phase 2: Wait for network idle (Puppeteer networkidle0 equivalent)
    // Check if there are no pending requests for NETWORK_IDLE_MS
    let idle_start = std::time::Instant::now();
    let mut last_activity = std::time::Instant::now();

    while idle_start.elapsed()
        < Duration::from_millis(ms::SCREENSHOT_WAIT - start.elapsed().as_millis() as u64)
    {
        let pending_requests: i64 = page
            .evaluate(
                r#"(function() {
                    if (!window.__cdpPendingRequests) return 0;
                    return window.__cdpPendingRequests;
                })()"#,
            )
            .await
            .ok()
            .and_then(|v| v.into_value().ok())
            .unwrap_or(0);

        if pending_requests > 0 {
            last_activity = std::time::Instant::now();
        }

        if last_activity.elapsed() >= Duration::from_millis(ms::NETWORK_IDLE) {
            break;
        }

        tokio::time::sleep(Duration::from_millis(ms::VIEWPORT_SETTLE)).await;
    }

    tokio::time::sleep(Duration::from_millis(ms::VIEWPORT_SETTLE)).await;
}
