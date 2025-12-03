#!/usr/bin/env bash
set -e

BINARY_NAME="chrome-devtools-cli"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
CONFIG_DIR="$HOME/.config/chrome-devtools-cli"
SKILL_NAME="chrome-devtools"
PROJECT_SKILL_DIR=".claude/skills/$SKILL_NAME"
USER_SKILL_DIR="$HOME/.claude/skills/$SKILL_NAME"

detect_rust_target() {
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

install_chrome_for_testing() {
    local platform=$(detect_chrome_platform) || { echo "Unsupported platform" >&2; return 1; }
    local chrome_dir="$CONFIG_DIR/chrome-for-testing"

    if [ -d "$chrome_dir" ] && [ -n "$(ls -A "$chrome_dir" 2>/dev/null)" ]; then
        echo "Chrome for Testing: $(ls -1 "$chrome_dir" | head -1)" >&2
        return 0
    fi

    echo "Downloading Chrome for Testing ($platform)..." >&2
    local api_url="https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json"
    local json=$(curl -sL "$api_url")
    local version=$(echo "$json" | grep -o '"version":"[^"]*"' | head -1 | cut -d'"' -f4)
    local url=$(echo "$json" | grep -o "\"url\":\"[^\"]*${platform}[^\"]*chrome-${platform}.zip\"" | head -1 | cut -d'"' -f4)

    [ -z "$url" ] && { echo "Download URL not found" >&2; return 1; }

    local install_dir="$chrome_dir/${platform}-${version}"
    mkdir -p "$install_dir"

    local tmp=$(mktemp -d)
    curl -sL "$url" -o "$tmp/chrome.zip" && unzip -q "$tmp/chrome.zip" -d "$tmp"
    mv "$tmp"/chrome-*/* "$install_dir/" 2>/dev/null || mv "$tmp"/*/* "$install_dir/"
    rm -rf "$tmp"

    [[ "$OSTYPE" == darwin* ]] && xattr -cr "$install_dir" 2>/dev/null || true
    echo "Chrome for Testing $version installed" >&2
}

build_binary() {
    echo "Building..." >&2
    cargo build --release 2>&1 | grep -E "Compiling|Finished|error" >&2 || { echo "Build failed" >&2; exit 1; }
    echo "target/release/$BINARY_NAME"
}

install_binary() {
    mkdir -p "$INSTALL_DIR"
    cp "$1" "$INSTALL_DIR/$BINARY_NAME"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
    [[ "$OSTYPE" == darwin* ]] && codesign --force --deep --sign - "$INSTALL_DIR/$BINARY_NAME" 2>/dev/null || true
    echo "Binary: $INSTALL_DIR/$BINARY_NAME" >&2
}

install_extension() {
    local src="extension/dist"
    local dst="$CONFIG_DIR/extension"

    if [ ! -d "$src" ]; then
        [ -f "extension/package.json" ] && command -v npm &>/dev/null && {
            echo "Building extension..." >&2
            (cd extension && npm install && npm run build) >&2
        }
    fi

    [ ! -f "$src/manifest.json" ] && { echo "Extension not found, skipping" >&2; return 0; }

    mkdir -p "$dst"
    rm -rf "$dst"/*
    cp -r "$src"/* "$dst/"
    echo "Extension: $dst" >&2
}

create_default_config() {
    local config="$CONFIG_DIR/config.toml"
    [ -f "$config" ] && return 0
    mkdir -p "$CONFIG_DIR"
    cat > "$config" << 'EOF'
[browser]
headless = true

[performance]
navigation_timeout_seconds = 30

[output]
default_screenshot_format = "png"

[dialog]
behavior = "dismiss"
EOF
    echo "Config: $config" >&2
}

get_skill_version() {
    [ -f "$1" ] && grep "^version:" "$1" 2>/dev/null | sed 's/version: *//' || echo "unknown"
}

install_skill() {
    [ ! -d "$PROJECT_SKILL_DIR" ] && return 0

    local proj_ver=$(get_skill_version "$PROJECT_SKILL_DIR/SKILL.md")
    local user_ver=$(get_skill_version "$USER_SKILL_DIR/SKILL.md")

    echo "" >&2
    echo "Claude Code Skill: $SKILL_NAME" >&2

    if [ -d "$USER_SKILL_DIR" ]; then
        echo "  Installed: v$user_ver, Available: v$proj_ver" >&2
        read -p "  Update? [y/N]: " choice
        [[ "$choice" =~ ^[yY]$ ]] && { rm -rf "$USER_SKILL_DIR"; mkdir -p "$(dirname "$USER_SKILL_DIR")"; cp -r "$PROJECT_SKILL_DIR" "$USER_SKILL_DIR"; echo "  Updated!" >&2; }
    else
        read -p "  Install to ~/.claude/skills/? [Y/n]: " choice
        [[ ! "$choice" =~ ^[nN]$ ]] && { mkdir -p "$(dirname "$USER_SKILL_DIR")"; cp -r "$PROJECT_SKILL_DIR" "$USER_SKILL_DIR"; echo "  Installed!" >&2; }
    fi
}

main() {
    echo "Installing Chrome DevTools CLI..." >&2
    echo "Platform: $(detect_rust_target)" >&2
    echo "" >&2

    install_chrome_for_testing
    install_binary "$(build_binary)"
    install_extension
    create_default_config
    install_skill

    echo "$PATH" | grep -q "$INSTALL_DIR" || echo "" >&2 && echo "Add to PATH: export PATH=\"\$HOME/.local/bin:\$PATH\"" >&2

    echo "" >&2
    echo "Installation complete!" >&2
    echo "" >&2
    echo "Usage:" >&2
    echo "  $BINARY_NAME navigate \"https://example.com\"" >&2
    echo "  $BINARY_NAME screenshot page.png --full-page" >&2
    echo "" >&2
}

main
