# GPUI migration plan

This document tracks the replacement of the Quickshell/QML runtime with a Rust
application built on GPUI. The existing shell remains the working runtime until
the replacement passes the compatibility gates below. This is an incremental
rewrite: install, update, and rollback must continue to work throughout it.

## Compatibility contract

The replacement must preserve these user-visible and integration contracts:

- The `intoo` command and its IPC target/function names, arguments, return values,
  and keybind behavior documented in [IPC.md](IPC.md).
- `~/.config/inir/config.json`, shipped defaults, legacy config paths, and
  runtime config reload behavior.
- ii, Waffle, and Island panel families, including multi-output placement,
  focus, input, animation, and family switching.
- Niri session startup, both systemd and OpenRC lifecycle paths, status/logging,
  graceful restart, and orphan cleanup.
- Existing setup, update, package-managed install, and rollback behavior on
  supported distributions.
- Current service behavior for compositor state, audio, network, notifications,
  tray, wallpaper, lock/session actions, and the other documented modules.

No QML payload or Quickshell dependency should be removed until the matching
replacement is in use and has a tested rollback path.

## Architecture destination

Keep distribution setup and lifecycle scripts during the early stages. Build the
new runtime from separated Rust components so UI work does not own operating
system behavior:

- `intoo-core`: configuration schema, paths, state, family and panel models.
- `intoo-platform`: compositor, DBus, audio, media, wallpaper, session, and
  notification adapters.
- `intoo-ipc`: compatible local IPC server/client and CLI integration.
- `intoo-wayland`: layer-shell surfaces, output placement, lock and capture
  integration, and tray ownership where required.
- `intoo-ui`: GPUI views and components, with family implementations kept
  separately composable.
- `intoo`: runtime process, startup phases, service coordination, and shutdown.

These are proposed boundaries, not a requirement to create every crate at once.

## Stages and gates

### 0. Contract inventory

Use `ARCHITECTURE.md`, `docs/IPC.md`, `docs/CONFIG_SYSTEM.md`, `docs/RUNTIME.md`,
and `docs/GENTOO.md` as the current behavior inventory. Before replacing any
contract, capture representative config, IPC, startup, and install behavior in
regression fixtures.

### 1. Surface-host feasibility spike

Prove that a GPUI-rendered surface can be hosted as a Wayland layer surface with
correct anchors, exclusive zones, transparency, keyboard/pointer focus, output
selection, and lifecycle under Niri. Also establish whether GPUI can share that
host with the required session-lock, screenshot, and tray integrations. Record
which pieces require direct Wayland protocol clients or another Rust library.

**Gate:** a multi-output bar/background prototype runs alongside Quickshell and
does not disturb the active desktop. If GPUI cannot render into a suitable
layer-shell surface, choose and document a bridge/renderer boundary before
porting panels.

### 2. Rust core and compatibility IPC

Implement path resolution, config read/write/watch, and Niri state access behind
Rust interfaces. Expose them through the current CLI/IPC contract while
Quickshell remains the UI runtime. Avoid competing writers for user config.

**Gate:** existing CLI calls, default/legacy config handling, and service
supervision remain compatible in a side-by-side session.

### 3. First production surface

Port a small always-visible surface using the proven host. Validate startup time,
multi-output behavior, input, accessibility, screen sharing, and graceful
restart. Keep the old surface available as a rollback option.

### 4. Shared UI and services

Port settings navigation, shared controls, and service domains incrementally.
Retain existing translation data and external helpers behind explicit
interfaces. Do not couple UI rendering to direct shell commands or DBus calls.

### 5. Panel families

Port ii, Waffle, and Island in separately reviewable groups. Match visual tokens,
configuration, panel enablement, deferred loading, and IPC behavior. Keep
Quickshell as the fallback until each family passes interaction and multi-output
parity checks.

### 6. Runtime and packaging cutover

Only after runtime parity, update the systemd/OpenRC service, launcher, setup,
payload manifests, Gentoo dependencies, Arch/Nix packaging, and docs. Append
numbered migrations; do not rewrite migration history. Remove Qt/Quickshell
dependencies after no supported install or rollback path uses them.

## Current findings

- The repository has no production Rust runtime yet. The isolated GPUI prototype
  builds and starts under Niri; Rust and Cargo are present in the development
  environment.
- `shell.qml` coordinates startup tiers, singleton creation, IPC, and family
  loading. The UI is split across three family trees and hundreds of modules.
