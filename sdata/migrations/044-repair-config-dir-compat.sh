#!/usr/bin/env bash

MIGRATION_ID="044-repair-config-dir-compat"
MIGRATION_TITLE="Repair legacy config compatibility link"
MIGRATION_DESCRIPTION="Finishes the canonical config directory migration for installations where the legacy directory remained in place."
MIGRATION_TARGET_FILE="~/.config/inir/config.json"
MIGRATION_REQUIRED=true

source "$(dirname -- "${BASH_SOURCE[0]}")/../lib/config-dir-compat.sh"

migration_check() {
  local xdg_config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
  local config_legacy="${xdg_config_home}/illogical-impulse"
  [[ -d "$config_legacy" && ! -L "$config_legacy" ]]
}

migration_preview() {
  local xdg_config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
  echo "  - merge legacy config into ${xdg_config_home}/inir without replacing canonical files"
  echo "  - preserve the legacy tree as a recoverable backup and create a compatibility link"
}

migration_apply() {
  inir_config_compat_apply
}
