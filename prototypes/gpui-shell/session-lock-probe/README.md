# Wayland session-lock SHM probe

An isolated protocol probe, not part of the iNtoo binary, launcher, QML, GPUI window system, or `session-lock-spike`. GPUI `WindowKind::SessionLock` remains fail-closed.

The probe creates one `ext_session_lock_v1`, tracks `wl_output` additions/removals, creates one pristine lock surface per output, and answers configure with `ack_configure(serial)` before a non-null opaque ARGB8888 `wl_shm` buffer at exact configured dimensions. It answers `finished` with `destroy` before `locked`, and `unlock_and_destroy` after `locked`, followed by a display sync. In a live test, press Enter in the terminal after `locked` to unlock.

## Safe local checks

From the repository root:

```sh
cargo test --locked --manifest-path prototypes/gpui-shell/session-lock-probe/Cargo.toml
cargo build --locked --manifest-path prototypes/gpui-shell/session-lock-probe/Cargo.toml
cargo run --locked --manifest-path prototypes/gpui-shell/session-lock-probe/Cargo.toml -- --help
cargo run --locked --manifest-path prototypes/gpui-shell/session-lock-probe/Cargo.toml -- --dry-run
```

These commands do not connect to Wayland. Default invocation refuses to connect.

## Live compositor test

Live testing requires a dedicated disposable Wayland compositor session with a documented recovery route. Never run from a normal desktop session: the protocol permits the session to remain locked if the client crashes. Confirm compositor policy and recovery before testing.

Only in that dedicated session:

```sh
INTOO_SESSION_LOCK_PROBE_CONFIRM=I_UNDERSTAND_THIS_LOCKS_THE_SESSION \
  cargo run --locked --manifest-path prototypes/gpui-shell/session-lock-probe/Cargo.toml -- --run
```

This project has not run a live compositor test. Manually verify authorization, each output's SHM frame, hotplug, configure dimensions/order, locked/finished behavior, and recovery before considering it compositor-validated.

## Limits

- Backing mappings and SHM buffers are retained until process exit so server-side storage cannot be freed early. Repeated configure events grow memory during a long run.
- This is not an authentication UI. It has no password flow.
- A configure allocation/validation error is logged and no null or mismatched buffer is committed; the compositor may leave the lock pending. Recover only through the disposable compositor's recovery path.
- `ext-session-lock-v1` is a staging protocol and the compositor may deny access by policy.
