#!/usr/bin/env bash

MIGRATION_ID="045-foot-terminal-to-ghostty"
MIGRATION_TITLE="Replace Foot terminal with Ghostty"
MIGRATION_DESCRIPTION="Moves the configured terminal and dock pin from Foot to Ghostty."
MIGRATION_TARGET_FILE="~/.config/inir/config.json"
MIGRATION_REQUIRED=true

migration_config_file() {
  local xdg_config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
  if [[ -f "${xdg_config_home}/inir/config.json" ]]; then
    printf '%s\n' "${xdg_config_home}/inir/config.json"
  elif [[ -f "${xdg_config_home}/illogical-impulse/config.json" ]]; then
    printf '%s\n' "${xdg_config_home}/illogical-impulse/config.json"
  else
    printf '%s\n' "${xdg_config_home}/inir/config.json"
  fi
}

migration_check() {
  local config
  config="$(migration_config_file)"
  [[ -f "$config" ]] || return 1
  command -v jq >/dev/null 2>&1 || return 1
  jq -e '(.apps.terminal == "foot") or ((.dock.pinnedApps // []) | index("foot") != null)' "$config" >/dev/null
}

migration_preview() {
  echo "  - replace apps.terminal=foot with apps.terminal=ghostty"
  echo "  - replace the legacy Foot dock pin with Ghostty"
}

migration_apply() {
  local config tmp
  config="$(migration_config_file)"
  migration_check || return 0
  tmp="$(mktemp "${config}.XXXXXX")" || return 1
  if ! jq 'if .apps.terminal == "foot" then .apps.terminal = "ghostty" else . end
    | if ((.dock.pinnedApps // []) | index("foot") != null)
      then .dock.pinnedApps |= map(if . == "foot" then "com.mitchellh.ghostty" else . end)
      else . end' "$config" > "$tmp"; then
    rm -f "$tmp"
    return 1
  fi
  chmod --reference="$config" "$tmp" 2>/dev/null || true
  mv "$tmp" "$config"
}
