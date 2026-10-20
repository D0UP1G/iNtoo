# GPUI Surface Coverage Register

This is a source-level migration register for the Quickshell/QML visual layer. It records QML loader owners, visible entry/exit states and actions, the current GPUI view and host path, data inputs, and available parity evidence. It is a planning artifact: **a GPUI source file or a host registration does not prove visual or runtime parity**.

The production shell is still Quickshell. `ARCHITECTURE.md` identifies `shell.qml` as the entry point; `scripts/intoo` selects `qs`, resolves a directory containing `shell.qml`, and starts Quickshell. Keep this register and prototype coverage separate from production replacement claims until an explicit cutover is reviewed.

## Status vocabulary

Use exactly one status per row:

- `missing`: no corresponding GPUI view/behavior was found.
- `source-only`: related GPUI code exists, but no production-like host path to this surface was found.
- `partial`: a GPUI equivalent is hosted or composed, with known missing behavior, data, states, or QML variants.
- `hosted`: a GPUI view is connected to the host for the surface; parity remains unverified.
- `runtime-verified`: a recorded live GPUI-vs-QML comparison covers the listed states and actions. No row currently meets this bar.

`host path` names the concrete GPUI `surface_manager.rs` function and/or `FamilyHost` composition. `—` means no equivalent was found during this source inspection. `Parity evidence` distinguishes source presence/tests from live visual comparison.

## Family roots and host boundaries

| QML root / gate | GPUI root / gate | Current evidence |
|---|---|---|
| `shell.qml` → `ShellIiCriticalPanels.qml` / `ShellIiPanels.qml` → `modules/ii/ShellIiPanelsImpl.qml` | `prototypes/gpui-shell/src/main.rs` → `surface_manager::start` → `FamilyHost` | QML startup/lazy-loader lifecycle is not replaced by the prototype. |
| `shell.qml` → `modules/waffle/critical/ShellWaffleCriticalPanels.qml` → `ShellWafflePanels.qml` → `modules/waffle/ShellWafflePanelsImpl.qml` | `surface_manager.rs` → `open_waffle_for_display` / Waffle overlay openers → `FamilyHost` | Only listed GPUI host paths count as present; shared deferred QML loaders still need equivalents. |
| `shell.qml` → `modules/iris/critical/ShellIrisCriticalPanels.qml` → `ShellIrisPanels.qml` → `modules/iris/ShellIrisPanelsImpl.qml` | `surface_manager.rs` → `open_iris_for_display` → `FamilyHost` → `IrisRuntime` | Iris bar content is mounted; the QML deferred surface set is not. |
| Shared security boundary: `modules/lock/Lock.qml` owns the session-lock lifecycle and chooses Material, Waffle, Waffle Safe, or Iris visuals while locked | No GPUI session-lock host found | A normal overlay/window is not evidence of compositor-enforced session locking; preserve QML production ownership until a native lock-surface path and authentication lifecycle are proven. |

## Surface register

`QML states/actions` lists loader conditions and principal user-visible entry/exit/action contract. `Adapter` names the current data/event source when found; missing adapters are explicit. `Parity evidence` below is source evidence only unless it names a live comparison artifact.

