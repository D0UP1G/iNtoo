# GPUI visual migration acceptance criteria

This document defines observable acceptance criteria for the GPUI port. It is
a verification specification, not evidence that any criterion has passed or
that the existing QML visuals have been fully migrated. The QML composition
roots remain the reference for lifecycle, feature gates, and surface ownership:

- `shell.qml`
- `modules/ii/critical/ShellIiCriticalPanels.qml`
- `modules/ii/ShellIiPanelsImpl.qml`
- `modules/waffle/critical/ShellWaffleCriticalPanels.qml`
- `modules/waffle/ShellWafflePanelsImpl.qml`
- `modules/iris/critical/ShellIrisCriticalPanels.qml`
- `modules/iris/ShellIrisPanelsImpl.qml`

Visual parity means the same supported configuration and service state yield
the same visible content, geometry, material, transitions, interaction regions,
and lifecycle behavior. Use paired QML and GPUI captures at matching output
resolution, scale, theme, and data state. Captures alone do not prove input,
focus, lifecycle, or failure parity.

## Horizontal ii bar slice

The horizontal ii bar uses five ordered layout zones (`left`, `centerLeft`,
`center`, `centerRight`, `right`). Each configured zone appears once, in that
order; the two right-aligned zones must not be replaced by duplicates of earlier
zones. Workspace buttons expose their number, active state, and occupancy to
assistive technology in addition to their visual state.

The current GPUI prototype also renders a text resource strip in Memory,
Temperature, CPU, GPU order. CPU and memory readings come from `/proc` at the
provider's three-second recommended sampling interval. The strip honors its
`bar.resources.show*Indicator` visibility toggles and configured warning or
caution thresholds, but it does not yet apply QML's `alwaysShowTemp`,
`alwaysShowCpu`, or `alwaysShowGpu` rules or the related active-player/media-title
visibility gates. QML's `services/ResourceUsage.qml` already provides
temperature sensing and GPU telemetry; these providers are not connected to
GPUI, so both entries show unavailable values. Swap is intentionally omitted
from the horizontal strip. This status is not evidence that its visuals match
QML.

## Vertical ii bar slice

The current GPUI prototype uses a 64 px vertical surface with a 46 px exclusive
zone; `bar.bottom` selects the right edge when true and the left edge when
false. It centers content between fixed 56 px / 88 px top and bottom reserves
as an approximation of QML's `middleHost`, not measured edge-section geometry.
Its vertical taskbar groups live Niri windows by app ID and Niri layout order,
adds resolvable XDG entries from `ii.dock.pinnedApps`, and focuses/cycles a
running group's window IDs or launches an idle pinned entry on click or
Enter/Space. The task list uses a viewport capped to the remaining middle
height, which can be zero, with vertical scrolling when space is available.
Taskbar visuals remain partial and do not establish full QML parity. This is a
status note; the scenarios below remain acceptance criteria, not passed checks.

```gherkin
Feature: ii vertical bar visual and interaction parity

  Background:
    Given the ii panel family is selected
    And the shell configuration is ready
    And the "iiVerticalBar" panel is enabled

  Scenario: Focus a workspace with pointer or keyboard
    Given the vertical bar has workspace buttons with active and occupancy state
    When a workspace button is clicked, or activated with Enter or Space
    Then Niri focuses that workspace once
    And assistive technology receives its number, active state, and occupancy

  Scenario: Render live resource indicators from available metrics
    Given resource providers return CPU, memory, and swap percentages
    And the corresponding indicators are configured as visible
    When the vertical bar renders
    Then each metric uses a circular progress indicator matching QML
    And its warning color changes at its configured threshold
    And a missing metric is announced as unavailable rather than 0 percent
    And a configured GPU indicator remains unavailable until a GPU provider exists

  Scenario Outline: Place the bar on the configured physical edge
    Given "bar.vertical" is true
    And "bar.bottom" is <right_side>
    When the shell renders on output "<output>"
    Then the vertical bar is attached to the <edge> edge of "<output>"
    And its width, screen rounding, corner treatment, and content alignment match the QML reference
    And its exclusive zone equals the configured base vertical bar width
    And the bar content is clipped to its visible shape
    And the transparent layer-shell area outside that shape does not intercept pointer input

    Examples:
      | right_side | edge  | output |
      | false      | left  | DP-1   |
      | true       | right | DP-1   |

  Scenario: Restrict the bar to configured outputs
    Given outputs "DP-1" and "HDMI-A-1" are connected
    And "bar.screenList" contains only "HDMI-A-1"
    When the shell renders the vertical bar
    Then exactly one vertical bar is visible on "HDMI-A-1"
    And no vertical bar or exclusive zone is created on "DP-1"

  Scenario: Fall back when every configured output name is stale
    Given output "DP-1" is connected
    And "bar.screenList" contains only the disconnected output name "OLD-OUTPUT"
    When the shell resolves the vertical bar outputs
    Then one vertical bar is rendered on the available output "DP-1"
    And the bar does not disappear from every output

  Scenario: Keep a configured output available after a hotplug change
    Given the vertical bar is visible on output "DP-1"
    When output "DP-1" is removed and output "DP-2" is added
    And the configured output list is empty or resolves to "DP-2"
    Then the old surface and reservation are removed from "DP-1"
    And one correctly positioned surface and reservation are created on "DP-2"
    And no stale pointer region remains on "DP-1"

  Scenario Outline: Hide and reveal the bar without stealing the desktop edge
    Given auto-hide is enabled
    And the bar is on the <edge> edge
    And neither the pointer hover region nor the Super-key reveal delay is active
    When the bar is hidden
    Then the bar content moves completely beyond the <edge> edge
    And only the configured hover strip can reveal it
    And the exclusive zone is zero unless "pushWindows" is enabled
    When the pointer enters the hover strip
    Then the bar reveals using the configured enter animation
    And the exclusive zone follows the configured "pushWindows" behavior

    Examples:
      | edge  |
      | left  |
      | right |

  Scenario: Reveal on a delayed Super-key press
    Given auto-hide and "showWhenPressingSuper" are enabled
    When the Super key remains pressed for the configured delay
    Then the vertical bar is revealed
    When the Super key is released before or after the delay
    Then the pending reveal timer is canceled or the temporary reveal is cleared
    And the bar returns to the pointer-driven visibility state

  Scenario: Suppress normal bar actions while the shell layout editor owns it
    Given the shell layout editor is actively dragging the vertical bar
    When the pointer presses or moves over the bar
    Then the edit frame receives the gesture
    And ordinary bar actions do not fire
    When the drag is canceled
    Then the bar returns to its original configured position without a stale drag proxy

  Scenario: Preserve independent scroll actions and context-menu direction
    Given the pointer is over the upper vertical bar section
    When the user scrolls
    Then the configured brightness action is applied
    When the pointer is over the lower vertical bar section
    And the user scrolls
    Then the configured volume action is applied
    When the user opens the bar context menu on a left-edge bar
    Then the menu opens toward the right of the bar
    When the user opens it on a right-edge bar
    Then the menu opens toward the left of the bar
    And focus loss or pointer exit closes the context menu as in the QML reference

  Scenario: Preserve visualizer rendering and process gating
    Given the visualizer is enabled and audio is playing
    And the output satisfies the configured visualizer multi-monitor mode
    And the bar background is enabled
    When the bar renders
    Then the selected visualizer type, density, color, opacity, fill, and motion match the QML reference
    And the visualizer process is not requested on an excluded output
    When audio stops, the bar is hidden, or game-mode minimal visuals apply
    Then the visualizer visibility and process demand follow the QML gating

  Scenario: Render optional vertical modules in configured combinations
    Given taskbar, battery, Bluetooth, network, audio, clock, date, and media modules have independent configured visibility
    When one or more services are unavailable
    Then unavailable indicators are hidden or show the same unavailable state as the QML reference
    And the remaining modules keep their configured order and spacing
    And a configured taskbar does not leave duplicate taskbar controls in the non-taskbar layout
    And short output height produces the same overflow, clipping, or compact behavior as the QML reference

  Scenario: Switch side while the bar is auto-hidden
    Given the bar is hidden on the left edge
    When "bar.bottom" changes so the bar moves to the right edge
    Then content, hover strip, blur region, rounding, and popups move to the right edge together
    And the old edge has no interactive region or exclusive-zone reservation
```

## ii vertical bar resource, battery, and media indicators

The current GPUI vertical bar is partial: CPU, memory, and swap snapshots are
connected and now render as circular rings with threshold warning colors; GPU
has no provider. UPower battery data is read from the system bus and MPRIS data
from the session bus; the vertical-bar loop polls both with a minimum two-second
delay between polls, with sequential bus reads able to stretch the cadence. The
configured `battery.low` threshold (default 20%, clamped to
0–100) drives the discharging warning color, which is suppressed while
charging. MPRIS toggle, previous, next, and volume intents use the player's
reported control capabilities. The OpenControls intent currently does nothing.
Polling is not signal-driven, and the provider does not explicitly recreate
cached bus connections after sampling failures. Player selection prefers the
first candidate reporting Playing in sorted bus-name order, then retries the
first candidate proxy; it does not preserve a user-selected player.
The scenarios below specify the required QML parity; they are not claims that
the current prototype passes them. Loading, unavailable, and no-device states
must come from explicit provider state, not be inferred from a numeric zero.

