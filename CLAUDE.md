# Chrome DevTools CLI - AI Agent Developer Guide

Rust CLI for Chrome automation via CDP. Daemon architecture, session-based event capture, Playwright export.

---

## Architecture

```
src/
├── main.rs                    # Entry point
├── cli/
│   ├── commands.rs            # Clap command definitions
│   └── dispatch.rs            # Command routing, session resolution
├── handlers/                  # Command implementations
│   ├── navigate.rs, click.rs, screenshot.rs, ...
│   ├── trace.rs               # trace <url> command + trace analysis
│   ├── sessions.rs            # History queries (events, network, console)
│   └── export.rs              # Playwright script generation
├── server/
│   ├── daemon.rs              # Unix socket RPC handlers
│   ├── session_pool.rs        # Session lifecycle, browser management
│   └── http.rs                # HTTP API for extension
├── chrome/
│   ├── collectors/            # CDP event capture
│   │   ├── network.rs         # Request/response capture
│   │   ├── console.rs         # Console messages
│   │   ├── extension.rs       # User action events (click, input, scroll)
│   │   └── trace.rs           # CDP Tracing domain (browser-level)
│   └── storage.rs             # NDJSON session storage
└── client/
    └── connection.rs          # Daemon client

extension/src/
├── content/index.ts           # User action capture (DOM events)
├── popup/popup.ts             # Extension popup UI (Select, Record, Trace, Screenshot)
└── service-worker.ts          # Event forwarding to HTTP API, trace/recording state
```

---

## Key Patterns

### Command Dispatch Flow
```rust
// cli/dispatch.rs
dispatch() → handle_via_daemon() or handle_history_command()
  → DaemonClient::request("method", params)
  → server/daemon.rs handles RPC
  → handlers/*.rs implementations
```

### Event Capture Flow
```
User action → content/index.ts → service-worker.ts → HTTP POST /api/events
  → server/http.rs → ExtensionCollector → NDJSON file
```

### Trace Capture Flow
```
CLI: trace <url> → daemon.rs → TraceCollector.start() → CDP Tracing.start
  → navigate → Tracing.end → stream chunks → trace.ndjson

Extension: Start Trace → HTTP POST /api/trace/start → TraceCollector.start()
  → ... user interaction ... → HTTP POST /api/trace/stop → Tracing.end
```

Note: Tracing is browser-level (one trace per browser instance), not per-tab.

### Session Resolution
```rust
// cli/dispatch.rs - For --user-profile flag
resolve_session_id(session_id: Option<String>, user_profile: bool)
  → explicit session_id OR daemon query for user-profile session
```

### OutputFormatter Trait
```rust
// All handler results implement this for --json support
impl OutputFormatter for MyResult {
    fn format_text(&self) -> String { ... }
    fn format_json(&self, pretty: bool) -> Result<String> { ... }
}
```

---

## Common Tasks

### Add Command
1. `cli/commands.rs`: Add enum variant with `#[command]` and `#[arg]`
2. `handlers/new.rs`: Implement handler returning `impl OutputFormatter`
3. `cli/dispatch.rs`: Add match arm in appropriate handler function
4. `handlers/mod.rs`: Export module

### Add Extension Event
1. `extension/src/content/index.ts`: Capture event and `sendToCli({event_name: {...}})`
2. `chrome/collectors/extension.rs`: Add variant to `ExtensionEvent` enum (serde renames)

### Add Collector
1. `chrome/collectors/new.rs`: Implement with `append()` to storage
2. `chrome/collectors/mod.rs`: Add to `CollectorSet`, update `attach()`

---

## Storage

```
~/.config/chrome-devtools-cli/
├── sessions/{id}/              # Per-session data
│   ├── events.db               # SQLite database (all events unified)
│   ├── events.db-shm           # SQLite shared memory
│   ├── events.db-wal           # SQLite write-ahead log
│   ├── screenshots/            # Screenshot files
│   └── recordings/{rid}/       # Frame JPEGs + metadata.json
├── extension/                  # Built extension files
├── chrome-for-testing/         # Browser binary
└── config.toml                 # User config (user_data_dir for profile path)
```

**SQLite Schema** (`events.db`):
- `events` table: `id`, `event_type`, `data` (JSON), `timestamp_ms`
- Event types: `click`, `input`, `select`, `hover`, `scroll`, `keypress`, `screenshot`, `snapshot`, `dialog`, `navigate`, `network`, `console`, `error`, `trace`

---

## Extension Events

| Event | Trigger | Key Fields |
|-------|---------|------------|
| `click` | pointerdown/click | aria, css, xpath, rect, url, ts |
| `input` | focusout/beforeunload | aria, css, value, url, ts |
| `select` | select element change | aria, css, value, url, ts |
| `hover` | mouseenter (explicit) | aria, css, rect, url, ts |
| `scroll` | scroll (300ms debounce) | x, y, url, ts |
| `keypress` | keydown (Enter/Tab/Escape) | key, aria, css, url, ts |
| `screenshot` | extension capture | filename, url, ts |
| `snapshot` | DOM snapshot | html, url, ts |
| `dialog` | alert/confirm/prompt | ok (accept/dismiss), url, ts |
| `navigate` | load/pushState/popState | url, nav_type, ts |
| `recording_start/stop` | HTTP API | recording_id, ts |

---

## HTTP API Endpoints (Extension)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/session` | GET | Get active session info |
| `/api/events` | POST | User action events from extension |
| `/api/screenshots` | POST | Save screenshot from extension |
| `/api/recording/start` | POST | Start screen recording |
| `/api/recording/stop` | POST | Stop screen recording |
| `/api/recording/frame` | POST | Save recording frame |
| `/api/trace/start` | POST | Start CDP trace |
| `/api/trace/stop` | POST | Stop CDP trace |
| `/api/trace/status` | POST | Check trace status |

---

## Constants

| Location | Constant | Value |
|----------|----------|-------|
| `config.rs` | default port | 9222 |
| `config.rs` | navigation_timeout | 30s |
| `devices.rs` | DEVICE_PRESETS | 8 devices |
| `trace/analyzer.rs` | Core Web Vitals thresholds | LCP 2.5s, CLS 0.1, etc. |

---

## Debug

```bash
RUST_LOG=debug chrome-devtools-cli navigate "https://example.com"
```

**Extension not capturing**: Verify `~/.config/chrome-devtools-cli/extension/` has built files

**Build extension**: `cd extension && npm run build`