- `scripts/intoo`, the systemd unit, OpenRC session backend, Makefile, Nix, Arch,
  and Gentoo packaging currently assume Quickshell and its runtime payload.
- GPUI layer-shell integration creates one surface per enumerated output and
  observes output add/remove and bounds changes. Input and production lifecycle
  still need proof. A normal application window is not a replacement for shell
  surfaces.
- GPUI is pre-1.0 and its API may change; pin a revision or release and record
  the supported toolchain when the prototype is created.
- The GPUI prototype now watches the resolved runtime config path for ii
  critical composition changes. It polls file metadata/content hash every
  200 ms, waits 120 ms for a stable image, parses one complete `ConfigLoad`
  snapshot from the bytes that were hashed, and applies newer revisions on the
  GPUI app context. This is file polling, not config IPC or a filesystem event
  subscription. Equal content hashes do not recreate surfaces.
- The current live reconciler is limited to ii background, the selected
  horizontal or vertical bar, backdrop, and dock. The bar uses its own
  `bar.screenList`; the dock uses `dock.screenList`; empty or fully stale lists
  fall back to every available output. The background follows its QML root and
  targets every output. The selected bar gate is `iiBar` when
  `bar.vertical` is false and `iiVerticalBar` when true, so a mode switch closes
  the old variant before creating the new one. `iiDock` remains gated by both
  `enabledPanels` and `dock.enable`, whose QML default is true.
- GPUI display IDs do not expose Wayland output names in this prototype. The
  existing adapter matches Niri output names by logical geometry from
  `niri msg -j outputs`; if that command or snapshot is unavailable, named-list
  matching falls back to all outputs. Display change events reconcile the ii
  target lists even when the change is not a bounds change. A newly resolved
  output name replaces the affected critical surfaces so their captured output
  identity is current; a live session is still needed to validate compositor
  rename delivery.
- Theme tokens are resolved again for each accepted config snapshot. A change
  to appearance, bar style, bar placement/targets, critical panel gates,
  background, or dock config replaces the affected critical surfaces after
  closing their prior windows, releasing their layer/input/exclusive-zone
  resources first. Output add/remove/bounds events use the latest accepted ii
  snapshot. Other family composition, deferred panels, lock, transient
  surfaces, and cross-process IPC reconfiguration are outside this slice.
  These claims have not been validated in a live GPUI/Wayland session.

## Current implementation task

