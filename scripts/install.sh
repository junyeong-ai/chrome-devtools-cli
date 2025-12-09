#!/usr/bin/env bash
set -e

BINARY_NAME="chrome-devtools-cli"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
CONFIG_DIR="$HOME/.config/chrome-devtools-cli"
REPO="anthropics/chrome-devtools-cli"
SKILL_NAME="chrome-devtools"
PROJECT_SKILL_DIR=".claude/skills/$SKILL_NAME"
USER_SKILL_DIR="$HOME/.claude/skills/$SKILL_NAME"

detect_platform() {
    local os=$(uname -s | tr '[:upper:]' '[:lower:]')
    local arch=$(uname -m)

    case "$os" in
        linux) os="unknown-linux-gnu" ;;
        darwin) os="apple-darwin" ;;
        *) echo "Unsupported OS: $os" >&2; exit 1 ;;
    esac

    case "$arch" in
        x86_64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
    esac

    echo "${arch}-${os}"
}

detect_chrome_platform() {
    case "$(uname -s)-$(uname -m)" in
        Darwin-arm64) echo "mac-arm64" ;;
        Darwin-x86_64) echo "mac-x64" ;;
        Linux-*) echo "linux64" ;;
        *) return 1 ;;
    esac
}

get_latest_version() {
    curl -sf "https://api.github.com/repos/$REPO/releases/latest" \
        | grep '"tag_name"' \
        | sed -E 's/.*"v([^"]+)".*/\1/' \
        || echo ""
}

download_binary() {
    local version="$1"
    local target="$2"
    local archive="${BINARY_NAME}-v${version}-${target}.tar.gz"
    local url="https://github.com/$REPO/releases/download/v${version}/${archive}"
    local checksum_url="${url}.sha256"

    echo "📥 Downloading $archive..." >&2
    if ! curl -fLO "$url" 2>&1 | grep -v "%" >&2; then
        echo "❌ Download failed" >&2
        return 1
    fi

    echo "🔐 Verifying checksum..." >&2
    if curl -fLO "$checksum_url" 2>&1 | grep -v "%" >&2; then
        if command -v sha256sum >/dev/null; then
            sha256sum -c "${archive}.sha256" >&2 || return 1
        elif command -v shasum >/dev/null; then
            shasum -a 256 -c "${archive}.sha256" >&2 || return 1
        else
            echo "⚠️  No checksum tool found, skipping verification" >&2
        fi
    fi

    echo "📦 Extracting..." >&2
    tar -xzf "$archive" 2>&1 | grep -v "x " >&2
    rm -f "$archive" "${archive}.sha256"

    echo "$BINARY_NAME"
}

build_from_source() {
    echo "🔨 Building from source..." >&2
    if ! cargo build --release 2>&1 | grep -E "Compiling|Finished|error" >&2; then
        echo "❌ Build failed" >&2
        exit 1
    fi
    echo "target/release/$BINARY_NAME"
}

install_binary() {
    local binary_path="$1"

    mkdir -p "$INSTALL_DIR"
    cp "$binary_path" "$INSTALL_DIR/$BINARY_NAME"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"

    if [[ "$OSTYPE" == "darwin"* ]]; then
        codesign --force --deep --sign - "$INSTALL_DIR/$BINARY_NAME" 2>/dev/null || true
    fi

    echo "✅ Installed to $INSTALL_DIR/$BINARY_NAME" >&2
}

install_chrome_for_testing() {
    local platform=$(detect_chrome_platform) || { echo "Unsupported platform" >&2; return 1; }
    local chrome_dir="$CONFIG_DIR/chrome-for-testing"

    if [ -d "$chrome_dir" ] && [ -n "$(ls -A "$chrome_dir" 2>/dev/null)" ]; then
        echo "✅ Chrome for Testing: $(ls -1 "$chrome_dir" | head -1)" >&2
        return 0
    fi

    echo "📥 Downloading Chrome for Testing ($platform)..." >&2
    local api_url="https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json"
    local json=$(curl -sL "$api_url")
    local version=$(echo "$json" | grep -o '"version":"[^"]*"' | head -1 | cut -d'"' -f4)
    local url=$(echo "$json" | grep -o "\"url\":\"[^\"]*${platform}[^\"]*chrome-${platform}.zip\"" | head -1 | cut -d'"' -f4)

    [ -z "$url" ] && { echo "❌ Download URL not found" >&2; return 1; }

    local install_dir="$chrome_dir/$version"
    mkdir -p "$install_dir"

    local tmp=$(mktemp -d)
    curl -sL "$url" -o "$tmp/chrome.zip" && unzip -q "$tmp/chrome.zip" -d "$install_dir"
    rm -rf "$tmp"

    [[ "$OSTYPE" == darwin* ]] && xattr -cr "$install_dir" 2>/dev/null || true
    echo "✅ Chrome for Testing $version installed" >&2
}