```gherkin
Feature: ii vertical bar live resource and device indicators

  Background:
    Given the ii panel family and vertical bar are enabled
    And the taskbar module is disabled so resource and media indicators are shown

  Scenario Outline: Render resource circular meters across the valid range
    Given the <metric> provider is ready with a valid percentage of <percent>
    And the <metric> indicator is configured as visible
    When the vertical bar renders
    Then the <metric> indicator shows the QML circular progress meter at <percent> percent
    And zero percent has an empty progress arc and full percentage has a complete arc
    And its displayed icon and geometry match the QML resource indicator

    Examples:
      | metric | percent |
      | memory | 0       |
      | memory | 100     |
      | swap   | 0       |
      | swap   | 100     |
      | CPU    | 0       |
      | CPU    | 100     |

  Scenario Outline: Apply configured warning thresholds at and above the boundary
    Given the <metric> warning threshold is <threshold> percent
    And the <metric> provider is ready with a valid percentage of <percent>
    When the vertical bar renders
    Then the indicator uses the warning color if and only if <percent> is at least <threshold>

    Examples:
      | metric | threshold | percent |
      | memory | 95        | 94      |
      | memory | 95        | 95      |
      | swap   | 85        | 86      |
      | CPU    | 90        | 90      |

  Scenario Outline: Keep resource provider states distinct from valid zero
    Given the <metric> provider reports <state>
    When the vertical bar renders
    Then it shows <presentation>
    And it does not render an unavailable or loading state as a zero-percent meter

    Examples:
      | metric | state       | presentation                       |
      | CPU    | loading     | explicit loading indicator         |
      | CPU    | unavailable | explicit unavailable indicator     |
      | swap   | unavailable | explicit unavailable indicator     |
      | GPU    | no provider | explicit unavailable indicator     |

  Scenario: Reject non-finite resource readings
    Given a resource provider returns NaN or positive or negative infinity
    When the vertical bar renders
    Then the value is treated as unavailable
    And no NaN or infinity text, invalid arc, or warning-color comparison is shown

  Scenario: Respect resource visibility independently
    Given CPU is configured hidden and memory is configured visible
    When the vertical bar renders
    Then no CPU meter or placeholder occupies a resource slot
    And the memory indicator remains visible in the configured order

  Scenario Outline: Render battery state without confusing absent hardware and charge
    Given the battery provider reports <state>
    When the vertical bar renders
    Then it shows <presentation>

    Examples:
      | state                              | presentation                                      |
      | loading                            | explicit loading indicator                       |
      | unavailable                        | explicit unavailable indicator                   |
      | no battery device                  | no battery indicator, matching QML availability  |
      | present at 0 percent, not charging | empty fill, 0 percent, low warning color          |
      | present at 100 percent, not charging | full fill, 100 percent, normal color            |
      | present at 10 percent, charging    | 10 percent with charging icon and normal color    |

  Scenario: Apply the configured low-battery boundary only while discharging
    Given a battery is present and the low threshold is 20 percent
    When charge is exactly 20 percent and the battery is not charging
    Then the battery indicator uses the low warning color
    When the same battery is charging
    Then the low warning color is suppressed and the charging icon is shown

  Scenario Outline: Present MPRIS metadata and playback progress
    Given the MPRIS provider reports <player_state>
    And the available metadata is <metadata>
    And the track duration is <duration>
    When the vertical bar renders
    Then the media indicator shows <presentation>

    Examples:
      | player_state | metadata                       | duration | presentation                                             |
      | loading      | not yet available              | unknown  | explicit loading state                                  |
      | unavailable  | not available                  | unknown  | explicit unavailable state                              |
      | no player    | none                           | none     | no-player state with zero progress                      |
      | ready        | empty title and artist         | positive | fallback title, omitted empty artist, valid progress    |
      | ready        | title and empty artist         | zero     | title, omitted artist, no duration progress             |
      | ready        | title and artist               | positive | title, artist, and position divided by positive duration |

  Scenario: Do not expose unsupported playback controls from metadata alone
    Given a player provides metadata but its action capabilities are absent
    When the vertical bar renders its metadata-only indicator
    Then it does not show an enabled previous, play/pause, or next action
    And any separately implemented playback controls reflect the player's reported capabilities
```

## ii right-sidebar Media card

The Media card uses the shared MPRIS snapshot and action runtime only in the
compact right sidebar. Standard `SidebarRightContent` omits it. In compact
mode, an explicit `sidebar.right.controlsSectionOrder` controls membership
and order among the implemented `sliders`, `toggles`, and `media` sections;
unsupported `devices` and `quickActions` entries are omitted without
placeholders. These scenarios record the current visual slice and its known
gaps; they do not claim full QML parity.

```gherkin
Feature: ii right-sidebar Media card

  Scenario: Omit Media from the standard right sidebar
    Given the standard `SidebarRightContent` is rendered
    Then it does not include a Media section

  Scenario: Place compact Media relative to supported controls
    Given the compact sidebar's `sidebar.right.controlsSectionOrder` includes `media`
    When the compact right sidebar renders its supported controls
    Then the Media card appears in relative order with Quick Sliders and Quick Toggles
    And membership and order match the explicit config list for implemented ids
    And `devices` and `quickActions` do not create placeholder cards

  Scenario: Hide the Media section when there is no player
    Given compact `sidebar.right.controlsSectionOrder` includes `media`
    And the shared MPRIS runtime reports NoPlayer
    When the compact right sidebar renders its Media section
    Then the Media card is hidden

  Scenario Outline: Show the reported player state and known metadata
    Given the shared MPRIS runtime reports <state>
    And title, artist, and position percentage are independently optional
    When the compact right sidebar renders its Media card
    Then the card shows <state_label>
    And it shows each non-empty title or artist that is available
    And it shows only the supplied percentage as read-only progress

    Examples:
      | state       | state_label      |
      | Unavailable | Media unavailable|
      | Playing     | Playing          |
      | Paused      | Paused           |
      | Stopped     | Stopped          |

  Scenario: Gate transport actions by MPRIS capabilities
    Given a player snapshot supplies independent previous, toggle, and next capabilities
    When the Media card renders its transport row
    Then each previous, play/pause, or next button appears only when its matching capability is true
    And activating a visible button dispatches the corresponding action to the shared runtime

  Scenario: Disclose remaining Media card parity gaps
    Given the current GPUI Media card is rendered
    Then it has no artwork, elapsed or total time, duration, seek control, or player switcher
    And it has no shuffle or loop controls
    And it uses generic sidebar styling without Media-specific QML style or configuration bindings
```

## ii Volume Mixer visual slice

```gherkin
Feature: ii audio mixer presentation

  Scenario: Expose current read-only Output limitations honestly
    Given the Output tab receives a PipeWire snapshot
    Then the device list contains raw output nodes and does not claim a default route
    And the selector opens a device list but does not offer a route-changing action
    And stream rows are not grouped or enriched with QML application icons
    And volume and mute controls remain disabled with no write action available
    And the dialog does not claim a focus trap or tab-arrow navigation

  Scenario: Preserve unknown stream fields in the Output presentation
    Given a PipeWire output stream has a volume value but no mute value
    Then the mixer does not present its mute state as known
    Given a PipeWire output stream has no volume value
    Then the mixer does not present a numeric level as known
    And it omits the level fill from the bar

  Scenario: Disclose Output level scaling limits
    Given a PipeWire output stream has only per-channel volume values
    Then the current level bar does not average them into a scalar level
    And the current level bar uses a fixed maximum of 1.0
    And the mixer does not claim the QML sink-specific level range

  Scenario: Distinguish output and input device selection
    Given the mixer is shown for the output or input direction
    Then the device selector displays the matching supplied direction glyph
    And its disclosure indicator reflects whether the picker is open

  Scenario: Present PipeWire input sources as a read-only snapshot
    Given the Input tab receives a PipeWire snapshot
    Then its device picker includes only `Audio/Source` nodes
    And its stream list includes only `Stream/Input/Audio` capture nodes
    And the heading says the default input is unknown
    And device names may fall back to raw names or ephemeral registry IDs
    And volume and mute controls are visible but disabled with no write action

  Scenario: Preserve input stream volume and mute as optional values
    Given an input stream snapshot omits its volume or mute field
    Then the Input row does not substitute zero volume or an unmuted state
    And the volume level uses a fixed maximum of 1.0 when a scalar is present
    And per-channel values are not collapsed into a scalar
    And the dialog does not claim a focus trap or tab-arrow navigation

  Scenario: Show empty device picker state
    Given the device picker is open
    And the host supplied no devices
    Then a direction-specific empty state is shown in the picker

  Scenario: Present unavailable streams honestly
    Given a stream snapshot is marked unavailable
    Then its row uses muted visuals and says Unavailable
    And it does not display volume or mute state
    And volume and mute controls are omitted

  Scenario Outline: Distinguish stream provider state from an empty ready result
    Given the mixer receives <state> from the stream provider
    When the stream list is empty
    Then the mixer shows <presentation>

    Examples:
      | state       | presentation                    |
      | loading     | Loading audio streams           |
      | unavailable | Audio streams unavailable       |
      | error       | Unable to load audio streams    |
      | ready       | direction-specific no-apps text |

  Scenario: Scale the stream level visualization to its track
    Given an available stream has a supplied volume and maximum
    Then the level fill is proportional to volume divided by maximum
    And the displayed percentage is derived from the supplied volume
```

## ii overlay launcher row

The overlay launcher row remains at the top-right of the overlay. The current
buttons preserve toggling and keyboard activation, but this is only a visual
slice and does not establish full button parity.

```gherkin
Feature: ii overlay launcher visual slice

  Scenario: Preserve overlay launcher placement and toggle activation
    Given an enabled ii panel launcher is rendered
    Then the launcher row is positioned at the overlay's top-right
    And a closed-state button has a transparent resting surface
    And hover and visible-focus styling are available
    When the user clicks the button or activates it with Enter or Space
    Then it dispatches the existing panel toggle action

  Scenario: Record launcher presentation and left sidebar limits
    Given the GPUI ii overlay launcher row is rendered
    Then the left-sidebar button reflects the host's `left_open` state
    And its hover appearance differs for open and closed states
    And distro, avatar, ping, and theme or launcher-style/config bindings are absent
    And the opened left sidebar still has unavailable tab content
```

## ii left feature sidebar tabs

The feature sidebar exposes config-derived tab entries, selection, and keyboard
focus handling, but does not yet connect any selected tab body to a content
adapter. This status is based on static code review only; live UI and
assistive-technology behavior have not been validated.

```gherkin
Feature: ii left feature sidebar tab strip

  Scenario: Render enabled tabs in configured order
    Given the host supplies enabled tab records and normalized `sidebar.left.tabOrder`
    When the left feature sidebar renders its tab strip
    Then it shows supported enabled tabs in the supplied order
    And each tab reports its selected state and selects its stable ID on click or Enter/Space
    And the selected ID updates the current sidebar state
    And only the selected tab is in the tab sequence

  Scenario: Keep unimplemented tab behavior visible as a limitation
    Given a left feature sidebar tab is selected
    Then its content body remains unavailable
    And labels and Material Symbols use hardcoded English mappings
    And Left/Right wrap across list boundaries, and Home/End move selection and focus using stable handles
    And keyboard focus scrolls the target tab into view
    And it has no reorder/edit interaction or `aria-controls`/tabpanel relationships
```

The keyboard/focus logic was statically reviewed by #24. No interactive UI
session or accessibility-tree validation has been performed.

## ii GlanceHeader clock visual

The clock-only visual appears below the left-sidebar tab strip when Widgets is
selected. It consumes the existing snapshot's opaque clock label; no separate
time/date formatting is performed. Its state is Loading before the first
snapshot, Ready when a label is available, and Unavailable after a snapshot
without a label. Locale and clock-format configuration parity, service
buttons, and widget content remain outside this slice. #24 completed static
review after the accessible-name fix; no interactive UI session or
accessibility-tree validation has been performed.

```gherkin
Feature: ii GlanceHeader clock presentation

  Scenario: Render a ready clock label verbatim
    Given the selected left sidebar tab is Widgets
    And the snapshot supplies `clock_label`
    When the clock visual renders
    Then it displays that single opaque string unchanged
    And it does not derive or append a date
    And it does not render service buttons

  Scenario: Omit the clock visual for other selected tabs
    Given a left sidebar tab other than Widgets is selected
    Then the GlanceHeader clock visual is absent

  Scenario Outline: Distinguish clock loading and unavailable states
    Given the selected left sidebar tab is Widgets
    And the clock state is <state>
    When the clock visual renders
    Then it presents the matching <presentation>
    And it does not synthesize a time or date

    Examples:
      | state       | presentation              |
      | Loading     | explicit loading state    |
      | Unavailable | explicit unavailable state|
```

## left Widgets MediaPlayerWidget

This is a partial implementation. #47 statically reviewed the placement; no
live UI session or accessibility-tree validation has been performed. It does
not establish QML parity.

