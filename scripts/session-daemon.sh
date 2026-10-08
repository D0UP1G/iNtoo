#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib/session-backend.sh"

daemon="${1:-}"
action="${2:-start}"
shift 2 || exit 2
case "$daemon" in
    wlsunset) unit=inir-wlsunset.service ;;
    xembedsniproxy) unit=inir-xembedsniproxy.service ;;
    discover-overlay) unit=discover-overlay.service ;;
    *) echo "Unsupported session daemon: $daemon" >&2; exit 2 ;;
esac

case "$action" in
    stop)
        if inir_uses_systemd; then
            systemctl --user stop "$unit" >/dev/null 2>&1 || true
        fi
        if [[ "$daemon" == discover-overlay ]]; then
            pkill -u "$(id -u)" -f '(^|/)discover-overlay([[:space:]]|$)' >/dev/null 2>&1 || true
        else
            pkill -u "$(id -u)" -x "$daemon" >/dev/null 2>&1 || true
        fi
        ;;
    status)
        if inir_uses_systemd && systemctl --user is-active --quiet "$unit"; then
            exit 0
        fi
        if [[ "$daemon" == discover-overlay ]]; then
            pgrep -u "$(id -u)" -f '(^|/)discover-overlay([[:space:]]|$)' >/dev/null
        else
            pgrep -u "$(id -u)" -x "$daemon" >/dev/null
        fi
        ;;
    start)
        if [[ "$daemon" == xembedsniproxy ]]; then
            export QT_NO_XDG_DESKTOP_PORTAL=1 QT_QPA_PLATFORM=xcb
        fi
        if inir_uses_systemd; then
            if [[ "$daemon" == discover-overlay ]]; then
                exec systemctl --user start "$unit"
            fi
            args=(--user --quiet --collect --service-type=exec "--unit=$unit")
            if [[ "$daemon" == xembedsniproxy ]]; then
                args+=(--property=BindsTo=inir.service --property=After=inir.service
                    --property=Restart=on-failure --property=RestartSec=1s
                    --setenv=QT_NO_XDG_DESKTOP_PORTAL=1 --setenv=QT_QPA_PLATFORM=xcb)
            fi
            for name in WAYLAND_DISPLAY DISPLAY XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS NIRI_SOCKET; do
                [[ -z "${!name:-}" ]] || args+=("--setenv=$name=${!name}")
            done
            exec systemd-run "${args[@]}" -- "$daemon" "$@"
        fi
        exec "$daemon" "$@"
        ;;
    *) echo "Unsupported daemon action: $action" >&2; exit 2 ;;
esac