install_extension() {
    local src="extension/dist"
    local dst="$CONFIG_DIR/extension"

    if [ ! -d "$src" ]; then
        [ -f "extension/package.json" ] && command -v npm &>/dev/null && {
            echo "📦 Building extension..." >&2
            (cd extension && npm install && npm run build) >&2
        }
    fi

    [ ! -f "$src/manifest.json" ] && { echo "⚠️  Extension not found, skipping" >&2; return 0; }

    mkdir -p "$dst"
    rm -rf "$dst"/*
    cp -r "$src"/* "$dst/"
    echo "✅ Extension: $dst" >&2
}

create_default_config() {
    local config="$CONFIG_DIR/config.toml"
    [ -f "$config" ] && return 0
    mkdir -p "$CONFIG_DIR"
    cat > "$config" << 'EOF'
# Chrome DevTools CLI Configuration

[browser]
headless = true
port = 9222
window_width = 1280
window_height = 800
# chrome_path = "/path/to/chrome"
# user_data_dir = "/path/to/profile"
# profile_directory = "Default"
# extension_path = "/path/to/extension"
disable_web_security = false
reuse_browser = false

[performance]
navigation_timeout_seconds = 30
network_idle_timeout_ms = 2000
trace_categories = ["loading", "devtools.timeline", "blink.user_timing"]

[emulation]
default_device = "Desktop"

[network]
# proxy = "http://proxy:8080"
# user_agent = "Custom User Agent"

[output]
default_screenshot_format = "png"
screenshot_quality = 90
json_pretty = false

[dialog]
behavior = "dismiss"

[server]
cdp_port_range = [9222, 9299]
http_port_range = [9300, 9399]
ws_port_range = [9400, 9499]

[filters]
network_exclude_types = ["Image", "Stylesheet", "Font", "Media"]
network_exclude_domains = [
    "google-analytics.com",
    "googletagmanager.com",
    "doubleclick.net",
    "facebook.com",
    "facebook.net"
]
console_levels = ["error", "warn"]
network_max_body_size = 10000
EOF
    echo "✅ Config: $config" >&2
}

get_skill_version() {
    local skill_md="$1"
    [ -f "$skill_md" ] && grep "^version:" "$skill_md" 2>/dev/null | sed 's/version: *//' || echo "unknown"
}

compare_versions() {
    local ver1="$1" ver2="$2"
    [ "$ver1" = "$ver2" ] && { echo "equal"; return; }
    [ "$ver1" = "unknown" ] || [ "$ver2" = "unknown" ] && { echo "unknown"; return; }
    [ "$(printf '%s\n' "$ver1" "$ver2" | sort -V | head -n1)" = "$ver1" ] && echo "older" || echo "newer"
}

backup_skill() {
    local backup_dir="$USER_SKILL_DIR.backup_$(date +%Y%m%d_%H%M%S)"
    cp -r "$USER_SKILL_DIR" "$backup_dir"
    echo "   📦 Backup: $backup_dir" >&2
}