```gherkin
Feature: left Widgets MediaPlayerWidget

  Scenario: Gate the player widget by tab selection and config
    Given the left sidebar receives `sidebar.widgets.media` and normalized `sidebar.widgets.widgetOrder`
    When the sidebar renders with Widgets not selected
    Then the MediaPlayerWidget is absent
    When the sidebar renders with Widgets selected
    Then the MediaPlayerWidget appears only if `sidebar.widgets.media` is true
    And normalized `sidebar.widgets.widgetOrder` includes `media`

  Scenario: Place the player card in the Widgets body
    Given Widgets is selected and MediaPlayerWidget is enabled and ordered
    When the feature sidebar renders
    Then the player card appears in the body below the custom header
    And it appears before the generic unavailable section
    And `widgetOrder` gates membership but does not position media relative to unimplemented widget IDs

  Scenario: Require an active MPRIS state and a non-empty title
    Given Widgets is selected and the widget is enabled and ordered
    When MPRIS reports Playing, Paused, or Stopped with a non-empty title
    Then the shared media card is rendered with its known metadata
    When MPRIS reports NoPlayer or Unavailable, or the title is empty
    Then the MediaPlayerWidget is omitted

  Scenario: Use the shared MPRIS card and capability-gated actions
    Given Widgets is selected and MediaPlayerWidget is enabled
    And the shared MPRIS runtime supplies a player snapshot
    When the MediaPlayerWidget renders
    Then it presents the supplied MPRIS state and known metadata through the shared media card
    And it exposes only transport actions supported by the player's capabilities

  Scenario: Preserve the current visual scope
    Given the MediaPlayerWidget renders a player snapshot
    Then it does not invent artwork or a Cava visualization
    And it has no YT Music-specific visual or animation
    And it remains a partial visual slice without a full QML parity claim
```

## left Widgets StatusRings

StatusRings is partially implemented as up to three circular rings. CPU and
RAM use real `/proc` samples; battery uses UPower. #24 completed static review;
no live UI or accessibility-tree validation has been performed. This partial
slice does not establish full QML parity. After cached data ages past
eight seconds, every enabled ring shows an em dash, a “Stale sample” label, and
an accessible stale state. A fresh UPower Unavailable report remains distinct
from stale cache. The per-display cache prefers recent VerticalBar samples;
TopBar supplies a fallback only when the VerticalBar source has not published
in eight seconds. Relative ordering is applied among implemented
MediaPlayerWidget and StatusRings entries only; unimplemented widget IDs have
no rendered position.

```gherkin
Feature: left Widgets StatusRings

  Scenario: Gate StatusRings by selected tab, enablement, and membership
    Given the host supplies `sidebar.widgets.status` and normalized `sidebar.widgets.widgetOrder`
    When the sidebar renders with Widgets not selected
    Then StatusRings is absent
    When the sidebar renders with Widgets selected
    Then StatusRings is present only when `sidebar.widgets.status` is true
    And normalized `sidebar.widgets.widgetOrder` includes `status`

  Scenario: Respect supported per-ring visibility toggles
    Given Widgets is selected and StatusRings is enabled and ordered
    When the status widget renders
    Then CPU, RAM, and battery rings follow `sidebar.widgets.statusRings.showCpu`, `sidebar.widgets.statusRings.showRam`, and `sidebar.widgets.statusRings.showBattery`
    And disabled rings are omitted

  Scenario: Use only real supported metric sources
    Given StatusRings receives live metric snapshots
    Then CPU and RAM use readings from `/proc`
    And battery uses UPower data
    And missing CPU, RAM, or unavailable battery readings show a dash and accessible unknown label
    And an absent battery device has no battery ring
    And no ring is filled with a fabricated zero or placeholder value

  Scenario: Mark old cache values as stale
    Given the per-display StatusRings cache has not been updated for eight seconds
    When StatusRings renders its enabled rings
    Then each enabled ring shows an em dash and the Stale sample label
    And its accessible name reports stale data

  Scenario: Keep the initial cache miss distinct from stale data
    Given StatusRings has not received its first per-display sample
    When StatusRings renders its enabled rings
    Then each enabled ring shows an em dash and accessible unknown value
    And no ring is marked Stale sample

  Scenario: Distinguish a fresh UPower failure from stale cached data
    Given a fresh StatusRings snapshot reports UPower Unavailable
    When the battery ring renders
    Then it shows the unavailable battery state with an unknown value
    And it does not label the sample as stale

  Scenario: Follow configured order among implemented widget entries
    Given Widgets is selected and MediaPlayerWidget and StatusRings are both enabled and ordered
    When the Widgets body renders
    Then the relative order of those entries matches normalized `sidebar.widgets.widgetOrder`
    And no placement is claimed for unimplemented widget IDs

  Scenario: Prefer the recent vertical-family cache owner
    Given the per-display cache contains a VerticalBar StatusRings sample newer than eight seconds
    When TopBar samples resources for the same display
    Then it does not replace the recent VerticalBar sample or start a competing battery poll
    When no VerticalBar sample has been published for eight seconds
    Then TopBar may publish its resource and battery fallback sample

  Scenario: Omit disk and temperature from the bounded slice
    Given the QML configuration enables disk and temperature rings
    When the bounded GPUI StatusRings slice renders
    Then disk and temperature rings remain omitted
    And the slice does not claim QML parity
```

## left Widgets QuickLaunch (partial)

QuickLaunch is implemented as a partial slice. Static #24 re-review is
complete; live UI and accessibility-tree validation have not been performed.

```gherkin
Feature: left Widgets QuickLaunch

  Scenario: Render only configured shortcuts
    Given the Widgets tab is selected
    And `sidebar.widgets.quickLaunch` contains shortcut records with `icon`, `name`, and `cmd`
    When QuickLaunch renders
    Then it displays only those configured shortcut records in their configured order
    And it does not insert QML fallback defaults or XDG application entries

  Scenario: Keep an unset shortcut list empty
    Given `sidebar.widgets.quickLaunch` is absent or empty
    When QuickLaunch renders
    Then no shortcut entries are fabricated

  Scenario: Launch through the host callback
    Given a configured shortcut has a command
    When the user activates it
    Then the host trims the command and verifies it still matches a configured shortcut
    And it parses the command into argv and launches without a shell
    And it accepts plain commands and quoted arguments

  Scenario: Reject unsupported shell syntax
    Given a configured shortcut command contains shell operators or substitutions
    When the user activates it
    Then the command is rejected instead of interpreted by a shell

  Scenario: Derive running state from Niri windows
    Given Niri reports windows with app IDs and titles
    When QuickLaunch evaluates a configured shortcut's command
    Then it marks the shortcut running when the parsed first-token basename matches a window app ID or title case-insensitively
    And wrapper commands such as `flatpak` or `env` are matched by their wrapper basename

  Scenario: Exclude shortcut editing interactions
    Given QuickLaunch renders configured shortcuts
    Then it has no edit, reorder, remove, or context-menu actions

  Scenario: Show keyboard focus
    Given QuickLaunch renders a configured shortcut
    When keyboard navigation focuses the shortcut
    Then the button shows a visible focus treatment and supports Enter and Space activation

  Scenario: Bound the parity claim
    Given the QuickLaunch slice is present
    Then it remains partial and does not claim QML parity
```

## ii QuickNote (partial)

QuickNote is a partial current-tab editor backed by the persisted Notepad JSON
snapshot. It has per-display in-memory state, so updates from another open
panel are not live-synced; Reload fetches a newer snapshot. On a detected write
conflict the draft remains available, and Reload rebases the cancel baseline
without replacing that draft so the user can retry saving. Legacy `notepad.txt`
migration, tab CRUD, and file migration UI are not implemented. The editor has
basic insertion, cursor movement, deletion, cancel, save, and clear, but no
selection, IME composition, clipboard editing, or Up/Down movement. Disabled
actions are omitted from the accessibility tree; an Unavailable retained draft
is a focusable labeled TextInput with an accessible value, and pending save
status uses Role::Status. Static code review only; no live UI or
accessibility-tree validation has been performed. This remains partial, not
QML parity. The slice is gated by selected Widgets, `sidebar.widgets.note`,
and `widgetOrder` membership for `note`.

```gherkin
Feature: ii QuickNote

  Scenario: Load only the current tab snapshot
    Given Notepad provides a current-tab snapshot
    When QuickNote loads
    Then it displays the snapshot's current content
    And it does not expose other tabs or fabricate a default blank note

  Scenario: Represent loading, empty content, and unavailable store
    Given the current-tab snapshot is loading, has empty saved content, or is unavailable
    When QuickNote renders
    Then Loading and Unavailable have distinct states
    And empty saved content appears as an empty editor with its prompt

  Scenario: Edit current content through the adapter
    Given QuickNote has loaded current-tab content
    When the user edits and saves, cancels, or clears the content
    Then the corresponding action is sent through the adapter
    And save or clear preserves unrelated tabs and JSON fields

  Scenario: Reload the latest persisted snapshot without losing a draft
    Given QuickNote has an unsaved draft and the persisted current-tab snapshot may have changed
    When the user activates Reload
    Then the current JSON snapshot is read asynchronously
    And the unsaved draft remains visible
    And the cancel baseline is rebased to the newly loaded content
    And the user can retry saving the retained draft

  Scenario: Keep displays independent until reload
    Given QuickNote is open on multiple displays
    When one display changes the persisted note
    Then other open displays do not receive a live snapshot update
    And Reload can fetch the newer persisted content

  Scenario: Preserve a draft after a write conflict
    Given the saved store changed during a save attempt
    When the adapter reports a conflict
    Then QuickNote keeps the draft and reports the conflict
    And Reload remains available before retrying the save

  Scenario: Expose accessible editor and status states
    Given QuickNote is showing an Unavailable state with a retained draft
    Then the draft is a focusable labeled TextInput with an accessible value
    And disabled action buttons are omitted from the accessibility tree
    And pending save feedback uses a status role

  Scenario: Limit text editing behavior
    Given the current note editor is focused
    Then it supports basic insertion, horizontal and Home/End cursor movement, deletion, and Enter
    And Up/Down movement, selection, IME composition, and clipboard editing remain unavailable

  Scenario: Exclude tab and file management
    Given QuickNote renders the current note
    Then it provides no tab create, rename, delete, or switch controls
    And it provides no file migration UI
```

## left Widgets WeekStrip (partial)

WeekStrip is implemented as a partial locale-aware week view. Static source
review only; no live UI validation has been performed. It requires GNU `date`
and `locale`, and the current date snapshot refreshes about every 60 seconds
while the panel polling loop runs, so midnight rollover can lag roughly a
minute. This is not QML parity.

