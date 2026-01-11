use crate::chrome::event_store::EventMetadata;
use crate::{ChromeError, Result, config::FilterConfig};
use chromiumoxide::{
    Page,
    cdp::browser_protocol::network::{
        EnableParams as NetworkEnableParams, EventRequestWillBeSent, EventResponseReceived,
        GetResponseBodyParams,
    },
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use url::Url;

use super::super::storage::SessionStorage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRequest {
    pub id: String,
    pub url: String,
    pub method: String,
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub resource_type: Option<String>,
    pub mime_type: Option<String>,
    pub request_headers: Option<serde_json::Value>,
    pub response_headers: Option<serde_json::Value>,
    pub response_body: Option<String>,
    pub response_size: Option<i64>,
    pub timestamp: DateTime<Utc>,
}

impl EventMetadata for NetworkRequest {
    fn event_type(&self) -> &'static str {
        "network"
    }
    fn timestamp_ms(&self) -> Option<u64> {
        Some(self.timestamp.timestamp_millis() as u64)
    }
}

#[derive(Debug, Clone)]
struct PendingRequest {
    id: String,
    url: String,
    method: String,
    resource_type: Option<String>,
    request_headers: Option<serde_json::Value>,
    timestamp: DateTime<Utc>,
}

pub struct NetworkCollector {
    storage: Arc<SessionStorage>,
    pending: Arc<RwLock<HashMap<String, PendingRequest>>>,
    filter_config: FilterConfig,
}

impl NetworkCollector {
    pub fn new(storage: Arc<SessionStorage>, filter_config: FilterConfig) -> Self {
        Self {
            storage,
            pending: Arc::new(RwLock::new(HashMap::new())),
            filter_config,
        }
    }

    pub async fn attach(&self, page: &Arc<Page>) -> Result<()> {
        page.execute(NetworkEnableParams::default())
            .await
            .map_err(|e| ChromeError::General(format!("Failed to enable Network domain: {}", e)))?;

        let pending = self.pending.clone();
        let filter_config = self.filter_config.clone();

        let mut request_stream = page
            .event_listener::<EventRequestWillBeSent>()
            .await
            .map_err(|e| {
                ChromeError::General(format!("Failed to attach network listener: {}", e))
            })?;

        tokio::spawn(async move {
            while let Some(event) = request_stream.next().await {
                let url = &event.request.url;
                let resource_type = event.r#type.as_ref().map(|t| format!("{:?}", t));

                if !should_collect_request(url, resource_type.as_deref(), &filter_config) {
                    continue;
                }

                let pending_req = PendingRequest {
                    id: event.request_id.inner().to_string(),
                    url: url.clone(),
                    method: event.request.method.clone(),
                    resource_type,
                    request_headers: Some(event.request.headers.inner().clone()),
                    timestamp: Utc::now(),
                };

                pending
                    .write()
                    .await
                    .insert(pending_req.id.clone(), pending_req);
            }
        });

        let storage = self.storage.clone();
        let pending = self.pending.clone();
        let page_clone = page.clone();
        let max_body_size = self.filter_config.network_max_body_size;

        let mut response_stream = page
            .event_listener::<EventResponseReceived>()
            .await
            .map_err(|e| {
                ChromeError::General(format!("Failed to attach response listener: {}", e))
            })?;

        tokio::spawn(async move {
            while let Some(event) = response_stream.next().await {
                let request_id = event.request_id.clone();
                let request_id_str = request_id.inner().to_string();

                if let Some(pending_req) = pending.write().await.remove(&request_id_str) {
                    let response = &event.response;
                    let mime_type = response.mime_type.clone();

                    let response_body = if should_capture_body(&mime_type) {
                        fetch_response_body(&page_clone, request_id.clone(), max_body_size).await
                    } else {
                        None
                    };

                    let request = NetworkRequest {
                        id: pending_req.id,
                        url: pending_req.url,
                        method: pending_req.method,
                        status: Some(response.status as u16),
                        status_text: Some(response.status_text.clone()),
                        resource_type: pending_req.resource_type,
                        mime_type: Some(mime_type),
                        request_headers: pending_req.request_headers,
                        response_headers: Some(response.headers.inner().clone()),
                        response_body,
                        response_size: Some(response.encoded_data_length as i64),
                        timestamp: pending_req.timestamp,
                    };

                    storage.append("network", &request).ok();
                }
            }
        });

        Ok(())
    }

    pub fn get_requests(&self) -> Result<Vec<NetworkRequest>> {
        self.storage.read_all("network")
    }

    pub fn get_requests_filtered(
        &self,
        domain: Option<&str>,
        status: Option<u16>,
    ) -> Result<Vec<NetworkRequest>> {
        let requests: Vec<NetworkRequest> = self.storage.read_all("network")?;

        Ok(requests
            .into_iter()
            .filter(|r| {
                let domain_match = domain.map(|d| r.url.contains(d)).unwrap_or(true);
                let status_match = status.map(|s| r.status == Some(s)).unwrap_or(true);
                domain_match && status_match
            })
            .collect())
    }

    pub fn count(&self) -> usize {
        self.storage.count("network")
    }
}

fn should_collect_request(url: &str, resource_type: Option<&str>, config: &FilterConfig) -> bool {
    if url.starts_with("chrome-extension://") {
        return false;
    }

    if let Some(res_type) = resource_type
        && config
            .network_exclude_types
            .iter()
            .any(|t| t.eq_ignore_ascii_case(res_type))
    {
        return false;
    }

    if let Ok(parsed) = Url::parse(url)
        && let Some(domain) = parsed.host_str()
        && config
            .network_exclude_domains
            .iter()
            .any(|d| domain.contains(d))
    {
        return false;
    }

    true
}

fn should_capture_body(mime_type: &str) -> bool {
    mime_type.contains("json")
        || mime_type.contains("text")
        || mime_type.contains("xml")
        || mime_type.contains("javascript")
        || mime_type.contains("html")
}

async fn fetch_response_body(
    page: &Arc<Page>,
    request_id: chromiumoxide::cdp::browser_protocol::network::RequestId,
    max_body_size: usize,
) -> Option<String> {
    let params = GetResponseBodyParams::new(request_id);

    match page.execute(params).await {
        Ok(result) => {
            let body = &result.body;
            if body.len() > max_body_size {
                let truncated = body
                    .char_indices()
                    .take_while(|(i, _)| *i < max_body_size)
                    .map(|(_, c)| c)
                    .collect::<String>();
                Some(format!("{}... [truncated]", truncated))
            } else {
                Some(body.clone())
            }
        }
        Err(_) => None,
    }
}
