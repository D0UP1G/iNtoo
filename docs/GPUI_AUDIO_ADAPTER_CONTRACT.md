# GPUI audio mixer adapter contract

Status: read-only native provider spike exists; no audio UI is bound to it. The
provider must not be treated as ready until its initial registry sync completes,
and the mixer must not be mounted with sample or fabricated devices.

## Existing behavior and data sources

`modules/ii/sidebarRight/volumeMixer/VolumeDialogContent.qml` presents either
the default output sink or default input source, a selectable device list, and
the per-application streams. Output streams come from
`MprisController.mixerAppNodes`; input streams come from `Audio.inputAppNodes`.
The related `VolumeMixerEntry.qml` displays stream name/icon/volume/mute, writes
volume and mute to the PipeWire node, routes default sink/source writes through
`Audio.setSinkVolume` / `Audio.setSourceVolume`, and changes other stream nodes
directly. The output sink slider permits up to 1.5; other nodes cap at 1.0.
The `Audio` service also owns device enumeration, default-device selection,
friendly names, and volume protection. Dialog footer actions (Details, Done)
belong to the parent dialog, not the content component.

## Current GPUI boundary

`prototypes/gpui-shell/src/families/ii/volume_mixer.rs` is a renderer over
`VolumeMixerInput`: owned device/stream snapshots, a style, and callbacks for
device picker, device selection, setting volume, and toggling mute. It has no
service binding. `prototypes/gpui-shell/src/audio_provider.rs` now starts a
dedicated PipeWire thread with a persistent registry listener, binds Node
proxies, emits owned snapshots for device/stream nodes, and listens for Props
updates. PipeWire proxies remain thread-confined; consumers receive snapshots
through an async channel. This spike has no volume/mute/default-route writes and
is not mounted by the UI.

The crate's `pipewire::init()` uses a process-wide `OnceLock`; the provider does
not call `deinit()`, whose safety contract requires a single process-level call
after all PipeWire threads and resources have stopped. Provider `Drop` sends a
stop signal and hands the worker join to a reaper thread, so UI teardown does
not wait on PipeWire. An atomic process lease allows one worker loop at a time;
if another loop is active or stopping, a new provider immediately reports
Unavailable without spawning a waiting thread. The lease is released when the
worker exits or its thread cannot be started.
Worker panics and reaper spawn failures update status and are sent to the event
channel. `shutdown_background()` uses the same non-blocking path and returns
immediately; callers must not synchronously join its worker on the GPUI thread.

## Transport candidate

