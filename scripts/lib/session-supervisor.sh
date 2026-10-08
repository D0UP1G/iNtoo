#!/usr/bin/env bash
set -euo pipefail

source "$(dirname -- "${BASH_SOURCE[0]}")/session-backend.sh"
dir="$1"
qs_bin="$2"
[[ -n "${NIRI_SOCKET:-}" && -S "$NIRI_SOCKET" ]] || {
    echo 'A valid NIRI_SOCKET from the Niri session is required' >&2
    exit 1
}
state="$(inir_session_state_path "$dir")" || { echo 'A writable user XDG_RUNTIME_DIR is required' >&2; exit 1; }
mkdir -p "$(dirname "$state")"
chmod 0700 "$(dirname "$state")"
exec 9> "$state.lock"
flock -n 9 || exit 0
printf '%s %s\n' "$$" "$(awk '{print $22}' "/proc/$$/stat")" > "$state.pid"

child=""
stop_child_group() {
    [[ -n "$child" ]] || return 0
    kill -TERM -- "-$child" 2>/dev/null || true
    local deadline=$((SECONDS + 5))
    while kill -0 "$child" 2>/dev/null && (( SECONDS < deadline )); do
        sleep 0.1 9>&- &
        wait "$!" || true
    done
    sleep 0.2 9>&- &
    wait "$!" || true
    kill -KILL -- "-$child" 2>/dev/null || true
    wait "$child" 2>/dev/null || true
}
cleanup() {
    trap - EXIT TERM INT HUP
    if [[ -n "$child" ]]; then
        stop_child_group
    fi
    rm -f "$state.pid"
}
trap cleanup EXIT
trap 'exit 0' TERM INT HUP

wayland="${WAYLAND_DISPLAY:?WAYLAND_DISPLAY is required}"
[[ "$wayland" == /* ]] || wayland="${XDG_RUNTIME_DIR}/$wayland"
session_alive() {
    [[ -S "$wayland" && -n "${NIRI_SOCKET:-}" && -S "$NIRI_SOCKET" ]]
}

rapid=0
window=$SECONDS
while session_alive; do
    setsid bash -c 'export INIR_SESSION_SHELL_PGID=$$; exec "$@"' inir-session "$qs_bin" -n -p "$dir" 9>&- &
    child=$!
    while kill -0 "$child" 2>/dev/null; do
        session_alive || exit 0
        sleep 0.5 9>&- &
        wait "$!" || true
    done
    wait "$child" 2>/dev/null || true
    stop_child_group
    child=""
    session_alive || break
    if (( SECONDS - window >= ${INIR_RESTART_WINDOW:-60} )); then
        rapid=0
        window=$SECONDS
    fi
    rapid=$((rapid + 1))
    if (( rapid >= ${INIR_MAX_RAPID_RESTARTS:-5} )); then
        echo 'iNtoo stopped after repeated shell exits; inspect inir logs before restarting' >&2
        exit 1
    fi
    sleep 1 9>&- &
    wait "$!" || true
done
