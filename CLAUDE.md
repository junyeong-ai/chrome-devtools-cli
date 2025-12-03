# Chrome DevTools CLI - AI Agent Developer Guide

Essential knowledge for implementing features and debugging this Rust CLI tool.

---

## Core Patterns

### BrowserSessionManager

**Implementation** (`chrome/session_manager.rs`):
```rust
// Browser lifecycle
manager.get_or_create_browser().await?;
manager.get_or_create_page().await?;

// Tab management
manager.new_page(url).await?;
manager.select_page(index).await?;
manager.close_page(index).await?;
```

**Why**: Central component managing browser lifecycle and session persistence.

**Session file**: `~/.config/chrome-devtools-cli/session.toml` stores `session_id`, `debug_port`, `active_page_url`. Used for `--keep-alive` browser reconnection.

---

### CollectorSet

**Mechanism** (`chrome/collectors/mod.rs`):
```rust
pub struct CollectorSet {
    pub network: NetworkCollector,
    pub console: ConsoleCollector,
    pub pageerror: PageErrorCollector,
    pub issues: IssuesCollector,
}
collectors.attach(&page).await?;  // CDP event subscription
```

**Why**: Unified collector management for CDP event capture.

**Storage**: NDJSON files in `~/.config/chrome-devtools-cli/sessions/{id}/`

---

### OutputFormatter

**Pattern**:
```rust
impl OutputFormatter for MyResult {
    fn format_text(&self) -> String { /* colored output */ }
    fn format_json(&self, pretty: bool) -> Result<String> { /* serde_json */ }
}
```

**CRITICAL**: All handler results must implement this trait for `--json` support.

---

## Development Tasks

### Add New Command

1. **cli/commands.rs**: Add Command enum variant
   ```rust
   #[command(about = "My command")]
   MyCommand { #[arg()] param: String },
   ```

2. **handlers/my_handler.rs**: Implement handler + result type

3. **cli/dispatch.rs**: Wire command to handler

4. **handlers/mod.rs**: Export module

---

### Add New Collector

1. **chrome/collectors/my_collector.rs**: Implement collector
2. **chrome/collectors/mod.rs**: Add to `CollectorSet`
3. Update `CollectorSet::attach()` for event subscription

---

### Add Config Field

**config.rs**: Add field to appropriate section with `#[serde(default)]`

---

## Common Issues

### Session Connection Lost

**Symptom**: `ConnectionLost` error on browser operations

**Cause**: Stale session file pointing to dead process

**Fix**: `session_manager.rs:connect_to_existing()` detects and triggers fresh launch

---

### Collector Not Capturing

**Symptom**: Empty network/console data despite page activity

**Cause**: CDP event subscription failed or page closed

**Fix**: Ensure `collectors.attach(&page)` called after page creation

---

### Page Not Found

**Symptom**: Session restored but operations fail

**Cause**: Session restored but pages array empty

**Fix**: `restore_pages_with_url()` creates new page with saved URL

---

## Key Constants

**Locations**:
- `session_manager.rs`: `SESSION_MAX_AGE_SECS = 3600`
- `config.rs`: Default port (9222), timeout (30s)
- `devices.rs`: 8 preset devices (`DEVICE_PRESETS`)
- `trace/analyzer.rs`: Core Web Vitals thresholds

**To modify**: Edit constant in source, or add to `Config` struct + `config.toml` for user configuration.

---

## File Paths

```
~/.config/chrome-devtools-cli/
├── chrome-for-testing/    # Auto-installed browser
├── chrome-profile/        # Browser profile data
├── extension/             # Chrome extension
├── sessions/{id}/         # Per-session NDJSON data
├── config.toml            # User configuration
└── session.toml           # Active session info
```

---

This guide contains only implementation-critical knowledge. For user documentation, see [README.md](README.md).
