use crate::{Result, chrome::NetworkRequest, chrome::PageProvider, output};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct NetworkRequestList {
    pub total_count: usize,
    pub requests: Vec<NetworkRequest>,
}

impl output::OutputFormatter for NetworkRequestList {
    fn format_text(&self) -> String {
        use crate::output::text;

        let mut output = text::section("Network Requests");
        output.push_str(&format!(
            "\n{}\n\n",
            text::key_value("Total", &self.total_count.to_string())
        ));

        if self.requests.is_empty() {
            output.push_str("No requests captured\n");
            return output;
        }

        for req in &self.requests {
            let status = req
                .status
                .map(|s| s.to_string())
                .unwrap_or_else(|| "pending".to_string());

            output.push_str(&format!("  {} {} - {}\n", status, req.method, req.url));
        }

        output
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_list(
    provider: &impl PageProvider,
    domain_filter: Option<&str>,
    status_filter: Option<u16>,
) -> Result<NetworkRequestList> {
    let requests = provider
        .collectors()
        .network
        .get_requests_filtered(domain_filter, status_filter)?;

    Ok(NetworkRequestList {
        total_count: requests.len(),
        requests,
    })
}