| Key | QML surface owner / states and actions | GPUI view / host path | Adapter | Parity evidence | Status |
|---|---|---|---|---|---|
| `ii/iiBackground` | `modules/ii/critical/ShellIiCriticalPanels.qml` → `modules/background/Background.qml`; enabled per panel/output, wallpaper loading/ready/missing, desktop interaction | `components/desktop_background.rs`; `open_background_for_display` | Host-resolved output, wallpaper path/image, colors | Component source only; no desktop-widget/item comparison | `partial` |
| `ii/iiBar` | Critical host selects stock `modules/bar/Bar.qml`, pill `modules/pill/PillBar.qml`, or M3 `modules/barM3/M3Bar.qml`; horizontal orientation, config-selected style, widgets/edit mode, launchers and popups | `components/top_bar.rs`; `open_for_display` → `FamilyHost` → `TopBar` | `ShellConfig`, `ThemeTokens`, Niri, MPRIS, panel events | Hosted GPUI root; QML variants/modules not exhaustively compared | `partial` |
| `ii/iiVerticalBar` | Critical host selects `modules/verticalBar/VerticalBar.qml` when `bar.vertical`; vertical widgets, launchers, media and battery states | `components/vertical_bar.rs`; `open_vertical_for_display` → `FamilyHost` | `ShellConfig`, theme, MPRIS; platform adapters | Hosted GPUI root; no paired visual/runtime proof | `partial` |
| `ii/iiDock` | `modules/dock/Dock.qml`; edge/visibility, pinned/running apps, launch, focus, context menu, preview | `families/ii/dock_surface.rs` + `dock.rs`; `open_ii_dock_for_display` | Desktop entries, Niri task/output state | GPUI surface/source present; no full QML interaction comparison | `partial` |
| `ii/iiBackdrop` | `modules/background/Backdrop.qml`; optional per-output surface; dim, blur/filter, vignette and wallpaper motion | `components/desktop_backdrop.rs`; `open_backdrop_for_display` | Host output/wallpaper/effect config | GPUI source documents unsupported filters; no parity capture | `partial` |
| `ii/iiNotificationPopup` | `modules/ii/ShellIiPanelsImpl.qml` → `modules/notificationPopup/NotificationPopup.qml`; notifications arrive, timeout/dismiss/action | `families/ii/notification_popup.rs`; `surface_manager::reconcile_ii_notification_popups` | Notification snapshots/actions, bounded D-Bus artwork decoding, existing local/theme icon resolution, configured output list/position | Host and source wiring compile with tests; sender HTTP(S) artwork is rejected pending a bounded safe fetcher. Lock/game suppression, QML transitions/drag behavior, and measured height/font-scale parity remain open | `partial` |
| `ii/iiOnScreenDisplay` | `modules/onScreenDisplay/OnScreenDisplay.qml`; volume/brightness/connectivity/media/mic/keyboard layout indicators; per-output routing | `families/ii/osd.rs`; `surface_manager::reconcile_ii_osd`; MPRIS transport action and active-track refresh routes | MPRIS snapshots/actions with 250 ms sampling while media OSD is active; existing local/file artwork only. PipeWire lacks default-sink semantics; brightness, mic, keyboard-layout, connection, and voice-search event providers remain absent | Material media OSD is hosted and refreshes track metadata/artwork path; remote artwork, non-media event producers, lock/game suppression, exact transitions, and paired runtime capture remain open | `partial` |
| `ii/iiBootGreeting` | `modules/bootGreeting/BootGreeting.qml`; startup greeting, dismissal/time bound | — | — | No GPUI surface found | `missing` |
| `ii/iiCheatsheet` | `modules/cheatsheet/Cheatsheet.qml`; opens/closes on `GlobalStates.cheatsheetOpen`, keybind browsing and periodic table | `families/ii/cheatsheet.rs`, `cheatsheet_elements.rs`, `keybind_provider.rs`, `cheatsheet_host.rs`; opt-in per-display Material overlay and Niri/Hypr parser adapters | Explicit `--ii-cheatsheet-preview-display` route; controlled search editor, two accessible tabs, GPUI clipboard copy feedback; parser launch is off the UI thread and compositor selection uses session signals | Production trigger, page persistence, config-reload parity, and live visual/keybind comparison remain unverified | `partial` |
| `ii/iiControlPanel` | `modules/controlPanel/ControlPanel.qml`; `controlPanelOpen`, quick actions, sliders, media/weather/system/wallpaper sections | `families/ii/control_panel.rs`; `surface_manager::open_ii_control_panel` | MPRIS snapshot; other service-backed sections explicitly unavailable | Hosted control-panel view; keep-loaded lifecycle, wallpaper/system/weather actions and live visual parity remain open | `partial` |
| `ii/iiDashboard` | `modules/dashboard/Dashboard.qml`; open/close, configured dashboard cards/layout | `families/ii/sidebars.rs` dashboard composition; `open_ii_panel_for_display` → `IiPanelSurface` (`IiPanelKind::Dashboard`) | Partial snapshots/state in panel surface | GPUI Dashboard kind exists; no full QML dashboard/layout comparison | `partial` |
| `ii/iiEqualizer` | `ShellIiPanelsImpl.qml` equalizer panel; ensure/refresh, supported/ready/error, band edits and presets | — | — | No GPUI equalizer surface or host registration found | `missing` |
| `ii/iiLock` | Shared `modules/lock/Lock.qml` → `LockSurface.qml`; session lock/recovery, PAM/fingerprint attempts, media and power actions; Material variant | `families/ii/lock.rs` presentational view only; no GPUI host route | Sanitized presentation snapshot and host action callback; `LockContext`, PAM, fprintd, and ext-session-lock host are not implemented in GPUI | Material view source and input routing exist without a secure host; no protocol/auth lifecycle or runtime comparison. Shared root also selects Waffle, Waffle Safe, and Iris variants and owns `WlSessionLock` (`modules/lock/Lock.qml`) | `source-only` |
| `ii/iiMediaControls` | `modules/mediaControls/MediaControls.qml`; player selection/presets, play state, artwork, progress/seek, lyrics, close | `components/media_controls_surface.rs`, `media_controls_popup.rs`; `open_media_controls_surfaces` | MPRIS provider with ordered per-player snapshots, targeted transport/seek actions, and local/HTTP(S) artwork | Hosted multi-player card/popup source and tests; QML eligibility/routing filters, lyrics, and paired runtime visuals remain unverified | `partial` |
| `ii/iiOnScreenKeyboard` | `modules/onScreenKeyboard/OnScreenKeyboard.qml`; open/close, layout and key activation | — | — | No GPUI OSK surface found | `missing` |
| `ii/iiOverlay` | `modules/ii/overlay/Overlay.qml`; open when overlay/pinned widgets/native dialog state is active; widget interaction | — | — | No GPUI overlay host found | `missing` |
| `ii/iiOverview` | `modules/overview/Overview.qml`; open/close with retention, workspace overview/search/modes/app launch | `families/ii/overview_overlay.rs`, `overview.rs`; `open_ii_overview` | Niri outputs/windows plus desktop-entry/search source | Hosted source path; no all-mode live comparison | `partial` |
| `ii/iiPolkit` | `modules/polkit/Polkit.qml`; authentication prompt, response/error/accept/cancel | — | — | No GPUI Polkit host found | `missing` |
| `ii/iiRegionSelector` | `modules/regionSelector/RegionSelector.qml`; select region, annotate, accept/cancel | — | — | No GPUI selector host found | `missing` |
| `ii/iiScreenCorners` | `modules/screenCorners/ScreenCorners.qml`; per-corner trigger zones | — | — | No GPUI trigger surface found | `missing` |
| `ii/iiSessionScreen` | `modules/sessionScreen/SessionScreen.qml`; open, session/power actions, cancel | `families/ii/session_screen.rs` + `session_screen_host.rs`; opt-in `--session-screen-preview-display <index>` → `toggle_ii_session_screen` → `open_ii_session_screen_surface` | Explicit display-list index, Material theme, English fallback labels; no focused-output, wallpaper, warning, hibernate-capability, localization, or production Session provider; optional typed action handler must acknowledge before close | Hosted preview route; root focus and 250 ms generation-guarded exclusive layer removal exist. No action adapters by default, mascot/blur/editorial/zzz parity or live QML comparison | `partial` |
| `ii/iiSidebarLeft` | `modules/sidebarLeft/SidebarLeft.qml`; lazy panel, tabs, app/widgets/media/notes and close | `families/ii/panel_surface.rs`, `sidebars.rs`, `left_sidebar_tabs.rs`; `open_ii_panel_for_display` | Snapshot inputs; specific feature adapters vary and require audit | Hosted panel kind; individual QML tabs and actions not parity-verified | `partial` |
| `ii/iiSidebarRight` | `modules/sidebarRight/SidebarRight.qml`; toggles/calendar/tools/notifications/audio/network/weather, close | `families/ii/panel_surface.rs`, `sidebars.rs`; `open_ii_panel_for_display` | Partial audio/weather/notification state; inspect each tab input | Hosted panel kind; tab/data/action parity incomplete | `partial` |
| `ii/iiTilingOverlay` | `modules/tilingOverlay/TilingOverlay.qml`; picker/OSD states, choose layout, dismiss | — | — | No GPUI overlay host found | `missing` |
| `ii/iiWallpaperSelector` | `modules/wallpaperSelector/WallpaperSelector.qml`; browse/select/apply/cancel | `families/ii/wallpaper_gallery.rs` is embedded in sidebar; no selector window route | Gallery input state only | Related source is not the QML selector contract | `source-only` |
| `ii/iiWallpaperLauncher` | `modules/wallpaperLauncher/WallpaperLauncher.qml`; quick browse, preview, apply/cancel | `families/ii/wallpaper_gallery.rs` embedded in sidebar; no launcher route | Gallery input state only | Related source only; preview/apply/cancel route absent | `source-only` |
| `ii/iiCoverflowSelector` | `modules/wallpaperSelector/WallpaperCoverflow.qml`; coverflow navigation, selection/apply/cancel | `families/ii/wallpaper_gallery.rs` is not a coverflow host | Gallery input state only | No coverflow equivalent/host found | `missing` |
| `ii/iiClipboard` | `modules/clipboard/ClipboardPanel.qml`; clipboard history/search/select/paste/clear | `families/ii/clipboard.rs`, `families/ii/clipboard_host.rs`; `open_ii_clipboard_preview(display)` / `toggle_ii_clipboard_preview(display)` → per-output surface | GPUI `CliphistProvider` adapter, sanitized history preview, browser-markup copy filter, typed actions, runtime-config pin merge, and config-watcher reconciliation; no shortcut/launcher producer | Opt-in route available on explicit display; dismissal blurs and destroys the Exclusive layer surface immediately to release compositor keyboard focus. A 250 ms generation-guarded output-route grace lets a toggle reopen invalidate stale close work. Production trigger remains unimplemented; QML visual comparison and live clipboard actions remain unverified; QML/GPUI concurrent writes retain a compare-to-rename race | `partial` |
| `ii/iiShellUpdate` | `modules/shellUpdate/ShellUpdateOverlay.qml`; update availability/progress/error/actions | — | — | No GPUI update overlay found | `missing` |
| `ii/iiRecordingOsd` | `modules/recordingOsd/RecordingOsd.qml`; visible while recording, status/stop | — | — | No GPUI recording OSD host found | `missing` |
| `ii/iiWorkspaceStrip` | `modules/workspaceStrip/WorkspaceStrip.qml`; workspaces, selection, cards/media and drag states | Overview has workspace UI, but no persistent strip host | Niri snapshot used elsewhere | No GPUI strip-specific registration | `missing` |
| `waffle/wBar` | `modules/waffle/bar/WaffleBar.qml` via critical root; bottom taskbar, start, tasks, tray, clock, notifications | `families/waffle/runtime.rs`, `taskbar.rs`; `open_waffle_for_display` → `FamilyHost` | XDG desktop entries, clock, Niri task snapshot; tray and notification indicators are currently empty/fixed | Hosted; source explicitly clears tray and unread count; no live visual comparison | `partial` |
| `waffle/wBackground` | `modules/waffle/background/WaffleBackground.qml`; per-output wallpaper/background/widgets | `components/desktop_background.rs`; `open_waffle_background_for_display` | Host-resolved static background inputs | Hosted basic view; Waffle widgets/layout parity unverified | `partial` |
| `waffle/wBackdrop` | `modules/waffle/backdrop/WaffleBackdrop.qml`; optional backdrop and dim/effect behavior | `components/desktop_backdrop.rs`; `open_waffle_backdrop_for_display` | Host effect inputs | Hosted basic view; effect parity gaps documented in component | `partial` |
| `waffle/wNotificationPopup` | `modules/waffle/notificationPopup/WaffleNotificationPopup.qml`; arrival, popup timeout/action/dismiss | `families/waffle/notification_overlay.rs` is notification center, not popup | Notification service feeds center | No popup-equivalent registration | `missing` |
| `waffle/wOnScreenDisplay` | `modules/waffle/onScreenDisplay/WaffleOSD.qml`; transient OSD and indicators | — | — | No GPUI OSD registration | `missing` |
| `waffle/wStartMenu` | `modules/waffle/startMenu/WaffleStartMenu.qml`; open/close, search, all-apps/back, result launch | `families/waffle/start_menu.rs`, `taskbar/start_menu_overlay.rs`; `open_waffle_overlay` | XDG desktop-entry source and query/launch actions | Hosted; no QML comparison for all pages/states | `partial` |
| `waffle/wActionCenter` | `modules/waffle/actionCenter/WaffleActionCenter.qml`; quick actions, expand/options, dismiss | `families/waffle/action_center.rs`, `action_center_overlay.rs`; `open_action_center_overlay` | Controller/events; per-control live service adapters require audit | Hosted; data/effect parity not proven | `partial` |
| `waffle/wNotificationCenter` | `modules/waffle/notificationCenter/WaffleNotificationCenter.qml`; groups, search, DND, calendar, dismiss/action | `families/waffle/notification_center.rs`, `notification_overlay.rs`; `open_notification_overlay` | `notification_service.rs`, notification controllers/events | Hosted; no paired QML runtime comparison | `partial` |
| `waffle/wWidgets` | `modules/waffle/widgets/`; configured widget open/close and widget actions | — | — | No GPUI Waffle widget surface found | `missing` |
| `waffle/wLock` | Shared `modules/lock/Lock.qml`; Waffle and Waffle Safe variants, lock/unlock, PAM/fingerprint states and power actions | — | Shared QML `LockContext`; no GPUI equivalent found | No GPUI compositor session-lock host or authenticated recovery path | `missing` |
| `waffle/wPolkit` | Shared `modules/polkit/Polkit.qml`; auth prompt/accept/cancel | — | — | No GPUI Polkit host | `missing` |
| `waffle/wSessionScreen` | Shared `modules/sessionScreen/SessionScreen.qml`; session actions and cancel | — | — | No GPUI session host | `missing` |
| `waffle/wTaskView` | `modules/waffle/taskview/WaffleTaskView.qml`; open/close, inspect/select/focus task | `families/waffle/task_view.rs`, `task_view_overlay.rs`; `open_task_view_overlay` | Niri windows/tasks and task-view controller | Hosted; no parity proof for grouping/previews/gestures | `partial` |
| `waffle/iiBootGreeting` | Shared `modules/bootGreeting/BootGreeting.qml`; startup state | — | — | No GPUI surface found | `missing` |
| `waffle/iiCheatsheet` | Shared `modules/cheatsheet/Cheatsheet.qml`; keybind search/browse/close | — | — | No GPUI surface found | `missing` |
| `waffle/iiOnScreenKeyboard` | Shared `modules/onScreenKeyboard/OnScreenKeyboard.qml`; keys/layout/open/close | — | — | No GPUI OSK host | `missing` |
| `waffle/iiOverlay` | Shared `modules/ii/overlay/Overlay.qml`; pinned widgets/native-dialog states | — | — | No GPUI overlay host | `missing` |
| `waffle/iiOverview` | Shared `modules/overview/Overview.qml`; workspace/search and app launch | `families/ii/overview_overlay.rs`; `open_ii_overview` is enabled for ii family only | Niri adapter | Waffle-specific route not found | `missing` |
| `waffle/iiRegionSelector` | Shared `modules/regionSelector/RegionSelector.qml`; select/annotate/cancel | — | — | No GPUI host | `missing` |
| `waffle/iiScreenCorners` | Shared `modules/screenCorners/ScreenCorners.qml`; corner trigger zones | — | — | No GPUI host | `missing` |
| `waffle/iiWallpaperSelector` | Shared `modules/wallpaperSelector/WallpaperSelector.qml`; browse/apply/cancel | Sidebar gallery source only; no selector host | Gallery inputs only | No Waffle route/equivalent | `missing` |
| `waffle/iiWallpaperLauncher` | Shared `modules/wallpaperLauncher/WallpaperLauncher.qml`; preview/apply/cancel | Sidebar gallery source only; no launcher host | Gallery inputs only | No Waffle route/equivalent | `missing` |
| `waffle/iiCoverflowSelector` | Shared `modules/wallpaperSelector/WallpaperCoverflow.qml`; coverflow/apply/cancel | Sidebar gallery source only; no coverflow host | Gallery inputs only | No Waffle route/equivalent | `missing` |
| `waffle/iiClipboard` | `modules/clipboard/ClipboardPanel.qml`, gated off for Waffle family | Waffle-specific `families/waffle/clipboard.rs`, `clipboard_overlay.rs`; `open_clipboard_overlay` | `clipboard_provider.rs`, controller/events | Waffle uses its own `wClipboard` surface; shared ii panel is intentionally gated | `partial` |
| `waffle/iiRecordingOsd` | Shared `modules/recordingOsd/RecordingOsd.qml`; recording status/stop | — | — | No GPUI host | `missing` |
| `waffle/iiTilingOverlay` | Shared `modules/tilingOverlay/TilingOverlay.qml`; picker/OSD/layout choice | — | — | No GPUI host | `missing` |
| `waffle/iiWorkspaceStrip` | Shared `modules/workspaceStrip/WorkspaceStrip.qml`; persistent workspace strip | Task view has workspace/task content but no strip host | Niri task data elsewhere | No GPUI strip registration | `missing` |
| `iris/irisBackground` | `modules/iris/critical/ShellIrisCriticalPanels.qml` → `modules/iris/background/IrisBackground.qml`; background vs desktop-widget mode | `families/iris/background_host.rs`; opt-in `--iris-background-preview-display <index>` → exact GPUI `DisplayId` on a Bottom layer; output identity only from matching Niri snapshot | Typed config path resolver; Niri output mapping when available; no WallpaperListener preview/Awww/motion providers | Opaque still-image config snapshot or fallback plus explicit Awaiting/provider status; desktop widgets, production QML gate, live preview, animation, and runtime parity are not implemented | `partial` |
| `iris/irisBar` | `modules/iris/bar/IrisBar.qml` / `IrisIsland.qml`; edge/zone presentation, expand/collapse, palette/orbit, pieces and dock | `families/iris/runtime.rs`, `chassis.rs`, `palette.rs`, `orbit.rs`, `dock.rs`; `open_iris_for_display` → `IrisRuntime` | Theme/options, Niri windows/workspaces, XDG app catalog | Hosted root; runtime has empty activity/piece inputs; no QML visual comparison | `partial` |
| `iris/irisSidebarLeft` | `modules/iris/sidebar/IrisLeftSidebarPanel.qml`; configured sections, open/close, content actions | ii sidebar renderer is source-only for Iris; no Iris sidebar host | No Iris section adapter | No Iris route | `missing` |
| `iris/irisSidebarRight` | `modules/iris/sidebar/IrisRightSidebarPanel.qml`; configured sections, open/close, content actions | ii sidebar renderer is source-only for Iris; no Iris sidebar host | No Iris section adapter | No Iris route | `missing` |
| `iris/irisNotificationPopup` | `modules/iris/notificationPopup/IrisNotificationPopup.qml`; popup arrivals, fullscreen/game-mode gate, actions | Waffle notification service/view is a different family surface; no Iris popup host | Notification service exists; Iris popup binding absent | No Iris route | `missing` |
| `iris/irisStudio` | `modules/iris/studio/IrisStudio.qml`; open/edit/preview/persist Iris pieces | `families/iris/chassis.rs` / style code are not Studio UI | No editor persistence adapter | No Studio host | `missing` |
| `iris/irisSettings` | `modules/iris/settings/IrisSettingsOverlay.qml`; settings overlay/warm state, page navigation/edit | `mvp_settings.rs` is not a full Iris settings surface | Config parser only | No Iris settings host | `missing` |
| `iris/irisSettingsWindow` | `modules/iris/settings/IrisSettingsWindow.qml`; detached settings window when configured | `mvp_settings.rs` is not equivalent standalone UI | Config parser only | No GPUI window path | `missing` |
| `iris/irisOnScreenDisplay` | `modules/iris/onScreenDisplay/IrisOSD.qml`; per-output conditions and transient indicators | — | — | No GPUI Iris OSD host | `missing` |
| `iris/irisSessionScreen` | `modules/iris/session/IrisSessionScreen.qml`; session/power actions/cancel | — | — | No GPUI Iris host | `missing` |
| `iris/irisLock` | Shared `modules/lock/Lock.qml` → Iris lock variant; lock/auth/unlock and delayed visual release | — | Shared QML `LockContext`; no GPUI equivalent found | No GPUI compositor session-lock host, Iris lock view, or authenticated recovery path | `missing` |
| `iris/irisPolkit` | `modules/iris/polkit/IrisPolkit.qml`; auth/accept/cancel | — | — | No GPUI Polkit host | `missing` |
| `iris/irisOverlay` | Shared `modules/ii/overlay/Overlay.qml`; overlay open/close and widgets | — | — | No GPUI host | `missing` |
| `iris/irisCheatsheet` | Shared `modules/cheatsheet/Cheatsheet.qml`; keybind search/browse/close | — | No Iris-specific visual or data/host adapter | No Iris cheatsheet view or host route found | `missing` |
| `iris/irisOnScreenKeyboard` | `modules/iris/osk/IrisOnScreenKeyboard.qml`; keys/layout/open/close | — | — | No GPUI Iris OSK host | `missing` |
| `iris/irisRegionSelector` | Shared `modules/regionSelector/RegionSelector.qml`; select/annotate/cancel | — | — | No GPUI host | `missing` |
| `iris/irisLockRehearsal` | `modules/iris/lock/IrisLockRehearsal.qml`; lock design preview/edit | — | — | No GPUI host | `missing` |
| `iris/irisWallpaperLauncher` | Shared `modules/wallpaperLauncher/WallpaperLauncher.qml`; preview/apply/cancel | ii wallpaper gallery source only; no Iris route | Gallery inputs only | No Iris host | `missing` |
| `iris/irisCoverflowSelector` | Shared `modules/wallpaperSelector/WallpaperCoverflow.qml`; coverflow/apply/cancel | ii wallpaper gallery source only; no Iris route | Gallery inputs only | No Iris host | `missing` |
| `iris/irisRecordingOsd` | Shared `modules/recordingOsd/RecordingOsd.qml`; visible while recording, stop/action | — | — | No GPUI host | `missing` |
| `iris/irisTilingOverlay` | Shared `modules/tilingOverlay/TilingOverlay.qml`; picker/OSD/layout selection | — | — | No GPUI host | `missing` |

