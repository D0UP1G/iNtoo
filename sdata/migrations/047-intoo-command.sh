#!/usr/bin/env bash
# shellcheck shell=bash

MIGRATION_ID="047-intoo-command"
MIGRATION_TITLE="Rename launcher command to intoo"
MIGRATION_DESCRIPTION="Updates direct launcher calls in Niri bindings and startup; inir stays compatible."
MIGRATION_TARGET_FILE="${XDG_CONFIG_HOME:-$HOME/.config}/niri/config.kdl"
MIGRATION_REQUIRED=false

migration_check() {
    python3 "${REPO_ROOT:-.}/scripts/lib/rename-launcher.py" --check \
        --config-home "${XDG_CONFIG_HOME:-$HOME/.config}"
}

migration_preview() {
    printf '%s\n' '  Rename spawn and spawn-at-startup commands from inir to intoo.'
}

migration_apply() {
    python3 "${REPO_ROOT:-.}/scripts/lib/rename-launcher.py" \
        --config-home "${XDG_CONFIG_HOME:-$HOME/.config}"
}
