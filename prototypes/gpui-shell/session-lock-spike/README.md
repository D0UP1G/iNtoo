# GPUI ext-session-lock backend spike

This isolated Cargo workspace vendors the pinned Zed `gpui` and `gpui_linux`
crates so backend changes are reproducible without modifying Cargo's git cache.
The snapshot is pinned to Zed commit
`56934ec0560bce8929025497411de7f5322fb6ca`; upstream Apache and GPL license
texts are retained.

The spike adds `WindowKind::SessionLock` and a Wayland protocol role wrapper
that can bind the optional v1 manager, request one shared lock, create and
remove one lock surface per output, acknowledge configure serials, enforce the
configured size/non-null first-buffer order, and gate the protocol destructor
on lock state. The wrapper is not connected to `WaylandClient` event dispatch or
the GPUI window lifecycle. The app's Wayland and X11 paths reject
`SessionLock` before creating any surface/window. Do not route the application
to it or treat this spike as a lock screen.

The missing integration seam is GPUI's initial synchronous draw: a session-lock
surface must not commit until it receives and acknowledges `configure`, while
`App::open_window` currently draws immediately after creating the platform
window. A complete implementation also needs `WaylandClient` dispatch for lock
and lock-surface events, plus output reconciliation that creates lock surfaces
for outputs added during the lock. Keep the early refusal in place until those
paths are connected and verified.

Build/check with:

```sh
cargo check --locked --manifest-path prototypes/gpui-shell/session-lock-spike/Cargo.toml
cargo test --locked --manifest-path prototypes/gpui-shell/session-lock-spike/Cargo.toml
cargo fmt --all --manifest-path prototypes/gpui-shell/session-lock-spike/vendor/zed/Cargo.toml -- --check
```