## Standalone and shared windows

These roots are not all `enabledPanels` loader IDs; they remain part of the overall visual migration scope.

| Key | QML root / state and actions | GPUI view / host path | Adapter | Parity evidence | Status |
|---|---|---|---|---|---|
| `standalone/settings` | `settings.qml` → shared settings navigation/pages; open destination, edit config, save/cancel | `mvp_settings.rs` has a prototype settings model, but no complete GPUI settings window/route | GPUI config loader/model only | No full settings route or page inventory mapping | `source-only` |
| `standalone/waffleSettings` | `waffleSettings.qml` → Waffle settings GUI | No GPUI equivalent | — | No host/view found | `missing` |
| `standalone/welcome` | `welcome.qml`; first-run/setup steps, completion/dismissal | No GPUI equivalent | — | No host/view found | `missing` |
| `standalone/killDialog` | `killDialog.qml`; confirm/cancel process termination | No GPUI equivalent | — | No host/view found | `missing` |
| `standalone/familyTransition` | `FamilyTransitionOverlay.qml`; family-switch transition and completion | Config family selection exists; no transition surface | Config monitor only | No animated GPUI transition | `missing` |
| `standalone/mascotAssistant` | `modules/mascotAssistant/MascotAssistantWindow.qml`; open/close, transcript, streaming reply and input | No GPUI window | Python service is not a GPUI view | No GPUI host | `missing` |
| `standalone/closeConfirm` | `modules/closeConfirm/CloseConfirm.qml` plus family-specific content; confirm/cancel close | No GPUI equivalent | — | No host/view found | `missing` |
| `standalone/irisSettingsWindow` | Also loader-owned as `iris/irisSettingsWindow`; detached Iris settings | No GPUI window | — | See family row | `missing` |
| `standalone/settingsOverlayRail` | `shell.qml:650-656` → `modules/settings/SettingsOverlay.qml`; non-Iris overlay mode, rail navigation, page/section navigation and close | `mvp_settings.rs` has a model but no overlay window or `surface_manager` route | Config model only; no GPUI overlay event route | QML root and gate confirmed; no GPUI overlay route found | `missing` |
| `standalone/settingsOverlayFocus` | `shell.qml:658-662` → `modules/settings/SettingsFocus.qml`; focus-style settings overlay, page navigation/edit and close | No GPUI focus-style settings overlay | Config model only | No GPUI view/host or visual comparison found | `missing` |
| `standalone/settingsOverlayEditorial` | `shell.qml:664-668` → `modules/settings/SettingsEditorial.qml`; editorial-style settings overlay, page navigation/edit and close | No GPUI editorial settings overlay | Config model only | No GPUI view/host or visual comparison found | `missing` |
| `ii/iiAltSwitcherVisual` | `shell.qml:683-688` → `modules/altSwitcher/AltSwitcher.qml`; ii visual task switcher, preset-dependent presentation, select/focus/cancel; gated to ii and visual presets | Waffle task view exists, but no ii alt-switcher view or host route | Niri task/window snapshot used by other surfaces; alt-switcher event binding absent | QML visual root/gate confirmed; no GPUI equivalent found | `missing` |
| `shared/japaneseLookup` | `shell.qml:698-700` → `modules/japaneseLookup/JapaneseLookup.qml`; lookup activation, query/input and result interaction | No GPUI view or `surface_manager` route found | No GPUI Japanese lookup adapter found | QML lazy root confirmed; no GPUI equivalent found | `missing` |
| `ii/iiLayoutEditorWindow` | `modules/ii/ShellIiPanelsImpl.qml:498` → `ShellLayoutEditorWindow`; open/edit panel layout, apply/save or cancel, styled from current family appearance tokens | No GPUI layout-editor window or host route found | No GPUI layout-edit state/event adapter found | QML component creation confirmed; no GPUI equivalent found | `missing` |

