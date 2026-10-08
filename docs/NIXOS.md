# NixOS

> Experimental. The NixOS module and Home Manager integration are maintained separately from the Gentoo setup path.

iNtoo provides a flake with:

| Output | Purpose |
|---|---|
| `packages.<system>.default` | Packaged iNtoo runtime and `inir` launcher |
| `nixosModules.inir` | NixOS module for system package + user service |
| `homeModules.inir` | Home Manager module for user package + user service |

The module does not run `./setup install` or `./setup update`. Nix owns the installed files, and iNtoo runs from the package store path.

The package and modules are ordinary Nix expressions under `nix/`. Flakes are only one entrypoint, so traditional Nix configurations can import them directly. Both entrypoints use the same `package.nix`, NixOS module, and Home Manager module rather than maintaining separate implementations.

## Without flakes

Point `inirSrc` at a local checkout or a source pinned with your preferred Nix fetcher:

```nix
{ pkgs, ... }:
let
  inirSrc = /path/to/iNtoo;
in
{
  imports = [
    (import (inirSrc + "/nix/nixos-module.nix"))
  ];

  programs.inir = {
    enable = true;
    package = pkgs.callPackage (inirSrc + "/nix/package.nix") { inherit pkgs; };
    service.compositor = "niri";
  };
}
```

For Home Manager, import `nix/home-module.nix` instead. The package expression accepts the consumer's `pkgs` set explicitly, so traditional configurations can choose or pin nixpkgs without converting the project to a flake. Both modules use that same package expression by default unless `programs.inir.package` is overridden.

## With Niri on NixOS

Add iNtoo to your flake inputs:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    inir.url = "github:D0UP1G/iNtoo";
  };
}
```

The default input tracks the stable branch. To test development builds before
they reach `main`, point the iNtoo input at `github:D0UP1G/iNtoo`
instead and rebuild through Nix.

Then import the iNtoo module in your NixOS configuration:

```nix
{ config, inputs, ... }: {
  imports = [
    inputs.inir.nixosModules.inir
  ];

  programs.niri.enable = true;

  programs.inir = {
    enable = true;
    service.compositor = "niri";
    extraPackages = [ config.programs.niri.package ];
  };
}
```

`programs.inir.service.compositor = "niri"` creates the user unit wiring under `niri.service.wants/inir.service`. The unit waits for `niri.service` readiness, inherits Niri's published `DISPLAY`/Wayland environment, and stays ahead of XDG desktop autostart so the tray watcher is ready before applications register. It is not wired to `graphical-session.target`, so it will not auto-start under KDE, GNOME, or other desktop sessions.

`extraPackages = [ config.programs.niri.package ];` puts the same `niri` client binary used by your compositor on iNtoo's runtime `PATH`, so features that call `niri msg` use the matching package.

NixOS already provides Niri through `programs.niri.enable`. A separate `niri-flake` input is not required for a normal installation; use one only when you specifically need what that external flake provides.

Recorder runtime dependencies are provided by the iNtoo package itself, including
`wf-recorder`, `slurp`, `xdg-user-dirs`, `xdg-utils`, PipeWire/Pulse tooling, and
FFmpeg when available in nixpkgs. They do not need to be duplicated in
`programs.inir.extraPackages`.

EasyEffects remains an opt-in audio feature. To use the native iNtoo equalizer on NixOS,
make EasyEffects visible to the iNtoo service, for example with
`programs.inir.extraPackages = [ pkgs.easyeffects ];` in addition to your compositor package.
The nixpkgs EasyEffects wrapper owns its plugin search path; iNtoo verifies that the LSP
Equalizer is actually available at runtime. On a fresh EasyEffects setup with an empty output
pipeline, iNtoo can bootstrap its neutral 10-band `iNtoo Equalizer` preset. Existing non-empty
effect chains are never replaced automatically.

For useful default shortcuts, merge iNtoo actions into `programs.niri.settings.binds`:

```nix
{
  programs.niri.settings.binds = {
    "Mod+Space" = {
      repeat = false;
      action.spawn = [ "inir" "overview" "toggle" ];
    };

    "Mod+V".action.spawn = [ "inir" "clipboard" "toggle" ];
    "Mod+Comma".action.spawn = [ "inir" "settings" ];
    "Mod+Slash".action.spawn = [ "inir" "cheatsheet" "toggle" ];
    "Mod+Shift+W".action.spawn = [ "inir" "panelFamily" "cycle" ];

    "Mod+Alt+L" = {
      allow-when-locked = true;
      action.spawn = [ "inir" "lock" "activate" ];
    };

    "Mod+Shift+S".action.spawn = [ "inir" "region" "screenshot" ];
    "Mod+Shift+X".action.spawn = [ "inir" "region" "ocr" ];
    "Mod+Shift+A".action.spawn = [ "inir" "region" "search" ];
  };
}
```

## Home Manager

If you manage your user session with Home Manager, import the Home Manager module instead:

```nix
{ inputs, pkgs, ... }: {
  imports = [
    inputs.inir.homeModules.inir
  ];

  programs.inir = {
    enable = true;
    service.compositor = "niri";
    extraPackages = [ pkgs.niri ];
  };
}
```

`extraPackages` is added to the Home Manager service `PATH`. If your Niri session
uses a different package, put that matching package there instead.

The Home Manager module can also expose the packaged runtime at:

```text
~/.config/quickshell/inir
```

That symlink keeps tools that expect the traditional config path working, but it is opt-in because it will conflict with an existing repo checkout at the same path. Enable it with:

```nix
programs.inir.configSymlink.enable = true;
```

## Manual service wiring

To create the service but avoid auto-start wiring:

```nix
programs.inir.service.compositor = null;
```

Then start it manually while a managed Niri session is active:

```bash
systemctl --user start inir.service
```

## Notes

- Use `inir logs --full` for runtime errors.
- The packaged `inir` launcher wraps Quickshell and runtime tools in `PATH`.
- User preferences still live in iNtoo's normal config/state files; the packaged QML source itself is immutable.
- `inir update` is not the right update path for a Nix install. Update through your flake inputs and rebuild.