prompt_skill_installation() {
    [ ! -d "$PROJECT_SKILL_DIR" ] && return 0

    local project_version=$(get_skill_version "$PROJECT_SKILL_DIR/SKILL.md")

    echo "" >&2
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" >&2
    echo "🤖 Claude Code Skill: $SKILL_NAME (v$project_version)" >&2
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" >&2
    echo "" >&2

    if [ -d "$USER_SKILL_DIR" ]; then
        local existing_version=$(get_skill_version "$USER_SKILL_DIR/SKILL.md")
        local comparison=$(compare_versions "$existing_version" "$project_version")

        echo "Status: Installed (v$existing_version)" >&2
        echo "" >&2

        case "$comparison" in
            equal)
                echo "✅ Latest version" >&2
                read -p "Reinstall? [y/N]: " choice
                [[ "$choice" =~ ^[yY]$ ]] && { backup_skill; rm -rf "$USER_SKILL_DIR"; mkdir -p "$(dirname "$USER_SKILL_DIR")"; cp -r "$PROJECT_SKILL_DIR" "$USER_SKILL_DIR"; echo "   ✅ Reinstalled" >&2; }
                ;;
            older)
                echo "🔄 Update available: v$project_version" >&2
                read -p "Update? [Y/n]: " choice
                [[ ! "$choice" =~ ^[nN]$ ]] && { backup_skill; rm -rf "$USER_SKILL_DIR"; mkdir -p "$(dirname "$USER_SKILL_DIR")"; cp -r "$PROJECT_SKILL_DIR" "$USER_SKILL_DIR"; echo "   ✅ Updated" >&2; }
                ;;
            *)
                read -p "Reinstall? [y/N]: " choice
                [[ "$choice" =~ ^[yY]$ ]] && { backup_skill; rm -rf "$USER_SKILL_DIR"; mkdir -p "$(dirname "$USER_SKILL_DIR")"; cp -r "$PROJECT_SKILL_DIR" "$USER_SKILL_DIR"; echo "   ✅ Reinstalled" >&2; }
                ;;
        esac
    else
        echo "  [1] User-level install (recommended)" >&2
        echo "  [2] Project-level only" >&2
        echo "  [3] Skip" >&2
        echo "" >&2
        read -p "Choose [1-3] (default: 1): " choice

        case "$choice" in
            2) echo "✅ Using project-level skill" >&2 ;;
            3) echo "⏭️  Skipped" >&2 ;;
            *)
                mkdir -p "$(dirname "$USER_SKILL_DIR")"
                cp -r "$PROJECT_SKILL_DIR" "$USER_SKILL_DIR"
                echo "✅ Skill installed to ~/.claude/skills/" >&2
                ;;
        esac
    fi
}

main() {
    echo "🚀 Installing Chrome DevTools CLI..." >&2
    echo "" >&2

    local binary_path=""
    local target=$(detect_platform)
    local version=$(get_latest_version)

    if [ -n "$version" ] && command -v curl >/dev/null; then
        echo "Latest version: v$version" >&2
        echo "" >&2
        echo "  [1] Download prebuilt binary (recommended)" >&2
        echo "  [2] Build from source" >&2
        echo "" >&2
        read -p "Choose [1-2] (default: 1): " method

        case "$method" in
            2) binary_path=$(build_from_source) ;;
            *)
                binary_path=$(download_binary "$version" "$target") || {
                    echo "⚠️  Download failed, building from source" >&2
                    binary_path=$(build_from_source)
                }
                ;;
        esac
    else
        [ -z "$version" ] && echo "⚠️  Cannot fetch release, building from source" >&2
        binary_path=$(build_from_source)
    fi

    install_binary "$binary_path"
    echo "" >&2

    install_chrome_for_testing
    install_extension
    create_default_config
    echo "" >&2

    if echo "$PATH" | grep -q "$INSTALL_DIR"; then
        echo "✅ $INSTALL_DIR is in PATH" >&2
    else
        echo "⚠️  Add to PATH: export PATH=\"\$HOME/.local/bin:\$PATH\"" >&2
    fi
    echo "" >&2

    if command -v "$BINARY_NAME" &>/dev/null; then
        "$BINARY_NAME" --version >&2
    fi

    prompt_skill_installation

    echo "" >&2
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" >&2
    echo "🎉 Installation Complete!" >&2
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" >&2
    echo "" >&2
    echo "Quick start:" >&2
    echo "  $BINARY_NAME navigate \"https://example.com\"" >&2
    echo "  $BINARY_NAME screenshot page.png --full-page" >&2
    echo "  $BINARY_NAME --help" >&2
    echo "" >&2
}

main
