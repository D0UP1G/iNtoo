# GPUI Waffle notification and calendar adapter contract

This document records the source behavior needed to bind
`prototypes/gpui-shell/src/families/waffle/notification_center.rs` to iNtoo's
existing notification and calendar services. It distinguishes persisted data
from live service capabilities so the GPUI prototype does not invent items or
advertise unsupported actions.

## Current GPUI boundary

The renderer accepts `NotificationCenterInput`:

- `groups: Vec<NotificationGroup>` and `notification_count`, `unread_count`,
  `silent`, `search_query`, `notification_state`;
- a month grid (`weekdays`, `calendar_days`, `selected_date`), selected events,
  and `calendar_state`;
- callbacks for dismissing one notification/group, invoking an action, toggling
  DND, clearing all, searching, selecting dates/months, and focus controls.

The Rust prototype now declares `notification_service`, and that module contains
an opt-in live D-Bus protocol boundary. It is not started by `main` or
`surface_manager`, and its event receiver is not mounted on the GPUI executor.
The notification overlay still receives `unavailable_input` with empty groups
and no-op callbacks; the Waffle taskbar still hardcodes unread count to zero and
silent/DND to false. There is still no calendar reader/syncer or TimerService
adapter. Empty UI snapshots therefore do not prove service availability. Keep
`Loading`/`Unavailable` explicit until the service is mounted and a real model
snapshot has been loaded.

## Notification source of truth

`services/Notifications.qml` owns a Quickshell
`Quickshell.Services.Notifications.NotificationServer` (the desktop
`org.freedesktop.Notifications` server), plus iNtoo policy, grouping, popup
state, and persistence. A GPUI replacement must become the DBus owner to receive
new notifications; it cannot simultaneously rely on this Quickshell server as
an independent in-process API. Plan a deliberate single-owner cutover before
enabling the GPUI notification service.

The persistent file is `Directories.notificationsPath`, namely
`$XDG_STATE_HOME/user/notifications.json` (normally
`~/.local/state/user/notifications.json`). It contains a JSON array of objects
with these fields:

```json
{
  "notificationId": 42,
  "actions": [],
  "appIcon": "",
  "appName": "Example App",
  "body": "Notification body",
  "image": "",
  "summary": "Notification title",
  "time": 0,
  "urgency": "normal"
}
```

These are field names/types observed in `notifToJSON`; the values above are
illustrative schema only and are not sample notification data. On restore,
QML intentionally discards actions (`actions: []`): action identifiers cannot
be invoked after the sending application's live DBus notification has gone
away. The wrapper also has runtime-only `popup`, `isTransient`, and
`notification` state. `popup` drives unread counting (unless the legacy counter
setting is on); it is not currently persisted. Old saved rows therefore do not
reliably encode read/unread state.

Live QML operations and semantics:

| GPUI input/callback | QML operation / contract |
| --- | --- |
| `silent`, `on_toggle_silent` | `Notifications.silent`, persisted at `notifications.silent` in Config. |
| `on_clear_all` | `Notifications.discardAllNotifications()`: clear history, dismiss live tracked server notifications, emit discard-all. |
| `on_dismiss_notification(id)` | `Notifications.discardNotification(id)`: remove persisted row, cancel timeout, dismiss matching live server notification, emit discard. |
| `on_dismiss_group(app_id)` | Equivalent to dismissing each current row in that app group; source helper `discardNotificationsForApp(appName)` takes app name, not a separate stable app ID. |
| `on_notification_action(id, action_id)` | `Notifications.attemptInvokeAction(id, identifier)` works only for a still-tracked live notification; action is invoked and the notification is dismissed. Restored rows have no actions. |
| `groups` / count | QML groups by exact `appName`, newest group time first; group count includes all retained notifications. |
| `unread_count` | Default is popup count. With `notifications.useLegacyCounter`, it is a separate manual counter. Per-app unread is based on popup notifications. |
| search | QML filters app name, summary, or body, case-insensitively; group ordering is preserved. |

