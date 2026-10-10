# Install the official Ollama Linux runtime for the desktop companion.
# shellcheck shell=bash

install_ollama() {
    if command -v ollama >/dev/null 2>&1; then
        return 0
    fi
    # Doctor's targeted repair should not install unrelated dependencies.
    if [[ -n "${ONLY_MISSING_DEPS:-}" && " ${ONLY_MISSING_DEPS} " != *" ollama "* ]]; then
        return 0
    fi
    if ${ask:-true}; then
        local answer
        read -r -p "Install Ollama for the local Super+S assistant model? [Y/n] " answer
        [[ "$answer" =~ ^[Nn] ]] && return 0
    fi
    if ! command -v curl >/dev/null 2>&1; then
        log_warning "Ollama needs curl. Install Ollama manually from https://ollama.com/download."
        return 0
    fi

    local installer
    installer="$(mktemp)" || return 1
    if ! curl -fsSL https://ollama.com/install.sh -o "$installer"; then
        rm -f "$installer"
        log_error "Could not download the official Ollama installer."
        return 1
    fi
    if ! bash "$installer"; then
        rm -f "$installer"
        log_error "Ollama installation failed. Install it manually, then rerun setup."
        return 1
    fi
    rm -f "$installer"
    if ! command -v ollama >/dev/null 2>&1; then
        log_error "Ollama installer finished, but the 'ollama' command is not on PATH."
        return 1
    fi
    log_success "Ollama installed. The assistant will download qwen3.5:0.8b on first use."
}
