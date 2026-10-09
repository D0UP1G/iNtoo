# Gentoo installation

The Super+S desktop companion uses OpenCode. Setup installs `net-libs/nodejs`
with `npm` through Portage, then installs `opencode-ai@1` in a user-owned npm
prefix and links the CLI into `XDG_BIN_HOME` (normally `~/.local/bin`). An existing
OpenCode installation is preserved. Connect your model provider with
`opencode auth login`; see [the assistant guide](ASSISTANT.md).

iNtoo's setup script detects Gentoo and uses Portage. Niri and Quickshell are installed from GURU; the current Niri ebuild is keyworded for amd64. Quickshell's Gentoo installation instructions also point to that overlay. See the [Quickshell Gentoo guide](https://quickshell.org/docs/v0.3.0/guide/install-setup/#gentoo) and the [Gentoo Wayland compositor list](https://wiki.gentoo.org/wiki/Wayland_compositor).

## Install with setup

```bash
git clone https://github.com/D0UP1G/iNtoo.git
cd iNtoo
./setup install
```

The installer enables and syncs GURU, adds package-specific `~amd64` acceptance for the GURU packages it uses, and installs the core and selected feature dependencies. Portage still shows its package plan for review. Use `./setup install -y` to skip iNtoo's prompts and Portage's confirmation.

The package-specific keyword entries are written to `/etc/portage/package.accept_keywords/intoo`; remove that file if you uninstall iNtoo and no other setup uses those entries.

Session USE flags are written to `/etc/portage/package.use/intoo-session`. If `package.use` is a single file, setup maintains a marked block and preserves other entries. Niri is built with D-Bus and screencast support; PipeWire gets `sound-server`. Quickshell enables the Wayland, layer-shell, session-lock, toplevel management, tray, audio, screenshot capture (`screencopy`), notification, Bluetooth, network and related modules used by iNToo's QML imports. Hyprland support is also required by shared components imported on Niri. The Qt compatibility and multimedia packages enable their QML plugins. OpenRC uses elogind and its PAM integration; systemd profiles retain systemd integration. Existing packages with changed USE flags are rebuilt rather than skipped. Review Portage's plan, especially if switching from PulseAudio or changing init profiles. Apply any pending PAM configuration updates with your usual Gentoo configuration tool before logging in again.

If a previous GURU sync left a non-Git directory at Portage's configured GURU path, setup preserves it beside the sync path with an `.intoo-incomplete-*` suffix, then retries the sync. Rerun `./setup install` after an interrupted attempt.

## Manual dependencies

To install the compositor and shell yourself on amd64, first add GURU and accept its rolling ebuilds:

```bash
sudo emerge --ask app-eselect/eselect-repository
sudo eselect repository enable guru
sudo emaint sync -r guru
```

Add these lines to `/etc/portage/package.accept_keywords/intoo` on amd64:

```text
gui-apps/quickshell ~amd64
gui-wm/niri ~amd64
gui-apps/fuzzel ~amd64
app-misc/cliphist ~amd64
app-misc/brightnessctl ~amd64
gui-apps/wtype ~amd64
```

Then install the compositor, shell, QML modules and common runtime tools:

Quickshell must be built with the feature flags used by iNToo's imports, and the Qt compatibility and multimedia QML modules must be enabled. Installing the Qt libraries without `qml` does not provide their QML imports. Add these lines to `/etc/portage/package.use/intoo-session` (or the equivalent `package.use` file):

```text
gui-apps/quickshell sockets wayland layer-shell session-lock toplevel-management hyprland tray pipewire mpris pam upower notifications bluetooth networkmanager screencopy
dev-qt/qt5compat gui qml
dev-qt/qtmultimedia qml
```

```bash
sudo emerge --ask --verbose \
  gui-wm/niri::guru gui-apps/quickshell::guru gui-apps/fuzzel::guru \
  dev-qt/qt5compat dev-qt/qtmultimedia \
  app-misc/cliphist::guru \
  kde-frameworks/kirigami kde-frameworks/syntax-highlighting kde-apps/kdialog \
  app-misc/jq app-shells/fish dev-lang/python dev-vcs/git \
  net-misc/curl net-misc/wget net-misc/rsync sys-devel/bc sys-apps/ripgrep \
  x11-misc/xdg-utils gui-apps/wl-clipboard gui-apps/grim gui-apps/slurp \
  gui-apps/swayidle gui-apps/swaylock gui-apps/wlsunset \
  media-video/pipewire media-video/wireplumber media-sound/playerctl \
  x11-libs/libnotify sys-apps/dbus sys-apps/util-linux sys-auth/polkit \
  gnome-extra/polkit-gnome sys-apps/xdg-desktop-portal \
  sys-apps/xdg-desktop-portal-gtk sys-apps/xdg-desktop-portal-gnome dev-python/uv \
  kde-plasma/plasma-integration net-misc/networkmanager
```

Then install the shell's Niri files and user configuration:

```bash
./setup install --skip-deps
```

`./setup doctor` checks the installed Quickshell QML imports on Gentoo. If a feature module is missing, Doctor adds the session USE profile and rebuilds Quickshell with changed USE flags, along with the required Qt QML packages.

Optional controls in `./setup install` add audio, screenshot, toolkit and font packages. Gentoo package names and USE flags vary by profile; if an optional package is unavailable, install its Gentoo equivalent and rerun `./setup install --skip-deps`.

## OpenRC startup

On OpenRC, the installer adds a managed Niri startup entry using the absolute path to the installed `inir` executable. It looks like this (the path varies by `XDG_BIN_HOME`):

```kdl
spawn-at-startup "/home/alex/.local/bin/inir" "run" "--session"
```

The launcher supervises Quickshell in the user's session. It prevents duplicate supervisors, restarts a failed shell with a bounded retry limit, and stops when Niri's session sockets disappear. Shell helpers are cleaned up at restart; applications launched independently keep running. `inir restart`, `inir stop`, and `inir service start|stop|restart|status|logs` work on OpenRC. On systemd profiles, iNtoo uses its compositor-bound user service.

Setup preserves existing Niri configuration and follows active includes when maintaining its startup entries. Install, update, and Doctor repair the managed entry; switching back to systemd removes it to avoid two startup owners. `inir service enable` adds it manually; `inir service disable` removes it and stops the shell.

OpenRC needs a user D-Bus session and a writable, user-owned `XDG_RUNTIME_DIR`. The Gentoo GURU Niri package provides an OpenRC desktop session; from a logged-in TTY use:

```bash
dbus-run-session niri --session
```

For manual dependency installation on OpenRC, also emerge `sys-auth/elogind` and `sys-auth/pambase`, with the following package USE flags:

```text
gui-wm/niri dbus screencast -systemd
media-video/pipewire sound-server elogind -systemd -system-service
media-video/wireplumber elogind -systemd -system-service
sys-auth/elogind pam policykit
sys-auth/pambase elogind -systemd
```

Setup enables and starts installed `dbus` and `elogind` OpenRC services, and configures `i2c-dev` and `uinput` loading. Power actions use elogind's `loginctl`; night light and the XEmbed tray run as session processes. PipeWire starts through the [Gentoo launcher](https://github.com/gentoo/gentoo/blob/master/media-video/pipewire/files/gentoo-pipewire-launcher.in-r4); setup adds its startup entry when the launcher is installed and leaves custom audio startup entries intact.

On systemd profiles, setup enables `pipewire.socket`, `pipewire-pulse.socket`, and `wireplumber.service` for the user. If installation runs without a user D-Bus session, it schedules those units for the next login. Existing unit masks and a running non-PipeWire PulseAudio server are preserved.

If ydotool is installed, setup configures `/run/ydotoold.sock` for the `input` group with mode `0660`, while preserving an existing `command_args` configuration. Log out and back in after group changes. A custom ydotool socket requires the matching `YDOTOOL_SOCKET` in your session environment.

SDDM uses Gentoo's `display-manager` service and requires `gui-libs/display-manager-init`. Setup enables it for the next boot when SDDM is selected, preserving another configured display manager. NetworkManager is installed for the shell's network controls; choose its OpenRC service configuration yourself if you already use another network manager.

For a system-wide source installation without a systemd unit:

```bash
sudo make install INSTALL_SYSTEMD=0
```

Then configure Niri as your normal user with `./setup install --skip-deps` or, for an existing Niri configuration, `inir service enable`. Automatic backend detection checks the running manager, not just installed binaries. `INIR_INIT_SYSTEM=openrc` or `systemd` can explicitly select the backend in unusual session environments.

## Verification

```bash
python3 scripts/test-openrc-session.py
INIR_REQUIRE_NATIVE_STARTUP=1 python3 scripts/test-shell-startup.py
make test-local
```

The OpenRC tests use temporary Unix sockets and service stubs to verify startup idempotence, both init backends, Portage USE handling, crash recovery, stop/restart, and process ownership. When Quickshell and `dbus-run-session` are available, they also load a minimal QML shell with the offscreen Qt platform and check native restart, helper cleanup, and detached application survival. They do not reboot the machine or invoke real power actions. On a real OpenRC desktop, additionally check login, logout, restart, tray, night light, audio, clipboard, lock/suspend/resume, and the affected panel families. Hardware and PAM behavior require that live verification.

## Useful commands

```bash
./setup doctor
./setup update
inir logs
```

The CLI keeps the upstream-compatible `inir` command name. Repository updates come from the iNtoo checkout's `origin` remote, and update notifications query the iNtoo repository.
