use serde::Serialize;

pub trait OutputFormatter {
    fn format_text(&self) -> String;
    fn format_json(&self, pretty: bool) -> crate::Result<String>;
}

pub fn print_output<T: OutputFormatter>(
    data: &T,
    as_json: bool,
    json_pretty: bool,
) -> crate::Result<()> {
    let output = if as_json {
        data.format_json(json_pretty)?
    } else {
        data.format_text()
    };

    println!("{}", output);
    Ok(())
}

pub fn to_json<T: Serialize>(data: &T, pretty: bool) -> crate::Result<String> {
    if pretty {
        Ok(serde_json::to_string_pretty(data)?)
    } else {
        Ok(serde_json::to_string(data)?)
    }
}

pub mod text {
    use colored::Colorize;

    pub fn success(msg: &str) -> String {
        format!("{} {}", "✓".green().bold(), msg)
    }

    pub fn error(msg: &str) -> String {
        format!("{} {}", "✗".red().bold(), msg)
    }

    pub fn warning(msg: &str) -> String {
        format!("{} {}", "⚠".yellow().bold(), msg)
    }

    pub fn info(msg: &str) -> String {
        format!("{} {}", "ℹ".blue().bold(), msg)
    }

    pub fn bullet(msg: &str) -> String {
        format!("  • {}", msg)
    }

    pub fn section(title: &str) -> String {
        format!("\n{}\n{}", title.bold(), "─".repeat(title.len()))
    }

    pub fn subsection(title: &str) -> String {
        format!("\n{}", title.bold())
    }

    pub fn key_value(key: &str, value: &str) -> String {
        format!("  {}: {}", key.bold(), value)
    }

    pub fn table_header(columns: &[&str]) -> String {
        let header = columns
            .iter()
            .map(|c| format!("{:20}", c.bold()))
            .collect::<Vec<_>>()
            .join(" ");
        let divider = "─".repeat(columns.len() * 21);
        format!("{}\n{}", header, divider)
    }

    pub fn table_row(values: &[String]) -> String {
        values
            .iter()
            .map(|v| format!("{:20}", v))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn truncate(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len - 3])
        }
    }

    pub fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    pub fn format_duration_ms(ms: u64) -> String {
        if ms >= 1000 {
            format!("{:.2}s", ms as f64 / 1000.0)
        } else {
            format!("{}ms", ms)
        }
    }
}

pub struct TableBuilder {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl TableBuilder {
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
        }
    }

    pub fn headers(mut self, headers: Vec<String>) -> Self {
        self.headers = headers;
        self
    }

    pub fn row(mut self, row: Vec<String>) -> Self {
        self.rows.push(row);
        self
    }

    pub fn build(self) -> String {
        let mut output = String::new();

        if !self.headers.is_empty() {
            output.push_str(&text::table_header(
                &self.headers.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            ));
            output.push('\n');
        }

        for row in self.rows {
            output.push_str(&text::table_row(&row));
            output.push('\n');
        }

        output
    }
}

impl Default for TableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(text::truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        let result = text::truncate("hello world this is a long string", 15);
        assert_eq!(result.len(), 15);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_format_bytes_bytes() {
        assert_eq!(text::format_bytes(500), "500 B");
    }

    #[test]
    fn test_format_bytes_kb() {
        assert_eq!(text::format_bytes(2048), "2.00 KB");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(text::format_bytes(1024 * 1024 * 5), "5.00 MB");
    }

    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(text::format_bytes(1024 * 1024 * 1024 * 2), "2.00 GB");
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(text::format_duration_ms(500), "500ms");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(text::format_duration_ms(2500), "2.50s");
    }

    #[test]
    fn test_to_json_not_pretty() {
        #[derive(Serialize)]
        struct TestData {
            name: String,
        }
        let data = TestData {
            name: "test".to_string(),
        };
        let json = to_json(&data, false).unwrap();
        assert!(!json.contains('\n'));
    }

    #[test]
    fn test_to_json_pretty() {
        #[derive(Serialize)]
        struct TestData {
            name: String,
        }
        let data = TestData {
            name: "test".to_string(),
        };
        let json = to_json(&data, true).unwrap();
        assert!(json.contains('\n'));
    }

    #[test]
    fn test_table_builder() {
        let table = TableBuilder::new()
            .headers(vec!["Name".to_string(), "Value".to_string()])
            .row(vec!["foo".to_string(), "bar".to_string()])
            .row(vec!["baz".to_string(), "qux".to_string()])
            .build();

        assert!(table.contains("Name"));
        assert!(table.contains("foo"));
        assert!(table.contains("baz"));
    }

    #[test]
    fn test_table_builder_default() {
        let builder = TableBuilder::default();
        let table = builder.build();
        assert!(table.is_empty());
    }

    #[test]
    fn test_success_message() {
        let msg = text::success("Operation completed");
        assert!(msg.contains("Operation completed"));
    }

    #[test]
    fn test_error_message() {
        let msg = text::error("Something failed");
        assert!(msg.contains("Something failed"));
    }

    #[test]
    fn test_key_value() {
        let msg = text::key_value("Port", "9222");
        assert!(msg.contains("Port"));
        assert!(msg.contains("9222"));
    }
}