```gherkin
Feature: left Widgets WeekStrip

  Scenario: Apply the default visibility gate
    Given the Widgets tab is selected
    And `sidebar.widgets.week` is enabled, defaulting to true
    And normalized `widgetOrder` contains `week`
    When the Widgets body renders
    Then WeekStrip is included
    And it is omitted when any of those gates is false

  Scenario: Render seven locale-ordered calendar days
    Given GNU `date` and `locale` are available for the selected system locale
    When WeekStrip builds a snapshot for the current week
    Then it renders seven local calendar days beginning on the locale's first weekday
    And weekday abbreviations come from locale formatting
    And the localized month label appears for a non-current week
    And weekday abbreviations are truncated to two Unicode scalar values
    And today and weekends receive their distinct visual treatments

  Scenario: Navigate by buttons and wheel
    Given WeekStrip has a calendar snapshot
    When the user activates Previous or Next by click, Enter, or Space
    Then it requests the adjacent week
    When the user scrolls up or down over WeekStrip
    Then it requests the previous or next week respectively
    And Today appears for a non-current week and returns the offset to zero

  Scenario: Show loading and report date or locale errors
    Given WeekStrip is initially loading or rebuilding after navigation
    When the snapshot is not ready
    Then it exposes a Loading status
    Given GNU `date` or `locale` is missing, fails, or returns invalid data
    When snapshot construction fails
    Then WeekStrip shows the returned error or a generic unavailable message
    And it does not fabricate calendar dates or locale labels

  Scenario: Order WeekStrip among implemented Widgets
    Given WeekStrip and one or more of MediaPlayerWidget, StatusRings, QuickLaunch, and QuickNote are enabled
    And their IDs appear in normalized `widgetOrder`
    When the Widgets body renders
    Then their relative order follows `widgetOrder`
    And unimplemented widget IDs have no rendered placement

  Scenario: Refresh after date rollover
    Given the panel polling loop remains active
    When about 60 seconds pass
    Then WeekStrip rebuilds its snapshot from the current local date
    And date rollover may remain visible for up to roughly one minute
```

## left Widgets WorldClock (partial)

WorldClock is a read-only, partial card. It requires `/usr/bin/date` and a
usable `/usr/share/zoneinfo` database. The system timezone is detected only
when `/etc/localtime` points into that database. Refresh runs on visibility
and at the configured cadence: 1 second with seconds enabled, otherwise 30
seconds. Settings are configuration-only; no live UI validation has been
performed. This does not claim QML parity.

```gherkin
Feature: left Widgets WorldClock

  Scenario: Apply WorldClock visibility gates
    Given the Widgets tab is selected
    And `sidebar.widgets.worldClock` is enabled, defaulting to false
    And normalized `widgetOrder` contains `worldclock`
    When the Widgets body renders
    Then WorldClock is included
    And it is omitted when any of those gates is false

  Scenario: Use configured custom timezones
    Given `worldClock_settings.timezones` contains one or more timezone identifiers
    When WorldClock builds its rows
    Then it uses the configured identifiers in their configured order
    And it does not replace them with automatic suggestions
    And it moves the local timezone first only when local highlighting is enabled and that timezone is already configured

  Scenario: Suggest timezones when configuration is empty
    Given `worldClock_settings.timezones` is empty
    When WorldClock builds its rows
    Then it uses region-based suggestions based on the detected local zone
    And it limits the result to four identifiers
    And it uses global suggestions when no local region can be detected
    And it shows an Auto marker

  Scenario: Validate and sample timezone identifiers
    Given a configured or suggested timezone identifier
    When the adapter samples its clock
    Then it validates safe identifier components and resolves the zone beneath `/usr/share/zoneinfo`
    And it invokes GNU `/usr/bin/date` with the timezone in the `TZ` environment
    And invalid or unavailable zones remain visible as Unavailable rows with an error

  Scenario: Render timezone row data and configured format
    Given a timezone row has a valid date sample
    When WorldClock renders the row
    Then it shows the zone-derived city label, local time, and GMT offset
    And it includes the locale-formatted date only when `showDate` is enabled
    And it follows `use24Hour` and `showSeconds`
    And it shows day/night and relative Tomorrow or Yesterday indicators when data permits
    And `highlightLocal` controls local-row pinning and accent treatment

  Scenario: Report missing local timezone information
    Given `/etc/localtime` cannot be resolved to a zoneinfo identifier
    When WorldClock loads
    Then it shows a local-timezone unavailable status
    And it does not infer a local zone from unrelated system settings

  Scenario: Refresh visible clock rows
    Given WorldClock becomes visible
    When its snapshot is first built and later refreshed
    Then the first build occurs on visibility
    And subsequent refreshes run every second when `showSeconds` is true
    And otherwise subsequent refreshes run every 30 seconds

  Scenario: Keep timezone editing outside the card
    Given WorldClock renders its timezone rows
    Then it provides no timezone add, remove, reorder, or settings controls

  Scenario: Order WorldClock among implemented Widgets
    Given WorldClock and other implemented Widgets are enabled
    And their IDs appear in normalized `widgetOrder`
    When the Widgets body renders
    Then WorldClock follows the relative order of implemented MediaPlayerWidget, StatusRings, QuickLaunch, QuickNote, and WeekStrip entries
    And unimplemented widget IDs have no rendered placement
```

## left Widgets QuickWallpaper (partial)

QuickWallpaper is a read-only preview slice. It has no wallpaper selection
adapter and has not been validated in a live GPUI session or accessibility
tree. Hardcoded English copy is used because a translation binding is not
available in this prototype.

```gherkin
Feature: left Widgets QuickWallpaper

  Scenario: Apply QuickWallpaper visibility gates
    Given the left sidebar is open on the Widgets tab
    And `sidebar.widgets.wallpaper` is enabled, defaulting to false
    And normalized `widgetOrder` contains `wallpaper`
    When the Widgets body renders
    Then QuickWallpaper is included
    And it is omitted when any gate is false

  Scenario: Scan the configured wallpaper directory
    Given `wallpapers.directory` is configured
    When QuickWallpaper becomes visible
    Then it scans that directory one level deep
    And it accepts regular non-symlink files with jpg, jpeg, png, webp, avif, bmp, and svg extensions
    And it retains at most 8192 candidates with the newest ctime values
    And it sorts displayed files by ctime descending with a path tie-breaker
    And it limits displayed rows to 512
    And it may omit older files when either limit is reached
    And a generation plus visibility/configuration guard discards stale scan results

  Scenario: Resolve the default wallpaper directory
    Given `wallpapers.directory` is absent or empty
    When the adapter resolves its scan directory
    Then it uses the XDG Pictures user directory when available
    And otherwise it uses `~/Pictures/Wallpapers`

  Scenario: Render wallpaper previews and unsupported files
    Given the directory scan has completed
    When QuickWallpaper renders its horizontal thumbnail strip
    Then each tile is 130 by 78 pixels
    And PNG, JPEG, WebP, BMP, and SVG previews under 64 MiB are attempted
    And AVIF, oversized files, and decode failures show a fallback state
    And Loading, unavailable-directory, and empty states are available

  Scenario: Approximate the current wallpaper marker
    Given `background.wallpaperPath` contains the configured global wallpaper
    When the gallery marks its current tile
    Then it strips a literal `file://` prefix and compares the resulting path
    And it does not resolve the active output's effective wallpaper

  Scenario: Keep wallpaper actions unavailable
    Given QuickWallpaper renders its header and tiles
    Then it shows only the wallpaper icon and title in the header
    And random selection, selector launch, and tile selection have no callback
    And individual tiles are labeled groups, not focusable selection controls

  Scenario: Navigate the read-only preview carousel
    Given the carousel has one focus stop and multiple wallpaper previews
    When it receives Left or Right, Home or End, or mouse-wheel input
    Then it scrolls the strip and updates its preview cursor
    And no wallpaper is selected
    And live keyboard and accessibility-tree behavior remains unvalidated
```

## left Widgets ContextCard weather (partial)

This slice migrates only current weather. It has no timer, timer-idle, timer
navigation, refresh action, or provider settings UI. Weather requests require
manually configured coordinates; city-only configuration remains unavailable
because geocoding is not implemented. The config parser follows
`bar.weather.fetchInterval` in minutes, with fallback/default 10 and the QML
5–60 minute range. `IiPanelSurface::set_weather_config` is a synchronous
invalidation seam, but the current GPUI host does not wire a live config reload
caller; do not interpret this slice as supporting live weather-setting edits.
The host reads and writes QML's `${Directories.state}/user/weather-cache.json`
shape (`savedAt` in epoch milliseconds and `data`) with a three-day expiry.
Refresh retains the last real snapshot, marks it stale, and exposes its age and
refresh failure accessibly. Legacy `wCode` values are reverse-mapped when
known; an unknown icon code does not discard the measured snapshot and uses a
conservative cloudy glyph. Cache writes use unique exclusively created
same-directory temporary files before atomic rename. No live GPUI session or
accessibility tree has been validated.

```gherkin
Feature: left Widgets ContextCard current weather

  Scenario: Apply weather card visibility gates
    Given the left sidebar is open on the Widgets tab
    And `sidebar.widgets.context` is enabled
    And `sidebar.widgets.contextShowWeather` is enabled
    And `bar.weather.enabled` is true
    And normalized `widgetOrder` contains `context`
    When the Widgets body renders
    Then the current-weather card is included
    And it is omitted when any gate is false

  Scenario: Load configured current weather
    Given valid manual latitude and longitude are configured
    And curl is available
    When the card becomes visible
    Then Open-Meteo current conditions are requested on the background executor
    And the card shows temperature, condition, weather icon, humidity, wind, and feels-like value
    And the configured city is shown when present
    And the request uses configured temperature and wind units
    And weather data is refreshed at `bar.weather.fetchInterval` minutes while visible
    And an absent interval uses 10 minutes and values are clamped to 5–60
    And the current host does not reload weather config live

  Scenario: Show explicit loading and unavailable states
    Given weather is loading, misconfigured, or its provider request fails
    When the card renders
    Then it exposes an accessible status with the current state or failure reason
    And it does not invent temperature or measurement values

  Scenario: Keep a cached snapshot visible while refreshing
    Given a usable weather snapshot is available in memory or the QML-compatible cache
    And its cache age is no more than three days
    When a refresh starts or the provider request fails
    Then the real snapshot remains visible and is marked stale
    And the status exposes its update age and, on failure, the failure reason
    And the cached values are not presented as freshly updated

  Scenario: Reject invalid or expired weather cache entries
    Given the cache is malformed, has a future timestamp, is older than three days, or mismatches a GPUI source fingerprint
    When weather loads
    Then the cache is ignored and no reading is fabricated
    And the card shows loading or an explicit unavailable state until real data is available

  Scenario: Do not use location discovery
    Given only a city name is configured without manual coordinates
    When weather is requested
    Then the card reports that coordinates are required
    And it does not use GPS, IP-geolocation, or geocoding

  Scenario: Discard a result after the card is hidden
    Given an Open-Meteo request is in flight
    When the Widgets tab or one of the visibility gates closes
    Then the request result is not applied to the hidden card
    And hide/show changes invalidate the request synchronously

  Scenario: Match QML weather icons
    Given an Open-Meteo WMO weather code is returned
    When its Material Symbol is rendered
    Then WMO 65 and 82 use `weather_hail`
    And WMO 71, 77, and 85 use `cloudy_snowing`
    And WMO 73, 75, and 86 use `weather_snowy`
    And clear and partly-cloudy night variants follow QML

  Scenario: Preserve the weather-only scope
    Given current-weather mode is rendered
    Then timer and timer-idle modes are absent
    And refresh and timer-navigation actions are absent
    And live visual and accessibility-tree validation remains outstanding
