# Chrome DevTools CLI

[![Rust](https://img.shields.io/badge/rust-1.91.1%2B%20(2024%20edition)-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-0.1.0-blue?style=flat-square)](https://github.com/user/chrome-devtools-cli/releases)

> **🌐 [한국어](README.md)** | **English**

---

> **⚡ Browser automation CLI that controls Chrome from your terminal**
>
> - 📸 **Screenshots** (full page, element selection, PNG/JPEG/WebP)
> - 📊 **Performance Analysis** (Core Web Vitals: LCP, FID, CLS, TTFB)
> - 🖱️ **Input Automation** (click, typing, form fill, dialogs)
> - 🔄 **Session Persistence** (reuse browser connection across commands)

---

## ⚡ Quick Start (1 minute)

```bash
# 1. Install
git clone https://github.com/user/chrome-devtools-cli
cd chrome-devtools-cli
./scripts/install.sh

# 2. Start using! 🎉
chrome-devtools-cli navigate "https://example.com"
chrome-devtools-cli screenshot page.png
chrome-devtools-cli click "#button"
```

**Tip**: Use `--keep-alive` flag to reuse browser for faster consecutive operations.

---

## 🎯 Key Features

### Screenshots & Recording
```bash
# Screenshots
chrome-devtools-cli screenshot page.png                    # Viewport
chrome-devtools-cli screenshot full.png --full-page        # Full page
chrome-devtools-cli screenshot el.png --selector "#hero"   # Specific element

# Recording & Export
chrome-devtools-cli record -o video.mp4 --duration 10      # Screen recording
chrome-devtools-cli pdf -o page.pdf                        # PDF export
```

### Browser Automation
```bash
# Navigation
chrome-devtools-cli navigate "https://example.com" --wait-for load
chrome-devtools-cli reload --hard
chrome-devtools-cli back && chrome-devtools-cli forward

# Input
chrome-devtools-cli click "#login-button"
chrome-devtools-cli fill "#email" "user@example.com"
chrome-devtools-cli type "#search" "query" --delay 50
chrome-devtools-cli press Enter
chrome-devtools-cli select "#dropdown" --label "Option 1"

# Dialog handling
chrome-devtools-cli dialog --accept --text "input value"
```

### Performance Analysis
```bash
chrome-devtools-cli trace "https://example.com" -o trace.json
chrome-devtools-cli analyze trace.json
# Output: LCP 1.8s [Good] | FID 45ms [Good] | CLS 0.03 [Good] | TTFB 280ms [Good]
```

### Device Emulation
```bash
chrome-devtools-cli emulate "iPhone 14"
chrome-devtools-cli viewport 1920 1080 --pixel-ratio 2
chrome-devtools-cli devices  # List 8 presets
```

### Session Management
```bash
# Browser reuse
chrome-devtools-cli --keep-alive navigate "https://example.com"
chrome-devtools-cli --keep-alive screenshot page.png
chrome-devtools-cli stop

# Multi-tab
chrome-devtools-cli new-page --url "https://google.com"
chrome-devtools-cli pages
chrome-devtools-cli select-page 1
chrome-devtools-cli close-page 0
```

### DOM & Accessibility Inspection
```bash
chrome-devtools-cli inspect "#element" --all           # Element details
chrome-devtools-cli query "button" --count             # Selector match count
chrome-devtools-cli a11y --interactable                # Accessibility tree
chrome-devtools-cli dom "#container" --depth 3         # DOM tree
chrome-devtools-cli html --selector "#content"         # Extract HTML
```

### Data Collection & Debugging
```bash
chrome-devtools-cli network --domain api.example.com   # Network requests
chrome-devtools-cli console --filter error             # Console messages
chrome-devtools-cli eval "document.title"              # Execute JavaScript
chrome-devtools-cli cookies list                       # View cookies
chrome-devtools-cli storage get "token"                # localStorage
```

### Session Data Utilization
```bash
chrome-devtools-cli sessions list                              # Session list
chrome-devtools-cli sessions network <id> --status 500         # Error requests
chrome-devtools-cli sessions console <id> --level error        # Error logs
chrome-devtools-cli sessions export <id> --format playwright   # Script conversion
```

---

## 📦 Installation

### Method 1: Install Script (Recommended) ⭐

```bash
git clone https://github.com/user/chrome-devtools-cli
cd chrome-devtools-cli
./scripts/install.sh
```

The install script automatically:
- Builds and installs binary (`~/.local/bin/`)
- Downloads Chrome for Testing
- Creates default config file

### Method 2: Manual Build

```bash
git clone https://github.com/user/chrome-devtools-cli
cd chrome-devtools-cli
cargo build --release
cp target/release/chrome-devtools-cli ~/.local/bin/
```

**Requirements**: Rust 1.91.1+, curl, unzip

---

## ⚙️ Configuration

### Config File

**Location**: `~/.config/chrome-devtools-cli/config.toml`

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
chrome-devtools-cli config init   # Create default config
chrome-devtools-cli config show   # Show current config
chrome-devtools-cli config edit   # Edit in editor
chrome-devtools-cli config path   # Config file path
```

### Priority Order

```
CLI flags > Environment variables > Config file > Defaults
```

---

## 📚 Command Reference

| Command | Description | Example |
|---------|-------------|---------|
| `navigate <url>` | Navigate to URL | `chrome-devtools-cli navigate "https://example.com"` |
| `screenshot` | Take screenshot | `chrome-devtools-cli screenshot -o page.png --full-page` |
| `click <selector>` | Click element | `chrome-devtools-cli click "#button"` |
| `fill <selector> <text>` | Fill input field | `chrome-devtools-cli fill "#email" "user@test.com"` |
| `type <selector> <text>` | Type with delay | `chrome-devtools-cli type "#input" "hello" --delay 50` |
| `press <key>` | Press key | `chrome-devtools-cli press Enter` |
| `trace <url>` | Performance trace | `chrome-devtools-cli trace "https://example.com" -o trace.json` |
| `analyze <file>` | Analyze trace | `chrome-devtools-cli analyze trace.json` |
| `emulate <device>` | Device emulation | `chrome-devtools-cli emulate "iPhone 14"` |
| `eval <expr>` | Execute JavaScript | `chrome-devtools-cli eval "document.title"` |
| `wait <condition>` | Wait for condition | `chrome-devtools-cli wait selector --selector "#el"` |

### Common Options

| Option | Description | Scope |
|--------|-------------|-------|
| `--json` | JSON format output | All commands |
| `--keep-alive` | Keep browser session | All commands |
| `--headless=false` | Show browser window | All commands |
| `--port <PORT>` | Specify debug port | All commands |
| `--user-profile` | Persist user profile | All commands |

---

## 🔧 Troubleshooting

### Browser Connection Failed

```bash
chrome-devtools-cli stop
rm -f ~/.config/chrome-devtools-cli/session.toml
```

### Element Not Found

```bash
# Wait for page load
chrome-devtools-cli navigate "https://example.com" --wait-for load

# Wait for element
chrome-devtools-cli wait selector --selector "#element" --timeout 10000
```

### Reinstall Chrome for Testing

```bash
rm -rf ~/.config/chrome-devtools-cli/chrome-for-testing
./scripts/install.sh
```

---

## 🚀 Developer Guide

**Architecture, debugging, contribution guide**: See [CLAUDE.md](CLAUDE.md)

---

## 💬 Support

- **GitHub Issues**: [Report issues](https://github.com/user/chrome-devtools-cli/issues)
- **Developer docs**: [CLAUDE.md](CLAUDE.md)

---

<div align="center">

**🌐 [한국어](README.md)** | **English**

**Version 0.1.0** • Rust 2024 Edition

Made with ❤️ for automation

</div>