Urgency is represented as `Low`, `Normal`, or `Critical` enum string in QML
wrappers. Ingress filters blocked apps, rate-limits noncritical notifications,
and may suppress popups under DND, quiet-hours, game-mode, or an open notification
surface; notifications can still be retained in history. DND itself is
persisted configuration. The Rust adapter must not infer unread state from
timestamp or urgency.

For a real binding, use a GPUI-owned asynchronous DBus notification service
which implements the freedesktop notifications methods/signals and preserves
live notification IDs/actions long enough to dispatch invocation and dismissal.
Read the existing JSON file as history only, merge it according to an explicit
ID/persistence policy, and write compatible rows atomically. Do not implement
only file polling and then wire the live action callbacks to no-ops: that would
render actions the sender cannot receive and would leave new notifications
unavailable.

## Calendar source of truth

There are two independent sources; the calendar pane merges them:

1. `services/Events.qml` stores local reminders in
   `$XDG_STATE_HOME/user/events.json`, shaped as
   `{ "nextId": number, "events": [...] }`. Each event has `id`, `title`,
   `description`, ISO `dateTime`, `category`, `priority`, `reminderMinutes`,
   `recurrence`, `notified`, `reminderNotified`, and `createdAt`. Upcoming/date
   queries hide already-notified events; selected-day history includes them.
   `Events` mutates this JSON and emits add/remove/update/trigger signals.
2. `services/CalendarSync.qml` reads configured ICS URLs, fetches them with
   curl, parses with `services/calendar_ics.js`, caches results in
   `$XDG_STATE_HOME/user/calendar-sync-cache.json`, and periodically refreshes
   while enabled and online. It reports `enabled`, `ready`, `fetching`,
   `sourceStatuses`, `events`, `eventsUpdated`, and source errors. Parsed event
   fields include `uid`, `title`, `description`, `startDate`, `endDate`,
   `allDay`, `sourceId`, and `sourceColor`. External source configuration is
   `calendar.externalSync.{enable,sources,refreshMinutes}`.

`CalendarWidget.qml` combines both sources. Calendar day dots count upcoming
local events plus matching external events; selected-day detail includes all
local events (including notified) plus external events, sorted all-day first
then by start time. Upcoming view checks the next three dates, merges the two
sources, sorts similarly, and caps at five. Local event dots use the accent
color; external dots use unique source colors. Date selection, month navigation,
collapse, locale, and `calendar.showUpcoming` are presentation/config state.

The GPUI `CalendarDay` and `CalendarEvent` types can represent the display
projection, but they are not the source schema: the adapter must merge the two
schemas, resolve source colors and local/external labels, generate locale-aware
month and weekday labels, and distinguish `Loading`, `Ready` (including a
confirmed empty calendar), and `Unavailable`. Reading the two JSON files alone
does not perform configured ICS refresh, network/error state, or source updates.

## Host lifecycle and interaction requirements

The host owns the adapter and supplies snapshots/callbacks to the renderer.
It should subscribe to DBus notification arrival/close/action events, local
event-file changes, calendar refresh/source updates, and relevant config
changes, then publish new immutable GPUI inputs on the GPUI app executor.
Callbacks must carry stable IDs and route back to their originating service;
they must never mutate UI-owned copies as a substitute for service operations.

The existing QML notification center's surface lifecycle is defined in
`WaffleNotificationCenter.qml`: a full-output transparent click-away surface is
on `WlrLayer.Top`; the panel is a separate right-anchored `WlrLayer.Overlay`
surface with exclusive keyboard focus while visible. `NotificationCenterContent`
closes on Escape, clears notifications on Delete, toggles DND on Ctrl+D, and
closes the panel before starting a focus session. `FocusFooter.qml` uses
`TimerService.focusTime`, bounded from 5 minutes to 120 minutes in 5-minute
steps; the action starts/stops Pomodoro and may close the panel. The current
GPUI `FocusControls` contract does not yet carry every pause/reset/config
operation, so those must be explicitly added before claiming behavioral parity.

