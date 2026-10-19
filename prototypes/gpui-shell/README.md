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

## GPUI Luna MVP work

The MVP work reuses the existing `surface_manager.rs` layer-shell/output owner,
`config.rs` JSON reader, `theme.rs` tokens, `niri.rs` workspace adapter, and
Waffle's XDG desktop-entry catalog and GPUI search view. It adds
`mvp_settings.rs` for validated settings, atomic updates to the existing config,
and a statically registered panel-widget contract. Theme and module settings
use the `gpuiMvp` namespace; panel edge/height reuse `bar.bottom` and
`bar.height`, while wallpaper uses `background.wallpaperPath`. The ii top bar uses the clock and
workspace providers from that registry. Its Settings button opens a
keyboard-focusable horizontal settings strip within the panel surface. It
controls theme, accent, text scale, corner radius, panel edge/height, wallpaper
path, and module enable/order. Changes are written to the existing config;
surface geometry and wallpaper are reapplied by the config watcher. Unknown
JSON fields are retained. The Apps button opens the existing GPUI XDG launcher,
which searches parsed desktop entries and starts the selected entry via `gio`
with an argument vector. Launch errors are written to diagnostics.

The prototype still does not have a live Niri session check for the new panel
controls or launcher on every supported compositor. Output add/remove and
bounds reconciliation is provided by the existing surface manager, but this
MVP work has not been exercised in a live session. The former QML runtime,
installer, startup paths, and other panel families remain unchanged.

The panel modules are static Rust implementations. To add one, implement
`PanelPlugin` in `src/mvp_settings.rs` (stable `id`, user-facing `name`,
optional `start`/`stop`, namespaced settings, and a `PanelWidget` result), then
register it in `PanelPluginRegistry::builtins`. For example:

```rust
struct GreetingPlugin;

impl PanelPlugin for GreetingPlugin {
    fn id(&self) -> &'static str {
        "greeting"
    }

    fn name(&self) -> &'static str {
        "Greeting"
    }

    fn widget(&self, context: &PanelPluginContext<'_>) -> Option<PanelWidget> {
        let text = context
            .plugin_settings
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("Hello");
        Some(PanelWidget::Text {
            text: text.into(),
            accessible_name: "Greeting".into(),
        })
    }
}
```

The widget context contains the plugin's own `gpuiMvp.greeting` JSON value in
`plugin_settings` (`null` when the namespace is absent). For example:

```json
{
  "gpuiMvp": {
    "panelModules": ["greeting"],
    "enabledModules": ["greeting"],
    "greeting": { "message": "Hello from config" }
  }
}
```

The host associates each returned widget with its plugin ID and renders text
widgets through the shared panel renderer; it does not need a `greeting`
branch in the host. Workspace widgets use the common workspace rail renderer.

Add `Box::new(GreetingPlugin)` to the registry, then add `"greeting"` to
`gpuiMvp.panelModules` and `gpuiMvp.enabledModules` in the existing config.
Build and run again; static modules require a recompile:

```sh
cargo run --locked --manifest-path prototypes/gpui-shell/Cargo.toml
```

No installation, session startup, or Quickshell files are changed by this
prototype. Output hotplug and resize reconciliation remains owned by the
existing surface manager. See the verification notes below before treating
this prototype as a finished replacement for the production shell.

## Checks recorded for the MVP work

- `cargo check --locked --manifest-path prototypes/gpui-shell/Cargo.toml` —
  passed; existing unused/dead-code warnings remain.
- `cargo fmt --manifest-path prototypes/gpui-shell/Cargo.toml -- --check` —
  passed.
- `cargo test --locked --manifest-path prototypes/gpui-shell/Cargo.toml` —
  passed (13 tests, including config preservation, validation, malformed-file
  safety, panel order/enablement, and plugin widget output).
- `cargo run --locked --manifest-path prototypes/gpui-shell/Cargo.toml` —
  started in the available Wayland/Niri session and remained running until the
  test process received Ctrl+C. I did not stop the existing shell.
- Visual behavior, keyboard input, wallpaper rendering, launcher search, and
  multi-monitor hotplug were not verified interactively in that session.