An isolated GPUI + Wayland layer-shell prototype lives in
`prototypes/gpui-shell/`. It reads the existing config path, creates surfaces
for enumerated outputs, and observes output add/remove and bounds changes. The
ii horizontal and vertical bars read Niri workspace/focused-window state and
support workspace actions. The horizontal bar now renders a configurable text
resource strip in Memory, Temperature, CPU, GPU order. It samples CPU and
memory from `/proc` at the provider's three-second recommended interval,
honors the corresponding `bar.resources.show*Indicator` toggles and configured
warning/caution thresholds, and displays icon/value pairs. The GPUI strip does
not yet apply QML's `alwaysShowTemp`, `alwaysShowCpu`, or `alwaysShowGpu` rules
or the related active-player/media-title visibility gates; it currently uses
only `show*Indicator` for these entries. QML's `services/ResourceUsage.qml`
already provides temperature sensors and GPU telemetry, but these providers are
not connected to GPUI, so those entries remain visibly unavailable. Swap is
intentionally omitted from the horizontal strip even though the shared `/proc`
adapter reads it for the vertical bar. This slice does not establish QML visual
parity. The vertical bar also reads CPU, memory, and swap snapshots from
`/proc` and honors resource-indicator visibility and warning threshold settings.
Resource values render as native circular rings with a dim
track, percentage arc, and threshold warning color; absent values keep an
unavailable label rather than becoming zero. This is a visual implementation
change, not evidence of QML parity. Its taskbar-versus-resources composition and
borderless group separators follow the existing bar configuration. The middle
stack is centered between reserved top and bottom edge sections to approximate
QML's `middleHost`; these reserves use fixed 56 px and 88 px values rather than
the source content's measured geometry. The output surface is 64 px wide and
reserves a 46 px exclusive zone; `bar.bottom` selects the right edge when true
and the left edge when false. These are prototype geometry values, not proof of
edge-placement or reserve parity. Font-size scaling and fractional-height
rounding remain unimplemented. The vertical taskbar groups live Niri windows by
app ID, orders each group by Niri layout position, and adds resolvable pinned
XDG entries from `ii.dock.pinnedApps`. Clicking or pressing Enter/Space on a
running group focuses the next window ID in that group; activating an idle
pinned entry launches it through its XDG desktop entry. The taskbar viewport is
capped to the remaining middle height, including a zero-height cap when the
fixed reserves and other groups consume the available space, and uses vertical
scrolling when space remains. Entries are text controls without full QML icon,
indicator, or material parity. The vertical bar now samples
UPower's system bus DisplayDevice and MPRIS players on the session bus, with a
minimum two-second delay between polls while its provider loop is active; the
sequential bus reads can stretch the actual cadence further. Battery
warning color uses configured `battery.low` (default 20%, clamped to 0–100) and
is suppressed while charging. MPRIS toggle, previous, next, and volume intents
are gated by the reported `CanControl`, `CanPlay`, `CanPause`, `CanGoPrevious`,
`CanGoNext`, and volume-property capabilities. The OpenControls intent is
currently a no-op. Polling is property-based rather than signal-driven, and the
provider does not explicitly drop/recreate cached bus connections on sampling
failures; recovery after a disconnect has not been established. Player
selection sorts bus names, preferring the first candidate that reports Playing
and otherwise retrying the first candidate proxy; there is no user-selected
player policy. GPU usage remains unavailable without a supported backend. The
horizontal bar's
five configured zones preserve their order, and workspace controls expose
active/occupancy state to accessibility APIs. The ii right system sidebar now
mounts a header selected by the `sidebar.right.headerStyle` config. Its profile
and classic variants are skeletal and use `USER`, `/etc/os-release`'s `ID`, and
system uptime. The sidebar honors normalized `sidebar.right.sectionOrder`, with
the selected header rendered in the system slot. The notifications section is
mounted at its configured position and has a DTO-driven panel for unavailable,
loading, empty, and ready states; ready DTOs render app/title/body/urgency cards
with supplied action buttons. The current sidebar adapter always supplies an
Unavailable state, an empty list, and a no-op action callback. No live D-Bus
intake, notification history, grouping, search, or Do Not Disturb state is
connected to this panel. QML unread/total count, mute and clear actions,
grouping, per-card dismiss/expand, notification icons, and timestamps are not
migrated. Other ordered sections show explicit unavailable states for widgets;
these are placeholders without connected functionality.
The left Widgets tab now renders the clock-only `GlanceHeader` visual beneath
the tab strip when Widgets is selected. It displays the existing snapshot's
single opaque clock label verbatim, shows Loading before the first snapshot and
Unavailable when a received snapshot has no label. The provider currently
runs `date +"%H:%M · %a, %b %-d"`; locale and clock-format configuration parity,
separate time/date styling, service buttons, and most of the Widgets body are
not implemented. #24 completed static review after the accessible-name fix;
no interactive UI or accessibility-tree validation has been performed. The
Widgets `MediaPlayerWidget` is now mounted in the body
below the custom header and before the generic unavailable section, only while
Widgets is selected, `sidebar.widgets.media` is enabled (default `true`),
normalized `sidebar.widgets.widgetOrder` includes `media`, and the shared MPRIS
state is Playing, Paused, or Stopped with a non-empty title. It reuses the
shared MPRIS card, including capability-gated transport controls. `widgetOrder`
currently gates media membership only; relative placement among unimplemented
widget IDs is not represented. Artwork, Cava visualization, YT Music-specific
visuals, and animation are not implemented. This remains a partial visual
slice, not QML parity. #47 statically reviewed the placement; no live UI or
accessibility-tree validation has been done.
StatusRings now renders up to three circular rings for CPU, RAM, and battery.
It is gated by selected Widgets, `sidebar.widgets.status`, normalized
`sidebar.widgets.widgetOrder` membership, and the
`sidebar.widgets.statusRings.showCpu`, `sidebar.widgets.statusRings.showRam`,
and `sidebar.widgets.statusRings.showBattery` toggles. CPU and RAM use real
`/proc` samples; battery uses UPower. Missing metric values show a dash and an
accessible unknown label rather than a fabricated value or filled ring. CPU
remains unknown until `/proc` has supplied two samples. An absent battery is
omitted; an unavailable UPower battery is represented as unknown. Disk and
temperature rings are not implemented. After cached data ages past eight
seconds, every enabled ring shows an em dash, a “Stale sample” label, and an
accessible stale state. Before the first sample, a cache miss shows an unknown
value without a stale annotation; stale applies only to a previously received
sample. A fresh UPower Unavailable report remains distinct from a stale cache.
The per-display cache prefers recent VerticalBar samples;
TopBar supplies a fallback only when no VerticalBar sample has been published
in the last eight seconds, avoiding competing battery pollers. `widgetOrder`
controls relative order for the implemented MediaPlayerWidget and StatusRings
entries; unimplemented widget IDs have no rendered placement. This is partial
visual work, not QML parity. #24 completed static review; no live UI or
accessibility-tree validation has been performed.
QuickLaunch is implemented as a partial Widgets slice. It is gated by the
selected Widgets tab, `sidebar.widgets.quickLaunch` enablement, and normalized
`widgetOrder` membership; it renders configured shortcut records only, with
no QML defaults or XDG application substitution. Commands are trimmed and
checked against the configured value before launch, then parsed into argv and
spawned without a shell. Plain commands and quoted arguments work; unsupported
shell syntax is rejected. Buttons expose configured names (with a command
basename fallback), keyboard activation, and visible focus styling. Running
state uses the parsed first-token basename matched against Niri app IDs or
titles. This is a limited heuristic: wrapper commands such as `flatpak` or
`env` are matched by the wrapper basename. Editing, reordering, and context
menus are not implemented. #24 static re-review is complete; no live UI or
accessibility-tree validation has been performed. This is not QML parity.
QuickNote is implemented as a partial current-tab slice, gated by the selected
Widgets tab, `sidebar.widgets.note` enablement, and `widgetOrder` membership
for the `note` ID.
It reads `$XDG_STATE_HOME/user/notepad-tabs.json` (or
`~/.local/state/user/notepad-tabs.json`) and exposes the saved current tab's
title and content. Loading, an empty saved note (shown through the editor's
empty placeholder), and an unavailable store have separate presentations; no
blank tab or note is fabricated. Basic insertion,
cursor movement, deletion, cancel, save, and clear are available. Save and
clear preserve other tabs and JSON fields, write through a temporary file, and
reject an observed concurrent update. On conflict the current draft is kept;
Reload asynchronously reads the current JSON snapshot while retaining the
draft and rebasing the cancel baseline, after which save can be retried. Each
display has its own in-memory snapshot, so external updates are not live-synced
between open panels; use Reload to fetch a newer persisted snapshot. A narrow
compare/rename race remains with concurrent QML writes because both runtimes
do not share a lock. The adapter does not migrate legacy `notepad.txt`, create
a tab, or expose tab CRUD or file migration UI. Selection, IME composition,
clipboard editing, and Up/Down movement are not implemented. Disabled actions
are omitted from the accessibility tree; the retained draft in Unavailable is
a focusable, labeled TextInput with an accessible value, and pending save status
uses Role::Status. Static review only; no live UI or accessibility-tree
validation has been performed. This is not QML parity.
WeekStrip is implemented as a partial Widgets slice. It is gated by the
selected Widgets tab, `sidebar.widgets.week` (default `true`), and normalized
`widgetOrder` membership for `week`. It builds seven local calendar days in
the locale's first-weekday order, with localized abbreviated weekday names
(truncated to two Unicode scalar values), day numbers, weekend styling, and a
highlight for today. GNU `date` and `locale` are required for calendar
formatting and the locale's first weekday; missing commands, unsupported
locale output, or malformed results produce visible unavailable/error text
rather than guessed date data. Previous/next buttons, Today when viewing an
offset week, Enter/Space activation, and wheel navigation update the offset.
`widgetOrder` controls WeekStrip's relative position among the implemented
MediaPlayerWidget, StatusRings, QuickLaunch, and QuickNote subset; unported
widget IDs have no rendered position. The current date snapshot refreshes on
navigation and about every 60 seconds while the panel polling loop runs, so
midnight rollover can lag by roughly a minute. Static source review only; no
live UI validation has been performed. This remains partial, not QML parity.
WorldClock is implemented as a partial, read-only Widgets card. It is gated by
the selected Widgets tab, `sidebar.widgets.worldClock` (default `false`), and
normalized `widgetOrder` membership for `worldclock`. A configured non-empty
`worldClock_settings.timezones` list supplies custom rows; an empty list uses
region-based suggestions (detected local zone first when available, followed
by regional/global suggestions, capped at four) and displays an Auto marker.
GNU `/usr/bin/date` samples
each timezone with `TZ`; identifiers are validated against the zoneinfo tree
and unsafe or unavailable zones render an Unavailable row with an error. The
local zone is detected only from an `/etc/localtime` symlink into
`/usr/share/zoneinfo`; other local-time configurations are reported
unavailable. Rows show city labels derived from the zone identifier, time,
GMT offset, optional locale-formatted date, day/night icon, and Tomorrow or
Yesterday when relative-day data is available. Configured 12/24-hour format,
seconds, date visibility, and local highlighting are honored. Visible rows
refresh on opening and every second with seconds enabled, otherwise every 30
seconds. The card has no timezone editing/settings controls; settings remain
configuration-only. `widgetOrder` places it relative to implemented
MediaPlayerWidget, StatusRings, QuickLaunch, QuickNote, and WeekStrip entries;
unimplemented widget IDs have no visible position. Static source review only;
no live UI validation has been performed. This is partial, not QML parity.

