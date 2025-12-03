use crate::{
    Result,
    chrome::{ConsoleLevel, ConsoleMessage, PageProvider},
    output,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ConsoleOutput {
    pub messages: Vec<ConsoleMessage>,
    pub total_count: usize,
}

impl output::OutputFormatter for ConsoleOutput {
    fn format_text(&self) -> String {
        use crate::output::text;

        let mut output = text::section(&format!("Console Messages ({} total)", self.total_count));
        output.push('\n');

        for msg in &self.messages {
            let level_str = match msg.level {
                ConsoleLevel::Log => text::info("LOG"),
                ConsoleLevel::Debug => text::info("DEBUG"),
                ConsoleLevel::Info => text::info("INFO"),
                ConsoleLevel::Warning => text::warning("WARN"),
                ConsoleLevel::Error => text::error("ERROR"),
            };

            output.push_str(&format!("[{}] {}\n", level_str, msg.text));

            if let Some(ref url) = msg.url {
                output.push_str(&format!("  at {}:{}\n", url, msg.line.unwrap_or(0)));
            }
        }

        output
    }

    fn format_json(&self, pretty: bool) -> Result<String> {
        output::to_json(self, pretty)
    }
}

pub async fn handle_console(
    provider: &impl PageProvider,
    filter: Option<&str>,
    limit: Option<usize>,
) -> Result<ConsoleOutput> {
    let filter_level = filter.and_then(|f| f.parse::<ConsoleLevel>().ok());

    let mut messages = provider
        .collectors()
        .console
        .get_messages_filtered(filter_level)?;

    if let Some(limit_val) = limit {
        messages.truncate(limit_val);
    }

    Ok(ConsoleOutput {
        total_count: messages.len(),
        messages,
    })
}
