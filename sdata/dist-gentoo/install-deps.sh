# Gentoo dependency installer for iNtoo.
# This file is sourced by sdata/subcmd-install/1.deps-router.sh.

# shellcheck shell=bash

if ! command -v emerge >/dev/null 2>&1; then
  log_error "Portage (emerge) was not found; install dependencies manually."
  return 1
fi

gentoo_emerge() {
  local -a flags=(--verbose)
  if [[ "${GENTOO_REBUILD_SESSION:-false}" == true ]]; then
    flags+=(--update --changed-use)
  else
    flags+=(--noreplace)
  fi
  if ${ask:-true}; then
    flags+=(--ask)
  else
    flags+=(--ask=n)
  fi
  v pkg_sudo emerge "${flags[@]}" "$@"
}

gentoo_configure_session_use() {
  local config_root config_dir use_path tmp
  config_root="$(portageq envvar PORTAGE_CONFIGROOT 2>/dev/null || true)"
  config_dir="${config_root:-/}"
  config_dir="${config_dir%/}/etc/portage"
  use_path="$config_dir/package.use"
  tmp="$(mktemp)" || return 1
  if inir_uses_systemd; then
    printf '%s\n' 'gui-wm/niri dbus screencast systemd' \
      'media-video/pipewire sound-server systemd -elogind' \
      'media-video/wireplumber systemd -elogind' > "$tmp"
  else
    printf '%s\n' 'gui-wm/niri dbus screencast -systemd' \
      'media-video/pipewire sound-server elogind -systemd -system-service' \
      'media-video/wireplumber elogind -systemd -system-service' \
      'sys-auth/elogind pam policykit' \
      'sys-auth/pambase elogind -systemd' > "$tmp"
  fi
  {
    printf '%s\n' 'gui-apps/quickshell sockets wayland layer-shell session-lock toplevel-management hyprland tray pipewire mpris pam upower notifications bluetooth networkmanager screencopy'
    # The Qt libraries can be installed without the QML plugins iNtoo imports.
    printf '%s\n' 'dev-qt/qt5compat gui qml' 'dev-qt/qtmultimedia qml'
    printf '%s\n' 'app-text/tessdata_fast l10n_en l10n_es l10n_ru l10n_ja l10n_zh'
    printf '%s\n' 'media-libs/lsp-plugins lv2'
  } >> "$tmp"
  if [[ -d "$use_path" || ! -e "$use_path" ]]; then
    v pkg_sudo install -d -m 0755 "$use_path" || { rm -f "$tmp"; return 1; }
    v pkg_sudo install -m 0644 "$tmp" "$use_path/intoo-session" || { rm -f "$tmp"; return 1; }
  else
    local merged
    merged="$(mktemp)" || { rm -f "$tmp"; return 1; }
    python3 - "$use_path" "$tmp" > "$merged" <<'PY'
from pathlib import Path
import sys
start = '# BEGIN iNtoo session USE flags'
end = '# END iNtoo session USE flags'
inside = False
for line in Path(sys.argv[1]).read_text().splitlines():
    if line == start:
        inside = True
    elif line == end:
        inside = False
    elif not inside:
        print(line)
if inside:
    raise SystemExit('Unclosed iNtoo USE block; repair package.use before retrying')
print(start)
print(Path(sys.argv[2]).read_text(), end='')
print(end)
PY
    if [[ $? -ne 0 ]]; then rm -f "$tmp" "$merged"; return 1; fi
    v pkg_sudo install -m 0644 "$merged" "$use_path" || { rm -f "$tmp" "$merged"; return 1; }
    rm -f "$merged"
  fi
  rm -f "$tmp"
}

