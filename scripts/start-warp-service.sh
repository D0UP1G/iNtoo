#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib/session-backend.sh"

if inir_uses_systemd; then
    exec systemctl start warp-svc.service
fi
exec pkexec rc-service warp-svc start