## QML surfaces without an enabledPanels identifier

The following surfaces are created by `LazyLoader`/direct component composition and therefore are not represented by a `PanelLoader.identifier`. They still need explicit GPUI coverage and parity states.

| Key | QML owner / states and actions | GPUI view / host path | Adapter | Parity evidence | Status |
|---|---|---|---|---|---|
| `waffle/wClipboard` | `modules/waffle/clipboard/WaffleClipboard.qml`; open/close, clipboard history/search/select/paste/clear | `families/waffle/clipboard_overlay.rs`; `open_clipboard_overlay` (runtime feature is keyed as `wClipboard`) | `clipboard_provider.rs`, clipboard controller/events | Hosted overlay; QML visual and interaction comparison absent | `partial` |
| `waffle/wAltSwitcher` | `modules/waffle/altSwitcher/WaffleAltSwitcher.qml`; compositor task-switch trigger, preset/visual/no-visual modes, choose/cancel | `families/waffle/task_view.rs` is task-view UI, not full alt-switcher; no equivalent host | Niri tasks used elsewhere | No alt-switcher parity or dedicated route | `missing` |
| `waffle/wShellEditHud` | `modules/waffle/background/WaffleShellEditHud.qml`; edit-mode controls and save/cancel layout | No Waffle edit HUD host | No Waffle layout-edit adapter | No equivalent view found | `missing` |
| `iris/irisBackdrop` | `modules/iris/ShellIrisPanelsImpl.qml` lazy shared `modules/background/Backdrop.qml`; Niri/deferred/config gate, effect states | Shared `components/desktop_backdrop.rs` is hosted for ii/Waffle only; no Iris call to `open_backdrop_for_display` found | No Iris backdrop route | Source exists; Iris host route absent | `source-only` |
| `iris/irisDesktopWidgets` | `modules/iris/ShellIrisPanelsImpl.qml` lazy `Background {}` under `iris.modules.desktopWidgets`; per-output widgets and edit | `components/desktop_background.rs` is a wallpaper view, not widget/layout surface; no Iris widget host | No Iris widget/layout adapter | No GPUI widget composition/host | `missing` |
| `iris/irisSidebarEdgeTriggers` | `modules/iris/sidebar/IrisSidebarEdge.qml`; edge gestures reveal left/right sidebar | No Iris edge-trigger surface | No Iris trigger state adapter | No equivalent host found | `missing` |