The current implementation candidate is the official `pipewire` Rust crate
from [pipewire-rs](https://pipewire.pages.freedesktop.org/pipewire-rs/), with
SPA bindings from `libspa`. The documented API exposes a persistent `MainLoop`,
`Context`, `Core`, and `Registry` for graph changes, plus node listeners,
parameter enumeration/subscription, and `Node::set_param` for Props updates.
See the [Registry API](https://pipewire.pages.freedesktop.org/pipewire-rs/pipewire/registry/index.html),
[Node API](https://pipewire.pages.freedesktop.org/pipewire-rs/pipewire/node/struct.Node.html),
and [SPA parameter documentation](https://pipewire.pages.freedesktop.org/pipewire/group__spa__param.html).
The prototype pins both `pipewire` and `libspa` to 0.10.1 in `Cargo.toml` and
`Cargo.lock`. `cargo check --locked --manifest-path prototypes/gpui-shell/Cargo.toml`
compiled those crates against local PipeWire 1.6.8 development headers. The
`pipewire` and `libspa` manifests declare Rust 1.80 as their MSRV. The
repository itself declares no Rust MSRV; this check used Rust 1.97.1 and does
not prove the whole prototype builds on 1.80.

The registry/listener spike compiles, but has not been exercised against a live
PipeWire session. It classifies nodes using `media.class`, and availability of
those properties and readable Props depends on the server/session permissions.
In particular, it does not yet surface per-node subscription failures as a
separate state. `Node::set_param` requires node permissions and has not been
tested on the project's sink, source, and application-stream node types.
WirePlumber default-device selection needs a separate metadata/policy write;
Registry membership alone does not make a node the preferred default.

Gentoo's existing setup/docs establish PipeWire `sound-server`, WirePlumber,
and `wpctl` runtime dependencies. This spike additionally compiles against
`libpipewire-0.3` and SPA development headers via pkg-config on the current
machine. The crates' build scripts use `system-deps`/pkg-config metadata and
runtime `bindgen`, so a build environment also needs discoverable headers,
pkg-config and libclang. No such build dependency strategy is declared or
verified across supported Gentoo profiles. Do not claim the current runtime
dependency profile is sufficient to build/distribute the GPUI prototype until
that is resolved.

## Required typed host contract

The host-side owner should provide the following operations (names are
illustrative; semantics are binding requirements):

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AudioDirection { Output, Input }

#[derive(Clone, Debug)]
pub struct AudioDeviceSnapshot {
    pub id: String,                 // stable PipeWire node id, not list index
    pub display_name: String,       // Audio.friendlyDeviceName equivalent
    pub available: bool,
    pub is_default: bool,
}

#[derive(Clone, Debug)]
pub struct AudioStreamSnapshot {
    pub id: String,                 // stable PipeWire node id
    pub display_name: String,       // MprisController.streamDisplayName equivalent
    pub icon: Option<String>,       // resolved icon name/path if available
    pub volume: f32,                // current node volume, not normalized UI value
    pub max_volume: f32,            // 1.5 for protected/default output sink policy,
                                    // otherwise 1.0 unless service reports another cap
    pub muted: bool,
    pub available: bool,
}

#[derive(Clone, Debug)]
pub struct AudioMixerSnapshot {
    pub direction: AudioDirection,
    pub default_device_id: Option<String>,
    pub devices: Vec<AudioDeviceSnapshot>,
    pub streams: Vec<AudioStreamSnapshot>,
    pub revision: u64,
}

pub trait AudioMixerAdapter {
    fn snapshot(&self, direction: AudioDirection) -> AudioMixerSnapshot;
    fn subscribe(&self, sink: /* GPUI-owned event sender */) -> /* unsubscribe handle */;
    fn select_device(&self, direction: AudioDirection, device_id: &str)
        -> Result<(), AudioAdapterError>;
    fn set_stream_volume(&self, direction: AudioDirection, stream_id: &str, value: f32)
        -> Result<(), AudioAdapterError>;
    fn set_stream_muted(&self, direction: AudioDirection, stream_id: &str, muted: bool)
        -> Result<(), AudioAdapterError>;
}
```

The event payload should identify direction and contain a fresh snapshot (or a
revision-tagged delta); updates must be marshalled onto the GPUI app context.
Every command validates that the node still exists and is available, clamps to
the service policy, and reports failure so the host can refresh state and show
an error. A stale UI click must never be applied to a different node after
re-enumeration. Selecting a default device must call the service's default
selection path, not mutate an arbitrary node property.

## Lifecycle and binding

The GPUI entity owns only the presentation snapshot and transient picker state.
On mounting the output or input mixer, the host takes a snapshot and subscribes
to service changes; on unmount it drops the subscription. While mounted, service
events replace/update the snapshot and refresh the entity. Direction changes
switch the snapshot/subscription source. Closing the dialog closes the picker.
An empty stream list is a real service result and maps to the existing empty
message; service-not-ready/error is a distinct state and must not be presented
as “no apps playing audio.” Missing devices remain unavailable rather than
being synthesized. Device selection and stream changes should reflect back
from subsequent service snapshots, rather than optimistically inventing state.

The prototype renderer now has a direction-specific selector glyph and open
disclosure, a no-device picker state, a proportional stream level track, and
explicit Ready/Loading/Unavailable/Error stream-list states. An unavailable
individual stream displays only its supplied name/icon and an Unavailable
label; it does not reveal volume/mute values or controls. The separate
`streams_state` and optional `streams_status_message` input fields must be set
from the provider; an empty list means “no apps” only in Ready state. Device
enumeration still has no provider state, and the volume visualization is not a
draggable slider. The wider overlay also
has three tabs (output, input, music) and persists selected tab index, while the
sidebar dialog supplies Details/Done actions; these are parent surface
responsibilities and must be migrated with their respective hosts.

## Handoff

Next, validate live registry classification, node permissions, Props events,
and channel volume values on output device, input device, playback stream, and
capture stream nodes. Then implement safe node writes and WirePlumber default
device selection with stale-ID rejection before connecting the renderer. Keep
the renderer unmounted until a real connected snapshot reaches Ready; an empty
snapshot after successful sync is valid data, but Connecting/error is not an
empty-state result.