```

## left Widgets ControlsCard (partial)

ControlsCard is a read-only visual preview. No provider supplies its current
toggle states, and none of its actions are connected. Its copy is hardcoded in
English; live UI and accessibility-tree validation have not been performed.

```gherkin
Feature: left Widgets ControlsCard

  Scenario: Apply ControlsCard visibility gates
    Given the Widgets tab is selected
    And `sidebar.widgets.controls` is enabled, defaulting to true
    And normalized `widgetOrder` contains `controls`
    When the Widgets body renders
    Then ControlsCard is included
    And it is omitted when any card gate is false

  Scenario: Honor per-control visibility flags
    Given ControlsCard is included
    When `controlsCard.show*` flags are read
    Then each of the eight flags defaults to true
    And each flag independently controls its matching toggle or action glyph
    And the toggle group precedes the action group
    And a separator appears when both groups contain visible glyphs

  Scenario: Render truthful unavailable controls
    Given one or more ControlsCard glyphs are visible
    When the prototype has no matching state provider or action
    Then toggles and actions render as noninteractive 40 by 40 pixel groups
    And toggle glyphs use 22px and action glyphs use 20px
    And each glyph label reports unknown state and unavailable control
    And a status message reports that states and actions are disconnected
    And no action callback is available
    And no active state or action result is fabricated

  Scenario: Keep missing hover and translation behavior explicit
    Given ControlsCard is rendered
    Then its controls have no QML hover feedback while unavailable
    And its labels remain hardcoded in English
```

## ii right sidebar notification section

The notification section is rendered at its configured
`sidebar.right.sectionOrder` position. Its DTO-driven panel can display
Unavailable, Loading, Empty, and Ready states; supplied notification records
render app/title/body/urgency cards and their supplied action buttons. The
current adapter supplies Unavailable with no records and a no-op action
callback. Live D-Bus intake, history, grouping, search, and Do Not Disturb state
are not integrated. QML unread/total counts, mute/clear controls, grouping,
per-card dismiss/expand, notification icons, and timestamps remain unmigrated.
This is an implementation status note; it does not mark any acceptance
criterion or parity check as passed.

## ii right sidebar Quick Sliders

Quick Sliders is mounted at its configured `sidebar.right.sectionOrder`
position. Its DTO renderer supports Loading, Unavailable, and Ready states,
including an explicit empty Ready state. Valid range controls expose keyboard,
pointer, and wheel value-change callbacks; invalid ranges render as static
Unavailable rows. The current integrated adapter supplies Unavailable, no
records, and a no-op callback; it has no live audio or brightness source and
does not fabricate values. Shared sidebar styling, QML per-control style and
configuration bindings, and live range/value integration remain incomplete.
This status note does not mark acceptance criteria or parity as passed.

## ii right sidebar Quick Toggles

Quick Toggles is mounted at its configured `sidebar.right.sectionOrder`
position. Its DTO grid supports Loading, Ready, Empty, Unavailable, and Error
states; supplied enabled tiles report requested state through a callback on
click or Enter/Space. The integrated adapter always provides Unavailable, no
tiles, and a no-op callback, so it does not invent switch data. Classic/Android
QML icon styles, secondary actions, config-backed filtering, and an explicitly
bounded inner scrolling viewport are not implemented. This status note does
not change or mark the acceptance criteria as passed.

## ii Dock pinned icons and launch slice

This is a bounded acceptance target for the pinned/icon/launch and base
running-window interaction of the ii Dock. Previews, context menus, drag
reordering, badges, and hover effects remain separate parity work.

```gherkin
Feature: ii Dock pinned apps, icons, and launch

  Scenario: Preserve pinned-only order before the running section
    Given the ii Dock has pinned app ids "org.example.Editor", "browser.desktop", and "missing-app"
    And the first two ids resolve to desktop entries
    And neither resolved app has a running window on the dock output
    When `separatePinnedFromRunning` is enabled, as in the current default
    Then the pinned-only entries appear first in configured relative order
    And the unresolved id "missing-app" is omitted

  Scenario: Include a resolved pinned app without a running window
    Given "org.example.Editor" is pinned
    And its desktop entry resolves
    And no window for that app is running on the dock output
    When the dock builds its app list
    Then one pinned app control is rendered for that desktop entry

  Scenario: Place every running app in first-open order after pinned-only apps
    Given "org.example.Editor" is pinned and has one or more windows on the dock output
    And "browser.desktop" is pinned and has no running window
    And an unpinned app has a running window on the dock output
    When the dock builds its app list
    Then "browser.desktop" appears first in the pinned-only section
    And a separator divides the pinned-only and running sections
    And all running apps, including "org.example.Editor", appear once in first-open order
    And the running "org.example.Editor" has one app control with its pinned state
    And that control shows per-window indicators rather than separate window buttons

  Scenario: Cycle focus from the app control through its running windows
    Given "org.example.Editor" has multiple windows and one is focused
    When the user clicks its app control or activates it with Enter or Space
    Then focus moves to the next window in the app's sorted window order
    And another activation wraps to the first window after the last

  Scenario: Skip an unknown pin without a launch target
    Given "missing-app" is pinned
    And no desktop entry resolves for that id
    And no matching window is running
    When the dock builds its app list
    Then no app control is rendered for "missing-app"
    And no control is exposed that cannot activate a launch target

  Scenario: Resolve an app icon from the active XDG icon theme
    Given a pinned desktop entry names an icon available through the active XDG icon theme
    When the dock renders the app control
    Then it displays the resolved themed icon image
    And its accessible name identifies the application

  Scenario: Fall back when the desktop icon cannot be resolved
    Given a pinned desktop entry has no resolvable icon image
    When the dock renders the app control
    Then it displays the themed "image-missing" icon when available
    And otherwise it displays the neutral glyph fallback
    And its accessible name still identifies the application

  Scenario: Launch a pinned app with no running windows
    Given a pinned app has a resolved desktop entry and no running windows
    When the user clicks its dock control or activates it with Enter or Space
    Then the host launches the resolved desktop entry

  Scenario Outline: Orient the dock along its configured output edge
    Given the ii Dock is configured on the <edge> edge
    When the dock renders
    Then its app controls follow the <orientation> axis
    And each app control retains its accessible application name and keyboard activation

    Examples:
      | edge   | orientation |
      | top    | horizontal  |
      | bottom | horizontal  |
      | left   | vertical    |
      | right  | vertical    |

  Scenario: Expose one app control per application in the keyboard focus order
    Given the ii Dock contains pinned apps and running applications
    When the user navigates the dock with Tab and Shift+Tab
    Then each app control has an accessible name that includes its window count and states
    And Enter or Space activates the focused app control
```

## ii Dock hover window previews

The QML reference is `modules/dock/DockApps.qml`, `DockPreview.qml`,
`DockWindowPreview.qml`, `DockAppButton.qml`, and
`services/WindowPreviewService.qml`. Its preview is a separate anchored popup;
the capture service initializes lazily and captures only requested window IDs
after the popup's intentional-hover delay. These criteria describe that
observable contract. The current GPUI prototype has an anchored popup host,
preview cards, and a read-only cache adapter that validates PNG structure and
classifies cached frames. It does not initiate screenshot capture, and its
popup behavior and image decoding have not been verified in a live compositor
session. These scenarios remain acceptance criteria; their presence does not
mean the prototype passes them or has visual/runtime parity.

```gherkin
Feature: ii Dock hover window previews

  Background:
    Given the ii Dock is enabled on a configured output
    And the Niri window snapshot is available

  Scenario: Open a preview after the configured hover delay
    Given "dock.hoverPreview" is enabled
    And an app control has one or more live windows
    And neither a drag nor a context menu is active
    When the pointer remains over that app control for "dock.hoverPreviewDelay" milliseconds
    Then a non-focus-stealing popup is anchored to that app control
    And it is placed on the side facing away from the dock edge
    And its content lists that app's live windows in dock window order
    And no screenshot capture is requested before the hover delay expires

  Scenario: Suppress previews when disabled or another dock interaction owns input
    Given hover previews are disabled, or a drag or context menu is active
    When the pointer enters an app control
    Then no preview popup is opened
    And no screenshot capture is requested for that hover

  Scenario: Show a loading placeholder when no cached frame exists
    Given a preview popup is open for a live window
    And its cached screenshot is absent
    Then the window title and application identity remain visible
    And the preview area shows a loading placeholder while capture is pending
    When capture is unavailable or fails
    Then the preview area shows an unavailable placeholder

  Scenario: Identify a stale cached frame while its refresh is pending
    Given a preview popup is open for a live window
    And a cached screenshot exists but is older than the requested maximum age
    When a refresh capture is pending
    Then the valid cached frame is displayed immediately
    And the preview identifies that frame as stale or updating
    And it does not present the stale frame as a current capture
    When the refreshed frame becomes available
    Then the preview displays the refreshed frame and clears its stale or updating indication

  Scenario: Request only the hovered app's stale or missing frames
    Given the preview service has initialized lazily
    And the hovered app has window IDs "41" and "42"
    And an unrelated app has window ID "99"
    When the preview capture debounce expires
    Then the capture request contains only stale or missing IDs from "41" and "42"
    And it does not capture window "99"
    And a valid cached frame can be displayed immediately while refresh is pending
    And that stale cached frame is identified as stale or updating until refreshed

  Scenario: Activate a specific window from its preview card
    Given the preview popup shows multiple live windows
    When the user clicks a window card or activates it with Enter or Space
    Then the host focuses that card's exact Niri window ID once
    And the popup closes unless "dock.keepPreviewOnClick" is enabled

  Scenario: Close a specific window from its preview card
    Given the pointer is over a preview card with a close control
    When the user activates the close control
    Then the host closes that card's exact Niri window ID once
    And closing the window does not immediately dismiss the popup during its close-grace interval

  Scenario: Keep the popup open while the pointer transfers from dock to popup
    Given the preview popup is open
    When the pointer leaves the app control and enters the popup
    Then the popup remains open
    When the pointer leaves both the dock and popup for the dismiss delay
    Then the popup closes

  Scenario: Follow app and window lifecycle changes
    Given a preview popup is open
    When the app's live window list changes
    Then the cards update without duplicate cards for the same window
    When the last live window closes
    Then the popup closes and pending capture work for removed windows is discarded

  Scenario: Close a preview with Escape
    Given the preview popup is visible
    When the user presses Escape
    Then the popup closes without activating or closing a window

  Scenario Outline: Anchor previews away from every dock edge
    Given the dock is positioned on the <edge> edge
    When an app preview opens
    Then the popup is placed on the <popup_side> side of the app control
    And it remains within the same output when space permits

    Examples:
      | edge   | popup_side |
      | bottom | above      |
      | top    | below      |
      | left   | right      |
      | right  | left       |
