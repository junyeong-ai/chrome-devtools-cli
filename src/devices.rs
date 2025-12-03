use crate::{ChromeError, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub pixel_ratio: f64,
    pub user_agent: String,
    pub touch: bool,
    pub mobile: bool,
    pub landscape: bool,
}

impl DeviceProfile {
    pub fn validate(&self) -> Result<()> {
        if self.width < 320 || self.height < 320 {
            return Err(ChromeError::ConfigError(
                "Device dimensions must be at least 320x320".into(),
            ));
        }

        if self.pixel_ratio < 0.5 || self.pixel_ratio > 5.0 {
            return Err(ChromeError::ConfigError(
                "Pixel ratio must be between 0.5 and 5.0".into(),
            ));
        }

        if self.user_agent.is_empty() {
            return Err(ChromeError::ConfigError(
                "User agent cannot be empty".into(),
            ));
        }

        Ok(())
    }
}

pub static DEVICE_PRESETS: Lazy<Vec<DeviceProfile>> = Lazy::new(|| {
    vec![
        DeviceProfile {
            name: String::from("Desktop"),
            width: 1920,
            height: 1080,
            pixel_ratio: 1.0,
            user_agent: String::from(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            ),
            touch: false,
            mobile: false,
            landscape: true,
        },
        DeviceProfile {
            name: String::from("iPhone 14"),
            width: 390,
            height: 844,
            pixel_ratio: 3.0,
            user_agent: String::from(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            ),
            touch: true,
            mobile: true,
            landscape: false,
        },
        DeviceProfile {
            name: String::from("iPad Pro"),
            width: 1024,
            height: 1366,
            pixel_ratio: 2.0,
            user_agent: String::from(
                "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            ),
            touch: true,
            mobile: true,
            landscape: false,
        },
        DeviceProfile {
            name: String::from("Pixel 7"),
            width: 412,
            height: 915,
            pixel_ratio: 2.625,
            user_agent: String::from(
                "Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36",
            ),
            touch: true,
            mobile: true,
            landscape: false,
        },
        DeviceProfile {
            name: String::from("Galaxy S23"),
            width: 360,
            height: 800,
            pixel_ratio: 3.0,
            user_agent: String::from(
                "Mozilla/5.0 (Linux; Android 14; SM-S911B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36",
            ),
            touch: true,
            mobile: true,
            landscape: false,
        },
        DeviceProfile {
            name: String::from("iPhone SE"),
            width: 375,
            height: 667,
            pixel_ratio: 2.0,
            user_agent: String::from(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            ),
            touch: true,
            mobile: true,
            landscape: false,
        },
        DeviceProfile {
            name: String::from("Tablet"),
            width: 768,
            height: 1024,
            pixel_ratio: 2.0,
            user_agent: String::from(
                "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            ),
            touch: true,
            mobile: false,
            landscape: false,
        },
        DeviceProfile {
            name: String::from("4K Display"),
            width: 3840,
            height: 2160,
            pixel_ratio: 1.0,
            user_agent: String::from(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            ),
            touch: false,
            mobile: false,
            landscape: true,
        },
    ]
});

pub fn get_device_by_name(name: &str) -> Result<DeviceProfile> {
    DEVICE_PRESETS
        .iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))
        .cloned()
        .ok_or_else(|| ChromeError::DeviceNotFound(name.to_string()))
}

pub fn load_custom_devices(path: Option<PathBuf>) -> Result<Vec<DeviceProfile>> {
    let devices_path = if let Some(p) = path {
        p
    } else {
        let config_dir = crate::config::default_config_dir()?;
        config_dir.join("devices.toml")
    };

    if !devices_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&devices_path)?;
    let wrapper: DevicesWrapper = toml::from_str(&content)?;

    for device in &wrapper.devices {
        device.validate()?;
    }

    Ok(wrapper.devices)
}

#[derive(Deserialize)]
struct DevicesWrapper {
    devices: Vec<DeviceProfile>,
}