Follow the already-defined GPUI surface-host policy: keep the persistent bar on
the top layer and put the panel plus click-away backdrop in a separate
output-sized overlay surface with exclusive keyboard input only while open.
Close and focus restoration must be idempotent, scoped to the owning output,
and safe when that output disappears. Do not mount the notification/calendar
panel on the bar surface or expose a keyboard-exclusive surface when closed.

## Implementation decision / blocker

The declared Rust module is only a protocol service slice, not a complete
notification adapter or UI binding. The existing calendar data files can
support a read-only bootstrap, but do not provide configured external-calendar
synchronization. Keep unsupported data unavailable and do not wire fabricated
notifications or events into the renderer. The next implementation step is a
host-owned service handle and event pump, then real per-output overlay inputs
and callbacks; calendar and focus adapters remain separate work.

## Prototype service ownership and state flow

The prototype notification server is an explicit opt-in and is disabled in
normal startup. It may request `org.freedesktop.Notifications` only when the
developer explicitly enables the GPUI owner mode (currently
`INTOO_GPUI_OWN_NOTIFICATIONS=1`). Name acquisition must use D-Bus
`DO_NOT_QUEUE` and must not replace or queue behind an existing owner. If
Quickshell or another notification daemon already owns the name, GPUI reports
an ownership conflict and leaves the current service untouched. Live ownership
verification therefore belongs in an isolated session bus; never test it by
starting a second owner on the user's production session bus.

Separate transport ownership from notification data readiness. A successful
name acquisition proves only that GPUI can receive new DBus requests. It does
not prove that persisted history is loaded or that unread state is known. The
overlay must continue to show `Loading`/`Unavailable` until history restore and
the notification model are ready. Owner states must distinguish disabled,
connecting, ready, unavailable/name-owned, error, and stopping; shutdown must
release the bus name and cancel its worker before the runtime exits.

Persistence and read state have not yet been implemented. When they are added,
they must preserve protocol `uint32` IDs separately from persisted/UI IDs and
handle updates (`replacement_id`), restart collisions, and wraparound without
misrouting `CloseNotification` or `ActionInvoked`. Writes must be serialized
and atomic. Restored rows have no live action handles and must have no actions;
their historical unread state is unknown, so they must restore as read.

The operation model must keep timeout/read separate from dismissal:

- `Timeout` / `MarkRead` clears popup-unread state and retains the history row;
  it does not send `CloseNotification`.
- `Dismiss` removes one row, persists the change, and closes its live DBus
  notification when one exists. Reentrant external-close callbacks are
  idempotent.
- `ClearAll` removes all rows and closes all tracked live notifications.
- `InvokeAction` is valid only while a live sender/action handle exists; invoke
  the action and then dismiss the corresponding notification. Restored rows
  cannot invoke actions.

Groups are keyed by exact `appName`, matching QML's group model; carry sender
application identity separately when it is needed for policy or launch actions.
Popup unread state is runtime state and cannot be reconstructed from saved
history. Ingress received while popups are inhibited is retained as read. If
legacy manual unread mode is enabled, its count must remain separate from
popup-unread notifications. Loading saved history must complete before the
service publishes a ready notification snapshot; queued ingress during restore
must be merged before the first published snapshot. The UI host owns immutable
snapshot revisions and applies worker events on the GPUI executor.

The intended invariant is that the first backend slice implements only opt-in,
fail-fast D-Bus ownership and the live protocol event boundary. Persisted
history, the complete ingress policy, unread reconciliation, and Waffle UI
binding must remain explicitly unavailable until implemented; transport Ready
must not be presented as notification-data Ready.

### Current source gaps (not intended guarantees)

- `main.rs` only declares `mod notification_service`; neither `main` nor
  `surface_manager::start` calls `NotificationServiceHandle::start_if_opted_in`.
  Consequently the env opt-in currently has no effect in the running prototype.
