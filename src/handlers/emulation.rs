use crate::{
    ChromeError, Result,
    chrome::{PageProvider, models::Viewport},
    devices::{self, DeviceProfile},
    output,
    timeouts::{ms, secs},
};
use chromiumoxide::cdp::browser_protocol::emulation::{
    SetDeviceMetricsOverrideParams, SetTouchEmulationEnabledParams, SetUserAgentOverrideParams,
};
use chromiumoxide::cdp::browser_protocol::page::ReloadParams;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EmulationResult {
    pub device_name: String,
    pub viewport: Viewport,
    pub user_agent: String,
    pub status: String,
}

impl output::OutputFormatter for EmulationResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        format!(
            "{}\n{}\n{}\n{}\n{}",
            text::success(&format!("Emulating: {}", self.device_name)),
            text::key_value(
                "Viewport",
                &format!("{}x{}", self.viewport.width, self.viewport.height)
            ),
            text::key_value("Pixel Ratio", &format!("{}", self.viewport.pixel_ratio)),
            text::key_value("Mobile", &self.viewport.is_mobile.to_string()),
            text::key_value("Touch", &self.viewport.has_touch.to_string())
        )
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_emulate(
    provider: &impl PageProvider,
    device_name: &str,
) -> Result<EmulationResult> {
    let device = devices::get_device_by_name(device_name)?;
    apply_device_emulation(provider, &device).await
}

pub async fn handle_viewport(
    provider: &impl PageProvider,
    width: u32,
    height: u32,
    pixel_ratio: Option<f64>,
) -> Result<EmulationResult> {
    let custom_device = DeviceProfile {
        name: "Custom Viewport".to_string(),
        width,
        height,
        pixel_ratio: pixel_ratio.unwrap_or(1.0),
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".to_string(),
        touch: false,
        mobile: false,
        landscape: width > height,
    };

    custom_device.validate()?;
    apply_device_emulation(provider, &custom_device).await
}

async fn apply_device_emulation(
    provider: &impl PageProvider,
    device: &DeviceProfile,
) -> Result<EmulationResult> {
    let page = provider.get_or_create_page().await?;

    let metrics_params = SetDeviceMetricsOverrideParams::builder()
        .width(device.width as i64)
        .height(device.height as i64)
        .device_scale_factor(device.pixel_ratio)
        .mobile(device.mobile)
        .build()
        .map_err(|e| ChromeError::General(format!("Failed to build metrics params: {}", e)))?;

    page.execute(metrics_params)
        .await
        .map_err(|e| ChromeError::General(format!("Failed to set device metrics: {}", e)))?;

    if let Ok(user_agent_params) = SetUserAgentOverrideParams::builder()
        .user_agent(device.user_agent.clone())
        .build()
    {
        page.execute(user_agent_params)
            .await
            .map_err(|e| ChromeError::General(format!("Failed to set user agent: {}", e)))?;
    }

    if let Ok(touch_params) = SetTouchEmulationEnabledParams::builder()
        .enabled(device.touch)
        .build()
    {
        page.execute(touch_params)
            .await
            .map_err(|e| ChromeError::General(format!("Failed to set touch emulation: {}", e)))?;
    }

    let current_url = page.url().await.ok().flatten().unwrap_or_default();
    if !current_url.is_empty() && current_url != "about:blank" {
        let reload_params = ReloadParams::builder().ignore_cache(true).build();

        page.execute(reload_params)
            .await
            .map_err(|e| ChromeError::General(format!("Failed to reload page: {}", e)))?;

        let reload_timeout = std::time::Duration::from_secs(secs::EMULATION_RELOAD);
        let start = std::time::Instant::now();
        tokio::time::sleep(std::time::Duration::from_millis(ms::POLL_INTERVAL)).await;

        loop {
            if let Ok(result) = page.evaluate("document.readyState").await
                && let Ok(ready_state) = result.into_value::<String>()
                && ready_state == "complete"
            {
                break;
            }

            if start.elapsed() > reload_timeout {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(ms::VIEWPORT_SETTLE)).await;
        }

        let expected_width = device.width as i64;
        let viewport_timeout = std::time::Duration::from_secs(secs::READY_STATE);
        let viewport_start = std::time::Instant::now();

        while viewport_start.elapsed() < viewport_timeout {
            if let Ok(result) = page.evaluate("window.innerWidth").await
                && let Ok(actual_width) = result.into_value::<i64>()
                && actual_width == expected_width
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(ms::VIEWPORT_SETTLE)).await;
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(ms::VIEWPORT_SETTLE)).await;

    Ok(EmulationResult {
        device_name: device.name.clone(),
        viewport: Viewport {
            width: device.width,
            height: device.height,
            pixel_ratio: device.pixel_ratio,
            is_mobile: device.mobile,
            has_touch: device.touch,
        },
        user_agent: device.user_agent.clone(),
        status: "applied".to_string(),
    })
}
