use crate::server::protocol::{Request, Response};
use crate::{ChromeError, Result, timeouts::secs};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub struct DaemonClient {
    stream: UnixStream,
    timeout: Duration,
}

impl DaemonClient {
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket_path)
            .await
            .map_err(|e| ChromeError::Connection(format!("Failed to connect to daemon: {}", e)))?;

        Ok(Self {
            stream,
            timeout: Duration::from_secs(secs::REQUEST / 2),
        })
    }

    pub async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);
        let request = Request::new(id, method, params);

        let json = serde_json::to_string(&request)?;
        self.stream
            .write_all(format!("{}\n", json).as_bytes())
            .await
            .map_err(|e| ChromeError::General(format!("Write error: {}", e)))?;

        let (read_half, _) = self.stream.split();
        let mut reader = BufReader::new(read_half);
        let mut line = String::new();

        match tokio::time::timeout(self.timeout, reader.read_line(&mut line)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(ChromeError::General(format!("Read error: {}", e))),
            Err(_) => {
                return Err(ChromeError::General(
                    "RPC error -32001: Request timed out.".to_string(),
                ));
            }
        }

        let response: Response = serde_json::from_str(&line)?;

        if let Some(error) = response.error {
            return Err(ChromeError::General(format!(
                "RPC error {}: {}",
                error.code, error.message
            )));
        }

        Ok(response.result.unwrap_or(Value::Null))
    }

    pub async fn ping(&mut self) -> Result<bool> {
        let result = self.request("ping", Value::Null).await?;
        Ok(result
            .get("pong")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    pub async fn create_session(&mut self, headless: bool) -> Result<String> {
        self.create_session_with_profile(headless, None).await
    }

    pub async fn create_session_with_profile(
        &mut self,
        headless: bool,
        profile_directory: Option<String>,
    ) -> Result<String> {
        let mut params = serde_json::json!({ "headless": headless });
        if let Some(profile) = profile_directory {
            params["profile_directory"] = serde_json::Value::String(profile);
        }
        let result = self.request("session.create", params).await?;

        result
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ChromeError::General("Invalid response".to_string()))
    }

    /// Get or create user-profile session (auto-join/cleanup)
    pub async fn get_or_create_user_profile_session(&mut self, headless: bool) -> Result<String> {
        // Empty string triggers user-profile mode in daemon
        self.create_session_with_profile(headless, Some(String::new()))
            .await
    }

    pub async fn list_sessions(&mut self) -> Result<Vec<Value>> {
        let result = self.request("session.list", Value::Null).await?;
        Ok(result.as_array().cloned().unwrap_or_default())
    }

    pub async fn get_user_profile_session_id(&mut self) -> Result<Option<String>> {
        let sessions = self.list_sessions().await?;
        for s in sessions {
            if s.get("uses_user_profile").and_then(|v| v.as_bool()) == Some(true)
                && let Some(id) = s.get("session_id").and_then(|v| v.as_str())
            {
                return Ok(Some(id.to_string()));
            }
        }
        Ok(None)
    }

    pub async fn destroy_session(&mut self, session_id: &str) -> Result<()> {
        self.request(
            "session.destroy",
            serde_json::json!({"session_id": session_id}),
        )
        .await?;
        Ok(())
    }

    pub async fn navigate(&mut self, session_id: &str, url: &str) -> Result<Value> {
        self.request(
            "navigate",
            serde_json::json!({
                "session_id": session_id,
                "url": url
            }),
        )
        .await
    }

    pub async fn screenshot(&mut self, session_id: &str) -> Result<String> {
        let result = self
            .request("screenshot", serde_json::json!({"session_id": session_id}))
            .await?;

        result
            .get("data")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ChromeError::General("Invalid screenshot response".to_string()))
    }

    pub async fn shutdown_daemon(&mut self) -> Result<()> {
        self.request("shutdown", Value::Null).await?;
        Ok(())
    }
}

pub fn is_daemon_running(socket_path: &Path) -> bool {
    socket_path.exists() && std::os::unix::net::UnixStream::connect(socket_path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_daemon_running_false() {
        assert!(!is_daemon_running(&PathBuf::from("/tmp/nonexistent.sock")));
    }
}
