# GPUI visual surface map

This map inventories the current QML visual entrypoints for planning the GPUI
port. The QML composition roots are the source of truth for which surfaces are
resident, deferred, on-demand, family-specific, or shared. A visually complete
port must cover both the visible pixels and the states that make those pixels
appear, move, focus, or disappear.

## Entry and composition roots

| Scope | Current entrypoints | Port order / parity gate |
|---|---|---|
| Shell startup and family selection | `shell.qml`, `GlobalStates.qml`, `FamilyTransitionOverlay.qml`; wrappers `ShellIiPanels.qml`, `ShellWafflePanels.qml`, `ShellIrisPanels.qml` | First. Preserve ready/loading startup, selected family, transition, lock state, panel enablement, open/closed state, and per-output routing. |
| ii composition | `modules/ii/critical/ShellIiCriticalPanels.qml`, `modules/ii/ShellIiPanelsImpl.qml` | Critical surfaces appear at first frame; deferred and on-demand surfaces follow without changing their visibility conditions. |
| Waffle composition | `modules/waffle/critical/ShellWaffleCriticalPanels.qml`, `modules/waffle/ShellWafflePanelsImpl.qml` | Same lifecycle gates as current Waffle composition; retain shared modules that run in this family. |
| Island composition | `modules/iris/critical/ShellIrisCriticalPanels.qml`, `modules/iris/ShellIrisPanelsImpl.qml` | Preserve Island's small critical chassis and deferred/on-demand pages, including compiled-on-demand failure/ready states. |
| Shared visual/config foundation | `modules/common/Appearance.qml`, `modules/waffle/looks/Looks.qml`, `modules/iris/style/IrisStyle.qml`, `modules/common/widgets/`, `services/` | Port after the surface host contract is stable; same data/config must produce corresponding family-specific tokens, typography, motion, and shared controls. |

## Prioritized visual surfaces