```

## ii full-output backdrop renderer

```gherkin
Feature: data-driven ii desktop backdrop

  Scenario: Preserve QML backdrop and wallpaper layer order
    When the ii host enables backdrop presentation
    Then it creates a transparent per-output Layer::Background backdrop surface
    And the main wallpaper surface uses Layer::Bottom
    And the backdrop surface uses an empty input region and exclusive zone zero
    And it uses namespace "intoo-gpui-backdrop"
    And it does not create a second Layer::Background surface
    And compositor stacking has not yet been verified in a live Niri session

  Scenario: Resolve configured wallpaper per output
    Given a matching output and parsed backdrop configuration
    When useMainWallpaper is true
    Then the host supplies that output's main wallpaper
    When useMainWallpaper is false
    Then the host prefers that output's backdropPath
    And falls back to global backdrop wallpaperPath and then main wallpaper
    And it does not fabricate a path or image

  Scenario: Render an output-specific still wallpaper
    Given the host supplies a ready still wallpaper for output "DP-1"
    When the full-output backdrop is rendered for "DP-1"
    Then the supplied image covers the output bounds
    And the backdrop reports the output name to assistive technology
    And no wallpaper path or output is resolved by the component itself

  Scenario Outline: Report wallpaper loading and missing states
    Given the host reports <state> for the output wallpaper
    When the backdrop is rendered
    Then it shows <presentation>

    Examples:
      | state   | presentation                    |
      | loading | Loading wallpaper               |
      | missing | Wallpaper unavailable           |

  Scenario: Apply supported dim and vignette overlays
    Given a still wallpaper is available
    And the host supplies a dim percentage and enabled vignette
    When visual effects are not reduced
    Then the backdrop applies the clamped black dim overlay
    And the optional bottom vignette uses supplied intensity and radius

  Scenario: Respect reduced effects
    Given reduced effects are enabled
    Then the backdrop suppresses the optional vignette
    And it reports the reduced-effects state

  Scenario Outline: Report unsupported media and image filters
    Given the wallpaper type or requested effect is <unsupported>
    Then no fake frame or filter result is rendered
    And an honest unsupported-state message is exposed

    Examples:
      | unsupported                 |
      | video wallpaper             |
      | animated wallpaper          |
      | blur                         |
      | non-neutral saturation      |
      | non-neutral contrast        |
```

## Shared GPUI surface host

```gherkin
Feature: shared GPUI layer-surface host

  Scenario: Create critical surfaces only after configuration is ready
    Given the selected family has a critical background, bar, and any required reservations
    And configuration is not ready
    When the GPUI application starts
    Then it does not expose partially configured critical surfaces
    When configuration becomes ready
    Then enabled critical surfaces are created with their configured output, anchors, size, layer, input mode, and exclusive zone
    And disabled critical panels create neither a visible surface nor an input region

  Scenario: Keep family critical readiness separate from deferred readiness
    Given a selected family's critical surfaces are ready
    And deferred surfaces are still loading
    When the shell evaluates startup readiness
    Then the critical family can become visible without waiting for deferred panels
    And each deferred surface remains gated by the equivalent of shell-entry and deferred-panel readiness
    And a failed critical surface follows the configured family failure or fallback path

  Scenario: Enumerate all existing outputs exactly once
    Given outputs "DP-1" and "HDMI-A-1" are available at startup
    When the surface host starts
    Then each eligible per-output surface is created once on each resolved output
    And each surface uses that output's logical bounds and scale
    And no surface or exclusive zone is duplicated

  Scenario: Handle output add, removal, rename, and bounds changes
    Given one or more GPUI surfaces are associated with named outputs
    When an output is added, removed, renamed, or its bounds or scale change
    Then each affected surface is reconciled to the current output set and geometry
    And removed outputs have no surviving surface, reservation, or input region
    And unaffected outputs keep their surfaces without a transient duplicate
    And refresh-only changes do not repeatedly close and recreate surfaces

  Scenario: Resolve configured output lists with the QML fallback rule
    Given a panel has an empty configured output list
    When the host resolves targets
    Then it targets every available output
    Given a panel has a non-empty list with at least one matching output
    When the host resolves targets
    Then it targets only matching outputs
    Given none of the configured names match an available output
    When the host resolves targets
    Then it falls back to available outputs rather than hiding the panel everywhere

  Scenario: Reconcile rapid output events without stale asynchronous work
    Given an output is removed while a surface create or resize is pending
    When later output events arrive before that operation completes
    Then stale work cannot recreate a surface for the removed output
    And the final surface set matches the latest output snapshot
    And each surface has at most one active close, create, or reconfigure operation

  Scenario: Reserve only the intended edge and extent
    Given a surface reserves one edge
    When its output is resized or the surface changes visibility
    Then the compositor sees the correct edge and exclusive-zone size
    And transparent visual headroom does not increase the reservation unless the QML reference does
    And hidden, disabled, detached, and closed surfaces release reservations at the same lifecycle point as QML

  Scenario: Respect input ownership, focus, and masks
    Given a surface contains interactive and transparent visual regions
    When the pointer is over a transparent region
    Then pointer input passes through wherever the QML reference mask passes it through
    When the pointer or keyboard is over an interactive region
    Then the correct surface receives input and focus
    And a non-focusable surface never steals keyboard focus
    And closing or hiding a surface releases its input region and restores focus safely

  Scenario: Preserve lock and family-transition surface ownership
    Given the screen becomes locked while ordinary family surfaces are visible
    When lock surfaces activate
    Then ordinary unlocked-only surfaces and their input regions are hidden or removed
    And security surfaces use the selected family's visual presentation
    When the selected family changes
    Then the configured transition or cut is shown
    And old-family surfaces and input regions are gone before the new family is considered ready
    And a failed new critical family follows the current fallback behavior

  Scenario: Reconcile live configuration changes
    Given a panel is open
    When its enabled-panel flag, output list, edge, size, style, or family changes
    Then the surface host applies the latest configuration without retaining stale geometry or appearance
    And disabling the panel closes it even if its previous open state remains true
    And re-enabling it follows the same readiness and visibility gates as a fresh surface

  Scenario: Reconcile the ii critical composition from runtime config snapshots
    Given the GPUI prototype is running with family "ii"
    And its runtime config file is edited or atomically replaced
    When the config file's metadata or content hash changes and remains stable through the debounce interval
    Then the host parses a complete config snapshot from the same bytes used for its content hash
    And a monotonically newer snapshot is applied on the GPUI UI thread
    And an unchanged content hash does not close and recreate critical surfaces
    And appearance and bar-style edits resolve the theme again before replacing affected surfaces

  Scenario: Keep exactly one ii bar variant and reconcile critical gates
    Given family "ii" is selected
    When `bar.vertical` is false
    Then only `iiBar` can create the horizontal GPUI bar
    When `bar.vertical` is true
    Then the horizontal bar is closed before `iiVerticalBar` can be created
    And only the selected variant's `enabledPanels` entry controls that bar
    And disabling `iiBackground`, `iiDock`, or `iiBackdrop` closes its matching surface and releases its layer resources
    And `iiDock.enable` defaults to true when omitted, subject to `iiDock` in `enabledPanels`

  Scenario: Resolve ii critical outputs using each QML composition rule
    Given the ii bar and dock have separate configured screen lists
    When either list is empty or none of its names match an available output
    Then that surface falls back to all available outputs
    When a list has at least one matching name
    Then only that surface targets the matching outputs
    And the ii background targets every available output because its QML root has no output-list gate
    When outputs are added, removed, renamed, or resized
    Then the latest config snapshot is used to reconcile the bar, background, backdrop, and dock
    And removed outputs have no surviving critical surface or exclusive/input reservation
```

## Family-wide visual parity

```gherkin
Feature: family-wide visual parity with QML composition roots

  Scenario Outline: Show only the selected family's critical composition
    Given configuration selects family "<family>"
    And the configured critical panels for that family are enabled
    When the shell reaches its first ready frame
    Then that family's background, primary chassis, and required reservations match its QML composition root
    And critical readiness is not reported until every required critical surface is ready
    And no other family's critical surface or input region is present

    Examples:
      | family |
      | ii     |
      | waffle |
      | iris   |

  Scenario: Match ii critical and deferred composition
    Given family "ii" is selected
    When the critical composition is rendered
    Then ii background, the selected classic, pill, M3, or vertical bar variant, and enabled dock match the QML critical root
    And only the configured active bar variant is compiled or presented
    When deferred panels become ready
    Then shared overlays and ii panels use their exact enabled, open, retention, memory-pressure, and close-grace gates
    And pill toast and OSD takeover excludes only the outputs hosted by the pill bar

  Scenario: Match Waffle critical and deferred composition
    Given family "waffle" is selected
    When the critical composition is rendered
    Then Waffle bar, background, and conditionally enabled backdrop match the QML critical root
    When deferred panels become ready
    Then Waffle notification popup and OSD are present only when enabled
    And Start Menu, Action Center, Notification Center, widgets, Task View, clipboard, and switcher follow their specific open, feature, and retention gates
    And shared ii panels loaded by the Waffle composition retain their current availability and visual styling

  Scenario: Match Island critical and deferred composition
    Given family "iris" is selected and the official Island gate is satisfied
    When the critical composition is rendered
    Then Island background gating, frame reservations, and Island bar match the QML critical root
    When deferred panels become ready
    Then Island sidebars, Studio, settings host, notifications, OSD, session, lock, PolicyKit, keyboard, and other panels follow their individual gates
    And compiled-on-demand panels distinguish not requested, loading, ready, and failure states
    And non-official Island mode does not expose Island-only surfaces

  Scenario Outline: Preserve panel disablement while open
    Given family "<family>" has panel "<panel>" open
    When the panel is removed from "enabledPanels" or its family-specific feature gate becomes false
    Then the panel stops rendering and releases its input region
    And the panel's global open state is closed when the QML composition closes it
    And re-enabling follows the panel's original lazy or compiled-on-demand loading path

    Examples:
      | family | panel              |
      | ii     | iiEqualizer        |
      | ii     | iiControlPanel     |
      | waffle | wWidgets           |
      | iris   | irisStudio         |
      | iris   | irisSidebarLeft    |

  Scenario: Preserve on-demand residency and memory-pressure behavior
    Given an on-demand panel has been opened and then closed
    When its close-grace interval is active
    Then it remains resident for the QML-equivalent interval without remaining visibly interactive
    When retention expires or low-memory policy shortens retention
    Then it is released at the equivalent QML deadline
    And reopening during the grace period does not create duplicate surfaces or reset state unexpectedly

  Scenario: Preserve settings hosts and navigation states
    Given settings are requested in the selected family
    When settings use overlay or standalone-window hosting
    Then the correct host, initial page, navigation, search, dirty state, and close behavior are rendered
    And config-not-ready, invalid page, delayed request, and page-load failure states have defined visible feedback
    And a live appearance edit updates the preview and persisted configuration without a stale preview surface

  Scenario: Preserve family-specific themes and shared component variants
    Given the same saved configuration and service data are rendered in QML and GPUI
    When each family selects its configured appearance preset, typography, density, shape, motion, material, and transparency settings
    Then shared controls adopt the same family-specific variant as their QML consumers
    And wallpaper-derived colors and per-surface overrides resolve to matching roles
    And reduced motion, game mode, compositor blur capability, and missing wallpaper have defined equivalent fallbacks

  Scenario: Compare representative data and environmental states
    Given each family has a paired QML reference and GPUI candidate at the same output geometry and scale
    When visual parity is reviewed
    Then captures cover empty, populated, loading, error, focused, hovered, pressed, disabled, and transient states where applicable
    And interaction checks cover pointer, wheel, keyboard, drag, focus transfer, dismissal, and reopen
    And differences in content, geometry, typography, materials, motion, clipping, or hit regions are recorded as failures or explicit unsupported cases
