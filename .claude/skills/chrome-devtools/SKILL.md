---
name: chrome-devtools
version: 0.1.0
description: |
  Control Chrome browser via DevTools Protocol using chrome-devtools-cli.
  Take screenshots, navigate URLs, interact with page elements, analyze performance (Core Web Vitals),
  emulate devices, monitor network/console, handle dialogs, inspect DOM/accessibility.
  Use when asked to: screenshot, capture page, browse website, test responsive, check performance,
  click/fill forms, debug page, view network requests, check console logs.
  Keywords: screenshot, browser, Chrome, webpage, navigate, performance, LCP, CLS, TTFB,
  click, fill, type, viewport, mobile, tablet, network, console, DevTools, inspect, DOM, a11y.
allowed-tools: Bash
---

# Chrome DevTools CLI

Control Chrome browser via DevTools Protocol.

## Commands

```bash
# Navigate
chrome-devtools-cli navigate "<url>" [--wait-for load]
chrome-devtools-cli reload [--hard]
chrome-devtools-cli back | forward

# Screenshot & Export
chrome-devtools-cli screenshot -o <path> [--full-page] [--selector "<css>"]
chrome-devtools-cli screenshot -o <path> --format jpeg --quality 85
chrome-devtools-cli record -o <path> --duration <sec> [--mp4]
chrome-devtools-cli pdf -o <path> [--landscape]

# Interact
chrome-devtools-cli click "<selector>"
chrome-devtools-cli fill "<selector>" "<text>"
chrome-devtools-cli type "<selector>" "<text>" [--delay 50]
chrome-devtools-cli press <Key>
chrome-devtools-cli select "<selector>" --label "<option>"
chrome-devtools-cli scroll "<selector>"
chrome-devtools-cli hover "<selector>"

# Execute
chrome-devtools-cli eval "<js-expression>"
chrome-devtools-cli wait selector --selector "<css>" [--timeout 5000]

# Inspect
chrome-devtools-cli inspect "<selector>" [--all]
chrome-devtools-cli query "<selector>" [--count]
chrome-devtools-cli dom "<selector>" [--depth 3]
chrome-devtools-cli a11y [--interactable]
chrome-devtools-cli html [--selector "<css>"]

# Performance
chrome-devtools-cli trace "<url>" -o trace.json
chrome-devtools-cli analyze trace.json

# Device
chrome-devtools-cli emulate "iPhone 14"
chrome-devtools-cli viewport <width> <height> [--pixel-ratio 2]
chrome-devtools-cli devices

# Monitor
chrome-devtools-cli network [--domain <host>] [--status <code>]
chrome-devtools-cli console [--filter error] [--limit 20]

# Data
chrome-devtools-cli cookies list | get <name> | set <name> <value> | delete <name> | clear
chrome-devtools-cli storage list | get <key> | set <key> <value> [--session-storage]

# Dialog
chrome-devtools-cli dialog --accept [--text "<input>"]
chrome-devtools-cli dialog --dismiss

# Session
chrome-devtools-cli --keep-alive <command>  # persist browser
chrome-devtools-cli pages                    # list tabs
chrome-devtools-cli new-page [--url "<url>"]
chrome-devtools-cli select-page <index>
chrome-devtools-cli close-page <index>
chrome-devtools-cli stop                     # terminate

# History
chrome-devtools-cli sessions list
chrome-devtools-cli sessions network <id> [--status <code>]
chrome-devtools-cli sessions console <id> [--level error]
chrome-devtools-cli sessions export <id> --format playwright
```

## Patterns

**Multi-step session** (persist browser with `--keep-alive`):
```bash
chrome-devtools-cli --keep-alive navigate "https://example.com" --wait-for load
chrome-devtools-cli --keep-alive fill "#email" "user@test.com"
chrome-devtools-cli --keep-alive click "#submit"
chrome-devtools-cli --keep-alive screenshot -o result.png
chrome-devtools-cli stop
```

**Wait for dynamic content**:
```bash
chrome-devtools-cli navigate "<url>" --wait-for load
chrome-devtools-cli wait selector --selector "#dynamic-element"
chrome-devtools-cli click "#dynamic-element"
```

**Responsive test**:
```bash
chrome-devtools-cli emulate "iPhone 14"
chrome-devtools-cli navigate "<url>" --wait-for load
chrome-devtools-cli screenshot -o mobile.png --full-page
```

**Performance analysis**:
```bash
chrome-devtools-cli trace "https://example.com" -o trace.json
chrome-devtools-cli analyze trace.json
# Output: LCP 1.8s [Good] | FID 45ms [Good] | CLS 0.03 [Good]
```

## Output

All commands support `--json` for structured output.

## Error Handling

- **Connection failed**: Run `chrome-devtools-cli stop` then retry
- **Element not found**: Use `wait selector --selector "<css>"` before interaction
- **Blank screenshot**: Add `--wait-for load` to navigation
- **Session stale**: Delete `~/.config/chrome-devtools-cli/session.toml`