pub fn list_all_devices(include_custom: bool) -> Result<Vec<DeviceProfile>> {
    let mut all_devices = DEVICE_PRESETS.to_vec();

    if include_custom {
        let custom = load_custom_devices(None)?;
        all_devices.extend(custom);
    }

    Ok(all_devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_presets_count() {
        assert_eq!(DEVICE_PRESETS.len(), 8);
    }

    #[test]
    fn test_device_presets_contains_desktop() {
        let desktop = DEVICE_PRESETS.iter().find(|d| d.name == "Desktop");
        assert!(desktop.is_some());

        let desktop = desktop.unwrap();
        assert_eq!(desktop.width, 1920);
        assert_eq!(desktop.height, 1080);
        assert!(!desktop.mobile);
        assert!(!desktop.touch);
    }

    #[test]
    fn test_device_presets_contains_iphone() {
        let iphone = DEVICE_PRESETS.iter().find(|d| d.name == "iPhone 14");
        assert!(iphone.is_some());

        let iphone = iphone.unwrap();
        assert!(iphone.mobile);
        assert!(iphone.touch);
        assert_eq!(iphone.pixel_ratio, 3.0);
    }

    #[test]
    fn test_get_device_by_name_found() {
        let device = get_device_by_name("Desktop").unwrap();
        assert_eq!(device.name, "Desktop");
    }

    #[test]
    fn test_get_device_by_name_case_insensitive() {
        let device = get_device_by_name("DESKTOP").unwrap();
        assert_eq!(device.name, "Desktop");

        let device = get_device_by_name("iphone 14").unwrap();
        assert_eq!(device.name, "iPhone 14");
    }

    #[test]
    fn test_get_device_by_name_not_found() {
        let result = get_device_by_name("NonExistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_device_profile_validate_valid() {
        let device = DeviceProfile {
            name: "Test".to_string(),
            width: 1024,
            height: 768,
            pixel_ratio: 2.0,
            user_agent: "Mozilla/5.0".to_string(),
            touch: false,
            mobile: false,
            landscape: true,
        };
        assert!(device.validate().is_ok());
    }

    #[test]
    fn test_device_profile_validate_invalid_dimensions() {
        let device = DeviceProfile {
            name: "Test".to_string(),
            width: 100,
            height: 768,
            pixel_ratio: 2.0,
            user_agent: "Mozilla/5.0".to_string(),
            touch: false,
            mobile: false,
            landscape: true,
        };
        assert!(device.validate().is_err());
    }

    #[test]
    fn test_device_profile_validate_invalid_pixel_ratio() {
        let device = DeviceProfile {
            name: "Test".to_string(),
            width: 1024,
            height: 768,
            pixel_ratio: 10.0,
            user_agent: "Mozilla/5.0".to_string(),
            touch: false,
            mobile: false,
            landscape: true,
        };
        assert!(device.validate().is_err());
    }

    #[test]
    fn test_device_profile_validate_empty_user_agent() {
        let device = DeviceProfile {
            name: "Test".to_string(),
            width: 1024,
            height: 768,
            pixel_ratio: 2.0,
            user_agent: "".to_string(),
            touch: false,
            mobile: false,
            landscape: true,
        };
        assert!(device.validate().is_err());
    }

    #[test]
    fn test_list_all_devices_presets_only() {
        let devices = list_all_devices(false).unwrap();
        assert_eq!(devices.len(), 8);
    }

    #[test]
    fn test_device_profile_serialization() {
        let device = DeviceProfile {
            name: "Test".to_string(),
            width: 1024,
            height: 768,
            pixel_ratio: 2.0,
            user_agent: "Mozilla/5.0".to_string(),
            touch: false,
            mobile: false,
            landscape: true,
        };

        let json = serde_json::to_string(&device).unwrap();
        assert!(json.contains("\"name\":\"Test\""));

        let parsed: DeviceProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, device.name);
        assert_eq!(parsed.width, device.width);
    }
}
