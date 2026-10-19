# GPUI Iris Dock Adapter Contract

The GPUI prototype now resolves running Iris Dock tasks from Niri's real window
snapshot (`id`, `app_id`, `is_focused`, `is_urgent`) and localized app names
from its XDG desktop-entry catalog. Activation cycles the app's live Niri
window IDs with `niri msg action focus-window --id`. Missing `app_id` values
are omitted; no app list is synthesized.

The remaining Dock host adapter must provide an output-scoped, atomic snapshot
and real actions. Suggested Rust-facing contract:

```rust
struct IrisDockSnapshot {
    output: DisplayId,
    apps: Vec<IrisDockApp>,       // resolved pin + running order
    pieces: Vec<IrisDockPiece>,   // configured Iris absorbed pieces
    reveal: DockRevealState,      // pointer/edge/fullscreen policy
}

trait IrisDockActions: Send + Sync {
    fn activate_or_launch(&self, app_id: &str); // restore minimized, cycle/focus, else launch
    fn open_palette(&self);                     // Iris search state, not a substitute launcher
    fn activate_piece(&self, piece_id: &str);
    fn toggle_pin(&self, app_id: &str);
    fn reorder(&self, app_id: &str, before: Option<&str>);
}
```

The host still needs to connect the root `dock.pinnedApps` ordering, minimized
window restoration, desktop-entry launching, Iris piece activation and reorder
persistence. It also needs an icon-theme resolver for `Icon` metadata and a
notification adapter that supplies badge counts. Until these sources exist,
the prototype shows running tasks only, uses no fabricated badges/icons/pieces,
and leaves those controls inert or absent.

The surface host must consume `iris.dock` options (`position`, `iconSize`,
`notch`, `launcher`, `autoHide`, `reserveSpace`, `magnification`, `badges`,
`reorder`, `arrange`, `screenList`) and mount one dock per eligible output.
Only `position`, `iconSize`, `notch`, and `launcher` currently reach the
renderer; edge reveal, reservation, magnification, drag ordering, output
selection and the launcher-to-palette callback are not yet wired.
