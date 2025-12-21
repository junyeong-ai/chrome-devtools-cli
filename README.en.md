# Chrome DevTools CLI

[![Rust](https://img.shields.io/badge/rust-1.91.1%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![DeepWiki](https://img.shields.io/badge/DeepWiki-junyeong--ai%2Fchrome--devtools--cli-blue.svg?logo=data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACwAAAAyCAYAAAAnWDnqAAAAAXNSR0IArs4c6QAAA05JREFUaEPtmUtyEzEQhtWTQyQLHNak2AB7ZnyXZMEjXMGeK/AIi+QuHrMnbChYY7MIh8g01fJoopFb0uhhEqqcbWTp06/uv1saEDv4O3n3dV60RfP947Mm9/SQc0ICFQgzfc4CYZoTPAswgSJCCUJUnAAoRHOAUOcATwbmVLWdGoH//PB8mnKqScAhsD0kYP3j/Yt5LPQe2KvcXmGvRHcDnpxfL2zOYJ1mFwrryWTz0advv1Ut4CJgf5uhDuDj5eUcAUoahrdY/56ebRWeraTjMt/00Sh3UDtjgHtQNHwcRGOC98BJEAEymycmYcWwOprTgcB6VZ5JK5TAJ+fXGLBm3FDAmn6oPPjR4rKCAoJCal2eAiQp2x0vxTPB3ALO2CRkwmDy5WohzBDwSEFKRwPbknEggCPB/imwrycgxX2NzoMCHhPkDwqYMr9tRcP5qNrMZHkVnOjRMWwLCcr8ohBVb1OMjxLwGCvjTikrsBOiA6fNyCrm8V1rP93iVPpwaE+gO0SsWmPiXB+jikdf6SizrT5qKasx5j8ABbHpFTx+vFXp9EnYQmLx02h1QTTrl6eDqxLnGjporxl3NL3agEvXdT0WmEost648sQOYAeJS9Q7bfUVoMGnjo4AZdUMQku50McDcMWcBPvr0SzbTAFDfvJqwLzgxwATnCgnp4wDl6Aa+Ax283gghmj+vj7feE2KBBRMW3FzOpLOADl0Isb5587h/U4gGvkt5v60Z1VLG8BhYjbzRwyQZemwAd6cCR5/XFWLYZRIMpX39AR0tjaGGiGzLVyhse5C9RKC6ai42ppWPKiBagOvaYk8lO7DajerabOZP46Lby5wKjw1HCRx7p9sVMOWGzb/vA1hwiWc6jm3MvQDTogQkiqIhJV0nBQBTU+3okKCFDy9WwferkHjtxib7t3xIUQtHxnIwtx4mpg26/HfwVNVDb4oI9RHmx5WGelRVlrtiw43zboCLaxv46AZeB3IlTkwouebTr1y2NjSpHz68WNFjHvupy3q8TFn3Hos2IAk4Ju5dCo8B3wP7VPr/FGaKiG+T+v+TQqIrOqMTL1VdWV1DdmcbO8KXBz6esmYWYKPwDL5b5FA1a0hwapHiom0r/cKaoqr+27/XcrS5UwSMbQAAAABJRU5ErkJggg==)](https://deepwiki.com/junyeong-ai/chrome-devtools-cli)

> **English** | **[한국어](README.md)**

**Control Chrome completely from your terminal.** From screenshots to automation, performance analysis — do everything without a browser window.

---

## Why Chrome DevTools CLI?

- **Fast** — Daemon architecture reuses browser connections, millisecond command execution
- **Complete** — 30+ commands cover all Chrome features
- **Automation** — JSON output, event capture, Playwright script generation

---

## Quick Start

```bash
# Install
git clone https://github.com/anthropics/chrome-devtools-cli && cd chrome-devtools-cli
./scripts/install.sh

# Use
chrome-devtools-cli navigate "https://example.com" --user-profile
chrome-devtools-cli screenshot -o page.png
chrome-devtools-cli click "#button"
```

---

## Key Features

### Browser Automation
```bash
chrome-devtools-cli navigate "https://example.com"    # Navigate
chrome-devtools-cli click "#login"                    # Click element
chrome-devtools-cli fill "#email" "user@test.com"     # Fill input field
chrome-devtools-cli type "#search" "query" --delay 50 # Type with delay
chrome-devtools-cli press Enter                       # Key press
chrome-devtools-cli select "#dropdown" --label "Option"  # Dropdown select
```

### Screenshots & PDF
```bash
chrome-devtools-cli screenshot -o page.png                  # Viewport
chrome-devtools-cli screenshot -o full.png --full-page      # Full page
chrome-devtools-cli screenshot -o el.png --selector "#hero" # Specific element
chrome-devtools-cli pdf -o page.pdf                         # PDF export
```

### Session Recording & Event Query
```bash
# Start/stop recording via browser extension
chrome-devtools-cli history events --user-profile --last 10m
chrome-devtools-cli history recordings --user-profile
chrome-devtools-cli history export --user-profile --format playwright
```

### Performance Analysis
```bash
# Capture trace directly via CLI
chrome-devtools-cli trace "https://example.com" -o trace.ndjson

# Or start/stop trace via extension's Start Trace button

# Analyze trace (Core Web Vitals)
chrome-devtools-cli analyze trace.ndjson
# LCP 1.8s [Good] | CLS 0.03 [Good] | TTFB 280ms [Good]
```

### Device Emulation
```bash
chrome-devtools-cli emulate "iPhone 14"
chrome-devtools-cli viewport 1920 1080 --pixel-ratio 2
chrome-devtools-cli devices                           # List 8 presets
```

### Data Collection
```bash
chrome-devtools-cli network --domain api.example.com  # Network requests
chrome-devtools-cli console --filter error            # Console messages
chrome-devtools-cli eval "document.title"             # Execute JavaScript
chrome-devtools-cli cookies list                      # View cookies
```

---

## Installation

### Auto Install (Recommended)
```bash
git clone https://github.com/anthropics/chrome-devtools-cli && cd chrome-devtools-cli
./scripts/install.sh
```

### Source Build
```bash
git clone https://github.com/anthropics/chrome-devtools-cli && cd chrome-devtools-cli
cargo build --release
cp target/release/chrome-devtools-cli ~/.local/bin/
```

**Requirements**: Rust 1.91.1+

---

## Configuration

### Config File
`~/.config/chrome-devtools-cli/config.toml`:
```toml
[browser]
headless = true
port = 9222

[performance]
navigation_timeout_seconds = 30

[output]
default_screenshot_format = "png"
screenshot_quality = 90
```

### Config Commands
```bash
chrome-devtools-cli config init    # Create default config
chrome-devtools-cli config show    # Show current config
chrome-devtools-cli config edit    # Edit in editor
```

**Priority**: CLI options > Environment variables > Config file

---

## Command Reference

| Command | Description |
|---------|-------------|
| `navigate <url>` | Navigate to URL |
| `screenshot` | Take screenshot |
| `click <selector>` | Click element |
| `fill <selector> <text>` | Fill input field |
| `type <selector> <text>` | Type with delay |
| `press <key>` | Press key |
| `select <selector>` | Select dropdown |
| `trace <url>` | Capture trace during page load |
| `analyze <file>` | Analyze trace (Core Web Vitals) |
| `emulate <device>` | Device emulation |
| `eval <expr>` | Execute JavaScript |
| `history events` | Query events |
| `history export` | Generate Playwright script |

### Common Options
- `--json` — JSON output
- `--user-profile` — Persist user profile session
- `--headless=false` — Show browser window
- `--last <duration>` — Time filter (e.g., 10m, 2h)

---

## Server Mode

```bash
chrome-devtools-cli server start   # Start daemon
chrome-devtools-cli server status  # Check status
chrome-devtools-cli server stop    # Stop daemon
```

---

## Troubleshooting

### Browser Connection Failed
```bash
chrome-devtools-cli server stop
rm -f ~/.config/chrome-devtools-cli/session.toml
```

### Reinstall Chrome
```bash
./scripts/install.sh --reinstall-chrome
```

### Debug
```bash
RUST_LOG=debug chrome-devtools-cli navigate "https://example.com"
```

---

## Support

- [GitHub Issues](https://github.com/anthropics/chrome-devtools-cli/issues)
- [Developer Guide](CLAUDE.md)

---

<div align="center">

**English** | **[한국어](README.md)**

Made with Rust

</div>