```

## Connected ii Overview Applications slice

```gherkin
Feature: ii Overview application catalog and launch

  Background:
    Given the ii Overview surface is open
    And the Applications mode is selected
    And an XDG desktop application source is available

  Scenario: Filter the desktop application catalog
    Given the catalog contains visible applications
    When the user enters text in the Applications search field
    Then only applications matching the current query are shown
    And clearing the query restores the visible catalog

  Scenario: Show an empty result state
    Given the catalog contains no visible applications matching the query
    When the Applications mode renders
    Then the user sees "No applications found"
    And no stale application rows remain visible

  Scenario: Report an unavailable application catalog
    Given no XDG desktop application source could be created
    When the Applications mode renders
    Then the user sees that the application catalog and launcher are unavailable

  Scenario: Report an application launch failure
    Given a visible application is selected
    When its desktop launch command fails or exits unsuccessfully
    Then the Overview remains open
    And the launch failure is shown in the Applications view

  Scenario: Close after a successful application launch
    Given a visible application is selected
    When its desktop launch command completes successfully
    Then the Overview closes
```

## Waffle taskbar visual and interaction slice

The QML reference is `modules/waffle/bar/tasks/Tasks.qml`,
`TaskAppButton.qml`, `TaskPreview.qml`, `WindowPreview.qml`,
`modules/waffle/bar/tray/Tray.qml`, and `TrayButton.qml`. Task state comes from
`services/TaskbarApps.qml`, minimized-window state from
`services/MinimizedWindows.qml`, and tray state from `TrayService`. The current
GPUI Niri task provider is
`prototypes/gpui-shell/src/families/waffle/taskbar/niri_tasks.rs`; it derives
visible windows from the live Niri window list and configured pinned IDs. The
current renderer is `prototypes/gpui-shell/src/families/waffle/taskbar.rs`.
`niri::snapshot(output)` filters workspaces by output, but its window list stays
global, matching QML's use of `CompositorService.sortedToplevels` for every
taskbar. The GPUI provider currently supplies no minimized-window state.

```gherkin
Feature: Waffle taskbar application and tray parity

  Background:
    Given the Waffle taskbar is enabled on the current output
    And task, minimized-window, desktop-entry, preview, and tray state is supplied by host adapters

  Scenario: Order pinned and running applications around the separator
    Given pinned applications with no windows and applications with running windows
    When the taskbar renders
    Then pinned applications appear first in their configured provider order
    And running applications appear after them in their configured provider order
    And exactly one separator appears only when both groups are non-empty
    And application entries with neither a pin nor a running window are omitted
    And changes to the configured separator visibility or style are reflected without changing group order

  Scenario: Show Niri per-output focus and window-count state
    Given the compositor is Niri
    And an application has multiple visible toplevels in the shared live Niri window list
    When a different toplevel becomes focused on that output
    Then that application's focused-window indicator moves to the matching window position
    And visible-window count and active state use the same global live window list on every output's taskbar
    And all taskbars agree on which application has the focused toplevel

  Scenario: Represent minimized windows and restore them
    Given an application has minimized windows and no visible toplevels
    When the current GPUI task provider snapshot is rendered
    Then its minimized count remains zero because the provider does not expose minimized windows
    And no minimized indicator or restore action is claimed as implemented
    And QML's minimized indicator and latest-window restore remain acceptance gaps until a GPUI minimized-window adapter and action exist

  Scenario: Keep tasks reachable when the task row exceeds available width
    Given the combined task buttons are wider than the task row
    When the taskbar renders
    Then the task row can scroll horizontally without covering the tray or clock
    And keyboard users can move focus to every overflowing task and activate it with Enter or Space
    And the focused task is brought into view

  Scenario: Show application icon, hover preview, and context actions
    Given a task has a matching desktop entry and one or more windows
    When the pointer hovers over the task for the configured preview delay
    Then the preview is anchored to that task and shows the supplied window previews and titles
    And moving between task and preview keeps the preview open
    When the user opens the task context menu
    Then desktop actions, launch or focus behavior, and pin or unpin action match the QML reference
    And missing desktop entries or preview captures use an explicit fallback state
    And icon loading, hover preview, preview residency, and task context menus remain acceptance gaps until their adapters and presentation exist

  Scenario: Present pinned tray items and hidden-item overflow
    Given the tray provider supplies pinned and unpinned items
    When the taskbar renders
    Then pinned items show their supplied icons and tooltips
    And an overflow control appears when unpinned items exist or tray reordering is active
    When the user opens the overflow control
    Then hidden items appear in the overflow menu and retain their supplied actions and context menus
    And pinning an item updates the tray provider and visible pinned list
    And real tray icons, smart activation, menu handles, overflow, pin drag, and tint configuration remain adapter-owned acceptance gaps

  Scenario: Preserve accessible task and tray interaction
    Given task and tray entries have stable provider identifiers and accessible labels
    When a user navigates the taskbar by keyboard or assistive technology
    Then each task exposes its application name, visible-window count, minimized count, and active state
    And each tray item exposes its provider label and activation action
    And focus order follows the visual order from Start through tasks to tray and clock
```

## Connected Waffle Clipboard slice

```gherkin
Feature: Waffle clipboard history interactions

  Background:
    Given the Waffle Clipboard overlay is open
    And the clipboard provider has returned a ready snapshot

  Scenario: Search clipboard rows
    Given the snapshot contains clipboard entries and configured pins
    When the user enters a query in the clipboard search field
    Then only matching rows are shown
    And configured pin rows remain ahead of matching history rows
    And clearing the query restores all rows in the snapshot

  Scenario: Pin a history entry for this shell session
    Given a history entry can be decoded
    When the user pins that entry
    Then the decoded text is added to the session pin list if it is not already pinned
    And the refreshed snapshot displays it as pinned
    And the pin is not written back to configuration

  Scenario: Copy a history entry or configured pin
    Given a visible history entry or configured pin is selected
    When the user activates Copy
    Then the provider copies the corresponding payload to the desktop clipboard
    And the overlay closes only after the copy succeeds
    When the provider reports a copy error
    Then the error is shown and the overlay remains open

  Scenario: Delete a history entry
    Given a deletable history entry is visible
    When the user activates Delete
    Then the provider deletes that entry
    And a successful follow-up read replaces the displayed snapshot
    And a mutation failure shows an error while retaining the last good snapshot

  Scenario: Require explicit confirmation before clearing history
    Given clipboard history entries are visible
    When the user activates Clear all
    Then the overlay asks for confirmation without wiping history
    When the user cancels
    Then history remains unchanged
    When the user confirms
    Then the provider wipes history
    And a successful follow-up read replaces the displayed snapshot

  Scenario: Invalidate rows when a successful mutation cannot be read back
    Given a delete or confirmed wipe succeeds
    When the provider fails to list the resulting clipboard state
    Then the overlay shows an unavailable state and the provider error
    And the prior snapshot is not presented as current
    And the user can request a refresh to retry the read
```

## ii Overview Dashboard renderer

```gherkin
Feature: ii Overview dashboard presentation

  Scenario: Render the live clock and date in the header card
    Given the Dashboard mode is open
    And the host has a local time and date snapshot
    Then the time and date appear in the first Dashboard card
    And the displayed string is supplied by the host

  Scenario: Render honest placeholders for unavailable sources
    Given the Dashboard mode is open
    And no providers exist for toggles, audio, brightness, media, weather, or resources
    Then each corresponding QML-shaped section remains visible
    And each section explains that its source is unavailable
    And none shows invented values or fake interactive controls

  Scenario: Render supplied dashboard cards
    Given the host supplies ready dashboard groups and items
    When Dashboard mode is open
    Then groups render as titled sections in a vertical card stack
    And each card displays only its supplied title, subtitle, glyph, and detail

  Scenario Outline: Report state independently for each dashboard section
    Given the host reports <state> for a dashboard section
    When Dashboard mode is open
    Then that section shows a distinct <presentation> state

    Examples:
      | state       | presentation |
      | loading     | loading      |
      | empty       | empty        |
      | unavailable | unavailable  |
      | error       | error        |

  Scenario: Keep QML section gates honest
    Given the QML dashboard has per-card visibility configuration
    And GPUI does not yet supply those visibility settings
    Then all known dashboard sections remain visible
    And the missing configuration gate is documented
```

## Iris Orbit Niri-layout geometry slice (partial)

Orbit now reads Niri window `is_floating`, `layout.pos_in_scrolling_layout`,
`layout.tile_size`, and `layout.tile_pos_in_workspace_view` fields when their
types and numeric ranges are valid. It derives tiled column/row proportions
separately for each workspace exposed on the selected output. Floating tiles
use their reported output-local position and tile size. A missing or malformed
placement field, or floating-state flag, switches the entire visible workspace
to a labelled window-grid fallback. The same workspace-wide fallback is used
if a projected rectangle is non-finite, non-positive, outside output bounds, or
if aggregate tiled dimensions overflow. Rendering does not clamp individual
coordinates or sizes. Orbit never mixes synthetic grid positions with reported
Niri bounds within one workspace. The fallback mode is included in each tile's
accessible name as approximate grid placement. Window controls remain
focusable and preserve click, Enter, and Space activation.

This slice does not produce application thumbnails: it shows an honest generic
window icon when no exact-window preview is supplied. Niri does not provide the
current horizontal scroll offset in the data consumed here, so tiled columns
are proportioned across the reported workspace column set; this does not claim
pixel-exact viewport placement. Orbit refresh is polling-based and live
geometry changes are not event-driven.

The selection slice initializes to the active workspace and its focused
window, falling back to the first workspace/window. Selection is retained by
Niri IDs across snapshots while those items remain present, and stale IDs are
cleared with a focused/first-item fallback. Arrow navigation follows
`iris.orbit.keys` (`niri` by default; `swapped` exchanges workspace and window
axes); Tab and Shift+Tab also move the selected window. Clicking another
workspace or a window on another workspace selects it. Clicking the selected
workspace or a window on the selected workspace activates it. Enter activates
the selected window, or the selected workspace when it has no windows; Escape
closes Orbit. Workspace/window selection and keyboard handlers live in the
runtime controller rather than the renderer. A concise status announcement
reports the selected workspace/window and changes only when selection changes.

Orbit search, recent and stashed window actions, and preview interactions are
still outside this slice. The QML Delete, Ctrl+M, Ctrl+F, and Ctrl+workspace
arrow actions are also explicitly deferred. Exact QML search and preview
behavior remains unverified.

```gherkin
Feature: Iris Orbit reported window layout

  Scenario: Show valid tiled layout proportions for each output workspace
    Given Orbit receives windows for two workspaces on its selected output
    And each tiled window has a valid scrolling position and positive tile size
    When Orbit renders the workspace cards
    Then tiled windows are grouped into reported columns and rows
    And tile widths and heights preserve the proportions reported by Niri
    And a different workspace does not change those proportions

  Scenario: Place a floating window from its workspace-view tile position
    Given an Orbit workspace contains a floating window with a valid tile position and size
    When Orbit renders the workspace card
    Then the window uses its normalized output-local reported placement
    And the card identifies the window as floating

  Scenario: Use a labeled grid when layout metadata is incomplete
    Given one or more windows lack valid floating state or placement metadata
    When Orbit renders the workspace card
    Then every visible window in that workspace uses the labeled window-grid fallback
    And Orbit does not imply that fallback locations came from Niri

  Scenario: Fall back for invalid projected output bounds or aggregate overflow
    Given a floating window projects outside its output, or tiled size aggregation overflows
    When Orbit derives the workspace layout
    Then every visible window in the workspace uses the labeled grid fallback
    And no individual tile is clipped into valid output bounds
    And the tile's accessible name identifies approximate fallback placement

  Scenario: Keep window activation available without a preview provider
    Given no exact-window thumbnail is available
    When Orbit renders and the user clicks or presses Enter or Space on a window
    Then Orbit shows a generic window icon
    And Niri receives one focus request for that window

  Scenario: Initialize and preserve selection by Niri ID
    Given Orbit opens with workspaces and windows from Niri
    When it initializes selection
    Then the active workspace and its focused window are selected when present
    And it falls back to the first workspace and window when needed
    When a later snapshot still contains the selected IDs
    Then Orbit preserves those selections
    When a later snapshot removes a selected ID
    Then Orbit clears the stale selection and selects a valid focused or first item

  Scenario: Navigate workspace and window selection
    Given Orbit is open with a selected workspace and window
    When the user presses workspace arrows configured by `iris.orbit.keys`
    Then the workspace selection moves one item and selects its focused or first window
    When the user presses window arrows, Tab, or Shift+Tab
    Then the window selection moves within the selected workspace
    When the user presses Enter
    Then Orbit activates the selected window or empty selected workspace
    When the user presses Escape
    Then Orbit closes

  Scenario: Select on click and activate the current selection
    Given a workspace or window outside the current selection is visible
    When the user clicks it
    Then Orbit selects it without sending a Niri focus request
    When the user clicks the selected workspace or a window on that workspace
    Then Orbit sends one Niri focus request for that item
