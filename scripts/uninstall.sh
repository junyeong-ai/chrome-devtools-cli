#!/usr/bin/env bash
set -e

BINARY_NAME="chrome-devtools-cli"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
CONFIG_DIR="$HOME/.config/chrome-devtools-cli"
USER_SKILL_DIR="$HOME/.claude/skills/chrome-devtools"

echo "Uninstalling Chrome DevTools CLI..."
echo ""

if [ -f "$INSTALL_DIR/$BINARY_NAME" ]; then
    rm "$INSTALL_DIR/$BINARY_NAME"
    echo "Removed: $INSTALL_DIR/$BINARY_NAME"
else
    echo "Binary not found"
fi

if [ -d "$USER_SKILL_DIR" ]; then
    echo ""
    echo "Claude Code Skill found: $USER_SKILL_DIR"
    read -p "Remove? [y/N]: " choice
    [[ "$choice" =~ ^[yY]$ ]] && { rm -rf "$USER_SKILL_DIR"; echo "Removed skill"; }
fi

if [ -d "$CONFIG_DIR" ]; then
    echo ""
    echo "Config directory: $CONFIG_DIR"
    ls -la "$CONFIG_DIR" 2>/dev/null | head -8
    echo ""
    read -p "Remove all config, sessions, Chrome for Testing, and extension? [y/N]: " choice
    [[ "$choice" =~ ^[yY]$ ]] && { rm -rf "$CONFIG_DIR"; echo "Removed config directory"; }
fi

echo ""
echo "Uninstallation complete!"
echo "To reinstall: ./scripts/install.sh"
