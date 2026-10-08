# OpenRC system service setup. Source without changing the system.
# shellcheck shell=bash

inir_openrc_enable_display_manager() {
    local root="${1:-}" config="${1:-}/etc/conf.d/display-manager" current override tmp
    [[ -x "$root/etc/init.d/display-manager" ]] || {
        log_warning "Install gui-libs/display-manager-init to enable SDDM on OpenRC"
        return 0
    }
    current="$(sed -nE 's/^[[:space:]]*DISPLAYMANAGER[[:space:]]*=[[:space:]]*"?([^"[:space:]#]+).*/\1/p' "$config" 2>/dev/null | tail -1)"
    override="$(sed -nE 's/^[[:space:]]*DISPLAYMANAGER[[:space:]]*=[[:space:]]*"?([^"[:space:]#]+).*/\1/p' "$root/etc/rc.conf" 2>/dev/null | tail -1)"
    current="${current//\'/}"
    override="${override//\'/}"
    current="${override:-$current}"
    if [[ -n "$current" && "$current" != sddm ]]; then
        log_info "Keeping OpenRC display manager: $current (choose SDDM in /etc/conf.d/display-manager)"
        return 0
    fi
    if [[ -z "$current" ]]; then
        tmp="$(mktemp)" || return 1
        [[ ! -f "$config" ]] || cat "$config" > "$tmp"
        printf '\nDISPLAYMANAGER="sddm"\n' >> "$tmp"
        v pkg_sudo install -d -m 0755 "$(dirname "$config")" || { rm -f "$tmp"; return 1; }
        v pkg_sudo install -m 0644 "$tmp" "$config" || { rm -f "$tmp"; return 1; }
        rm -f "$tmp"
    fi
    v pkg_sudo rc-update add display-manager default || return 1
    log_success "SDDM enabled for the next boot (the current login session is preserved)"
}

setup_openrc_services() {
    local root="${1:-}" service tmp group_id config changed=false
    tui_info "Setting up OpenRC services..."
    for service in dbus elogind; do
        if [[ ! -x "$root/etc/init.d/$service" ]]; then
            log_error "Required OpenRC service missing: $service. Install Gentoo dependencies before retrying."
            return 1
        fi
        v pkg_sudo rc-update add "$service" default || return 1
        v pkg_sudo rc-service "$service" start || return 1
    done

    tmp="$(mktemp)" || return 1
    printf 'i2c-dev\nuinput\n' > "$tmp"
    v pkg_sudo install -d -m 0755 "$root/etc/modules-load.d" || { rm -f "$tmp"; return 1; }
    v pkg_sudo install -m 0644 "$tmp" "$root/etc/modules-load.d/intoo.conf" || { rm -f "$tmp"; return 1; }
    rm -f "$tmp"
    for service in i2c-dev uinput; do
        v pkg_sudo modprobe "$service" || log_warning "Could not load $service; check your kernel configuration"
    done

    if [[ -x "$root/etc/init.d/ydotool" ]]; then
        config="$root/etc/conf.d/ydotool"
        if ! grep -Eq '^[[:space:]]*command_args=' "$config" 2>/dev/null; then
            group_id="$(getent group input | cut -d: -f3)"
            [[ "$group_id" =~ ^[0-9]+$ ]] || { log_error "The input group is required for ydotool"; return 1; }
            tmp="$(mktemp)" || return 1
            [[ ! -f "$config" ]] || cat "$config" > "$tmp"
            printf '\ncommand_args="--socket-path=/run/ydotoold.sock --socket-own=0:%s --socket-perm=0660"\n' "$group_id" >> "$tmp"
            v pkg_sudo install -d -m 0755 "$(dirname "$config")" || { rm -f "$tmp"; return 1; }
            v pkg_sudo install -m 0644 "$tmp" "$config" || { rm -f "$tmp"; return 1; }
            rm -f "$tmp"
            changed=true
        fi
        v pkg_sudo rc-update add ydotool default || return 1
        if [[ "$changed" == true ]]; then
            v pkg_sudo rc-service ydotool restart || return 1
        else
            v pkg_sudo rc-service ydotool start || return 1
        fi
    fi
    if [[ -x "$root/etc/init.d/bluetooth" ]] && command -v bluetoothctl >/dev/null 2>&1; then
        v pkg_sudo rc-update add bluetooth default || return 1
        v pkg_sudo rc-service bluetooth start || return 1
    fi
    if command -v sddm >/dev/null 2>&1; then
        inir_openrc_enable_display_manager "$root" || return 1
    fi
    log_success "OpenRC services configured"
}
