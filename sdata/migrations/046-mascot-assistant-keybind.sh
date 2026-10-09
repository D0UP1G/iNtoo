#!/usr/bin/env bash
# shellcheck shell=bash

MIGRATION_ID="046-mascot-assistant-keybind"
MIGRATION_TITLE="Desktop companion shortcut"
MIGRATION_DESCRIPTION="Adds Super+S for the OpenCode companion when that shortcut is free."
MIGRATION_TARGET_FILE="~/.config/niri/config.d/70-binds.kdl"
MIGRATION_REQUIRED=false

migration_check() {
    local config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
    local config="$config_home/niri/config.kdl"
    [[ -f "$config" ]] || return 1
    python3 "${REPO_ROOT:-.}/scripts/ai/assistant-keybind.py" --check "$config"
}

migration_preview() {
    printf '%s\n' '  + Mod+S repeat=false { spawn "inir" "assistant" "toggle"; }'
}

migration_apply() {
    local config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
    migration_check || return 0
    python3 "${REPO_ROOT:-.}/scripts/ai/assistant-keybind.py" "$config_home/niri/config.kdl"
}
