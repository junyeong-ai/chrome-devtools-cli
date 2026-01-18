---
name: chrome-devtools
description: |
  Control Chrome browser via DevTools Protocol. Navigate, click, fill forms, take screenshots, capture performance traces, and record user sessions with automatic event capture.
  Use when asked to: browse URL, click button, fill form, take screenshot, capture trace, analyze performance, record user actions, debug webpage, check network/console, generate Playwright test, interact with user through browser, describe page elements, find elements by ref_id, AI-driven browser automation.
allowed-tools: Bash, Read
---

# Chrome DevTools CLI

Control Chrome via CDP with automatic user action capture through extension.

## Core Workflow

```
AI executes commands → Browser shows results → User interacts → Extension captures → AI analyzes
```

**Essential flag**: `--user-profile` maintains session state and enables event capture.

## Quick Start

```bash
# Navigate and interact
chrome-devtools-cli navigate "<url>" --user-profile --headless=false
chrome-devtools-cli click --selector "<css>" --user-profile
chrome-devtools-cli fill "<text>" --selector "<css>" --user-profile

# AI Agent: discover elements, then interact via ref_id
chrome-devtools-cli describe --interactable --user-profile   # → [i0], [f1], [n2]...
chrome-devtools-cli click --ref i0 --user-profile
chrome-devtools-cli fill "<text>" --ref f1 --user-profile

# Capture and analyze
chrome-devtools-cli screenshot -o page.png --user-profile
chrome-devtools-cli history events --user-profile --last 10m
```

## Element Selection

Two ways to target elements:

| Method | Flag | Example | Use Case |
|--------|------|---------|----------|
| CSS Selector | `--selector` | `--selector "#login"` | Known selectors |
| ref_id | `--ref` | `--ref i0` | From `describe` output |

**ref_id prefixes**: `i` (interactive), `f` (form), `n` (navigation), `m` (media), `t` (text), `c` (container)

## Commands

### Navigation & Pages
```bash
navigate "<url>" [--headless=false]    # Go to URL
reload | back | forward | stop         # Navigation controls
pages                                  # List open pages
select-page <index>                    # Switch page
new-page [--url "<url>"]               # Create page
close-page <index>                     # Close page
```

### Interaction
```bash
click --selector "<css>" | --ref <id>
fill "<text>" --selector "<css>" | --ref <id>
type "<text>" --selector "<css>" | --ref <id> [--delay 50]
hover --selector "<css>" | --ref <id>
scroll --selector "<css>" | --ref <id>
select --selector "<css>" [--label "<text>"]
press <Key>                            # Enter, Tab, Escape
dialog [--accept] [--text "<input>"]
wait "<selector>" [--timeout 5000]
```

### AI Agent (Element Discovery)
```bash
describe [--interactable|-i] [--form|-f] [--navigation|-n]
label -o labeled.png                   # Vision AI overlay
a11y [--interactable]                  # Accessibility tree
inspect "<selector>"                   # Element properties
```

### Capture & Analysis
```bash
screenshot -o <path> [--full-page] [--selector "<css>"]
pdf -o <path>
trace "<url>" -o trace.ndjson          # Performance trace
analyze <trace.ndjson>                 # Core Web Vitals
```

### History & Export
```bash
history events [--last 10m] [--type click|input|scroll|navigate]
history network [--domain <d>] [--status 404|500]
history console [--level error|warning]
history export -o test.spec.ts         # Playwright script
```

### DOM & Evaluation
```bash
eval "<js>"
query "<selector>" [--count]
html [--selector "<css>"]
```

### Device Emulation
```bash
emulate "iPhone 14"
viewport <w> <h> [--pixel-ratio 2]
devices                                # List presets
```

## Key Flags

| Flag | Purpose |
|------|---------|
| `--user-profile` | **Required** for persistent session + event capture |
| `--headless=false` | Show browser window for user interaction |
| `--json` | Machine-readable output |
| `--ref <id>` | Element by ref_id from describe |
| `--selector <css>` | Element by CSS selector |
| `--last <duration>` | Time filter: 5m, 1h, 1d |

## Extension Popup

When `--headless=false`, extension popup offers:
- **Select Element**: User picks element visually
- **Start Recording**: Capture frame snapshots
- **Start Trace**: CDP performance trace
- **Take Screenshot**: Instant capture

Query user actions: `history events --user-profile --last 5m --json`

## Event Types

| Type | Trigger | Key Data |
|------|---------|----------|
| `click` | Mouse click | aria, css, xpath, rect |
| `input` | Form field change | aria, css, value |
| `scroll` | Page scroll | x, y |
| `keypress` | Enter/Tab/Escape | key, aria, css |
| `navigate` | Page load/SPA | url, nav_type |

## Server & Session

```bash
server start | status | stop
session-info --user-profile
session list | create | destroy <id>
```

## Troubleshooting

| Issue | Fix |
|-------|-----|
| Events not captured | Use `--user-profile` + `--headless=false` |
| Connection failed | `server stop` then retry |
| Session stale | Delete `~/.config/chrome-devtools-cli/session.toml` |
| Browser stuck | `pkill -f "Chrome for Testing"` |