- No code subscribes to the service's `Receiver<NotificationEvent>`. The
  existing `surface_manager` notification receiver handles only per-output
  `Toggle`/`Close` surface events. It does not pump service events onto the GPUI
  executor, update snapshots, or refresh windows.
- `open_notification_overlay` always builds `unavailable_input`. The overlay
  stores its input at construction and exposes no snapshot update method; all
  notification, DND, clear, group, and action callbacks are no-ops. The
  taskbar's unread and silent values remain hardcoded as `0` and `false`.
- `NotificationSnapshotState` remains `Loading` for both `Starting` and
  `OwnerReady`; there is no transition to `Ready`, history restore, or model
  revision. `Unavailable` is used for disabled and errors. The intended
  distinction between transport readiness and data readiness is sound, but
  there is no UI-facing state yet.
- The service exposes `shutdown()`, which releases the acquired name and emits
  `Stopped`; application lifecycle does not yet call it because the service is
  not wired into startup.
- `Builder::name` uses zbus's built-in `DO_NOT_QUEUE` behavior. Project code
  explicitly disables name replacement and replacement of existing owners.
  This is source/API evidence only; no D-Bus session runtime check has been
  performed. Never test ownership against the production session bus.
- `expire_timeout_ms` is retained with the live item but expiry is not
  scheduled or honored. Image, markup, persistence, DND, ingress
  filtering/rate limiting, popup/unread reconciliation, and policy projection
  are also absent. Only `actions` and `body` are advertised.
- D-Bus `CloseNotification`, host `dismiss`, and `invoke_action` now pass
  through one worker. Notify state mutation and publication, plus worker
  state/event transitions, use a shared ordering lock. This prevents concurrent
  close/action operations from publishing divergent snapshots. No Waffle
  callback consumes the event stream or calls the handle yet.

These are implementation gaps to close in bounded slices. Do not describe the
service as started, mounted, production-safe, history-ready, or visually
behaviorally complete until the corresponding source paths exist and are
reviewed.

Implementation intent: `prototypes/gpui-shell/src/notification_service.rs`
contains `start_if_opted_in`, which checks
`INTOO_GPUI_OWN_NOTIFICATIONS=1`. Since no caller invokes it today, launching
the prototype does not start this service even when that variable is set. Name
conflict/fail-fast behavior is not yet independently established; see the
source gaps above.

The source defines handlers for `GetCapabilities`, `GetServerInformation`,
`Notify`, and `CloseNotification`; it retains live action identifiers, emits
`ActionInvoked`/`NotificationClosed`, and publishes Added/Replaced/Closed
events. These behaviors have not yet been wired into app startup or observed
through a running GPUI host. In the service model, ownership status is separate
from snapshot state: acquisition reports `OwnerReady`, while notification
snapshot data remains `Loading`.

This is not yet a replacement notification center. It has no history restore or
atomic persistence, policy/rate limiting/DND/quiet hours/sound or popup expiry,
durable unread/read state or legacy-counter support, or QML ID-offset mapping.
The host does not yet consume the event stream or route Waffle overlay
callbacks. Only `actions` and `body` are advertised; images and markup are not
implemented. Calendar and focus services remain unavailable. No production
cutover has been performed.

Current source review details: `Notify` stores the sending unique bus name and
accepts a replacement only when the ID belongs to that sender. New IDs skip
occupied IDs after counter wrap. `CloseNotification` returns InvalidArgs for
unknown IDs. Outgoing host commands are processed by one worker; a dismiss is
removed from the live store and published as Closed only after the bus signal
succeeds, and action invocation emits ActionInvoked before NotificationClosed.
The `expire_timeout_ms` value is carried in each live item, but the adapter does
not yet schedule expiry or honor positive/default timeouts. This remains an
explicit protocol gap. `shutdown()` releases the owned name and changes status
to Stopped. Startup/fail-fast ownership behavior still requires independent
D-Bus runtime verification; no session test was run.