## Next-slice priority

1. **P0 — Establish a parity harness and complete ID routing.** For every `hosted` or `partial` row, record QML and GPUI screenshots at matched output size/scale and scripted open, close, focus, keyboard, pointer, config-disabled, and no-data states. Add a paired evidence link before moving any row to `runtime-verified`.
2. **P1 — Finish Material ii critical surfaces.** Complete bar variants/modules, dock interactions/previews, per-output background desktop items/widgets, backdrop effects, and separate control-panel/dashboard/sidebar routes.
3. **P1 — Fill common missing surfaces.** Lock, Polkit, session, OSK, notification popup/OSD, region selector, wallpaper selector/launcher/coverflow, overlay, tiling, workspace strip, and shell-update/recording surfaces are cross-family migration blockers.
4. **P1 — Close Waffle data/behavior gaps.** Connect task list, tray, notification counts/actions, widget surface and Waffle wallpaper/effect behavior; validate start menu, action center, notification center, clipboard, and task view against QML.
5. **P2 — Complete Iris.** Implement background/reservations and host every Iris loader (sidebars, settings/Studio, notification popup, OSD, lock/Polkit/session, OSK, wallpaper and edit/rehearsal flows); replace placeholder empty pieces/data in `IrisRuntime`.
6. **P0 final gate — Production lifecycle remains outstanding.** Only after all surfaces are runtime-verified should launcher, startup/service lifecycle, IPC, installation/update paths, and rollback be migrated and cut over. Keep Quickshell production path and QML intact until then.

## Inventory commands and evidence scope

Commands used to derive this register:

```sh
rg -n 'identifier:' modules/ii/ShellIiPanelsImpl.qml modules/ii/critical/ShellIiCriticalPanels.qml \
  modules/waffle/ShellWafflePanelsImpl.qml modules/waffle/critical/ShellWaffleCriticalPanels.qml \
  modules/iris/ShellIrisPanelsImpl.qml modules/iris/critical/ShellIrisCriticalPanels.qml
rg -n 'open_[a-zA-Z0-9_]+|open_window\(|FamilyComposition::|HostedFamily::|create_composition\(' \
  prototypes/gpui-shell/src/surface_manager.rs prototypes/gpui-shell/src/family_host.rs
rg -n 'exec.*qs|quickshell|shell.qml' scripts/intoo
```

These searches establish source declarations and wiring only. They do not verify that a view opens correctly in a Wayland session, matches QML geometry/theme/animation, or preserves input/focus behavior. Record exact runtime commands, environment/output, and screenshot or trace artifact in the relevant row before changing its status to `runtime-verified`.