QuickWallpaper is a partial, read-only wallpaper preview card. It is gated by
the open left sidebar with the Widgets tab selected, `sidebar.widgets.wallpaper`
(default `false`), and normalized `widgetOrder` membership for `wallpaper`.
Opening the visible widget scans the configured `wallpapers.directory`, falling
back to `PicturesLocation/Wallpapers` from the XDG user directories file and
then `~/Pictures/Wallpapers`. It scans one directory level and accepts regular,
non-symlink files with jpg, jpeg, png, webp, avif, bmp, or svg extensions. The
adapter bounds candidates at the 8192 newest ctime values and displayed rows at
512, with path tie-breakers for stable ordering. Large folders may omit older
files. A generation and visibility/configuration check discards scan results
if the sidebar closes or its directory/current-path configuration changes
while scanning; the widget retries when it becomes visible again. Tiles are
130 by 78 pixels in a horizontally scrollable strip;
loading, unavailable, empty, partial-scan, and image fallback states are
rendered. Current wallpaper is approximated from the global
`background.wallpaperPath` after stripping a literal `file://` prefix, rather
than resolving the active output's effective main wallpaper as QML does. GPUI
previews PNG, JPEG, WebP, BMP, and SVG when decoding succeeds; AVIF and images
over 64 MiB show an explicit fallback. The carousel is a single focus stop;
Left/Right, Home/End, and wheel input move its preview cursor and scroll the
strip without selecting a wallpaper. Individual tiles are labeled groups, not
focusable controls. The header only shows its wallpaper icon and title: random
selection, opening the wallpaper selector, and selecting a tile are not
connected. Strings are hardcoded in English because this prototype has no
translation binding. The pinned GPUI API has no hidden/presentation property
for the Material glyphs, so their accessibility exposure is unverified. Static
source review only; there has been no live GPUI session or accessibility-tree
validation. This is not visual or behavior parity with QML.

