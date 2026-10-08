<p align="center">
  <img src="docs/images/iris-2.32-principal.webp" alt="iNtoo desktop running on Niri" width="900">
</p>

<h1 align="center">iNtoo</h1>

<p align="center">
  <strong>A full desktop shell for Niri, built with Quickshell and adapted for Gentoo.</strong>
</p>

<p align="center">
  <a href="https://github.com/D0UP1G/iNtoo/releases"><img src="https://img.shields.io/github/v/release/D0UP1G/iNtoo?style=flat-square&label=release" alt="Latest release"></a>
  <a href="https://github.com/D0UP1G/iNtoo/stargazers"><img src="https://img.shields.io/github/stars/D0UP1G/iNtoo?style=flat-square" alt="GitHub stars"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-green?style=flat-square" alt="GPL-3.0 license"></a>
  <a href="https://github.com/D0UP1G/iNtoo"><img src="https://img.shields.io/badge/target-Gentoo-54487A?style=flat-square&logo=gentoo&logoColor=white" alt="Gentoo target"></a>
</p>

<p align="center">
  <a href="docs/GENTOO.md">Install on Gentoo</a> &bull;
  <a href="docs/KEYBINDS.md">Keyboard shortcuts</a> &bull;
  <a href="docs/IPC.md">IPC reference</a> &bull;
  <a href="docs/SETUP.md">Setup and updates</a>
</p>

---

**iNtoo is based on [iNiR](https://github.com/snowarch/inir) by snowarch, adapted for Gentoo.** It keeps iNiR's Quickshell desktop-shell foundation and adds a Gentoo-focused setup and runtime path, including Portage dependency handling and support for both systemd and OpenRC. iNtoo is a distinct, independently maintained project. See [NOTICE](NOTICE) for required attribution and license terms.

## What you get

- Three switchable desktop families: Material ii, Waffle, and Island
- Panels, docks, overview, notifications, media controls, clipboard tools, and settings
- Wallpaper-driven theming with presets and integrations for desktop applications
- Niri-first behavior, with Gentoo installation and session support

## Install on Gentoo

The installer configures dependencies through Portage and supports systemd and OpenRC.

```bash
git clone https://github.com/D0UP1G/iNtoo.git
cd iNtoo
./setup install
```

The setup wizard asks before dependency commands. Add `-y` to accept them automatically. For manual package setup, USE flags, and OpenRC details, see the [Gentoo guide](docs/GENTOO.md).

## Update

```bash
./setup update
```

The updater fast-forwards this checkout's `origin`, syncs the installed shell, applies migrations, and restarts the session. Keep local changes committed or stashed before updating.

## Documentation

| Guide | Description |
| --- | --- |
| [Gentoo](docs/GENTOO.md) | Installation, Portage, USE flags, and OpenRC |
| [Install](docs/INSTALL.md) | Installation options and first run |
| [Setup](docs/SETUP.md) | Updates, migrations, and rollback |
| [Keyboard shortcuts](docs/KEYBINDS.md) | Default key bindings |
| [IPC](docs/IPC.md) | Commands for scripts and key bindings |
| [Packages](docs/PACKAGES.md) | Runtime dependencies |

## Lineage and credits

iNtoo is based on [iNiR by snowarch](https://github.com/snowarch/inir), which grew from [illogical-impulse by end-4](https://github.com/end-4/dots-hyprland). The project also acknowledges ideas and visual work from [pctrade/end4-pC](https://github.com/pctrade/end4-pC) and [Gakuseei's Ricelin](https://github.com/Gakuseei/Ricelin). It runs on [Quickshell](https://quickshell.outfoxxed.me/) and is designed for the [Niri compositor](https://github.com/YaLTeR/niri).

The iNtoo name and identity are distinct from iNiR and iRiS. Upstream attribution and applicable terms are preserved in [NOTICE](NOTICE); iNtoo is distributed under [GPL-3.0](LICENSE).
