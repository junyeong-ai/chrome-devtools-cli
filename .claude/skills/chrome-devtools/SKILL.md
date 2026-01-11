---
name: chrome-devtools
description: |
  Control Chrome browser via DevTools Protocol. Navigate, click, fill forms, take screenshots, capture performance traces, and record user sessions with automatic event capture.
  Use when asked to: browse URL, click button, fill form, take screenshot, capture trace, analyze performance, record user actions, debug webpage, check network/console, generate Playwright test, interact with user through browser.
allowed-tools: Bash, Read
---

# Chrome DevTools CLI

Control Chrome via CDP with automatic user action capture through extension.

## Core Concept: Interactive Browser Control

This CLI enables **bidirectional communication** between AI and user through the browser:

```
AI executes commands → Browser shows results → User interacts → Extension captures → AI analyzes
```

The `--user-profile` flag is essential: it maintains session state and enables the extension to capture user actions.

## Quick Reference

```bash
# Always use --user-profile for persistent session with event capture
chrome-devtools-cli navigate "<url>" --user-profile --headless=false
chrome-devtools-cli click "<selector>" --user-profile
chrome-devtools-cli fill "<selector>" "<text>" --user-profile
chrome-devtools-cli screenshot -o <path> --user-profile
chrome-devtools-cli history events --user-profile --last 10m
```

## Extension Popup UI

When `--headless=false`, user sees browser with extension popup offering:

| Button | Function | AI Receives |
|--------|----------|-------------|
| Select Element | User picks element visually | Selector info for next command |
| Start Recording | Capture frame snapshots | Recording ID for playback |
| Start Trace | CDP performance trace | trace.ndjson for analysis |
| Take Screenshot | Instant capture | Screenshot file |

**Key Pattern**: After user interacts via popup, query events to see what they did:
```bash
chrome-devtools-cli history events --user-profile --last 5m --json
```

## Interactive Workflow Pattern

### 1. Open Browser for User
```bash
chrome-devtools-cli navigate "https://example.com" --user-profile --headless=false
```

### 2. User Interacts (clicks, types, scrolls)
Extension automatically captures all actions.

### 3. Query What User Did
```bash
chrome-devtools-cli history events --user-profile --last 10m
```

### 4. Take Screenshot to Verify State
```bash
chrome-devtools-cli screenshot -o /tmp/current.png --user-profile
```

### 5. Continue Based on User Actions
Analyze events, execute next commands, repeat.

## Commands by Category

### Navigation
```bash
chrome-devtools-cli navigate "<url>" --user-profile [--headless=false]
chrome-devtools-cli reload --user-profile
chrome-devtools-cli back --user-profile
chrome-devtools-cli forward --user-profile
chrome-devtools-cli stop --user-profile
```

### Page Management
```bash
chrome-devtools-cli pages --user-profile              # List all open pages
chrome-devtools-cli select-page <index> --user-profile
chrome-devtools-cli new-page --user-profile [--url "<url>"]
chrome-devtools-cli close-page <index> --user-profile
```

### Interaction
```bash
chrome-devtools-cli click "<selector>" --user-profile
chrome-devtools-cli fill "<selector>" "<text>" --user-profile
chrome-devtools-cli type "<selector>" "<text>" --user-profile [--delay 50]
chrome-devtools-cli press <Key> --user-profile    # Enter, Tab, Escape
chrome-devtools-cli select "<selector>" --user-profile [--label "<text>"]
chrome-devtools-cli hover "<selector>" --user-profile
chrome-devtools-cli scroll "<selector>" --user-profile
chrome-devtools-cli dialog --user-profile [--accept] [--text "<input>"]
chrome-devtools-cli wait "<selector>" --user-profile [--timeout 5000]
```

### Capture
```bash
chrome-devtools-cli screenshot -o <path> --user-profile [--full-page] [--selector "<css>"]
chrome-devtools-cli pdf -o <path> --user-profile
chrome-devtools-cli trace "<url>" -o trace.ndjson    # Performance trace
```

### Query History
```bash
chrome-devtools-cli history events --user-profile [--last 10m] [--type click|input|keypress|scroll|navigate]
chrome-devtools-cli history network --user-profile [--domain <domain>] [--status 404|500]
chrome-devtools-cli history console --user-profile [--level error|warning]
chrome-devtools-cli history recordings --user-profile
```

### Analysis
```bash
chrome-devtools-cli analyze <trace.ndjson>    # Core Web Vitals analysis
chrome-devtools-cli history export --user-profile -o test.spec.ts    # Playwright export
```

### DOM & Evaluation
```bash
chrome-devtools-cli eval "<js>" --user-profile
chrome-devtools-cli inspect "<selector>" --user-profile
chrome-devtools-cli query "<selector>" --user-profile [--count]
chrome-devtools-cli html --user-profile [--selector "<css>"]
chrome-devtools-cli a11y --user-profile [--interactable]
```

### Device Emulation
```bash
chrome-devtools-cli emulate "iPhone 14" --user-profile
chrome-devtools-cli viewport <width> <height> --user-profile [--pixel-ratio 2]
chrome-devtools-cli devices    # List available presets
```

## Key Flags

| Flag | Purpose |
|------|---------|
| `--user-profile` | **Required** for persistent session + event capture |
| `--headless=false` | Show browser window for user interaction |
| `--json` | Machine-readable output |
| `--last <duration>` | Time filter: 5m, 1h, 1d |

## Event Types Captured

| Type | When | Key Data |
|------|------|----------|
| `click` | Mouse click | aria, css, xpath, rect, url, ts |
| `input` | Form field change | aria, css, value, url, ts |
| `select` | Dropdown selection | aria, css, value, url, ts |
| `hover` | Element hover | aria, css, rect, url, ts |
| `scroll` | Page scroll | x, y, url, ts |
| `keypress` | Enter/Tab/Escape | key, aria, css, url, ts |
| `screenshot` | Extension capture | filename, url, ts |
| `snapshot` | DOM snapshot | html, url, ts |
| `dialog` | Alert/confirm/prompt | ok, url, ts |
| `navigate` | Page load/SPA transition | url, nav_type, ts |

## Troubleshooting

| Issue | Fix |
|-------|-----|
| Events not captured | Use both `--user-profile` and `--headless=false` |
| Connection failed | `chrome-devtools-cli server stop` then retry |
| Session stale | Delete `~/.config/chrome-devtools-cli/session.toml` |
| Browser stuck | `pkill -f "Chrome for Testing"` |

## Server Management

```bash
chrome-devtools-cli server start
chrome-devtools-cli server status
chrome-devtools-cli server stop
```

## Session Management

```bash
chrome-devtools-cli session-info --user-profile    # Get current session details
chrome-devtools-cli session list                   # List all sessions (daemon mode)
chrome-devtools-cli session create                 # Create new session
chrome-devtools-cli session destroy <id>           # Destroy session
```