```

## Source cross-check and open edge cases

The surface map paths in `docs/GPUI_SURFACE_MAP.md` were checked against the
repository; every path-like entry resolves to an existing file or directory.
The QML roots confirm a few details that should remain explicit during
implementation and parity review:

- The vertical bar maps `bar.bottom` to the **right** edge, while horizontal
  bars use that option for bottom placement. Treat this as orientation-specific
  semantics, including popup direction, blur/mask geometry, and hot edge.
- Its `screenList` has an all-outputs fallback when no configured name matches.
  Removing or renaming the last matching output must exercise that fallback.
- The vertical bar is gated by `GlobalStates.barOpen`, unlocked session state,
  widget-edit state, and shell-entry readiness. Auto-hide separately controls
  content translation, hover-mask width, delayed Super reveal, and reservation;
  these are separate visual and input states.
- The vertical content has a right-click menu, brightness and volume scroll
  regions, visualizer process/visibility gates, taskbar-dependent composition,
  optional device indicators, and a shell-edit drag frame. The broad surface
  map names most of the content but does not spell out all of these interactions.
- The shared host must account for family load failure and rollback/fallback,
  stale asynchronous output events, scale-only versus refresh-only display
  events, and transitions where the old family's input surfaces must not leak.
- Composition roots also create non-panel visual pieces such as ii ripple
  effects and the ii layout editor, Waffle's edit HUD, and Island's orbit hot
  corner/sidebar edge triggers. The surface map should include them in the
  relevant shared/family visual inventory before claiming exhaustive coverage.

The exact set of GPUI-supported compositor capabilities, source-data adapters,
and current-versus-target parity must be recorded by the implementation and
review agents. No capability or criterion is marked passed here.

## ii Widgets cryptocurrency ticker

The GPUI Crypto adapter, configuration, and ii left Widgets card are connected
in the prototype. The card gate is the selected Widgets tab,
`sidebar.widgets.crypto` (default `false`), and normalized
`sidebar.widgets.widgetOrder` membership for `crypto`. The provider worker also
requires the sidebar to be open and the Widgets tab enabled; it runs off the UI
thread and discards results after visibility or request-config changes. Rows
follow the configured `sidebar.widgets.crypto_settings.coins` order. The
adapter uses CoinGecko's Simple Price endpoint for USD prices and 24-hour
changes, then its one-day market-chart endpoint for sparklines. Its cache is
`${Directories.state}/user/crypto_cache.json`, using the QML-compatible
`cryptoData`, `sparklineData`, and millisecond `timestamp` fields. The bundled
`defaults/config.json` sets `refreshInterval` to 300 seconds; the QML fallback
when that field is absent is 60 seconds. The settings control supports 30–300
seconds in 30-second steps, and GPUI normalizes the configured value to that
range before using it for cache expiry and periodic scheduling. Initial loads
and an explicit refresh can request data outside the periodic interval. QML
currently schedules a first refresh shortly after reading the cache and does
not use its cached timestamp to decide freshness; GPUI instead refreshes when
the cache is absent or expired. These are documented migration-target
semantics, not exact claims of QML behavior. The GPUI card keeps available rows
visible while a refresh is running, shows cache freshness after a failed
refresh, and does not substitute numeric defaults for missing prices or
changes. Price/cache results become ready before a separate background history
task completes; pending history is exposed as a loading state. The history task
requests sparklines serially for at most the first 18 configured coins (the
QML `popularCoins` selection set); later configured rows keep their prices but
have no sparkline. History requests do not block the panel's price worker, and
their results only merge sparkline values into the latest cache. Sparklines use
provider history only and are omitted when fewer than two valid samples exist
or history is unavailable. The connected Refresh control issues a forced
refresh and is unavailable while loading or when there are no configured
coins; it supports pointer and Enter/Space activation. Crypto settings editing
is outside this slice. The implementation was format-checked and compile-checked
(`cargo fmt`, `cargo check --locked --offline`, and `git diff --check` reported
success by the implementation agent); no tests or Gherkin scenarios were run.
There has been no live GPUI visual or accessibility-tree validation, so this
does not establish visual or runtime parity.

```gherkin
Feature: ii Widgets cryptocurrency ticker

  Background:
    Given the ii left sidebar is available
    And the shell configuration is ready

  Scenario: Hide the ticker when its feature gate is disabled
    Given the left Widgets tab is selected
    And "sidebar.widgets.crypto" is false
    And normalized "sidebar.widgets.widgetOrder" contains "crypto"
    When the Widgets content is rendered
    Then the Crypto ticker is not rendered
    And no ticker price or sparkline request is started

  Scenario Outline: Require the Widgets surface and configured order membership
    Given "sidebar.widgets.crypto" is true
    And the Widgets content is rendered
    And <gate>
    When the left sidebar is displayed
    Then the Crypto ticker is not rendered on the Widgets surface

    Examples:
      | gate                                                        |
      | a tab other than Widgets is selected                         |
      | normalized "sidebar.widgets.widgetOrder" omits "crypto"    |

  Scenario: Place the ticker at its configured widget order
    Given "sidebar.widgets.crypto" is true
    And normalized "sidebar.widgets.widgetOrder" contains "crypto"
    And the order is ["week", "crypto", "media"]
    When the Widgets content is rendered
    Then the visible cards appear in the order Week, Crypto, Media
    And hidden cards do not create empty order slots

  Scenario: Preserve configured coin order
    Given the configured coin IDs are ["bitcoin", "ethereum", "solana"]
    And provider data is available for all three coins
    When the Crypto ticker is rendered
    Then its rows appear as BTC, ETH, SOL in that order
    And the rows use the provider's values for their matching coin IDs

  Scenario: Show an explicit empty state for no configured coins
    Given "sidebar.widgets.crypto" is true
    And the configured coin list is empty
    When the Crypto ticker is rendered
    Then it displays an explicit add-coins-in-settings message
    And it renders no coin rows or sparklines
    And it does not request prices for an empty coin list

  Scenario: Show loading when no cached values are available
    Given the configured coin list is non-empty
    And no usable cache is available
    And the price provider is loading
    When the Crypto ticker is rendered
    Then it exposes an explicit loading status
    And it does not show placeholder zero prices or fabricated coin rows

  Scenario: Keep usable cached data visible during refresh
    Given the cache contains provider values for configured coins
    And the cache is marked stale
    And a refresh is in progress
    When the Crypto ticker is rendered
    Then cached prices and changes remain visible in configured coin order
    And the ticker exposes both loading and stale-cache status
    And no missing value is substituted with a numeric default

  Scenario: Report a failed refresh while retaining cached values
    Given the cache contains usable values for configured coins
    And a price refresh fails or returns an invalid response
    When the provider reports the refresh failure
    Then the cached values remain visible
    And the ticker exposes that cached values are being shown after a failed refresh
    And it does not report fabricated prices or changes as current data

  Scenario: Report an error when neither provider nor cache has usable values
    Given the configured coin list is non-empty
    And no usable cache is available
    And the price provider reports an error
    When the Crypto ticker is rendered
    Then it displays an explicit unavailable or failed-to-load status
    And it does not render invented prices, percentage changes, or coin rows

  Scenario: Show only provider-backed sparkline data
    Given a configured coin has fewer than two valid historical samples
    When its row is rendered
    Then no trend line is drawn for that coin
    And the row exposes that historical trend data is unavailable
    When the provider supplies at least two valid historical samples
    Then the row may render a sparkline from those samples
    And it does not synthesize samples to fill missing history

  Scenario: Show quote rows before history enrichment completes
    Given the price provider has returned valid quotes
    And one or more history requests remain pending
    When the Crypto ticker is rendered
    Then quote rows are ready without waiting for history requests
    And rows with pending history expose a loading trend state
    And the panel price worker remains available for later refreshes

  Scenario: Bound historical chart requests
    Given more than 18 coin IDs are configured
    When quote and history data are loaded
    Then prices and rows are retained for every configured coin in order
    And history requests are made only for the first 18 configured coins
    And later rows expose unavailable trend data without fabricated samples

  Scenario: Refresh periodically using the normalized configured interval
    Given the ticker remains enabled and visible on the Widgets surface
    And the configured interval has been normalized to 30–300 seconds in 30-second steps
    And the price cache is absent or at least one normalized interval old
    When the periodic refresh becomes due
    Then the connected price provider receives a refresh request
    And periodic refresh requests are not scheduled more frequently than the normalized interval
    And an initial load or explicit user refresh may request data outside that periodic schedule

  Scenario: Invoke refresh only when a real provider action is connected
    Given the ticker has a connected refresh action
    And a refresh is not already in progress
    When the user clicks Refresh or activates it with Enter or Space
    Then exactly one provider refresh request is issued
    When no refresh action is connected
    Then no control is presented as an actionable refresh button
    And keyboard activation does not claim or simulate a refresh

  Scenario: Keep the refresh control unavailable while a request is active
    Given a provider refresh is in progress
    When the Crypto ticker is rendered
    Then its loading state is exposed
    And the refresh control cannot issue a duplicate request
```