ContextCard's current-weather mode is a partial, read-only Widgets card. It is
shown only when the left Widgets tab is selected, `sidebar.widgets.context`
and `sidebar.widgets.contextShowWeather` are enabled, `bar.weather.enabled`
is true, and normalized `widgetOrder` contains `context`. It uses manually
configured weather coordinates with Open-Meteo; it does not use GPS,
IP-geolocation, or geocoding. A configured city without coordinates produces
an explicit unavailable message. Requests run on the background executor,
refresh when the card becomes visible and at the configured
`bar.weather.fetchInterval` in minutes (QML fallback and bundled default 10,
clamped to the settings range 5–60) while visible, and discard results if the
widget is hidden during a request. Visibility changes synchronously invalidate
in-flight results. The request identity also includes city, manual coordinates,
temperature units, and interval; changing those through the weather config
setter invalidates a pending result before scheduling a replacement. The GPUI
host does not currently wire live config reload to that setter. The card renders the
current temperature, WMO-derived condition icon and description, configured
city when present, humidity, wind, and feels-like temperature. WMO icon codes
follow QML's WMO-to-wttr-to-Material-Symbol mapping: for example 65 and 82 use
`weather_hail`, 71/77/85 use `cloudy_snowing`, and 73/75/86 use `weather_snowy`.
Night variants replace clear with bedtime and partly cloudy with the night glyph.
Refreshes retain the last real in-memory snapshot and mark it stale while the
request is pending. On startup, the host reads QML's
`${Directories.state}/user/weather-cache.json` format (`savedAt` in epoch
milliseconds plus a `data` object) before waiting for the network. Entries older
than three days, malformed entries, and GPUI entries for a different city,
coordinate, or unit setting are ignored. Successful refreshes atomically write
the same compatible shape, with a source fingerprint and exact WMO code as
additional GPUI fields. A stale card keeps its actual timestamp and exposes
staleness, age, and refresh failure through status text/accessibility; it never
labels cached measurements as current or substitutes sample readings. Legacy
QML cache entries have no source fingerprint and are accepted for QML
compatibility. When a legacy entry lacks the extra exact WMO code, its `wCode`
is reverse-mapped where known; unknown legacy icon codes retain the measurements
and use a conservative cloudy icon. Cache writes use unique same-directory
temporary files created exclusively before atomic rename, so panels on
different outputs do not share a temporary path. Loading and unavailable states
remain explicit when no usable snapshot exists.
Timer, timer-idle, refresh, and timer-navigation
views/actions are outside this weather-only slice. Copy is currently English;
live GPUI and accessibility-tree validation have not been performed. This is
partial, not QML parity.

