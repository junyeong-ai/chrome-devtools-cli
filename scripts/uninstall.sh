#!/usr/bin/env bash
set -e

BINARY_NAME="chrome-devtools-cli"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
CONFIG_DIR="$HOME/.config/chrome-devtools-cli"
SKILL_NAME="chrome-devtools"
USER_SKILL_DIR="$HOME/.claude/skills/$SKILL_NAME"

echo "🗑️  Uninstalling Chrome DevTools CLI..."
echo ""

# Binary
if [ -f "$INSTALL_DIR/$BINARY_NAME" ]; then
    rm "$INSTALL_DIR/$BINARY_NAME"
    echo "✅ Removed binary: $INSTALL_DIR/$BINARY_NAME"
else
    echo "⚠️  Binary not found"
fi
echo ""

# Skill
if [ -d "$USER_SKILL_DIR" ]; then
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "🤖 Claude Code Skill"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "Location: $USER_SKILL_DIR"
    echo ""
    read -p "Remove skill? [y/N]: " -n 1 -r
    echo ""

    if [[ $REPLY =~ ^[Yy]$ ]]; then
        read -p "Create backup? [Y/n]: " -n 1 -r
        echo ""

        if [[ ! $REPLY =~ ^[Nn]$ ]]; then
            backup_dir="$USER_SKILL_DIR.backup_$(date +%Y%m%d_%H%M%S)"
            cp -r "$USER_SKILL_DIR" "$backup_dir"
            echo "📦 Backup: $backup_dir"
        fi

        rm -rf "$USER_SKILL_DIR"
        echo "✅ Removed skill"

        [ -d "$HOME/.claude/skills" ] && [ -z "$(ls -A "$HOME/.claude/skills")" ] && rmdir "$HOME/.claude/skills"
    else
        echo "⏭️  Kept skill"
    fi
    echo ""
fi

# Config
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "⚙️  Configuration & Data"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

if [ -d "$CONFIG_DIR" ]; then
    echo "Location: $CONFIG_DIR"
    echo ""
    echo "Contents:"
    ls -1 "$CONFIG_DIR" 2>/dev/null | sed 's/^/  /'
    echo ""
    read -p "Remove all (config, sessions, Chrome for Testing, extension)? [y/N]: " -n 1 -r
    echo ""

    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf "$CONFIG_DIR"
        echo "✅ Removed: $CONFIG_DIR"
    else
        echo "⏭️  Kept configuration"
    fi
else
    echo "ℹ️  No configuration found"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ Uninstallation Complete!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Notes:"
echo "  • Project-level skill remains at .claude/skills/$SKILL_NAME"
echo "  • To reinstall: ./scripts/install.sh"
echo ""
