# GPUI clipboard adapter contract

The Waffle GPUI clipboard overlay now connects a blocking process adapter,
`prototypes/gpui-shell/src/families/waffle/clipboard_provider.rs`, through GPUI
background executor tasks. A standalone provider API is available in
`prototypes/gpui-shell/src/families/waffle/clipboard_provider.rs`. QML's
authoritative implementation is `services/deferred/Cliphist.qml`, backed by
`cliphist`, `scripts/clipboard-store.py`, `Quickshell.clipboardText`, and the
configured `clipboard.pinned` list.

## Snapshot

The overlay loads the initial `ClipboardState::{Loading, Unavailable, Ready}`
snapshot when opened and refreshes it after refresh/delete/clear. Its GPUI text
editor filters results and supports keyboard selection. Clear has distinct
request and confirmation actions. It reads
`clipboard.pinned` from the loaded configuration. Changed pins are session-only, retained by the shell controller while it runs,
and are discarded when the shell restarts; they are not written to config.
`Ready` with no matching entries means empty/search-no-results. `Unavailable`
must not include cached rows presented as current. Providers must sanitize and
truncate display previews, retain a stable identity for each row, and label
image/rich-text entries without exposing raw serialized markup.

## Operations

- `on_query(String)`: update the controlled GPUI search editor; filtering keeps
  pinned rows first as QML does.
- `on_copy_entry(id)` / `on_copy_pinned(id)`: copy/paste the selected payload
  through the desktop clipboard integration and close after success.
- `on_delete(id)`, `on_toggle_pin(id, pinned)`, `on_clear()`: mutate history
  and configured pins. Clear should require a second explicit confirmation.
- `on_refresh()`: refresh history asynchronously and report a new state/snapshot.
- `on_select(index)` and `on_close()`: selection and overlay lifecycle.

Do not synthesize clipboard values or claim an operation succeeded until the
provider reports the resulting state. Operation failures keep the last good
snapshot and display the provider error. If a mutation succeeds but its
follow-up list read fails, invalidate the snapshot so changed rows are not
shown as current; refresh remains available to retry.

## Rust process adapter

`families/waffle/clipboard_provider.rs` is a blocking adapter. Call every
method from GPUI's background executor, never from render/input callbacks. It
uses `cliphist list`, `cliphist decode <numeric-id>`, `cliphist delete` with the
original `id<TAB>preview` row plus newline on stdin, `cliphist wipe`, and
`wl-copy` with decoded bytes on stdin. It passes argv/stdin directly through
`std::process::Command`; it does not invoke a shell. Paths default to
`cliphist` and `wl-copy` and can be supplied by the caller. Configured pin text
is provided as input and is not persisted by this adapter; `copy_pinned(text)`
copies that text directly through `wl-copy` without a history lookup.

The adapter caps mapped history at 400 rows and configured pins at 200 entries;
preview text is whitespace-flattened and capped at 240 Unicode scalar values,
and decoded pin text at 8,000. These are row/preview bounds; stdout from
`cliphist list` and decoded binary content are not byte-capped before parsing
or copying. Missing executables return `Unavailable`; nonzero
commands/IO return `Command`; malformed list rows return `Parse`. Process calls
currently have no timeout or cancellation, so the background task may remain
blocked if an executable hangs. Clipboard bytes and configured pin contents
remain sensitive data and should not be logged.
