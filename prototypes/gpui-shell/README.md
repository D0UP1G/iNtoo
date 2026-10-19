# GPUI shell prototype

This standalone Rust binary is an in-progress GPUI implementation of iNtoo's
desktop shell. It reads the existing iNtoo configuration and creates Wayland
layer-shell surfaces on the detected outputs. For the `ii` environment, the
configured bar and background are created on outputs allowed by the relevant
panel and screen-list settings. Output add/remove and bounds changes are
handled while the prototype is running.

Quickshell remains the production runtime. The GPUI binary is for development
and visual/interaction checks; it does not yet provide full visual or feature
parity with the QML shell.

## Build and run

Run from the repository root in a Wayland session whose compositor advertises
`wlr-layer-shell-unstable-v1`:

```sh
cargo run --locked --manifest-path prototypes/gpui-shell/Cargo.toml
```

This starts the layer-shell prototype, not a regular floating application
window. It creates configured surfaces across detected outputs; use `Ctrl+C`
in the launching terminal to stop it. On compositors without the required
layer-shell protocol, startup fails. The first build fetches Rust dependencies;
GPUI is pinned in `Cargo.lock` to a Zed revision that provides the required
layer-shell API.

## Current scope

The prototype has a shared, token-backed Material UI module. The `ii`
VerticalBar and TopBar use parts of its surface-color and focusable-button
primitives. Theme resolution includes source-backed palettes and documented
fallbacks, and the built-in `ii`, `waffle`, and `iris` environments can be
selected within the prototype's current composition support. Waffle and Iris
are not fully migrated, and their rendering may retain family-specific styles.

The prototype also includes selected Niri, panel, backdrop, notification,
taskbar, and environment-specific interactions. Many QML surfaces, states,
effects, settings, lifecycle paths, and family-switching behaviors remain
unimplemented or partial. A `panelFamily` change that needs a different
composition currently requires restarting the prototype. The environment
registry is an in-process built-in registry, not dynamic third-party plugin
loading.