The ii Widgets Crypto card is connected to the sidebar section list and
configured widget order. It appears only on the selected Widgets tab when
`sidebar.widgets.crypto` is enabled and `widgetOrder` contains `crypto`. Its
CoinGecko adapter runs on the background executor, uses the shared QML cache
path and JSON fields, retains usable cached prices after a failed refresh, and
refreshes missing or expired data using the normalized 30–300 second interval.
Price/cache results become ready before a separate background history task
completes, so serial history requests do not hold the card in its initial
loading state or block the price worker. History is requested for at most the
first 18 configured coin IDs (the QML `popularCoins` selection set); all
configured price rows remain in order, while later rows have no sparkline.
History results merge only sparkline data into the latest cache. An explicit
Refresh action forces a provider request; periodic loading only runs while the
left sidebar and enabled Widgets tab are visible. Results are discarded if
visibility or the requested coin/interval configuration changes before
completion. The card renders configured-order USD price and 24-hour change
rows, provider-backed sparklines when at least two samples are available, and
loading, pending-history, stale-cache, unavailable, or empty states. Missing
price/change values remain absent instead of becoming zero. Settings editing,
live GPUI visual validation, and accessibility-tree validation are not part of
this slice. `cargo fmt`, `cargo check --locked --offline`, and
`git diff --check` passed according to the implementation agent; tests were
not run. This is an integrated, compile-checked prototype slice, not a runtime
parity claim.

ControlsCard is a partial read-only Widgets slice, gated by
`sidebar.widgets.controls` (default `true`) and normalized `widgetOrder`
membership for `controls`. It honors the eight `controlsCard.show*` visibility
flags (each defaults to `true`) and presents the source glyphs in their
toggle/action groups in the configured relative order. Each visible glyph is a
40 by 40 pixel noninteractive preview. The GPUI prototype has no state
providers or actions for dark mode, Do Not Disturb, night light, game mode,
network, Bluetooth, settings, or lock, so glyphs are labeled
unknown/unavailable and a status message states that actions are disconnected.
Toggle glyphs use 22px and action glyphs use
20px, matching the source sizes; the separator appears when both groups have
visible entries. Controls are
transparent and have no hover interaction while unavailable, so QML hover
feedback and active-state styling are not represented. Strings are hardcoded
in English because no translation adapter is available. This is not functional
or visual parity with QML.