| Priority | Family / surface | QML visual entrypoints | Required states and parity criteria |
|---|---|---|---|
| P0 | Shared layer surfaces | Critical roots above; prototype host `prototypes/gpui-shell/src/surface_manager.rs` | Top/bottom/left/right anchors as needed; transparent and opaque regions; correct exclusive zone; outputs added/removed/resized; focus and input ownership; lock screen and shutdown behavior; no stale or invisible input-blocking surfaces. |
| P1 | ii background and backdrop | `modules/background/Background.qml`, `modules/background/Backdrop.qml`; composition in `modules/ii/critical/ShellIiCriticalPanels.qml` and `modules/ii/ShellIiPanelsImpl.qml` | Wallpaper loaded/loading/missing/video-thumbnail; per-output wallpaper; blur/material and widget canvas; backdrop on/off; theme changes; scaling and output geometry. |
| P1 | ii horizontal bar | `modules/bar/Bar.qml`, `modules/bar/BarContent.qml`, `modules/bar/BarGroup.qml`, `modules/bar/Workspaces.qml`, `modules/bar/ClockWidget.qml`, `modules/bar/Resources.qml`, `modules/bar/UtilButtons.qml`, `modules/bar/SysTray.qml`; alternate styles `modules/pill/PillBar.qml`, `modules/barM3/M3Bar.qml` | Top/bottom placement, five configured zones, active/occupied/empty workspaces, active window, clock/date, tray, battery/network/audio/weather indicators, popups, compact-width compression, hidden/revealed/reserved auto-hide, hover/hold, theme/style changes, module reordering. |
| P1 | ii vertical bar | `modules/verticalBar/VerticalBar.qml`, `modules/verticalBar/VerticalBarContent.qml`, `modules/verticalBar/Resources.qml`, `modules/verticalBar/VerticalClockWidget.qml`, `modules/verticalBar/VerticalDateWidget.qml`, `modules/verticalBar/VerticalMedia.qml`, `modules/verticalBar/BatteryIndicator.qml` | Left/right placement; stacked controls and labels; overflow and narrow-height behavior; orientation-aware popups; configured output list and stale-output fallback; auto-hide and exclusive-zone behavior. |
| P1 | ii dock | `modules/dock/Dock.qml`, `modules/dock/` | Four edges; pinned/running/hovered/dragged apps; notification badges; app/window cycling; reorder and pin persistence; empty dock; output selection and edge collision. |
| P1 | Waffle background and taskbar | `modules/waffle/background/`, `modules/waffle/bar/`, `modules/waffle/backdrop/`; `modules/waffle/critical/ShellWaffleCriticalPanels.qml` | Wallpaper/backdrop states; bottom taskbar, start button, pinned/running apps, tray and clock; focused/hovered/pressed states; output geometry and taskbar reservation. Preserve Fluent density and `Looks.*` tokens. |
| P1 | Island background, field and chassis | `modules/iris/background/IrisBackground.qml`, `modules/iris/field/`, `modules/iris/frame/`, `modules/iris/bar/IrisBar.qml`, `modules/iris/bar/IrisIsland.qml`, `modules/iris/bar/island/` | Resting/expanded/closing; all four edges and side compact form; `full` and `menubar` layouts; joined contour/reservations; wallpaper glass, solid/tinted/compositor-blur material; motion and reduced-animation states; output changes. Preserve `IrisStyle`. |
| P2 | ii sidebars and dashboard | `modules/sidebar/`, `modules/sidebarLeft/`, `modules/sidebarRight/`, `modules/dashboard/`; composition in `modules/ii/ShellIiPanelsImpl.qml` | Left/right semantic roles and configurable physical edges; closed/open/expanded/held; focus transfer; resize/drag; populated/empty/loading/error content; calendar, notification, media, widgets and tools; dashboard layout and edit mode. |
| P2 | ii overview and quick surfaces | `modules/overview/`, `modules/controlPanel/`, `modules/clipboard/`, `modules/cheatsheet/`, `modules/mediaControls/`, `modules/equalizer/` | Overview modes/search/no results; apps/windows/workspaces; quick control enabled/disabled/loading/error; clipboard empty/search/image/pinned; media no-player/playing/paused/lyrics/preset; equalizer unavailable/loading/ready/error; keyboard navigation and focus. |
| P2 | Waffle primary panels | `modules/waffle/startMenu/`, `modules/waffle/actionCenter/`, `modules/waffle/notificationCenter/`, `modules/waffle/taskview/`, `modules/waffle/widgets/`, `modules/waffle/clipboard/`, `modules/waffle/altSwitcher/` | Start menu search, pinned/recent/empty results; quick settings toggles and unavailable devices; notification list/calendar/unread/read/empty; workspace/window preview states; widgets; clipboard; switcher presets. Preserve Waffle-specific layout and keyboard flows. |
| P2 | Island content and controls | `modules/iris/palette/`, `modules/iris/orbit/`, `modules/iris/control/`, `modules/iris/sidebar/`, `modules/iris/stage/`, `modules/iris/pieces/`, `modules/iris/dock/`, `modules/iris/widgets/` | Palette query/results/actions; Orbit workspace/window selection; controls on Island vs detached body; sidebar sections; page/card switching; pieces attached/edge/floating; dock `auto` and edge collisions; widget faces, stacks and editing. Preserve each surface's geometry and transition timing. |
| P2 | Island edge triggers | `modules/iris/ShellIrisPanelsImpl.qml` (Orbit hot-corner and sidebar edge triggers) | Preserve activation edge/corner and hit area, family/open-state gates, output routing, touch/pointer semantics, and ensure transparent trigger surfaces do not block unrelated input. |
| P2 | Settings and visual editors | `settings.qml`, `waffleSettings.qml`, `modules/settings/SettingsPageRegistry.qml`, `SettingsPageHost.qml`, `SettingsOverlay.qml`, `modules/settings/*.qml`, `modules/waffle/settings/`, `modules/iris/settings/`, `modules/iris/studio/`, `modules/iris/edit/`, `modules/iris/preview/` | Config-not-ready/loading; standalone vs overlay/window host; initial/requested/persisted page; page/section search; navigation edit mode; dirty/valid/error states; native-dialog open/close; theme/style editors and previews; config change reflected live and persisted. Preserve distinct Waffle page ordering and Island Settings/Studio/Customize modes. |
| P3 | Shared visual widgets and editing | `modules/common/widgets/`, `modules/background/widgets/`, `modules/background/desktopItems/`, `modules/iris/widgets/`, `modules/iris/edit/` | Every widget face's loading/empty/data/error states; add/remove/reorder/resize/stack; drag proxy and drop targets; widget look overrides, family switch persistence, and per-output placement. Shared widgets must be checked in each consuming family. |
| P3 | ii ripple effects and layout editor | `modules/common/widgets/FluidRipple.qml`, `modules/screenCorners/ScreenCorners.qml`, `modules/common/widgets/ShellLayoutEditorWindow.qml`, `modules/common/widgets/ShellEditChromePreview.qml` | Preserve configured ripple triggers, duration, glow/sparkle, reduced-motion behavior and screen-corner gating; preserve editor window sizing, current layout preview, drag/drop, save/cancel, and edit ownership of panels. |
| P3 | Waffle edit HUD | `modules/common/widgets/ShellEditHud.qml`, `modules/common/widgets/ShellEditHudContext.qml`, `modules/common/widgets/ShellEditChromePreview.qml` | Preserve edit-mode entry/exit, selection frame, drag/resize controls, contextual actions, family visibility, focus ownership, and cleanup after cancel or family switch. |
| P3 | Wallpaper and theme browsers | `modules/wallpaperSelector/`, `modules/wallpaperLauncher/`, `modules/iris/wallpaper/`, `modules/settings/ThemesConfig.qml`, `modules/settings/BackgroundConfig.qml`, `modules/settings/CustomThemeEditor.qml` | Search/filter/loading/no matches; folder navigation; static/video preview; selection/application progress/failure; user theme create/edit/delete/apply; current selection and per-output assignment. |
| P3 | Shared transient overlays | `modules/notificationPopup/`, `modules/onScreenDisplay/`, `modules/onScreenKeyboard/`, `modules/regionSelector/`, `modules/screenCorners/`, `modules/tilingOverlay/`, `modules/recordingOsd/`, `modules/shellUpdate/`, `modules/bootGreeting/`, `modules/workspaceStrip/`, `modules/ii/overlay/` | Toast arrival/timeout/action; OSD values changing/idle; OSK layouts/modifiers; region select/resize/annotate/confirm/cancel; hot-corner activation; tiling hints; recording on/off; update available/dismissed; greeting once-per-session; workspace strip hover/drag/close/focus; overlay pinned widgets/native dialogs. Respect input masks and dismissal. |
| P3 | Waffle transient presentation | `modules/waffle/notificationPopup/`, `modules/waffle/onScreenDisplay/`, shared `modules/lock/`, `modules/polkit/`, `modules/sessionScreen/` and Waffle presentation files | Fluent toast/OSD; lock authenticated/failed/fingerprint/pending; PolicyKit prompt/cancel/error; logout/reboot/shutdown/suspend confirmation; no other family presentation leaks through. |
| P3 | Island transient/system presentation | `modules/iris/notificationPopup/`, `modules/iris/onScreenDisplay/`, `modules/iris/lock/`, `modules/iris/polkit/`, `modules/iris/session/`, `modules/iris/osk/`, shared `modules/regionSelector/` | Island banner/OSD; lock rehearsal vs real lock, password/fingerprint/error; polkit request; session action/confirm; keyboard layouts; selection/annotation states; Island materials and safe edge clearance. |

