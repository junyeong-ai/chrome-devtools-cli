use crate::chrome::PageProvider;
use crate::output::{self, OutputFormatter};
use crate::{ChromeError, Result};
use chromiumoxide::cdp::browser_protocol::network::{
    GetCookiesParams, SetCookieParams, TimeSinceEpoch,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: f64,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalStorageEntry {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginStorage {
    pub origin: String,
    pub local_storage: Vec<LocalStorageEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageState {
    pub cookies: Vec<PlaywrightCookie>,
    pub origins: Vec<OriginStorage>,
}

impl StorageState {
    pub fn new() -> Self {
        Self {
            cookies: Vec::new(),
            origins: Vec::new(),
        }
    }
}

impl Default for StorageState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize)]
pub struct AuthExportResult {
    pub output: Option<String>,
    pub cookies_count: usize,
    pub origins_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_state: Option<StorageState>,
}

impl OutputFormatter for AuthExportResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        let mut lines = vec![text::success("Auth state exported")];
        lines.push(text::key_value("Cookies", &self.cookies_count.to_string()));
        lines.push(text::key_value("Origins", &self.origins_count.to_string()));
        if let Some(ref path) = self.output {
            lines.push(text::key_value("Output", path));
        }
        lines.join("\n")
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        if let Some(ref state) = self.storage_state {
            if pretty {
                serde_json::to_string_pretty(state).map_err(Into::into)
            } else {
                serde_json::to_string(state).map_err(Into::into)
            }
        } else {
            output::to_json(self, pretty)
        }
    }
}

pub async fn handle_auth_export(
    provider: &impl PageProvider,
    output: Option<&Path>,
) -> Result<AuthExportResult> {
    let page = provider.get_or_create_page().await?;

    let current_url = page
        .url()
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "about:blank".to_string());

    let origin = extract_origin(&current_url);

    let cookies_response = page
        .execute(GetCookiesParams::default())
        .await
        .map_err(|e| ChromeError::General(format!("Failed to get cookies: {}", e)))?;

    let cookies: Vec<PlaywrightCookie> = cookies_response
        .cookies
        .clone()
        .into_iter()
        .map(|c| PlaywrightCookie {
            name: c.name,
            value: c.value,
            domain: c.domain,
            path: c.path,
            expires: c.expires,
            http_only: c.http_only,
            secure: c.secure,
            same_site: c
                .same_site
                .map(|s| format!("{:?}", s))
                .unwrap_or_else(|| "None".to_string()),
        })
        .collect();

    let local_storage = get_local_storage(&page).await.unwrap_or_default();

    let origins = if local_storage.is_empty() {
        Vec::new()
    } else {
        vec![OriginStorage {
            origin,
            local_storage,
        }]
    };

    let storage_state = StorageState {
        cookies: cookies.clone(),
        origins: origins.clone(),
    };

    let cookies_count = storage_state.cookies.len();
    let origins_count = storage_state.origins.len();

    if let Some(path) = output {
        let json = serde_json::to_string_pretty(&storage_state)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, json)?;

        Ok(AuthExportResult {
            output: Some(path.display().to_string()),
            cookies_count,
            origins_count,
            storage_state: None,
        })
    } else {
        Ok(AuthExportResult {
            output: None,
            cookies_count,
            origins_count,
            storage_state: Some(storage_state),
        })
    }
}

async fn get_local_storage(page: &chromiumoxide::Page) -> Result<Vec<LocalStorageEntry>> {
    let script = r#"(function() {
        const entries = [];
        for (let i = 0; i < localStorage.length; i++) {
            const key = localStorage.key(i);
            if (key) {
                entries.push({ name: key, value: localStorage.getItem(key) || '' });
            }
        }
        return entries;
    })()"#;

    let result = page
        .evaluate(script)
        .await
        .map_err(|e| ChromeError::EvaluationError(e.to_string()))?;

    let entries: Vec<LocalStorageEntry> = result.into_value().unwrap_or_default();
    Ok(entries)
}

fn extract_origin(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        format!(
            "{}://{}{}",
            parsed.scheme(),
            parsed.host_str().unwrap_or("localhost"),
            parsed.port().map(|p| format!(":{}", p)).unwrap_or_default()
        )
    } else {
        url.to_string()
    }
}

#[derive(Debug, Serialize)]
pub struct AuthImportResult {
    pub cookies_imported: usize,
    pub origins_imported: usize,
}

impl OutputFormatter for AuthImportResult {
    fn format_text(&self) -> String {
        use crate::output::text;
        text::success(&format!(
            "Imported {} cookies, {} origins",
            self.cookies_imported, self.origins_imported
        ))
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_auth_import(
    provider: &impl PageProvider,
    input: &Path,
) -> Result<AuthImportResult> {
    let content = fs::read_to_string(input)
        .map_err(|e| ChromeError::General(format!("Failed to read file: {}", e)))?;

    let storage_state: StorageState = serde_json::from_str(&content)
        .map_err(|e| ChromeError::General(format!("Invalid storageState format: {}", e)))?;

    let page = provider.get_or_create_page().await?;

    let mut cookies_imported = 0;
    for cookie in &storage_state.cookies {
        let mut params = SetCookieParams::builder()
            .name(&cookie.name)
            .value(&cookie.value)
            .domain(&cookie.domain)
            .path(&cookie.path)
            .secure(cookie.secure)
            .http_only(cookie.http_only);

        if cookie.expires > 0.0 {
            params = params.expires(TimeSinceEpoch::new(cookie.expires));
        }

        if let Ok(built) = params.build()
            && page.execute(built).await.is_ok() {
                cookies_imported += 1;
            }
    }

    let mut origins_imported = 0;
    for origin in &storage_state.origins {
        let entries_json = serde_json::to_string(
            &origin
                .local_storage
                .iter()
                .map(|e| (&e.name, &e.value))
                .collect::<HashMap<_, _>>(),
        )
        .unwrap_or_else(|_| "{}".to_string());

        let script = format!(
            r#"(function() {{
                const entries = {entries_json};
                for (const [key, value] of Object.entries(entries)) {{
                    localStorage.setItem(key, value);
                }}
            }})()"#
        );

        if page.evaluate(script.as_str()).await.is_ok() {
            origins_imported += 1;
        }
    }

    Ok(AuthImportResult {
        cookies_imported,
        origins_imported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_origin() {
        assert_eq!(
            extract_origin("https://example.com/path/to/page"),
            "https://example.com"
        );
        assert_eq!(
            extract_origin("http://localhost:3000/"),
            "http://localhost:3000"
        );
        assert_eq!(
            extract_origin("https://sub.example.com:8080/api"),
            "https://sub.example.com:8080"
        );
    }

    #[test]
    fn test_storage_state_serialization() {
        let state = StorageState {
            cookies: vec![PlaywrightCookie {
                name: "session".to_string(),
                value: "abc123".to_string(),
                domain: "example.com".to_string(),
                path: "/".to_string(),
                expires: 1234567890.0,
                http_only: true,
                secure: true,
                same_site: "Lax".to_string(),
            }],
            origins: vec![OriginStorage {
                origin: "https://example.com".to_string(),
                local_storage: vec![LocalStorageEntry {
                    name: "user_id".to_string(),
                    value: "123".to_string(),
                }],
            }],
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        assert!(json.contains("session"));
        assert!(json.contains("abc123"));
        assert!(json.contains("user_id"));
    }
}
