use crate::{ChromeError, Result};
use std::path::PathBuf;

pub fn find_chrome_executable() -> Result<PathBuf> {
    if let Some(path) = find_in_standard_locations()? {
        return Ok(path);
    }

    if let Some(path) = find_in_path() {
        return Ok(path);
    }

    Err(ChromeError::LaunchFailed(
        "Could not find Chrome/Chromium executable. Please specify with --chrome-path".into(),
    ))
}

#[cfg(target_os = "macos")]
fn find_in_standard_locations() -> Result<Option<PathBuf>> {
    if let Some(cft) = find_chrome_for_testing() {
        return Ok(Some(cft));
    }

    let paths = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ];

    for path in &paths {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(Some(p));
        }
    }

    Ok(None)
}

#[cfg(target_os = "macos")]
fn find_chrome_for_testing() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let config_dir = PathBuf::from(home).join(".config/chrome-devtools-cli");

    for pattern in ["chrome-for-testing", "chrome"] {
        let base = config_dir.join(pattern);
        if !base.exists() {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let cft_path = entry
                    .path()
                    .join("chrome-mac-arm64")
                    .join("Google Chrome for Testing.app")
                    .join("Contents/MacOS/Google Chrome for Testing");
                if cft_path.exists() {
                    return Some(cft_path);
                }

                let cft_x64 = entry
                    .path()
                    .join("chrome-mac-x64")
                    .join("Google Chrome for Testing.app")
                    .join("Contents/MacOS/Google Chrome for Testing");
                if cft_x64.exists() {
                    return Some(cft_x64);
                }
            }
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn find_in_standard_locations() -> Result<Option<PathBuf>> {
    let paths = [
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
    ];

    for path in &paths {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(Some(p));
        }
    }

    Ok(None)
}

#[cfg(target_os = "windows")]
fn find_in_standard_locations() -> Result<Option<PathBuf>> {
    let paths = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files\Chromium\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ];

    for path in &paths {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(Some(p));
        }
    }

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let user_chrome = PathBuf::from(&local_app_data)
            .join("Google")
            .join("Chrome")
            .join("Application")
            .join("chrome.exe");
        if user_chrome.exists() {
            return Ok(Some(user_chrome));
        }
    }

    Ok(None)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn find_in_standard_locations() -> Result<Option<PathBuf>> {
    Ok(None)
}

fn find_in_path() -> Option<PathBuf> {
    let binaries = if cfg!(windows) {
        vec!["chrome.exe", "chromium.exe"]
    } else {
        vec!["google-chrome", "chromium", "chromium-browser", "chrome"]
    };

    for binary in binaries {
        if let Ok(path) = which::which(binary) {
            return Some(path);
        }
    }

    None
}

pub mod signal {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    static SHUTDOWN: AtomicBool = AtomicBool::new(false);

    pub fn is_shutdown() -> bool {
        SHUTDOWN.load(Ordering::Relaxed)
    }

    pub fn set_shutdown() {
        SHUTDOWN.store(true, Ordering::Relaxed);
    }

    pub async fn setup_handlers() -> crate::Result<()> {
        let shutdown = Arc::new(AtomicBool::new(false));

        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};

            let mut sigint = signal(SignalKind::interrupt())?;
            let mut sigterm = signal(SignalKind::terminate())?;

            let shutdown_clone = shutdown.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = sigint.recv() => {
                        tracing::info!("Received SIGINT, shutting down...");
                        shutdown_clone.store(true, Ordering::Relaxed);
                        set_shutdown();
                    }
                    _ = sigterm.recv() => {
                        tracing::info!("Received SIGTERM, shutting down...");
                        shutdown_clone.store(true, Ordering::Relaxed);
                        set_shutdown();
                    }
                }
            });
        }

        #[cfg(windows)]
        {
            use tokio::signal::windows;

            let mut ctrl_c = windows::ctrl_c()?;
            let mut ctrl_break = windows::ctrl_break()?;

            let shutdown_clone = shutdown.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = ctrl_c.recv() => {
                        tracing::info!("Received Ctrl+C, shutting down...");
                        shutdown_clone.store(true, Ordering::Relaxed);
                        set_shutdown();
                    }
                    _ = ctrl_break.recv() => {
                        tracing::info!("Received Ctrl+Break, shutting down...");
                        shutdown_clone.store(true, Ordering::Relaxed);
                        set_shutdown();
                    }
                }
            });
        }

        Ok(())
    }
}