## Family acceptance gates

### Material ii

- Every enabled panel from `modules/ii/critical/ShellIiCriticalPanels.qml` and
  `modules/ii/ShellIiPanelsImpl.qml` has a GPUI counterpart and preserves its
  configured visibility and lifecycle.
- Horizontal, vertical, pill, and M3 bars preserve layout, styles, outputs,
  workspaces, compact behavior, popups, auto-hide, and exclusive-zone behavior.
- Dock, wallpaper/background, sidebars, overview, settings, and transient
  surfaces preserve all listed interactive and empty/error states.
- Appearance presets and configuration edits update the rendered result without
  losing persisted user choices.

### Waffle

- Every Waffle critical/deferred surface has a counterpart; shared panels are
  included only where the current composition loads them.
- Taskbar, Start Menu, Action Center, Notification Center, Task View, widgets,
  clipboard, switcher, and settings preserve Fluent layout, density, motion,
  native states, and keyboard interaction.
- Lock, PolicyKit, session, notification, and OSD use Waffle presentation when
  Waffle is selected.

### Island

- Island critical composition preserves background readiness, reservations, and
  chassis readiness; deferred surfaces keep their existing gates.
- Island and Dock work on all configured edges, with full/menubar layout,
  attached/floating pieces, page/card changes, and edge collision handling.
- Palette, Orbit, Controls, Settings, Studio, Customize, previews, themes,
  widgets and system surfaces preserve Island-specific geometry, materials,
  transitions, and open-state lifecycle.

### Cross-family

- Family switching follows configured order, presents the transition (or a cut
  when disabled), persists the selected family, and never leaves old-family
  surfaces or input regions behind.
- Output add/remove/rename/resize and stale configured output names do not hide
  surfaces or leave misplaced reservations.
- Locked-session state, config-not-ready/reload, panel disabled while open,
  on-demand close grace/retention, low-memory eviction, and delayed settings
  navigation produce a defined visual state.
- Compare screenshots at representative sizes and scales, but also exercise
  pointer, wheel, keyboard, drag, focus, loading, empty, failure, and dismissal
  paths. Screenshot resemblance alone is not parity.

## Recommended migration order

1. Prove the common GPUI surface host: multi-output geometry, layer-shell role,
   input/focus, reservations, lock lifecycle, and configure/close events.
2. Port shared tokens and primitives, then first-frame backgrounds and one
   primary edge surface for each family.
3. Complete ii, Waffle, and Island core surfaces in separate vertical slices,
   wiring their real config and service state as each slice lands.
4. Port settings/navigation and visual editors, then widgets and wallpaper
   tooling.
5. Port shared and family-specific transients, security surfaces, and less
   common panel states; finish with cross-family and multi-output parity passes.

The inventory is based on current composition roots and module catalogues. It is
a migration checklist, not a claim that any listed surface has already been
ported or that QML can be removed.