gentoo_repair_guru_destination() {
  local repo_path backup
  command -v portageq >/dev/null 2>&1 || return 0

  repo_path="$(portageq get_repo_path / guru 2>/dev/null || true)"
  [[ -n "$repo_path" ]] || return 0
  repo_path="${repo_path%/}"
  if [[ -z "$repo_path" || "$repo_path" != /*/guru ]]; then
    log_error "GURU reports an unsafe sync path: ${repo_path:-/}. Repair it manually, then rerun setup."
    return 1
  fi
  [[ -e "$repo_path" ]] || return 0
  [[ -e "${repo_path}/.git" ]] && return 0

  # Only move a destination that Portage identified as GURU and whose final
  # path component is guru. Keep its contents intact for manual recovery.
  backup="${repo_path}.intoo-incomplete-$(date +%Y%m%d-%H%M%S)-$$"
  log_warning "GURU's sync directory is not a Git checkout; preserving it at ${backup} before syncing."
  v pkg_sudo mv -- "$repo_path" "$backup" || {
    log_error "Could not preserve the existing GURU directory at ${backup}."
    return 1
  }
}

gentoo_prepare_guru() {
  if ! command -v eselect >/dev/null 2>&1 \
      || ! eselect repository list >/dev/null 2>&1; then
    gentoo_emerge app-eselect/eselect-repository || return 1
  fi
  if ! eselect repository list >/dev/null 2>&1; then
    log_error "eselect repository is unavailable after installing eselect-repository"
    return 1
  fi

  # `list` without -i also shows repositories that are known but disabled.
  # Restrict the check to configured repositories so a disabled GURU entry is
  # enabled before we attempt to sync it.
  if ! eselect repository list -i 2>/dev/null | grep -Eq '(^|[[:space:]])guru([[:space:]]|$)'; then
    v pkg_sudo eselect repository enable guru || return 1
  fi

  if [[ "${SKIP_SYSUPDATE:-false}" != "true" ]]; then
    if ! command -v emaint >/dev/null 2>&1; then
      log_error "emaint was not found; sync GURU manually with 'emaint sync -r guru'."
      return 1
    fi
    gentoo_repair_guru_destination || return 1
    v pkg_sudo emaint sync -r guru || return 1
  fi
}

gentoo_allow_shell_keywords() {
  local keyword arch config_root config_dir accept_path tmp atom
  arch="$(portageq envvar ARCH 2>/dev/null || true)"
  [[ -n "$arch" ]] || arch="$(uname -m 2>/dev/null || true)"
  case "$arch" in
    amd64|x86_64) keyword="~amd64" ;;
    *)
      log_error "iNtoo's Niri GURU package currently supports ~amd64; found ${arch:-unknown}."
      return 1
      ;;
  esac

  config_root="$(portageq envvar PORTAGE_CONFIGROOT 2>/dev/null || true)"
  [[ -n "$config_root" ]] || config_root="/"
  config_dir="${config_root%/}/etc/portage"
  [[ "$config_dir" == "/etc/portage" || "$config_dir" == "//etc/portage" ]] || config_dir="${config_dir//\/\//\/}"
  accept_path="${config_dir}/package.accept_keywords"

  local -a atoms=(
    "gui-apps/quickshell" "gui-wm/niri" "gui-apps/fuzzel"
    "app-misc/cliphist" "app-misc/brightnessctl" "gui-apps/wtype"
    "gui-apps/awww" "sys-apps/mission-center"
  )
  if [[ -d "$accept_path" || ! -e "$accept_path" ]]; then
    tmp="$(mktemp)" || return 1
    [[ ! -f "${accept_path}/intoo" ]] || cat "${accept_path}/intoo" > "$tmp"
    for atom in "${atoms[@]}"; do
      grep -Fqx "$atom $keyword" "$tmp" || printf '%s %s\n' "$atom" "$keyword" >> "$tmp"
    done
    v pkg_sudo install -d -m 0755 "$accept_path" || { rm -f "$tmp"; return 1; }
    v pkg_sudo install -m 0644 "$tmp" "${accept_path}/intoo" || { rm -f "$tmp"; return 1; }
    rm -f "$tmp"
  else
    # Portage also supports a single package.accept_keywords file. Preserve it
    # and append only missing entries instead of replacing user configuration.
    for atom in "${atoms[@]}"; do
      if ! grep -Fqx "$atom $keyword" "$accept_path"; then
        printf '%s %s\n' "$atom" "$keyword" | pkg_sudo tee -a "$accept_path" >/dev/null || return 1
      fi
    done
  fi
}

gentoo_install_selected() {
  local GENTOO_REBUILD_SESSION=false
  local only_missing="${ONLY_MISSING_DEPS:-}"
  if [[ -n "$only_missing" ]]; then
    local -A cmd_to_atom=(
      [qs]="gui-apps/quickshell::guru" [niri]="gui-wm/niri::guru"
      [jq]="app-misc/jq" [rsync]="net-misc/rsync" [curl]="net-misc/curl"
      [git]="dev-vcs/git" [python3]="dev-lang/python" [fish]="app-shells/fish"
      [magick]="media-gfx/imagemagick" [grim]="gui-apps/grim"
      [slurp]="gui-apps/slurp" [wl-copy]="gui-apps/wl-clipboard"
      [wl-paste]="gui-apps/wl-clipboard" [fuzzel]="gui-apps/fuzzel::guru"
      [playerctl]="media-sound/playerctl" [notify-send]="x11-libs/libnotify"
      [wlsunset]="gui-apps/wlsunset" [swaylock]="gui-apps/swaylock"
      [swayidle]="gui-apps/swayidle" [brightnessctl]="app-misc/brightnessctl::guru"
      [wtype]="gui-apps/wtype::guru" [ydotool]="x11-misc/ydotool"
      [ffmpeg]="media-video/ffmpeg" [wf-recorder]="gui-apps/wf-recorder"
      [swappy]="gui-apps/swappy" [tesseract]="app-text/tesseract"
      [mpv]="media-video/mpv" [socat]="net-misc/socat"
      [upower]="sys-power/upower" [cava]="media-sound/cava"
      [quickshell-qml-modules]="gui-apps/quickshell::guru"
      [pipewire]="media-video/pipewire" [wireplumber]="media-video/wireplumber"
      [loginctl]="sys-auth/elogind" [dbus-run-session]="sys-apps/dbus"
      [nmcli]="net-misc/networkmanager" [wpctl]="media-video/wireplumber"
      [cliphist]="app-misc/cliphist::guru" [syntax-highlighting]="kde-frameworks/syntax-highlighting"
      [kirigami]="kde-frameworks/kirigami" [kdialog]="kde-apps/kdialog"
      [uv]="dev-python/uv" [easyeffects]="media-sound/easyeffects"
      [qalc]="sci-libs/libqalculate" [blueman-manager]="net-wireless/blueman"
      [ddcutil]="app-misc/ddcutil" [missioncenter]="sys-apps/mission-center::guru"
      [trans]="app-i18n/translate-shell"
      [flock]="sys-apps/util-linux" [xdg-settings]="x11-misc/xdg-utils"
      [yt-dlp]="net-misc/yt-dlp"
      [ocr-eng]="app-text/tessdata_fast" [ocr-spa]="app-text/tessdata_fast"
      [ocr-rus]="app-text/tessdata_fast" [ocr-jpn]="app-text/tessdata_fast"
      [ocr-jpn-vert]="app-text/tessdata_fast" [ocr-chi-sim]="app-text/tessdata_fast"
      [ocr-chi-sim-vert]="app-text/tessdata_fast" [ocr-chi-tra]="app-text/tessdata_fast"
      [ocr-chi-tra-vert]="app-text/tessdata_fast"
      [awww]="gui-apps/awww::guru" [awww-daemon]="gui-apps/awww::guru"
      [kwriteconfig6]="kde-frameworks/kconfig"
      [lsp-plugins-lv2]="media-libs/lsp-plugins"
    )
    local -a requested=() unresolved=()
    local cmd atom needs_guru=false needs_session=false needs_quickshell=false
    read -r -a _gentoo_missing_cmds <<< "$only_missing"
    for cmd in "${_gentoo_missing_cmds[@]}"; do
      atom="${cmd_to_atom[$cmd]:-}"
      if [[ -z "$atom" ]]; then
        log_warning "No Gentoo repair mapping for Doctor command: $cmd"
        unresolved+=("$cmd")
        continue
      fi
      [[ "$atom" == *"::guru" ]] && needs_guru=true
      case "$cmd" in niri|pipewire|wireplumber|wpctl|loginctl|ocr-*|lsp-plugins-lv2) needs_session=true ;; esac
      [[ "$cmd" == "quickshell-qml-modules" ]] && needs_quickshell=true
      [[ " ${requested[*]} " == *" ${atom} "* ]] || requested+=("$atom")
    done

    if [[ "$needs_session" == true ]]; then
      gentoo_configure_session_use || return 1
      GENTOO_REBUILD_SESSION=true
    fi

    if [[ "$needs_quickshell" == true ]]; then
      gentoo_configure_session_use || return 1
      GENTOO_REBUILD_SESSION=true
      requested+=(dev-qt/qt5compat dev-qt/qtmultimedia)
    fi

    if [[ "$needs_guru" == true ]]; then
      gentoo_prepare_guru || return 1
      gentoo_allow_shell_keywords || return 1
    fi
    if [[ ${#requested[@]} -gt 0 ]]; then
      gentoo_emerge "${requested[@]}" || return 1
    fi
    if [[ ${#unresolved[@]} -gt 0 ]]; then
      log_error "Could not repair Gentoo dependencies: ${unresolved[*]}"
      return 1
    fi
    return 0
  fi

  gentoo_prepare_guru || return 1
  gentoo_allow_shell_keywords || return 1
  gentoo_configure_session_use || return 1
  GENTOO_REBUILD_SESSION=true

  local -a core=(
    app-eselect/eselect-repository
    gui-wm/niri::guru gui-apps/quickshell::guru gui-apps/fuzzel::guru
    dev-qt/qt5compat dev-qt/qtmultimedia
    app-misc/cliphist::guru
    kde-frameworks/kirigami kde-frameworks/syntax-highlighting kde-apps/kdialog
    kde-plasma/plasma-integration
    app-misc/jq app-shells/fish dev-lang/python dev-vcs/git
    net-misc/curl net-misc/wget net-misc/rsync
    sys-devel/bc sys-apps/ripgrep x11-misc/xdg-utils
    gui-apps/wl-clipboard gui-apps/grim gui-apps/slurp
    gui-apps/swayidle gui-apps/swaylock gui-apps/wlsunset
    media-video/pipewire media-video/wireplumber media-sound/playerctl
    x11-libs/libnotify
    sys-apps/dbus sys-apps/util-linux sys-auth/polkit
    gnome-extra/polkit-gnome sys-apps/xdg-desktop-portal
    sys-apps/xdg-desktop-portal-gtk sys-apps/xdg-desktop-portal-gnome
    dev-python/uv
    net-misc/networkmanager
  )
  if ! inir_uses_systemd; then
    core+=(sys-auth/elogind sys-auth/pambase)
  fi
  gentoo_emerge "${core[@]}" || return 1

  if ${INSTALL_AUDIO:-true}; then
    gentoo_emerge media-sound/pavucontrol media-sound/cava media-video/mpv net-misc/socat || return 1
  fi
  if ${INSTALL_TOOLKIT:-true}; then
    gentoo_emerge sys-power/upower gui-apps/wtype x11-misc/ydotool \
      app-misc/brightnessctl app-misc/ddcutil media-gfx/imagemagick \
      app-text/tesseract app-text/tessdata_fast || return 1
  fi
  if ${INSTALL_SCREENCAPTURE:-true}; then
    gentoo_emerge gui-apps/swappy gui-apps/wf-recorder media-video/ffmpeg || return 1
  fi
  if ${INSTALL_FONTS:-true}; then
    gentoo_emerge app-arch/unzip media-libs/fontconfig \
      media-fonts/dejavu media-fonts/liberation-fonts media-fonts/noto-emoji || return 1
    install-roboto-flex-font || true
    install-gabarito-font || true
    install-jetbrains-mono-nerd || true
    install-material-symbols-rounded || true
    install-material-symbols-outlined || true
  fi

  log_success "Gentoo dependencies installed through Portage"
}

gentoo_install_selected
