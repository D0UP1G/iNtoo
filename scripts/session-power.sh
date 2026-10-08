#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib/session-backend.sh"

action="${1:-}"
case "$action" in
    suspend|hibernate|poweroff|reboot) args=("$action" -i) ;;
    reboot-firmware) args=(reboot --firmware-setup) ;;
    *) echo 'Usage: session-power.sh <suspend|hibernate|poweroff|reboot|reboot-firmware>' >&2; exit 2 ;;
esac

if inir_uses_systemd; then
    exec systemctl "${args[@]}"
fi
if command -v loginctl >/dev/null 2>&1; then
    exec loginctl "${args[@]}"
fi
echo 'Power actions require elogind (loginctl) on OpenRC' >&2
exit 1
