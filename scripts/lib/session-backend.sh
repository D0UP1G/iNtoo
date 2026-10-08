#!/usr/bin/env bash

_INIR_SESSION_LIB_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

inir_uses_systemd() {
    case "${INIR_INIT_SYSTEM:-auto}" in
        systemd) command -v systemctl >/dev/null 2>&1 ;;
        openrc) return 1 ;;
        auto|"")
            command -v systemctl >/dev/null 2>&1 && {
                [[ -d /run/systemd/system ]] || [[ -S "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/systemd/private" ]]
            }
            ;;
        *) printf 'Invalid INIR_INIT_SYSTEM: %s\n' "$INIR_INIT_SYSTEM" >&2; return 1 ;;
    esac
}

inir_sync_niri_startup() {
    local action="${1:-enable}" launcher="${2:-${XDG_BIN_HOME:-$HOME/.local/bin}/inir}"
    local -a args=("$action" --config-home "${XDG_CONFIG_HOME:-$HOME/.config}" --launcher "$launcher")
    command -v gentoo-pipewire-launcher >/dev/null 2>&1 && args+=(--pipewire)
    python3 "$_INIR_SESSION_LIB_DIR/session-startup.py" "${args[@]}"
}

inir_session_state_path() {
    local dir="$1" runtime="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}" key
    [[ -d "$runtime" && -O "$runtime" && -w "$runtime" ]] || return 1
    key="$(printf '%s' "$dir" | sha256sum)" || return 1
    printf '%s/inir/session-%s' "$runtime" "${key%% *}"
}

inir_session_supervisor_pid() {
    local state pid started current
    local -a argv=()
    state="$(inir_session_state_path "$1")" || return 1
    [[ -f "$state.pid" ]] || return 1
    read -r pid started < "$state.pid" || return 1
    [[ "$pid" =~ ^[1-9][0-9]*$ && "$started" =~ ^[0-9]+$ && -O "/proc/$pid" ]] || return 1
    current="$(awk '{print $22}' "/proc/$pid/stat" 2>/dev/null)" || return 1
    [[ "$current" == "$started" ]] || return 1
    mapfile -d '' -t argv < "/proc/$pid/cmdline" || return 1
    [[ "${argv[1]:-}" == */session-supervisor.sh && "${argv[2]:-}" == "$1" ]] || return 1
    printf '%s\n' "$pid"
}

inir_escape_session_group() {
    local group="${INIR_SESSION_SHELL_PGID:-}" current
    [[ "$group" =~ ^[1-9][0-9]*$ ]] || return 0
    current="$(ps -o pgid= -p "$$" | tr -d ' ')"
    if [[ "$current" == "$group" ]]; then
        exec env INIR_INVOKED_FROM="${INIR_INVOKED_FROM:-${INVOKED_FROM:-$PWD}}" setsid bash "$@"
    fi
}

# Scopes are named only by thumbnail wrappers launched inside inir.service.
inir_thumbnail_scope_prefix() {
    local directory digest
    directory=$(readlink -f -- "$1") || return 1
    digest=$(printf '%s' "$directory" | sha256sum) || return 1
    printf 'inir-thumbnails-%s-' "${digest%% *}"
}

inir_thumbnail_cgroup_is_owned() {
    local line pattern="/${2:-no-owned-scope-}[0-9]+[.]scope(/|$)"
    while IFS= read -r line; do
        case "$line" in
            */inir.service|*/inir.service/*) return 0 ;;
        esac
        [[ -n "${2:-}" && "$line" =~ $pattern ]] && return 0
    done <<< "$1"
    return 1
}