The ii overlay launcher row remains positioned at the overlay's top-right.
Its buttons preserve click toggles and Enter/Space activation, with hover and
visible-focus styling; closed buttons have a transparent resting surface. The
left-sidebar button now receives the `left_open` state from the overlay host
and uses a selected appearance and matching hover style while that sidebar is
open. Distro, avatar, ping, and theme or
launcher-style/config bindings are not connected. The left-sidebar launcher
opens a sidebar whose tab content body has no connected adapter; this limited
launcher interaction does not establish full button or sidebar parity. The
left feature sidebar now renders a horizontally scrollable tab strip from the
config-derived enabled-tab DTO and normalized `sidebar.left.tabOrder`.
Selecting a tab updates the current in-memory selected ID and its selected
presentation. The strip supports Left/Right and Home/End navigation with
roving tab stops, stable per-tab focus handles, and scrolling the focused tab
into view; Left/Right wrap across the list boundaries. Tab labels and Material
Symbols are hardcoded in English. Bodies remain unavailable, and reorder/edit
interactions and `aria-controls`/tabpanel relationships are not implemented.
#24 statically reviewed the focus logic; there has been no interactive session
or a11y-tree validation.
Quick Sliders is mounted in `sidebar.right.sectionOrder` and has a DTO-driven
renderer for Loading, Unavailable, and Ready states (including an explicit
empty Ready state). Valid slider records expose keyboard, pointer, and wheel
value-change callbacks; invalid ranges render as static Unavailable rows. The
integrated sidebar currently supplies Unavailable, no slider records, and a
no-op callback, with no audio or brightness source adapter and no fabricated
values. Its shared sidebar styling does not yet provide QML per-control styling
or live configuration/range bindings, so visual and configuration parity
remain open. Quick Toggles is also mounted in `sidebar.right.sectionOrder` and
has a DTO-driven tile grid for Loading, Ready, Empty, Unavailable, and Error
states. Supplied enabled toggles send `(id, requested_state)` through the UI
callback on click or Enter/Space; the integrated adapter instead always sends
Unavailable with no tiles and a no-op callback, so no fake switches are shown.
Classic/Android QML icon styles, secondary actions, config-backed filtering, and
an explicitly bounded inner scroll viewport are not implemented. Wallpaper
banners,
avatars, account actions, and system information sections remain unavailable.
The Media card is wired to the shared MPRIS snapshot and action runtime only in
the compact right sidebar; standard `SidebarRightContent` omits it. In compact
mode, an explicit `sidebar.right.controlsSectionOrder` controls membership and
order among the implemented `sliders`, `toggles`, and `media` sections; the
unsupported `devices` and `quickActions` entries are omitted without
placeholders. The card hides when the runtime reports NoPlayer, shows explicit
Unavailable and Stopped states, and renders available title/artist metadata
and read-only percentage progress.
Previous, play/pause, and next buttons appear only when their corresponding
runtime capabilities are true. The card does not yet show artwork,
elapsed/total time or duration, seek controls, a player switcher, shuffle or
loop controls; its appearance is generic sidebar styling without QML
Media-specific style or configuration bindings. This is a functional visual
slice, not a parity claim.
The generic title row is removed; clicking outside the system card closes it,
clicks inside stop propagation, and panel-level Escape closes it. The click-away
gate exists for both sidebar roles, but hold-open and editor values are currently
hardcoded false; lock and fullscreen dismissal policies are not wired. Open
state is per display, unlike QML's single target output, and nested content does
not yet delegate Escape handling to the panel.
ii wallpaper/backdrop surfaces and
sidebars/dashboard are mounted. Waffle's taskbar uses a shared one-second Niri
snapshot to show visible windows and configured pinned apps, resolve labels
through XDG desktop entries, and order windows by focus; its scroll position is
persistent. The renderer places pinned no-window tasks before running tasks,
shows window-count labels/indicators, and highlights the active task. It remains
empty when no tasks are available; minimized-window data is missing, and
clicking the active task cannot minimize it. Task buttons render an image when
the matching XDG desktop entry names an existing icon file; if the entry is
missing or its icon cannot be resolved, lookup tries the app ID itself as a
safe theme name before showing the app label. Icon lookup reads the active theme
from `XDG_ICON_THEME`, GNOME GSettings, GTK settings, or KDE `kdeglobals`
according to the detected desktop; it follows `index.theme` inheritance and
ranks declared size/scale directories for the configured logical icon size,
then falls back to hicolor
and legacy `pixmaps`. Icon-theme changes are not observed live, and requested
size does not yet account for per-monitor scaling. Previews, context menus, and
tray parity are also missing. Its Start
Menu, Task View, Notification Center, Action Center, and Clipboard use separate
per-output overlays; Start Menu uses XDG application search and Task View reads
Niri state. Notification/calendar and quick-settings/media providers are not
connected. Clipboard now uses a Cliphist process adapter for listing, search,
copy, delete, pin, refresh, and confirmed wipe in the per-output overlay; pin
changes remain session-only, and provider timeout/cancellation is not yet
implemented. The Island runtime uses resolved
theme tokens, configured bar geometry, live
clock/date, an XDG-backed search palette, a Niri-backed Dock, and a mounted
Orbit surface backed by live workspace/window metadata. Orbit now maps Niri's
reported tiled column/row positions and tile sizes into per-workspace card
geometry, and floating tile positions into output-local normalized bounds. If
any visible window in a workspace lacks valid placement metadata, the whole
workspace switches to a labelled window grid, avoiding overlaps between real
and synthetic placements. Projected bounds outside the output, invalid
dimensions, and aggregate tiled-size overflow also trigger that workspace-wide
fallback; tile coordinates are not individually clipped. The fallback mode is
included in tile accessible names. The generic window icon remains because
live thumbnail capture is not implemented. Orbit selection now lives in the runtime
controller: opening selects the active workspace and focused window (falling
back to the first available IDs), subsequent Niri snapshots preserve valid IDs
and clear stale ones, and workspace/window arrows, Tab/Shift+Tab follow the
`iris.orbit.keys` setting. Clicking an unselected workspace/window selects it;
clicking an item in the current selection activates it, and Enter activates the
selected item. Search, recent/stashed actions, preview interactions, and the
QML Delete, Ctrl+M, Ctrl+F, and Ctrl+workspace-arrow actions remain
unimplemented. ii
Dock is mounted as a separate output-scoped surface. Its default
`separatePinnedFromRunning` mode now shows resolvable pinned-only XDG entries in
configured order, followed by all running apps in retained first-observed
order; each app has one control with window indicators. Running app activation
cycles focus through Niri layout order, and idle pinned apps launch through the
shared desktop-entry adapter. App icons use the shared active-theme resolver,
with `image-missing` and then a neutral glyph as fallbacks. Drag reordering,
context menus, notification badges, rich per-style dock materials and
per-monitor icon scaling remain parity gaps. The ii Dock now has a native GPUI
anchored hover-popup host with a configurable delay, live per-app Niri window
cards, focus and close actions, Escape dismissal, pointer-transfer/dismissal
timers, and edge-specific popup placement. Its read-only cache adapter checks
PNG signature, chunk framing, CRCs, and header structure, and classifies cached
frames as fresh or stale, or as missing, invalid, or unreadable. The card
renderer also has loading and fallback presentations, but the current host
does not initiate screenshot capture or supply a pending-capture state. Popup
behavior and image decoding have not been verified in a live compositor
session, so this implementation is not yet a parity claim.
ii Overview is
mounted as an output-scoped overlay with live Niri
workspace/window groups and keyboard selection, plus an XDG application grid,
search editor, launch action, and visible launch errors; Dashboard and Actions
remain unavailable. The ii volume mixer is mounted from the panel launcher as
an output-scoped dialog. A shared PipeWire snapshot provider feeds read-only
Output and Input tabs; the Music tab is unavailable. Tab preference is kept
per output in memory, the default route is unknown and shown as such in the
direction-specific selector header. The current Output picker lists raw PipeWire nodes using
description/name/ephemeral registry-ID fallbacks; it does not resolve friendly
device names or identify the default output. Stream fields remain optional: an
absent scalar volume is labeled unknown and its level fill is omitted, while an
absent mute field is labeled unknown. Per-channel values are not averaged; only
the scalar node volume is displayed. Levels use a fixed maximum of 1.0. Volume,
mute, and routing controls remain visible but disabled in read-only mode. The
GPUI rows do not yet match QML stream grouping, icon resolution, or sink level
range behavior. The selector disclosure can open the device list, but no route
can be selected. The Input tab filters PipeWire `Audio/Source` devices and
`Stream/Input/Audio` capture streams, also using raw names/IDs and read-only
snapshots; its default source is unknown. Input volume and mute controls are
shown only as disabled presentation controls, with no write callbacks. Fixed
1.0 scaling, raw node identity, and the lack of a dialog focus trap and
tab-arrow navigation remain parity gaps for both directions. These are known
gaps, not claims of completed audio-mixer migration. Theme/config adapters and
a family composition host are in place. Most QML surfaces remain
unported, including
background materials/effects, settings/editors, widgets, dock context menus and
remaining preview capture flows, many transient and security panels,
configuration reload, family transitions, and production runtime lifecycle.

The current theme contract provides a shared Material 3 base to the three
environment plugins (`ii`, `waffle`, and `iris`). `ThemeTokens.material` holds
typed generated palette roles plus shared typography, shape, and spacing;
`ThemeTokens.material.overrides` is the plugin-specific role override slot.
`ii` horizontal surfaces already receive `ThemeTokens`, and the vertical bar
now applies generated surface, active, foreground, muted, accent, and outline
roles when available. Waffle and Iris receive the same base through their
existing composition inputs, while retaining their present family-specific
renderers. This completes a token/context foundation only: reusable Material
GPUI controls, common component rendering, broader plugin-specific overrides,
and dynamically loaded third-party plugins remain unimplemented. Missing
generated palette entries remain unset; the code does not synthesize the QML
wallpaper-derived or style-specific palette.

Keep QML available throughout the migration. Each slice must be reviewed
against its source, including visual states, configuration bindings,
interaction, and output behavior. Do not change the default launcher or remove
Quickshell dependencies before every supported family and surface has a GPUI
counterpart and has passed the acceptance gates in `GPUI_ACCEPTANCE.md`.
