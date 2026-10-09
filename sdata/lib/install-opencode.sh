# OpenCode 1.x CLI used by the desktop companion.
# shellcheck shell=bash

install_opencode() {
    local bin_dir="${XDG_BIN_HOME:-$HOME/.local/bin}"
    local prefix="${XDG_DATA_HOME:-$HOME/.local/share}/inir/opencode"
    if command -v opencode >/dev/null 2>&1 \
        || [[ -x "$bin_dir/opencode" ]] || [[ -x "$HOME/.opencode/bin/opencode" ]]; then
        return 0
    fi
    # Doctor's targeted repairs should only install the requested dependency.
    if [[ -n "${ONLY_MISSING_DEPS:-}" && " ${ONLY_MISSING_DEPS} " != *" opencode "* ]]; then
        return 0
    fi
    if [[ -e "$bin_dir/opencode" || -L "$bin_dir/opencode" ]]; then
        log_error "Cannot install OpenCode over existing $bin_dir/opencode; inspect it first."
        return 1
    fi
    if ${ask:-true}; then
        local answer
        read -r -p "Install OpenCode for the Win+S companion in your user directory? [Y/n] " answer
        [[ "$answer" =~ ^[Nn] ]] && return 0
    fi
    if ! command -v npm >/dev/null 2>&1; then
        local -a flags=()
        case "${OS_GROUP_ID:-generic}" in
            arch)
                ${ask:-true} || flags+=(--noconfirm)
                v pkg_sudo pacman -S --needed "${flags[@]}" npm || return 1
                ;;
            fedora)
                ${ask:-true} || flags+=(-y)
                v pkg_sudo dnf install "${flags[@]}" npm || return 1
                ;;
            debian|ubuntu)
                ${ask:-true} || flags+=(-y)
                v pkg_sudo apt-get install "${flags[@]}" npm || return 1
                ;;
            *)
                log_warning "OpenCode needs npm. Install Node.js/npm, then run: npm install -g opencode-ai@1"
                # Keep experimental distro installers usable; Doctor reports
                # this missing feature dependency for a later repair.
                return 0
                ;;
        esac
    fi
    mkdir -p "$bin_dir" "$prefix" || return 1
    # User-owned npm prefix; never execute a remote installer as root.
    npm install --global --prefix "$prefix" 'opencode-ai@1' || return 1
    [[ -x "$prefix/bin/opencode" ]] || {
        log_error "OpenCode installation did not provide an executable."
        return 1
    }
    ln -s "$prefix/bin/opencode" "$bin_dir/opencode" || return 1
    log_success "OpenCode installed. Connect a provider with: opencode auth login"
}
