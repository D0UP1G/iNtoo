use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{Hash, Hasher},
    rc::Rc,
    sync::{
        OnceLock, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use async_channel::Sender;
use gpui::{
    App, AppContext, Bounds, DisplayChanges, DisplayEvent, DisplayId, Pixels, Size, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
    layer_shell::*, point, px,
};

use crate::audio_provider::{AudioProviderEvent, PipeWireAudioProvider, ProviderStatus};
use crate::components::desktop_backdrop::{
    BackdropStatus, DesktopBackdrop, DesktopBackdropInput, DesktopBackdropWithAccessibility,
    Vignette, WallpaperKind,
};
use crate::components::desktop_background::{
    DesktopBackground, DesktopBackgroundProps, DesktopBackgroundWithAccessibility, WallpaperStatus,
};
use crate::config::{ConfigLoad, ShellConfig};
use crate::families::ii::dock_surface::IiDockSurface;
use crate::families::ii::overview_overlay::IiOverviewOverlay;
use crate::families::ii::{
    panel_surface::{IiPanelEvent, IiPanelKind, IiPanelState, IiPanelSurface},
    volume_mixer::VolumeMixerTab,
};
use crate::families::iris::runtime::IrisRuntime;
use crate::families::waffle::{
    controller::{
        ActionCenterController, ActionCenterEvent, ActionCenterState, ClipboardController,
        ClipboardEvent, ClipboardOverlayState, NotificationCenterController,
        NotificationCenterEvent, NotificationCenterState, StartMenuController, StartMenuEvent,
        StartMenuState, TaskViewController, TaskViewEvent, TaskViewState,
    },
    taskbar::start_menu_adapter::desktop_entries::XdgDesktopEntrySource,
};
use crate::family_host::{ConfiguredFamilyInputs, FamilyHost, FamilyRuntimeInputs};
use crate::theme::ThemeTokens;
use crate::theme_resolver;

type SurfaceMap = Rc<RefCell<HashMap<DisplayId, WindowHandle<FamilyHost>>>>;
type VerticalSurfaceMap = Rc<RefCell<HashMap<DisplayId, WindowHandle<FamilyHost>>>>;
type BackgroundMap = Rc<RefCell<HashMap<DisplayId, WindowHandle<DesktopBackground>>>>;
type BackdropMap = Rc<RefCell<HashMap<DisplayId, WindowHandle<DesktopBackdrop>>>>;
type WaffleBackgroundMap =
    Rc<RefCell<HashMap<DisplayId, WindowHandle<DesktopBackgroundWithAccessibility>>>>;
type WaffleBackdropMap =
    Rc<RefCell<HashMap<DisplayId, WindowHandle<DesktopBackdropWithAccessibility>>>>;
type IiPanelSurfaceMap = Rc<RefCell<HashMap<DisplayId, WindowHandle<IiPanelSurface>>>>;
type IiDockSurfaceMap = Rc<RefCell<HashMap<DisplayId, WindowHandle<IiDockSurface>>>>;
type IiOutputNameMap = Rc<RefCell<HashMap<DisplayId, String>>>;
type WaffleOutputNameMap = Rc<RefCell<HashMap<DisplayId, Option<String>>>>;
type IiOverviewOverlayMap = Rc<RefCell<HashMap<DisplayId, WindowHandle<IiOverviewOverlay>>>>;
type IiPanelStateMap = Rc<RefCell<HashMap<DisplayId, IiPanelState>>>;
type MediaStateCache = Rc<RefCell<crate::components::vertical_bar::MediaState>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeConfigFingerprint {
    modified: Option<SystemTime>,
    length: Option<u64>,
    content_hash: Option<u64>,
    read_error: Option<std::io::ErrorKind>,
}

struct RuntimeConfigObservation {
    fingerprint: RuntimeConfigFingerprint,
    loaded: ConfigLoad,
}

#[derive(Clone)]
struct ConfigRevision {
    revision: u64,
    loaded: ConfigLoad,
}

#[derive(Clone)]
struct IiCriticalSnapshot {
    revision: u64,
    loaded: ConfigLoad,
    theme: ThemeTokens,
}

#[derive(Clone)]
struct NiriOutputGeometry {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Default)]
struct NiriOutputSnapshotState {
    last_success: Option<(Instant, Vec<NiriOutputGeometry>)>,
    last_attempt: Option<Instant>,
    consecutive_failures: u32,
    last_error: Option<String>,
}

static NIRI_OUTPUT_SNAPSHOT: OnceLock<RwLock<NiriOutputSnapshotState>> = OnceLock::new();
static NIRI_OUTPUT_REFRESH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static NIRI_OUTPUT_REFRESH_SENDER: OnceLock<async_channel::Sender<()>> = OnceLock::new();

fn report_config_load(revision: Option<u64>, loaded: &ConfigLoad) {
    let revision_label = revision
        .map(|revision| format!(" revision {revision}"))
        .unwrap_or_default();
    eprintln!(
        "GPUI config{revision_label}: source {:?}; {} diagnostic(s)",
        loaded.source,
        loaded.diagnostics.len()
    );
    for diagnostic in &loaded.diagnostics {
        eprintln!("GPUI config{revision_label}: {diagnostic}");
    }
}

fn observe_runtime_config(path: &std::path::Path) -> RuntimeConfigObservation {
    let metadata = std::fs::metadata(path).ok();
    let bytes = std::fs::read(path);
    let (loaded, content_hash, read_error) = match bytes {
        Ok(bytes) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hasher);
            (ConfigLoad::from_bytes(&bytes), Some(hasher.finish()), None)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (ConfigLoad::from_defaults(), None, Some(error.kind()))
        }
        Err(error) => (ConfigLoad::from_path(path), None, Some(error.kind())),
    };
    let modified = metadata.as_ref().and_then(|value| value.modified().ok());
    let length = metadata.as_ref().map(std::fs::Metadata::len);
    RuntimeConfigObservation {
        fingerprint: RuntimeConfigFingerprint {
            modified,
            length,
            content_hash,
            read_error,
        },
        loaded,
    }
}

fn watch_runtime_config(
    cx: &mut App,
    path: std::path::PathBuf,
    initial_fingerprint: RuntimeConfigFingerprint,
    revisions: async_channel::Sender<ConfigRevision>,
) {
    cx.spawn(async move |cx| {
        let mut fingerprint = initial_fingerprint;
        let mut revision = 0_u64;
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;
            let observed = observe_runtime_config(&path);
            if observed.fingerprint == fingerprint {
                continue;
            }
            // Config writes can be a sequence of truncation and rewrite. Wait
            // for a quiet interval, then parse exactly the bytes we hash.
            cx.background_executor()
                .timer(Duration::from_millis(120))
                .await;
            let stable = observe_runtime_config(&path);
            if stable.fingerprint != observed.fingerprint {
                continue;
            }
            let content_changed = stable.fingerprint.content_hash != fingerprint.content_hash
                || stable.fingerprint.read_error != fingerprint.read_error;
            fingerprint = stable.fingerprint;
            if !content_changed {
                continue;
            }
            revision = revision.saturating_add(1);
            if revisions
                .send(ConfigRevision {
                    revision,
                    loaded: stable.loaded,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

thread_local! {
    static II_VOLUME_MIXER_TABS: RefCell<HashMap<DisplayId, VolumeMixerTab>> = RefCell::default();
}

fn remembered_volume_mixer_tab(display: DisplayId) -> VolumeMixerTab {
    II_VOLUME_MIXER_TABS.with(|tabs| tabs.borrow().get(&display).copied().unwrap_or_default())
}

fn remember_volume_mixer_tab(display: DisplayId, tab: VolumeMixerTab) {
    II_VOLUME_MIXER_TABS.with(|tabs| {
        tabs.borrow_mut().insert(display, tab);
    });
}
type OverlayMap = Rc<
    RefCell<
        HashMap<
            DisplayId,
            WindowHandle<
                crate::families::waffle::taskbar::start_menu_overlay::WaffleStartMenuOverlay,
            >,
        >,
    >,
>;
type NotificationOverlayMap = Rc<
    RefCell<
        HashMap<
            DisplayId,
            WindowHandle<crate::families::waffle::notification_overlay::WaffleNotificationOverlay>,
        >,
    >,
>;
type TaskViewOverlayMap = Rc<
    RefCell<
        HashMap<
            DisplayId,
            WindowHandle<crate::families::waffle::task_view_overlay::WaffleTaskViewOverlay>,
        >,
    >,
>;
type ActionCenterOverlayMap = Rc<
    RefCell<
        HashMap<
            DisplayId,
            WindowHandle<crate::families::waffle::action_center_overlay::WaffleActionCenterOverlay>,
        >,
    >,
>;
type ClipboardOverlayMap = Rc<
    RefCell<
        HashMap<
            DisplayId,
            WindowHandle<crate::families::waffle::clipboard_overlay::WaffleClipboardOverlay>,
        >,
    >,
>;

fn read_json(path: &std::path::Path) -> Option<serde_json::Value> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

pub fn start(cx: &mut App) {
    let surfaces: SurfaceMap = Rc::default();
    let vertical_surfaces: VerticalSurfaceMap = Rc::default();
    let backgrounds: BackgroundMap = Rc::default();
    let backdrops: BackdropMap = Rc::default();
    let waffle_backgrounds: WaffleBackgroundMap = Rc::default();
    let waffle_backdrops: WaffleBackdropMap = Rc::default();
    let waffle_output_names: WaffleOutputNameMap = Rc::default();
    let ii_panel_surfaces: IiPanelSurfaceMap = Rc::default();
    let ii_dock_surfaces: IiDockSurfaceMap = Rc::default();
    let ii_output_names: IiOutputNameMap = Rc::default();
    let ii_overview_overlays: IiOverviewOverlayMap = Rc::default();
    let ii_panel_states: IiPanelStateMap = Rc::default();
    let (media_actions, media_action_receiver) = async_channel::unbounded();
    let media_cache: MediaStateCache =
        Rc::new(RefCell::new(crate::vertical_provider::unavailable_media()));
    let audio_provider = PipeWireAudioProvider::start();
    let audio_receiver = audio_provider.events();
    let audio_panel_surfaces = ii_panel_surfaces.clone();
    cx.spawn(async move |cx| {
        let _audio_provider = audio_provider;
        while let Ok(event) = audio_receiver.recv().await {
            let (status, snapshot) = match event {
                AudioProviderEvent::Snapshot(snapshot) => (
                    if snapshot.initial_registry_complete {
                        ProviderStatus::Ready
                    } else {
                        ProviderStatus::Connecting
                    },
                    Some(snapshot),
                ),
                AudioProviderEvent::Unavailable(_) => (ProviderStatus::Unavailable, None),
            };
            crate::families::ii::panel_surface::cache_audio_snapshot(status, snapshot.clone());
            cx.update(|cx| {
                let handles = audio_panel_surfaces
                    .borrow()
                    .values()
                    .copied()
                    .collect::<Vec<_>>();
                for handle in handles {
                    let snapshot = snapshot.clone();
                    let _ = handle.update(cx, |view, _, cx| {
                        view.set_audio_snapshot(status, snapshot, cx)
                    });
                }
            });
        }
    })
    .detach();
    let media_vertical_surfaces = vertical_surfaces.clone();
    let media_panel_surfaces = ii_panel_surfaces.clone();
    let media_state_cache = media_cache.clone();
    cx.spawn(async move |cx| {
        let mut provider = crate::vertical_provider::MprisProvider::default();
        loop {
            while let Ok(action) = media_action_receiver.try_recv() {
                provider.perform_media_action(action).await;
            }
            let state = provider.sample().await;
            *media_state_cache.borrow_mut() = state.clone();
            cx.update(|cx| {
                let vertical = media_vertical_surfaces
                    .borrow()
                    .values()
                    .copied()
                    .collect::<Vec<_>>();
                for handle in vertical {
                    let state = state.clone();
                    let _ = handle.update(cx, |host, _, cx| host.set_media_state(state, cx));
                }
                let panels = media_panel_surfaces
                    .borrow()
                    .values()
                    .copied()
                    .collect::<Vec<_>>();
                for handle in panels {
                    let state = state.clone();
                    let _ = handle.update(cx, |view, _, cx| view.set_media_state(state, cx));
                }
            });
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
        }
    })
    .detach();
    let overlays: OverlayMap = Rc::default();
    let notification_overlays: NotificationOverlayMap = Rc::default();
    let task_view_overlays: TaskViewOverlayMap = Rc::default();
    let action_center_overlays: ActionCenterOverlayMap = Rc::default();
    let clipboard_overlays: ClipboardOverlayMap = Rc::default();
    let (waffle_events, waffle_receiver) = async_channel::unbounded::<StartMenuEvent>();
    let (notification_events, notification_receiver) =
        async_channel::unbounded::<NotificationCenterEvent>();
    let (task_view_events, task_view_receiver) = async_channel::unbounded::<TaskViewEvent>();
    let (action_center_events, action_center_receiver) =
        async_channel::unbounded::<ActionCenterEvent>();
    let (clipboard_events, clipboard_receiver) = async_channel::unbounded::<ClipboardEvent>();
    let waffle_source = XdgDesktopEntrySource::new().map(std::sync::Arc::new);
    let waffle_controller = Rc::new(RefCell::new(StartMenuController::default()));
    let notification_controller = Rc::new(RefCell::new(NotificationCenterController::default()));
    let task_view_controller = Rc::new(RefCell::new(TaskViewController::default()));
    let action_center_controller = Rc::new(RefCell::new(ActionCenterController::default()));
    let clipboard_controller = Rc::new(RefCell::new(ClipboardController::default()));
    let runtime_config_path = crate::config::runtime_path();
    let initial_config = observe_runtime_config(&runtime_config_path);
    let initial_fingerprint = initial_config.fingerprint;
    let source_loaded = initial_config.loaded.clone();
    report_config_load(None, &source_loaded);
    let selected_environment = match ConfiguredFamilyInputs::select(&source_loaded.config) {
        Ok(environment) => environment,
        Err(error) => {
            eprintln!("GPUI environment registry: startup selection failed: {error:?}");
            return;
        }
    };
    let mut config = source_loaded.config.clone();
    config.panel_family = Some(selected_environment.descriptor().id.as_str().to_owned());
    let mut loaded = source_loaded;
    loaded.config = config.clone();
    clipboard_controller
        .borrow_mut()
        .set_pins(config.clipboard_pinned.clone());
    let state_root = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/state"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".local/state"));
    let generated = state_root.join("user/generated");
    let palette = read_json(&generated.join("colors.json"));
    let metadata = read_json(&generated.join("theme-meta.json"));
    let theme = theme_resolver::resolve_for_config(&loaded, palette.as_ref(), metadata.as_ref());
    let critical_config: Rc<RefCell<IiCriticalSnapshot>> =
        Rc::new(RefCell::new(IiCriticalSnapshot {
            revision: 0,
            loaded: loaded.clone(),
            theme: theme.clone(),
        }));

    let overlay_receiver = waffle_receiver.clone();
    let overlay_handles = overlays.clone();
    let overlay_controller = waffle_controller.clone();
    let overlay_config = config.clone();
    let overlay_theme = theme.clone();
    let overlay_source = waffle_source.clone();
    let overlay_sender = waffle_events.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = overlay_receiver.recv().await {
            cx.update(|cx| {
                let display = match event {
                    StartMenuEvent::Toggle { display }
                    | StartMenuEvent::Close { display }
                    | StartMenuEvent::Refresh { display } => display,
                };
                let was_open = matches!(
                    overlay_controller.borrow().state(display),
                    StartMenuState::Open { .. }
                );
                let open = match event {
                    StartMenuEvent::Toggle { .. } => matches!(
                        overlay_controller.borrow_mut().toggle(display),
                        StartMenuState::Open { .. }
                    ),
                    StartMenuEvent::Close { .. } => {
                        overlay_controller.borrow_mut().close(display);
                        false
                    }
                    StartMenuEvent::Refresh { .. } => was_open,
                };
                close_waffle_overlay(cx, &overlay_handles, display);
                if open {
                    open_waffle_overlay(
                        cx,
                        &overlay_handles,
                        display,
                        &overlay_config,
                        &overlay_theme,
                        &overlay_source,
                        &overlay_sender,
                    );
                }
            });
        }
    })
    .detach();

    let task_view_handles = task_view_overlays.clone();
    let task_view_state = task_view_controller.clone();
    let task_view_config = config.clone();
    let task_view_theme = theme.clone();
    let task_view_sender = task_view_events.clone();
    let task_view_taskbars = surfaces.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = task_view_receiver.recv().await {
            cx.update(|cx| {
                let display = match event {
                    TaskViewEvent::Toggle { display }
                    | TaskViewEvent::Close { display }
                    | TaskViewEvent::SelectWorkspace { display, .. } => display,
                };
                match event {
                    TaskViewEvent::SelectWorkspace { display, number } => {
                        task_view_state
                            .borrow_mut()
                            .select_workspace(display, number);
                        if let Some(handle) = task_view_handles.borrow().get(&display).copied() {
                            let _ =
                                handle.update(cx, |view, _, cx| view.select_workspace(number, cx));
                        }
                    }
                    TaskViewEvent::Toggle { display } => {
                        let open =
                            task_view_state.borrow_mut().toggle(display) == TaskViewState::Open;
                        close_task_view_overlay(cx, &task_view_handles, display);
                        if open
                            && !open_task_view_overlay(
                                cx,
                                &task_view_handles,
                                display,
                                &task_view_config,
                                &task_view_theme,
                                &task_view_state,
                                &task_view_sender,
                            )
                        {
                            task_view_state.borrow_mut().close(display);
                        }
                    }
                    TaskViewEvent::Close { display } => {
                        task_view_state.borrow_mut().close(display);
                        close_task_view_overlay(cx, &task_view_handles, display);
                    }
                }
                if let Some(taskbar) = task_view_taskbars.borrow().get(&display).copied() {
                    let _ = taskbar.update(cx, |host, _, cx| host.refresh(cx));
                }
            });
        }
    })
    .detach();

    let action_handles = action_center_overlays.clone();
    let action_state = action_center_controller.clone();
    let action_config = config.clone();
    let action_theme = theme.clone();
    let action_sender = action_center_events.clone();
    let action_taskbars = surfaces.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = action_center_receiver.recv().await {
            cx.update(|cx| {
                let display = match event {
                    ActionCenterEvent::Toggle { display }
                    | ActionCenterEvent::Close { display }
                    | ActionCenterEvent::SelectPage { display, .. } => display,
                };
                match event {
                    ActionCenterEvent::Toggle { display } => {
                        let open =
                            action_state.borrow_mut().toggle(display) == ActionCenterState::Open;
                        close_action_center_overlay(cx, &action_handles, display);
                        if open
                            && !open_action_center_overlay(
                                cx,
                                &action_handles,
                                display,
                                &action_config,
                                &action_theme,
                                &action_state,
                                &action_sender,
                            )
                        {
                            action_state.borrow_mut().close(display);
                        }
                    }
                    ActionCenterEvent::Close { display } => {
                        action_state.borrow_mut().close(display);
                        close_action_center_overlay(cx, &action_handles, display);
                    }
                    ActionCenterEvent::SelectPage { display, page } => {
                        action_state.borrow_mut().select_page(display, page);
                        if let Some(handle) = action_handles.borrow().get(&display).copied() {
                            let _ = handle.update(cx, |_, _, cx| cx.notify());
                        }
                    }
                }
                if let Some(taskbar) = action_taskbars.borrow().get(&display).copied() {
                    let _ = taskbar.update(cx, |host, _, cx| host.refresh(cx));
                }
            });
        }
    })
    .detach();

    let clipboard_handles = clipboard_overlays.clone();
    let clipboard_state = clipboard_controller.clone();
    let clipboard_config = config.clone();
    let clipboard_theme = theme.clone();
    let clipboard_sender = clipboard_events.clone();
    let clipboard_taskbars = surfaces.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = clipboard_receiver.recv().await {
            if let ClipboardEvent::PinsChanged { pins } = event {
                clipboard_state.borrow_mut().set_pins(pins);
                continue;
            }
            let display = match event {
                ClipboardEvent::Toggle { display } | ClipboardEvent::Close { display } => display,
                ClipboardEvent::PinsChanged { .. } => continue,
            };
            cx.update(|cx| {
                match event {
                    ClipboardEvent::Toggle { .. } => {
                        let open = clipboard_state.borrow_mut().toggle(display)
                            == ClipboardOverlayState::Open;
                        close_clipboard_overlay(cx, &clipboard_handles, display);
                        if open {
                            let pins = clipboard_state.borrow().pins().to_vec();
                            if !open_clipboard_overlay(
                                cx,
                                &clipboard_handles,
                                display,
                                &clipboard_config,
                                &clipboard_theme,
                                &clipboard_state,
                                &clipboard_sender,
                                &pins,
                            ) {
                                clipboard_state.borrow_mut().close(display);
                            }
                        }
                    }
                    ClipboardEvent::Close { .. } => {
                        clipboard_state.borrow_mut().close(display);
                        close_clipboard_overlay(cx, &clipboard_handles, display);
                    }
                    ClipboardEvent::PinsChanged { .. } => {}
                }
                if let Some(taskbar) = clipboard_taskbars.borrow().get(&display).copied() {
                    let _ = taskbar.update(cx, |host, _, cx| host.refresh(cx));
                }
            });
        }
    })
    .detach();

    let notification_handles = notification_overlays.clone();
    let notification_state = notification_controller.clone();
    let notification_config = config.clone();
    let notification_theme = theme.clone();
    let notification_receiver = notification_receiver.clone();
    let notification_sender = notification_events.clone();
    let notification_taskbars = surfaces.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = notification_receiver.recv().await {
            cx.update(|cx| {
                let display = match event {
                    NotificationCenterEvent::Toggle { display }
                    | NotificationCenterEvent::Close { display } => display,
                };
                let open = match event {
                    NotificationCenterEvent::Toggle { .. } => {
                        notification_state.borrow_mut().toggle(display)
                            == NotificationCenterState::Open
                    }
                    NotificationCenterEvent::Close { .. } => {
                        notification_state.borrow_mut().close(display);
                        false
                    }
                };
                close_notification_overlay(cx, &notification_handles, display);
                if open {
                    open_notification_overlay(
                        cx,
                        &notification_handles,
                        display,
                        &notification_config,
                        &notification_theme,
                        &notification_sender,
                    );
                }
                if let Some(taskbar) = notification_taskbars.borrow().get(&display).copied() {
                    let _ = taskbar.update(cx, |host, _, cx| host.refresh(cx));
                }
            });
        }
    })
    .detach();

    let (ii_panel_events, ii_panel_receiver) = async_channel::unbounded::<IiPanelEvent>();
    let (config_revisions, config_revision_receiver) = async_channel::unbounded();
    watch_runtime_config(
        cx,
        runtime_config_path,
        initial_fingerprint,
        config_revisions,
    );
    let revision_state = critical_config.clone();
    let revision_surfaces = surfaces.clone();
    let revision_vertical_surfaces = vertical_surfaces.clone();
    let revision_backgrounds = backgrounds.clone();
    let revision_backdrops = backdrops.clone();
    let revision_waffle_backgrounds = waffle_backgrounds.clone();
    let revision_waffle_backdrops = waffle_backdrops.clone();
    let revision_waffle_output_names = waffle_output_names.clone();
    let revision_docks = ii_dock_surfaces.clone();
    let revision_output_names = ii_output_names.clone();
    let revision_panel_events = ii_panel_events.clone();
    let revision_media_actions = media_actions.clone();
    let revision_media_cache = media_cache.clone();
    let revision_generated = generated.clone();
    let revision_ii_panel_surfaces = ii_panel_surfaces.clone();
    let revision_ii_panel_states = ii_panel_states.clone();
    let revision_ii_overviews = ii_overview_overlays.clone();
    cx.spawn(async move |cx| {
        while let Ok(update) = config_revision_receiver.recv().await {
            cx.update(|cx| {
                let previous = revision_state.borrow().clone();
                if update.revision <= previous.revision {
                    return;
                }
                report_config_load(Some(update.revision), &update.loaded);
                let selected_environment = match ConfiguredFamilyInputs::select(&update.loaded.config)
                {
                    Ok(environment) => environment,
                    Err(error) => {
                        eprintln!(
                            "GPUI environment registry revision {}: selection failed; retaining previous runtime snapshot: {error:?}",
                            update.revision
                        );
                        return;
                    }
                };
                let mut runtime_loaded = update.loaded.clone();
                runtime_loaded.config.panel_family =
                    Some(selected_environment.descriptor().id.as_str().to_owned());
                let palette = read_json(&revision_generated.join("colors.json"));
                let metadata = read_json(&revision_generated.join("theme-meta.json"));
                let theme = theme_resolver::resolve_for_config(
                    &runtime_loaded,
                    palette.as_ref(),
                    metadata.as_ref(),
                );
                let ii_family_update = previous.loaded.config.panel_family.as_deref() == Some("ii")
                    && runtime_loaded.config.panel_family.as_deref() == Some("ii");
                let exited_ii_family = previous.loaded.config.panel_family.as_deref() == Some("ii")
                    && runtime_loaded.config.panel_family.as_deref() != Some("ii");
                let waffle_family_update = previous.loaded.config.panel_family.as_deref()
                    == Some("waffle")
                    && runtime_loaded.config.panel_family.as_deref() == Some("waffle");
                let exited_waffle_family = previous.loaded.config.panel_family.as_deref()
                    == Some("waffle")
                    && runtime_loaded.config.panel_family.as_deref() != Some("waffle");
                let waffle_changed = waffle_family_update
                    && waffle_background_config_changed(&previous.loaded.config, &runtime_loaded.config);
                let changed = ii_family_update
                    && ii_critical_config_changed(&previous.loaded, &runtime_loaded);
                *revision_state.borrow_mut() = IiCriticalSnapshot {
                    revision: update.revision,
                    loaded: runtime_loaded,
                    theme: theme.clone(),
                };
                if exited_ii_family {
                    eprintln!(
                        "GPUI config revision {}: closed ii surfaces after panelFamily changed; restart to activate the target family",
                        update.revision
                    );
                    close_ii_critical_surfaces(
                        cx,
                        &revision_surfaces,
                        &revision_vertical_surfaces,
                        &revision_backgrounds,
                        &revision_backdrops,
                        &revision_docks,
                        &revision_ii_panel_surfaces,
                        &revision_ii_panel_states,
                        &revision_ii_overviews,
                        &revision_output_names,
                    );
                } else if exited_waffle_family {
                    close_waffle_background_surfaces(
                        cx,
                        &revision_waffle_backgrounds,
                        &revision_waffle_backdrops,
                        &revision_waffle_output_names,
                    );
                } else if changed {
                    refresh_niri_output_snapshot();
                    let snapshot = revision_state.borrow().clone();
                    for display in cx.displays() {
                        reconcile_ii_critical_for_display(
                            cx,
                            &revision_surfaces,
                            &revision_vertical_surfaces,
                            &revision_backgrounds,
                            &revision_backdrops,
                            &revision_docks,
                            &revision_output_names,
                            display.id(),
                            &snapshot,
                            &revision_panel_events,
                            &revision_media_actions,
                            &revision_media_cache,
                            true,
                        );
                        reconcile_ii_overview_for_display(
                            cx,
                            &revision_ii_overviews,
                            &revision_surfaces,
                            &revision_vertical_surfaces,
                            display.id(),
                            &snapshot.loaded.config,
                            &snapshot.theme,
                            &revision_panel_events,
                            true,
                        );
                        reconcile_ii_panel_for_display(
                            cx,
                            &revision_ii_panel_surfaces,
                            &revision_ii_panel_states,
                            &revision_vertical_surfaces,
                            &revision_surfaces,
                            display.id(),
                            &snapshot.loaded.config,
                            &snapshot.theme,
                            &revision_panel_events,
                            &revision_media_actions,
                            &revision_media_cache,
                            true,
                        );
                    }
                } else if waffle_changed {
                    refresh_niri_output_snapshot();
                    let snapshot = revision_state.borrow().clone();
                    for display in cx.displays() {
                        reconcile_waffle_background_for_display(
                            cx,
                            &revision_waffle_backgrounds,
                            &revision_waffle_backdrops,
                            &revision_waffle_output_names,
                            display.id(),
                            &snapshot.loaded.config,
                            &snapshot.theme,
                            true,
                        );
                    }
                }
            });
        }
    })
    .detach();
    let ii_panel_surfaces_for_events = ii_panel_surfaces.clone();
    let ii_panel_states_for_events = ii_panel_states.clone();
    let ii_vertical_bars_for_events = vertical_surfaces.clone();
    let ii_horizontal_bars_for_events = surfaces.clone();
    let ii_runtime_state = critical_config.clone();
    let ii_panel_sender = ii_panel_events.clone();
    let ii_media_actions = media_actions.clone();
    let ii_media_cache = media_cache.clone();
    let overview_handles = ii_overview_overlays.clone();
    let overview_bars = surfaces.clone();
    let overview_vertical_bars = vertical_surfaces.clone();
    let overview_sender = ii_panel_events.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = ii_panel_receiver.recv().await {
            cx.update(|cx| {
                let current = ii_runtime_state.borrow().clone();
                if current.loaded.config.panel_family.as_deref() != Some("ii") {
                    return;
                }
                match event {
                    IiPanelEvent::ToggleOverview { display } => {
                        if overview_handles.borrow().contains_key(&display) {
                            close_ii_overview(
                                cx,
                                &overview_handles,
                                &overview_bars,
                                &overview_vertical_bars,
                                display,
                            );
                        } else if current
                            .loaded
                            .config
                            .enabled_panels
                            .iter()
                            .any(|name| name == "iiOverview")
                        {
                            open_ii_overview(
                                cx,
                                &overview_handles,
                                display,
                                &current.theme,
                                &overview_sender,
                            );
                        }
                        return;
                    }
                    IiPanelEvent::CloseOverview { display } => {
                        close_ii_overview(
                            cx,
                            &overview_handles,
                            &overview_bars,
                            &overview_vertical_bars,
                            display,
                        );
                        return;
                    }
                    IiPanelEvent::SetOverviewMode { display, mode } => {
                        if let Some(handle) = overview_handles.borrow().get(&display).copied() {
                            let _ = handle.update(cx, |view, _, cx| view.set_mode(mode, cx));
                        }
                        return;
                    }
                    IiPanelEvent::SetOverviewQuery { display, query } => {
                        if let Some(handle) = overview_handles.borrow().get(&display).copied() {
                            let _ = handle.update(cx, |view, _, cx| view.set_query(query, cx));
                        }
                        return;
                    }
                    IiPanelEvent::LaunchOverviewApplication {
                        display,
                        application_id,
                    } => {
                        if let Some(handle) = overview_handles.borrow().get(&display).copied() {
                            let _ = handle.update(cx, |view, _, cx| {
                                view.launch_application(application_id, cx)
                            });
                        }
                        return;
                    }
                    _ => {}
                }
                apply_ii_panel_event(
                    cx,
                    &ii_panel_surfaces_for_events,
                    &ii_panel_states_for_events,
                    &ii_vertical_bars_for_events,
                    &ii_horizontal_bars_for_events,
                    event,
                    &current.loaded.config,
                    &current.theme,
                    &ii_panel_sender,
                    &ii_media_actions,
                    &ii_media_cache,
                );
            });
        }
    })
    .detach();

    let Some(family) = config.panel_family.as_deref() else {
        eprintln!("GPUI family host: panelFamily is missing; no surface will be opened");
        return;
    };
    let vertical = config.bar.vertical.unwrap_or(false);
    let required_panel = match family {
        "ii" if vertical => "iiVerticalBar",
        "ii" => "iiBar",
        // These are the family-owned critical bar gates in their QML roots.
        "waffle" => "wBar",
        "iris" => "irisBar",
        other => {
            eprintln!("GPUI family host: unsupported panelFamily '{other}'");
            return;
        }
    };
    if !config
        .enabled_panels
        .iter()
        .any(|panel| panel == required_panel)
    {
        if family != "ii" {
            eprintln!(
                "GPUI family host: '{required_panel}' is disabled; no family surface will be opened"
            );
            return;
        }
    }
    let mut iris_options = config.appearance.source_sections.get("iris.bar").cloned();
    let orbit_keys = config
        .appearance
        .source_sections
        .get("iris.orbit")
        .and_then(|options| options.get("keys"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("niri")
        .to_owned();
    iris_options
        .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .expect("Iris bar options are a JSON object")
        .insert("orbitKeys".into(), serde_json::Value::String(orbit_keys));
    let iris_dock_options = config.appearance.source_sections.get("iris.dock").cloned();
    let iris_dock_enabled = config
        .enabled_panels
        .iter()
        .any(|panel| panel == "irisDock");
    if family == "iris" && !IrisRuntime::can_resolve(&theme, iris_options.as_ref()) {
        eprintln!(
            "GPUI family host: Iris theme/preset cannot be resolved; no surface will be opened"
        );
        return;
    }

    if family == "ii" {
        refresh_niri_output_snapshot();
    }
    for display in cx.displays() {
        if family == "ii" {
            let snapshot = critical_config.borrow().clone();
            reconcile_ii_critical_for_display(
                cx,
                &surfaces,
                &vertical_surfaces,
                &backgrounds,
                &backdrops,
                &ii_dock_surfaces,
                &ii_output_names,
                display.id(),
                &snapshot,
                &ii_panel_events,
                &media_actions,
                &media_cache,
                true,
            );
            continue;
        }
        if family == "waffle" {
            reconcile_waffle_background_for_display(
                cx,
                &waffle_backgrounds,
                &waffle_backdrops,
                &waffle_output_names,
                display.id(),
                &config,
                &theme,
                false,
            );
            if waffle_display_enabled(cx, display.id(), &config.waffle.bar.screen_list) {
                open_waffle_for_display(
                    cx,
                    &surfaces,
                    display.id(),
                    &config,
                    &theme,
                    &waffle_events,
                    &waffle_source,
                    &notification_events,
                    &notification_controller,
                    &task_view_events,
                    &task_view_controller,
                    &action_center_events,
                    &action_center_controller,
                    &clipboard_events,
                    &clipboard_controller,
                );
            }
        } else if family == "iris" {
            if iris_display_enabled(cx, display.id(), &iris_options) {
                open_iris_for_display(
                    cx,
                    &surfaces,
                    display.id(),
                    &config,
                    &theme,
                    iris_options.clone(),
                    iris_dock_options.clone(),
                    iris_dock_enabled,
                );
            }
        } else if vertical {
            open_vertical_for_display(
                cx,
                &vertical_surfaces,
                display.id(),
                &config,
                &theme,
                &ii_panel_events,
                &media_actions,
                &media_cache,
            );
        } else {
            open_for_display(
                cx,
                &surfaces,
                display.id(),
                &config,
                &theme,
                &ii_panel_events,
            );
        }
        if family == "ii"
            && config
                .enabled_panels
                .iter()
                .any(|panel| panel == "iiBackground")
        {
            open_background_for_display(cx, &backgrounds, display.id(), &config, &theme);
        }
        if family == "ii" {
            open_backdrop_for_display(cx, &backdrops, display.id(), &config, &theme);
        }
        if family == "ii" {
            open_ii_dock_for_display(cx, &ii_dock_surfaces, display.id(), &config, &theme);
        }
    }

    let (niri_output_refreshed, niri_output_updates) = async_channel::unbounded();
    let startup_output_refresh = niri_output_refreshed.clone();
    let _ = NIRI_OUTPUT_REFRESH_SENDER.set(niri_output_refreshed);
    let output_query_already_finished = NIRI_OUTPUT_SNAPSHOT
        .get_or_init(|| RwLock::new(NiriOutputSnapshotState::default()))
        .read()
        .ok()
        .is_some_and(|snapshot| {
            snapshot.last_success.is_some()
                || (snapshot.last_attempt.is_some()
                    && !NIRI_OUTPUT_REFRESH_IN_FLIGHT.load(Ordering::Acquire))
        });
    if output_query_already_finished {
        let _ = startup_output_refresh.try_send(());
    }
    refresh_niri_output_snapshot();
    let output_state = critical_config.clone();
    let output_horizontal = surfaces.clone();
    let output_vertical = vertical_surfaces.clone();
    let output_backgrounds = backgrounds.clone();
    let output_backdrops = backdrops.clone();
    let output_waffle_backgrounds = waffle_backgrounds.clone();
    let output_waffle_backdrops = waffle_backdrops.clone();
    let output_waffle_names = waffle_output_names.clone();
    let output_docks = ii_dock_surfaces.clone();
    let output_names = ii_output_names.clone();
    let output_panels = ii_panel_surfaces.clone();
    let output_panel_states = ii_panel_states.clone();
    let output_panel_events = ii_panel_events.clone();
    let output_media_actions = media_actions.clone();
    let output_media_cache = media_cache.clone();
    cx.spawn(async move |cx| {
        while niri_output_updates.recv().await.is_ok() {
            cx.update(|cx| {
                let snapshot = output_state.borrow().clone();
                if snapshot.loaded.config.panel_family.as_deref() == Some("ii") {
                    for display in cx.displays() {
                        reconcile_ii_critical_for_display(
                            cx,
                            &output_horizontal,
                            &output_vertical,
                            &output_backgrounds,
                            &output_backdrops,
                            &output_docks,
                            &output_names,
                            display.id(),
                            &snapshot,
                            &output_panel_events,
                            &output_media_actions,
                            &output_media_cache,
                            false,
                        );
                        reconcile_ii_panel_for_display(
                            cx,
                            &output_panels,
                            &output_panel_states,
                            &output_vertical,
                            &output_horizontal,
                            display.id(),
                            &snapshot.loaded.config,
                            &snapshot.theme,
                            &output_panel_events,
                            &output_media_actions,
                            &output_media_cache,
                            false,
                        );
                    }
                } else if snapshot.loaded.config.panel_family.as_deref() == Some("waffle") {
                    for display in cx.displays() {
                        reconcile_waffle_background_for_display(
                            cx,
                            &output_waffle_backgrounds,
                            &output_waffle_backdrops,
                            &output_waffle_names,
                            display.id(),
                            &snapshot.loaded.config,
                            &snapshot.theme,
                            false,
                        );
                    }
                }
            });
        }
    })
    .detach();

    // GPUI display events do not expose Niri's connector-name changes when
    // geometry stays the same. Refresh the bounded Niri snapshot periodically
    // for Waffle so per-output wallpaper routing can notice output renames.
    if family == "waffle" {
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                cx.update(|_| refresh_niri_output_snapshot());
            }
        })
        .detach();
    }

    let observed_surfaces = surfaces.clone();
    let observed_vertical_surfaces = vertical_surfaces.clone();
    let observed_backgrounds = backgrounds.clone();
    let observed_backdrops = backdrops.clone();
    let observed_waffle_backgrounds = waffle_backgrounds.clone();
    let observed_waffle_backdrops = waffle_backdrops.clone();
    let observed_waffle_names = waffle_output_names.clone();
    let observed_ii_panel_surfaces = ii_panel_surfaces.clone();
    let observed_ii_dock_surfaces = ii_dock_surfaces.clone();
    let observed_ii_output_names = ii_output_names.clone();
    let observed_ii_panel_states = ii_panel_states.clone();
    let observed_config = config.clone();
    let observed_ii_critical = critical_config.clone();
    let observed_theme = theme.clone();
    let observed_iris_options = iris_options.clone();
    let observed_iris_dock_options = iris_dock_options.clone();
    let observed_iris_dock_enabled = iris_dock_enabled;
    let observed_waffle_events = waffle_events.clone();
    let observed_waffle_source = waffle_source.clone();
    let observed_notification_overlays = notification_overlays.clone();
    let observed_notification_controller = notification_controller.clone();
    let observed_notification_events = notification_events.clone();
    let observed_task_view_overlays = task_view_overlays.clone();
    let observed_task_view_controller = task_view_controller.clone();
    let observed_task_view_events = task_view_events.clone();
    let observed_action_center_overlays = action_center_overlays.clone();
    let observed_action_center_controller = action_center_controller.clone();
    let observed_action_center_events = action_center_events.clone();
    let observed_clipboard_overlays = clipboard_overlays.clone();
    let observed_clipboard_controller = clipboard_controller.clone();
    let observed_clipboard_events = clipboard_events.clone();
    let observed_ii_panel_events = ii_panel_events.clone();
    let observed_media_actions = media_actions.clone();
    let observed_media_cache = media_cache.clone();
    let observed_family = family.to_owned();
    cx.on_window_closed({
        let surfaces = surfaces.clone();
        let vertical_surfaces = vertical_surfaces.clone();
        let backgrounds = backgrounds.clone();
        let backdrops = backdrops.clone();
        let waffle_backgrounds = waffle_backgrounds.clone();
        let waffle_backdrops = waffle_backdrops.clone();
        let waffle_output_names = waffle_output_names.clone();
        let overlays = overlays.clone();
        let notification_overlays = notification_overlays.clone();
        let task_view_overlays = task_view_overlays.clone();
        let action_center_overlays = action_center_overlays.clone();
        let clipboard_overlays = clipboard_overlays.clone();
        let ii_panel_surfaces = ii_panel_surfaces.clone();
        let ii_dock_surfaces = ii_dock_surfaces.clone();
        let ii_panel_states = ii_panel_states.clone();
        let ii_overview_overlays = ii_overview_overlays.clone();
        let controller = waffle_controller.clone();
        let notification_controller = notification_controller.clone();
        let task_view_controller = task_view_controller.clone();
        let action_center_controller = action_center_controller.clone();
        let clipboard_controller = clipboard_controller.clone();
        move |cx, window_id| {
            surfaces
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
            vertical_surfaces
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
            ii_dock_surfaces
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
            backgrounds
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
            backdrops
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
            waffle_backgrounds
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
            waffle_backdrops
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
            let removed_waffle_outputs = waffle_output_names
                .borrow()
                .keys()
                .copied()
                .filter(|display| {
                    waffle_backgrounds
                        .borrow()
                        .get(display)
                        .is_none_or(|handle| handle.window_id() != window_id)
                        && waffle_backdrops
                            .borrow()
                            .get(display)
                            .is_none_or(|handle| handle.window_id() != window_id)
                })
                .collect::<Vec<_>>();
            for display in removed_waffle_outputs {
                if !waffle_backgrounds.borrow().contains_key(&display)
                    && !waffle_backdrops.borrow().contains_key(&display)
                {
                    waffle_output_names.borrow_mut().remove(&display);
                }
            }
            let removed_ii_outputs = ii_panel_surfaces
                .borrow()
                .iter()
                .filter_map(|(display, handle)| {
                    (handle.window_id() == window_id).then_some(*display)
                })
                .collect::<Vec<_>>();
            for display in removed_ii_outputs {
                ii_panel_surfaces.borrow_mut().remove(&display);
                ii_panel_states.borrow_mut().remove(&display);
            }
            let removed_outputs = overlays
                .borrow()
                .iter()
                .filter_map(|(display, handle)| {
                    (handle.window_id() == window_id).then_some(*display)
                })
                .collect::<Vec<_>>();
            for display in removed_outputs {
                overlays.borrow_mut().remove(&display);
                controller.borrow_mut().close(display);
            }
            let removed_notification_outputs = notification_overlays
                .borrow()
                .iter()
                .filter_map(|(display, handle)| {
                    (handle.window_id() == window_id).then_some(*display)
                })
                .collect::<Vec<_>>();
            for display in removed_notification_outputs {
                notification_overlays.borrow_mut().remove(&display);
                notification_controller.borrow_mut().close(display);
            }
            let removed_task_view_outputs = task_view_overlays
                .borrow()
                .iter()
                .filter_map(|(display, handle)| {
                    (handle.window_id() == window_id).then_some(*display)
                })
                .collect::<Vec<_>>();
            for display in removed_task_view_outputs {
                task_view_overlays.borrow_mut().remove(&display);
                task_view_controller.borrow_mut().close(display);
            }
            let removed_action_center_outputs = action_center_overlays
                .borrow()
                .iter()
                .filter_map(|(display, handle)| {
                    (handle.window_id() == window_id).then_some(*display)
                })
                .collect::<Vec<_>>();
            for display in removed_action_center_outputs {
                action_center_overlays.borrow_mut().remove(&display);
                action_center_controller.borrow_mut().close(display);
            }
            let removed_clipboard_outputs = clipboard_overlays
                .borrow()
                .iter()
                .filter_map(|(display, handle)| {
                    (handle.window_id() == window_id).then_some(*display)
                })
                .collect::<Vec<_>>();
            for display in removed_clipboard_outputs {
                clipboard_overlays.borrow_mut().remove(&display);
                clipboard_controller.borrow_mut().close(display);
            }
            let removed_ii_overviews = ii_overview_overlays
                .borrow()
                .iter()
                .filter_map(|(display, handle)| {
                    (handle.window_id() == window_id).then_some(*display)
                })
                .collect::<Vec<_>>();
            for display in removed_ii_overviews {
                ii_overview_overlays.borrow_mut().remove(&display);
                restore_ii_overview_focus(cx, &surfaces, &vertical_surfaces, display);
            }
        }
    })
    .detach();

    cx.observe_displays(move |event, cx| {
        let manage_ii = observed_family == "ii"
            && observed_ii_critical
                .borrow()
                .loaded
                .config
                .panel_family
                .as_deref()
                == Some("ii");
        match event {
            DisplayEvent::Added(display_id) => {
                if manage_ii || observed_family == "waffle" {
                    refresh_niri_output_snapshot();
                }
                if manage_ii {
                    let snapshot = observed_ii_critical.borrow().clone();
                    reconcile_ii_critical_for_display(
                        cx,
                        &observed_surfaces,
                        &observed_vertical_surfaces,
                        &observed_backgrounds,
                        &observed_backdrops,
                        &observed_ii_dock_surfaces,
                        &observed_ii_output_names,
                        display_id,
                        &snapshot,
                        &observed_ii_panel_events,
                        &observed_media_actions,
                        &observed_media_cache,
                        false,
                    );
                } else if observed_family == "waffle" {
                    let snapshot = observed_ii_critical.borrow().clone();
                    reconcile_waffle_background_for_display(
                        cx,
                        &observed_waffle_backgrounds,
                        &observed_waffle_backdrops,
                        &observed_waffle_names,
                        display_id,
                        &snapshot.loaded.config,
                        &snapshot.theme,
                        false,
                    );
                    if waffle_display_enabled(
                        cx,
                        display_id,
                        &observed_config.waffle.bar.screen_list,
                    ) {
                        open_waffle_for_display(
                            cx,
                            &observed_surfaces,
                            display_id,
                            &observed_config,
                            &observed_theme,
                            &observed_waffle_events,
                            &observed_waffle_source,
                            &observed_notification_events,
                            &observed_notification_controller,
                            &observed_task_view_events,
                            &observed_task_view_controller,
                            &observed_action_center_events,
                            &observed_action_center_controller,
                            &observed_clipboard_events,
                            &observed_clipboard_controller,
                        );
                    }
                } else if observed_family == "iris" {
                    reconcile_iris_displays(
                        cx,
                        &observed_surfaces,
                        &observed_config,
                        &observed_theme,
                        &observed_iris_options,
                        &observed_iris_dock_options,
                        observed_iris_dock_enabled,
                    );
                } else if observed_config.bar.vertical.unwrap_or(false) {
                    open_vertical_for_display(
                        cx,
                        &observed_vertical_surfaces,
                        display_id,
                        &observed_config,
                        &observed_theme,
                        &observed_ii_panel_events,
                        &observed_media_actions,
                        &media_cache,
                    );
                } else {
                    open_for_display(
                        cx,
                        &observed_surfaces,
                        display_id,
                        &observed_config,
                        &observed_theme,
                        &observed_ii_panel_events,
                    );
                }
            }
            DisplayEvent::Removed(display_id) => {
                close_ii_overview(
                    cx,
                    &ii_overview_overlays,
                    &observed_surfaces,
                    &observed_vertical_surfaces,
                    display_id,
                );
                close_waffle_overlay(cx, &overlays, display_id);
                waffle_controller.borrow_mut().remove(display_id);
                close_notification_overlay(cx, &observed_notification_overlays, display_id);
                observed_notification_controller
                    .borrow_mut()
                    .remove(display_id);
                close_task_view_overlay(cx, &observed_task_view_overlays, display_id);
                observed_task_view_controller
                    .borrow_mut()
                    .remove(display_id);
                close_action_center_overlay(cx, &observed_action_center_overlays, display_id);
                observed_action_center_controller
                    .borrow_mut()
                    .remove(display_id);
                close_clipboard_overlay(cx, &observed_clipboard_overlays, display_id);
                observed_clipboard_controller
                    .borrow_mut()
                    .remove(display_id);
                close_for_display(cx, &observed_surfaces, display_id);
                close_vertical_for_display(cx, &observed_vertical_surfaces, display_id);
                close_background_for_display(cx, &observed_backgrounds, display_id);
                close_backdrop_for_display(cx, &observed_backdrops, display_id);
                close_waffle_background_for_display(
                    cx,
                    &observed_waffle_backgrounds,
                    &observed_waffle_backdrops,
                    display_id,
                );
                observed_waffle_names.borrow_mut().remove(&display_id);
                close_ii_dock_for_display(cx, &observed_ii_dock_surfaces, display_id);
                observed_ii_output_names.borrow_mut().remove(&display_id);
                close_ii_panel_for_display(
                    cx,
                    &observed_ii_panel_surfaces,
                    &observed_ii_panel_states,
                    display_id,
                );
                if observed_family == "iris" {
                    reconcile_iris_displays(
                        cx,
                        &observed_surfaces,
                        &observed_config,
                        &observed_theme,
                        &observed_iris_options,
                        &observed_iris_dock_options,
                        observed_iris_dock_enabled,
                    );
                }
            }
            DisplayEvent::Changed { id, changes } => {
                if observed_family == "waffle" && changes.contains(DisplayChanges::BOUNDS) {
                    refresh_niri_output_snapshot();
                    let snapshot = observed_ii_critical.borrow().clone();
                    reconcile_waffle_background_for_display(
                        cx,
                        &observed_waffle_backgrounds,
                        &observed_waffle_backdrops,
                        &observed_waffle_names,
                        id,
                        &snapshot.loaded.config,
                        &snapshot.theme,
                        true,
                    );
                }
                if manage_ii {
                    refresh_niri_output_snapshot();
                    let snapshot = observed_ii_critical.borrow().clone();
                    reconcile_ii_critical_for_display(
                        cx,
                        &observed_surfaces,
                        &observed_vertical_surfaces,
                        &observed_backgrounds,
                        &observed_backdrops,
                        &observed_ii_dock_surfaces,
                        &observed_ii_output_names,
                        id,
                        &snapshot,
                        &observed_ii_panel_events,
                        &observed_media_actions,
                        &observed_media_cache,
                        false,
                    );
                }
                if changes.contains(DisplayChanges::BOUNDS) {
                    if ii_overview_overlays.borrow().contains_key(&id) {
                        close_ii_overview(
                            cx,
                            &ii_overview_overlays,
                            &observed_surfaces,
                            &observed_vertical_surfaces,
                            id,
                        );
                        if manage_ii {
                            open_ii_overview(
                                cx,
                                &ii_overview_overlays,
                                id,
                                &observed_theme,
                                &observed_ii_panel_events,
                            );
                        }
                    }
                    if observed_family == "waffle" {
                        if waffle_display_enabled(cx, id, &observed_config.waffle.bar.screen_list) {
                            resize_waffle_for_display(
                                cx,
                                &observed_surfaces,
                                id,
                                &observed_config,
                                &observed_theme,
                                &observed_waffle_events,
                                &observed_waffle_source,
                                &observed_notification_events,
                                &observed_notification_controller,
                                &observed_task_view_events,
                                &observed_task_view_controller,
                                &observed_action_center_events,
                                &observed_action_center_controller,
                                &observed_clipboard_events,
                                &observed_clipboard_controller,
                            );
                            if observed_notification_controller.borrow().state(id)
                                == NotificationCenterState::Open
                            {
                                close_notification_overlay(cx, &observed_notification_overlays, id);
                                open_notification_overlay(
                                    cx,
                                    &observed_notification_overlays,
                                    id,
                                    &observed_config,
                                    &observed_theme,
                                    &observed_notification_events,
                                );
                            }
                            if observed_task_view_controller.borrow().state(id)
                                == TaskViewState::Open
                            {
                                close_task_view_overlay(cx, &observed_task_view_overlays, id);
                                if !open_task_view_overlay(
                                    cx,
                                    &observed_task_view_overlays,
                                    id,
                                    &observed_config,
                                    &observed_theme,
                                    &observed_task_view_controller,
                                    &observed_task_view_events,
                                ) {
                                    observed_task_view_controller.borrow_mut().close(id);
                                }
                            }
                            if observed_action_center_controller.borrow().state(id)
                                == ActionCenterState::Open
                            {
                                close_action_center_overlay(
                                    cx,
                                    &observed_action_center_overlays,
                                    id,
                                );
                                if !open_action_center_overlay(
                                    cx,
                                    &observed_action_center_overlays,
                                    id,
                                    &observed_config,
                                    &observed_theme,
                                    &observed_action_center_controller,
                                    &observed_action_center_events,
                                ) {
                                    observed_action_center_controller.borrow_mut().close(id);
                                }
                            }
                            if observed_clipboard_controller.borrow().state(id)
                                == ClipboardOverlayState::Open
                            {
                                close_clipboard_overlay(cx, &observed_clipboard_overlays, id);
                                if !open_clipboard_overlay(
                                    cx,
                                    &observed_clipboard_overlays,
                                    id,
                                    &observed_config,
                                    &observed_theme,
                                    &observed_clipboard_controller,
                                    &observed_clipboard_events,
                                    &observed_clipboard_controller.borrow().pins().to_vec(),
                                ) {
                                    observed_clipboard_controller.borrow_mut().close(id);
                                }
                            }
                        } else {
                            close_for_display(cx, &observed_surfaces, id);
                            close_notification_overlay(cx, &observed_notification_overlays, id);
                            observed_notification_controller.borrow_mut().close(id);
                            close_task_view_overlay(cx, &observed_task_view_overlays, id);
                            observed_task_view_controller.borrow_mut().close(id);
                            close_action_center_overlay(cx, &observed_action_center_overlays, id);
                            observed_action_center_controller.borrow_mut().close(id);
                            close_clipboard_overlay(cx, &observed_clipboard_overlays, id);
                            observed_clipboard_controller.borrow_mut().close(id);
                        }
                    } else if observed_family == "iris" {
                        if iris_display_enabled(cx, id, &observed_iris_options) {
                            resize_iris_for_display(
                                cx,
                                &observed_surfaces,
                                id,
                                &observed_config,
                                &observed_theme,
                                observed_iris_options.clone(),
                                observed_iris_dock_options.clone(),
                                observed_iris_dock_enabled,
                            );
                        } else {
                            close_for_display(cx, &observed_surfaces, id);
                        }
                    } else if observed_family != "ii"
                        && observed_config.bar.vertical.unwrap_or(false)
                    {
                        resize_vertical_for_display(
                            cx,
                            &observed_vertical_surfaces,
                            id,
                            &observed_config,
                            &observed_theme,
                            &observed_ii_panel_events,
                            &observed_media_actions,
                            &media_cache,
                        );
                    } else if observed_family != "ii" {
                        resize_for_display(
                            cx,
                            &observed_surfaces,
                            id,
                            &observed_config,
                            &observed_theme,
                            &observed_ii_panel_events,
                        );
                    }
                    if manage_ii {
                        reconcile_ii_panel_for_display(
                            cx,
                            &observed_ii_panel_surfaces,
                            &observed_ii_panel_states,
                            &observed_vertical_surfaces,
                            &observed_surfaces,
                            id,
                            &observed_config,
                            &observed_theme,
                            &observed_ii_panel_events,
                            &observed_media_actions,
                            &media_cache,
                            false,
                        );
                    }
                }
            }
            _ => {}
        }
    })
    .detach();
}

/// Match the Waffle QML `screenList` behavior: empty lists select every
/// output, a live match restricts the bar to matching outputs, and a stale
/// list falls back to all outputs so the bar cannot disappear permanently.
fn waffle_display_enabled(cx: &App, display_id: DisplayId, screen_list: &[String]) -> bool {
    if screen_list.is_empty() {
        return true;
    }
    let displays = cx.displays();
    let named_outputs: Vec<_> = displays
        .iter()
        .filter_map(|display| niri_output_name(display.bounds()).map(|name| (display.id(), name)))
        .collect();
    if !named_outputs
        .iter()
        .any(|(_, name)| screen_list.iter().any(|selected| selected == name))
    {
        return true;
    }
    named_outputs
        .iter()
        .any(|(id, name)| *id == display_id && screen_list.iter().any(|selected| selected == name))
}

/// Reconcile Waffle's wallpaper windows independently from the Material ii
/// surfaces. Background panels are output-wide in QML and do not follow the bar
/// `screenList` filter.
fn reconcile_waffle_background_for_display(
    cx: &mut App,
    backgrounds: &WaffleBackgroundMap,
    backdrops: &WaffleBackdropMap,
    output_names: &WaffleOutputNameMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    replace: bool,
) {
    let output_name = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
        .and_then(|display| niri_output_name(display.bounds()));
    let previous_name = output_names
        .borrow_mut()
        .insert(display_id, output_name.clone());
    let output_name_changed = previous_name.is_some_and(|previous| previous != output_name);
    let replace = replace || output_name_changed;

    if config.panel_family.as_deref() != Some("waffle") {
        close_waffle_background_for_display(cx, backgrounds, backdrops, display_id);
        output_names.borrow_mut().remove(&display_id);
        return;
    }

    let waffle = &config.waffle.background;
    let backdrop_only = config
        .enabled_panels
        .iter()
        .any(|panel| panel == "wBackdrop")
        && waffle.backdrop.enabled.unwrap_or(true)
        && waffle.backdrop.hide_wallpaper.unwrap_or(false);
    let main_enabled = config
        .enabled_panels
        .iter()
        .any(|panel| panel == "wBackground")
        && !backdrop_only;
    if !main_enabled || replace {
        close_waffle_background_for_display_only(cx, backgrounds, display_id);
    }
    if main_enabled {
        open_waffle_background_for_display(
            cx,
            backgrounds,
            display_id,
            config,
            theme,
            output_name.as_deref(),
        );
    }

    let backdrop_enabled = config
        .enabled_panels
        .iter()
        .any(|panel| panel == "wBackdrop")
        && waffle.backdrop.enabled.unwrap_or(true);
    if !backdrop_enabled || replace {
        close_waffle_backdrop_for_display(cx, backdrops, display_id);
    }
    if backdrop_enabled {
        open_waffle_backdrop_for_display(
            cx,
            backdrops,
            display_id,
            config,
            theme,
            output_name.as_deref(),
        );
    }
}

fn close_waffle_background_surfaces(
    cx: &mut App,
    backgrounds: &WaffleBackgroundMap,
    backdrops: &WaffleBackdropMap,
    output_names: &WaffleOutputNameMap,
) {
    let mut displays = backgrounds
        .borrow()
        .keys()
        .chain(backdrops.borrow().keys())
        .copied()
        .collect::<Vec<_>>();
    displays.sort_by_key(|display| format!("{display:?}"));
    displays.dedup();
    for display in displays {
        close_waffle_background_for_display(cx, backgrounds, backdrops, display);
    }
    output_names.borrow_mut().clear();
}

fn close_waffle_background_for_display(
    cx: &mut App,
    backgrounds: &WaffleBackgroundMap,
    backdrops: &WaffleBackdropMap,
    display_id: DisplayId,
) {
    close_waffle_background_for_display_only(cx, backgrounds, display_id);
    close_waffle_backdrop_for_display(cx, backdrops, display_id);
}

fn close_waffle_background_for_display_only(
    cx: &mut App,
    backgrounds: &WaffleBackgroundMap,
    display_id: DisplayId,
) {
    let Some(handle) = backgrounds.borrow_mut().remove(&display_id) else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| window.remove_window()) {
        eprintln!("Could not close GPUI Waffle background surface for {display_id:?}: {error:#}");
    }
}

fn close_waffle_backdrop_for_display(
    cx: &mut App,
    backdrops: &WaffleBackdropMap,
    display_id: DisplayId,
) {
    let Some(handle) = backdrops.borrow_mut().remove(&display_id) else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| window.remove_window()) {
        eprintln!("Could not close GPUI Waffle backdrop surface for {display_id:?}: {error:#}");
    }
}

fn open_waffle_background_for_display(
    cx: &mut App,
    backgrounds: &WaffleBackgroundMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    output_name: Option<&str>,
) {
    // The Rust host has no AwwwBackend ownership signal. The operator must
    // disable external Awww ownership before enabling this GPUI surface; do
    // not infer Awww state from config or claim single-owner coordination.
    // Fullscreen and lock-screen visibility signals are also unavailable, and
    // GPUI image failure callbacks cannot try another path after decode fails.
    if backgrounds.borrow().contains_key(&display_id) {
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let configured_path = waffle_main_wallpaper_path(config, output_name);
    if let Some(path) = configured_path.as_ref()
        && classify_wallpaper(Some(path)) != WallpaperKind::Still
    {
        eprintln!(
            "Waffle GPUI background: {:?} wallpaper is not supported by the static surface; leaving it unrendered",
            classify_wallpaper(Some(path))
        );
    }
    let still_path = configured_path
        .as_ref()
        .filter(|path| classify_wallpaper(Some(path)) == WallpaperKind::Still)
        .cloned();
    let status = match (configured_path.as_ref(), still_path.as_ref()) {
        (None, _) => WallpaperStatus::Missing,
        (Some(path), None) if classify_wallpaper(Some(path)) != WallpaperKind::Still => {
            WallpaperStatus::Unsupported
        }
        (Some(_), None) => WallpaperStatus::Missing,
        (Some(_), Some(path)) if path.is_file() => WallpaperStatus::Ready,
        (Some(_), Some(_)) => WallpaperStatus::Missing,
    };
    let props = DesktopBackgroundProps {
        output_name: output_name.unwrap_or_default().to_owned(),
        wallpaper_path: still_path,
        decoded_image: None,
        status,
        colors: crate::components::desktop_background::DesktopBackgroundColors {
            background: theme.colors.background,
            foreground: theme.colors.foreground,
        },
    };
    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: display.bounds().size,
    };
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-waffle-background".to_owned()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-waffle-background".to_owned(),
            layer: Layer::Bottom,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(px(0.)),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    };
    let element_id = format!("waffle.background.{display_id:?}");
    let label = match output_name {
        Some(name) => format!("Waffle desktop wallpaper for output {name}"),
        None => "Waffle desktop wallpaper".to_owned(),
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|_| DesktopBackground::with_accessibility(props, element_id, label))
    }) {
        Ok(handle) => {
            let _ = handle.update(cx, |_, window: &mut Window, _| {
                window.set_input_region(Some(&[]));
            });
            backgrounds.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!(
                "Could not create GPUI Waffle background surface for {display_id:?}: {error:#}"
            );
        }
    }
}

fn open_waffle_backdrop_for_display(
    cx: &mut App,
    backdrops: &WaffleBackdropMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    output_name: Option<&str>,
) {
    if backdrops.borrow().contains_key(&display_id) {
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let waffle = &config.waffle.background;
    let backdrop = &waffle.backdrop;
    let wallpaper_path = waffle_backdrop_wallpaper_path(config, output_name);
    let wallpaper_kind = classify_wallpaper(wallpaper_path.as_deref());
    let status = match wallpaper_path.as_ref() {
        Some(_) if wallpaper_kind != WallpaperKind::Still => BackdropStatus::Unsupported,
        Some(path) if path.is_file() => BackdropStatus::Ready,
        _ => BackdropStatus::Missing,
    };
    let input = DesktopBackdropInput {
        output_name: output_name.unwrap_or_default().to_owned(),
        wallpaper_path,
        decoded_image: None,
        status,
        wallpaper_kind,
        use_aurora_style: false,
        aurora_overlay_opacity: 0.,
        enable_animation: false,
        dim_percent: backdrop.dim.unwrap_or(35.) as f32,
        saturation: backdrop.saturation.unwrap_or(0.) as f32,
        contrast: backdrop.contrast.unwrap_or(0.) as f32,
        blur_radius: backdrop.blur_radius.unwrap_or(32.) as f32,
        vignette: backdrop
            .vignette_enabled
            .unwrap_or(false)
            .then_some(Vignette {
                intensity: backdrop.vignette_intensity.unwrap_or(0.5) as f32,
                radius: backdrop.vignette_radius.unwrap_or(0.7) as f32,
            }),
        reduced_effects: false,
        background: theme
            .colors
            .background
            .unwrap_or_else(|| gpui::rgba(0x101014ff).into()),
        foreground: theme
            .colors
            .foreground
            .unwrap_or_else(|| gpui::rgba(0xffffffff).into()),
    };
    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: display.bounds().size,
    };
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-waffle-backdrop".to_owned()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-waffle-backdrop".to_owned(),
            layer: Layer::Background,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(px(0.)),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    };
    let element_namespace = format!("waffle.backdrop.{display_id:?}");
    let label = match output_name {
        Some(name) => format!("Waffle desktop backdrop for output {name}"),
        None => "Waffle desktop backdrop".to_owned(),
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|_| DesktopBackdrop::with_accessibility(input, element_namespace, label))
    }) {
        Ok(handle) => {
            let _ = handle.update(cx, |_, window: &mut Window, _| {
                window.set_input_region(Some(&[]));
            });
            backdrops.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!(
                "Could not create GPUI Waffle backdrop surface for {display_id:?}: {error:#}"
            );
        }
    }
}

fn waffle_main_wallpaper_path(
    config: &ShellConfig,
    output_name: Option<&str>,
) -> Option<std::path::PathBuf> {
    let per_output_main = config
        .background
        .multi_monitor_enabled
        .unwrap_or(false)
        .then(|| {
            output_name.and_then(|name| {
                config
                    .background
                    .wallpapers_by_monitor
                    .iter()
                    .find(|entry| entry.monitor.as_deref() == Some(name))
                    .and_then(|entry| entry.path.as_deref())
            })
        })
        .flatten()
        .filter(|path| !path.is_empty())
        .and_then(configured_wallpaper_path);
    let waffle_main = (!config.waffle.background.use_main_wallpaper.unwrap_or(true))
        .then(|| config.waffle.background.wallpaper_path.as_deref())
        .flatten()
        .filter(|path| !path.is_empty())
        .and_then(configured_wallpaper_path);
    let global_main = config
        .background
        .wallpaper_path
        .as_deref()
        .filter(|path| !path.is_empty())
        .and_then(configured_wallpaper_path);
    first_existing_path([per_output_main, waffle_main, global_main])
}

fn waffle_backdrop_wallpaper_path(
    config: &ShellConfig,
    output_name: Option<&str>,
) -> Option<std::path::PathBuf> {
    let backdrop = &config.waffle.background.backdrop;
    if backdrop.use_main_wallpaper.unwrap_or(true) {
        return waffle_main_wallpaper_path(config, output_name);
    }
    let per_output = config
        .background
        .multi_monitor_enabled
        .unwrap_or(false)
        .then(|| {
            output_name.and_then(|name| {
                config
                    .background
                    .wallpapers_by_monitor
                    .iter()
                    .find(|entry| entry.monitor.as_deref() == Some(name))
                    .and_then(|entry| entry.backdrop_path.as_deref())
            })
        })
        .flatten()
        .filter(|path| !path.is_empty())
        .and_then(configured_wallpaper_path);
    let global_backdrop = backdrop
        .wallpaper_path
        .as_deref()
        .filter(|path| !path.is_empty())
        .and_then(configured_wallpaper_path);
    let waffle_main = waffle_main_wallpaper_path(config, output_name);
    first_existing_path([per_output, global_backdrop, waffle_main])
}

fn first_existing_path<const N: usize>(
    candidates: [Option<std::path::PathBuf>; N],
) -> Option<std::path::PathBuf> {
    let mut first_configured = None;
    for path in candidates.into_iter().flatten() {
        first_configured.get_or_insert_with(|| path.clone());
        if path.is_file() {
            return Some(path);
        }
    }
    first_configured
}

fn configured_wallpaper_path(configured: &str) -> Option<std::path::PathBuf> {
    let path = configured.strip_prefix("file://").unwrap_or(configured);
    (!path.is_empty()).then(|| std::path::PathBuf::from(path))
}

fn close_ii_critical_surfaces(
    cx: &mut App,
    horizontal_bars: &SurfaceMap,
    vertical_bars: &VerticalSurfaceMap,
    backgrounds: &BackgroundMap,
    backdrops: &BackdropMap,
    docks: &IiDockSurfaceMap,
    panels: &IiPanelSurfaceMap,
    panel_states: &IiPanelStateMap,
    overviews: &IiOverviewOverlayMap,
    output_names: &IiOutputNameMap,
) {
    let mut displays = cx
        .displays()
        .into_iter()
        .map(|display| display.id())
        .collect::<Vec<_>>();
    for display in horizontal_bars.borrow().keys().copied() {
        if !displays.contains(&display) {
            displays.push(display);
        }
    }
    for display in vertical_bars.borrow().keys().copied() {
        if !displays.contains(&display) {
            displays.push(display);
        }
    }
    for display in backgrounds.borrow().keys().copied() {
        if !displays.contains(&display) {
            displays.push(display);
        }
    }
    for display in backdrops.borrow().keys().copied() {
        if !displays.contains(&display) {
            displays.push(display);
        }
    }
    for display in docks.borrow().keys().copied() {
        if !displays.contains(&display) {
            displays.push(display);
        }
    }
    for display in panels.borrow().keys().copied() {
        if !displays.contains(&display) {
            displays.push(display);
        }
    }
    for display in overviews.borrow().keys().copied() {
        if !displays.contains(&display) {
            displays.push(display);
        }
    }

    // Drop focus-stealing overlays before the bars they may return focus to.
    for display in displays {
        close_ii_overview(cx, overviews, horizontal_bars, vertical_bars, display);
        close_ii_panel_for_display(cx, panels, panel_states, display);
        close_for_display(cx, horizontal_bars, display);
        close_vertical_for_display(cx, vertical_bars, display);
        close_background_for_display(cx, backgrounds, display);
        close_backdrop_for_display(cx, backdrops, display);
        close_ii_dock_for_display(cx, docks, display);
    }
    output_names.borrow_mut().clear();
}

fn ii_critical_config_changed(previous: &ConfigLoad, next: &ConfigLoad) -> bool {
    let previous = &previous.config;
    let next = &next.config;
    previous.panel_family != next.panel_family
        || previous.enabled_panels != next.enabled_panels
        || previous.bar != next.bar
        || previous.background != next.background
        || previous.sidebar != next.sidebar
        || previous.dashboard != next.dashboard
        || previous.appearance != next.appearance
}

fn waffle_background_config_changed(previous: &ShellConfig, next: &ShellConfig) -> bool {
    previous.panel_family != next.panel_family
        || previous.enabled_panels != next.enabled_panels
        || previous.background != next.background
        || previous.waffle.background != next.waffle.background
        || previous.appearance != next.appearance
}

/// Reconcile only the ii critical composition. Bar and dock use their own
/// QML output lists (empty/stale lists fall back to all outputs); iiBackground
/// has no per-output list in its QML root and therefore targets every output.
fn reconcile_ii_critical_for_display(
    cx: &mut App,
    horizontal_bars: &SurfaceMap,
    vertical_bars: &VerticalSurfaceMap,
    backgrounds: &BackgroundMap,
    backdrops: &BackdropMap,
    docks: &IiDockSurfaceMap,
    output_names: &IiOutputNameMap,
    display_id: DisplayId,
    snapshot: &IiCriticalSnapshot,
    panel_events: &Sender<IiPanelEvent>,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
    media_cache: &MediaStateCache,
    replace: bool,
) {
    let config = &snapshot.loaded.config;
    let current_name = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
        .and_then(|display| niri_output_name(display.bounds()));
    let previous_name = if let Some(name) = current_name.as_ref() {
        output_names.borrow_mut().insert(display_id, name.clone())
    } else {
        output_names.borrow_mut().remove(&display_id)
    };
    // Treat unresolved→resolved and resolved→unresolved transitions as a
    // surface identity change, so output-scoped providers receive fresh names.
    let output_name_changed = previous_name != current_name;
    let replace = replace || output_name_changed;
    let selected_ii = config.panel_family.as_deref() == Some("ii");

    let vertical = config.bar.vertical.unwrap_or(false);
    let selected_bar_id = if vertical { "iiVerticalBar" } else { "iiBar" };
    let bar_enabled = selected_ii
        && config
            .enabled_panels
            .iter()
            .any(|panel| panel == selected_bar_id)
        && ii_panel_screen_enabled(cx, display_id, &config.bar.screen_list);

    // Close the inactive variant before creating the selected one, including
    // on a vertical/horizontal mode switch, so two bars never reserve input.
    if replace || vertical || !selected_ii {
        close_for_display(cx, horizontal_bars, display_id);
    }
    if replace || !vertical || !selected_ii {
        close_vertical_for_display(cx, vertical_bars, display_id);
    }
    if !bar_enabled {
        close_for_display(cx, horizontal_bars, display_id);
        close_vertical_for_display(cx, vertical_bars, display_id);
    } else if vertical {
        if replace {
            close_vertical_for_display(cx, vertical_bars, display_id);
        }
        resize_vertical_for_display(
            cx,
            vertical_bars,
            display_id,
            config,
            &snapshot.theme,
            panel_events,
            media_actions,
            media_cache,
        );
    } else {
        if replace {
            close_for_display(cx, horizontal_bars, display_id);
        }
        resize_for_display(
            cx,
            horizontal_bars,
            display_id,
            config,
            &snapshot.theme,
            panel_events,
        );
    }

    let background_enabled = selected_ii
        && config
            .enabled_panels
            .iter()
            .any(|panel| panel == "iiBackground")
        && !(config.background.backdrop.enabled.unwrap_or(true)
            && config.background.backdrop.hide_wallpaper.unwrap_or(false));
    if !background_enabled || replace {
        close_background_for_display(cx, backgrounds, display_id);
    }
    if background_enabled {
        resize_background_for_display(cx, backgrounds, display_id, config, &snapshot.theme);
    }

    let backdrop_enabled = selected_ii
        && config
            .enabled_panels
            .iter()
            .any(|panel| panel == "iiBackdrop")
        && config.background.backdrop.enabled.unwrap_or(true);
    if !backdrop_enabled || replace {
        close_backdrop_for_display(cx, backdrops, display_id);
    }
    if backdrop_enabled {
        open_backdrop_for_display(cx, backdrops, display_id, config, &snapshot.theme);
    }

    let dock_options = config.appearance.source_sections.get("ii.dock");
    let dock_enabled = selected_ii
        && config.enabled_panels.iter().any(|panel| panel == "iiDock")
        && dock_options
            .and_then(|options| options.get("enable"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
    let dock_screens = dock_options
        .and_then(|options| options.get("screenList"))
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let dock_targets_output = ii_panel_screen_enabled(cx, display_id, &dock_screens);
    if !dock_enabled || !dock_targets_output || replace {
        close_ii_dock_for_display(cx, docks, display_id);
    }
    if dock_enabled && dock_targets_output {
        resize_ii_dock_for_display(cx, docks, display_id, config, &snapshot.theme);
    }
}

fn ii_panel_enabled(config: &ShellConfig, panel: IiPanelKind) -> bool {
    let name = match panel {
        IiPanelKind::LeftSidebar => "iiSidebarLeft",
        IiPanelKind::RightSidebar => "iiSidebarRight",
        IiPanelKind::Dashboard => "iiDashboard",
    };
    config.enabled_panels.iter().any(|enabled| enabled == name)
        && (panel != IiPanelKind::Dashboard || config.dashboard.enabled)
}

/// QML `sidebar.screenList`: empty selects every output; a list with no live
/// matches falls back to every output; otherwise only named matches are used.
fn ii_panel_screen_enabled(cx: &App, display_id: DisplayId, screen_list: &[String]) -> bool {
    if screen_list.is_empty() {
        return true;
    }
    let names = cx
        .displays()
        .into_iter()
        .filter_map(|display| niri_output_name(display.bounds()).map(|name| (display.id(), name)))
        .collect::<Vec<_>>();
    if !names
        .iter()
        .any(|(_, name)| screen_list.iter().any(|selected| selected == name))
    {
        return true;
    }
    names
        .iter()
        .any(|(id, name)| *id == display_id && screen_list.iter().any(|selected| selected == name))
}

fn apply_ii_panel_event(
    cx: &mut App,
    surfaces: &IiPanelSurfaceMap,
    states: &IiPanelStateMap,
    vertical_bars: &VerticalSurfaceMap,
    horizontal_bars: &SurfaceMap,
    event: IiPanelEvent,
    config: &ShellConfig,
    theme: &ThemeTokens,
    events: &Sender<IiPanelEvent>,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
    media_cache: &MediaStateCache,
) {
    let display_id = match &event {
        IiPanelEvent::Toggle { display, .. }
        | IiPanelEvent::Close { display, .. }
        | IiPanelEvent::ToggleExpand { display }
        | IiPanelEvent::SelectLeftTab { display, .. }
        | IiPanelEvent::LaunchQuickCommand { display, .. }
        | IiPanelEvent::SaveQuickNote { display, .. }
        | IiPanelEvent::ClearQuickNote { display }
        | IiPanelEvent::CloseAll { display }
        | IiPanelEvent::ToggleOverview { display }
        | IiPanelEvent::CloseOverview { display }
        | IiPanelEvent::SetOverviewMode { display, .. }
        | IiPanelEvent::SetOverviewQuery { display, .. }
        | IiPanelEvent::LaunchOverviewApplication { display, .. }
        | IiPanelEvent::ToggleVolumeMixer { display }
        | IiPanelEvent::ToggleVolumeDevicePicker { display }
        | IiPanelEvent::CloseVolumeMixer { display }
        | IiPanelEvent::SelectVolumeMixerTab { display, .. } => *display,
    };
    if !ii_panel_screen_enabled(cx, display_id, &config.sidebar.screen_list) {
        if let Some(handle) = vertical_bars.borrow().get(&display_id).copied() {
            let _ = handle.update(cx, |host, _, cx| host.set_left_sidebar_open(false, cx));
        }
        if let Some(handle) = horizontal_bars.borrow().get(&display_id).copied() {
            let _ = handle.update(cx, |host, _, cx| host.set_left_sidebar_open(false, cx));
        }
        close_ii_panel_for_display(cx, surfaces, states, display_id);
        return;
    }
    let mut state = states
        .borrow()
        .get(&display_id)
        .cloned()
        .unwrap_or_default();
    if !states.borrow().contains_key(&display_id) {
        state.volume_mixer_tab = remembered_volume_mixer_tab(display_id);
    }
    if let IiPanelEvent::LaunchQuickCommand { command, .. } = &event {
        let command = command.trim();
        let launch_enabled = state.selected_left_tab_id.as_deref() == Some("widgets")
            && config.sidebar.widgets_launch_enabled
            && config.sidebar.widget_order.iter().any(|id| id == "launch")
            && config.sidebar.quick_launch.iter().any(|entry| {
                entry
                    .cmd
                    .as_deref()
                    .is_some_and(|configured| configured.trim() == command)
            });
        if launch_enabled {
            if let Err(error) = crate::families::ii::quick_launch::spawn_configured_command(command)
            {
                eprintln!("ii QuickLaunch: {error}");
            }
        }
        return;
    }
    if matches!(
        event,
        IiPanelEvent::SaveQuickNote { .. } | IiPanelEvent::ClearQuickNote { .. }
    ) {
        let note_enabled = state.selected_left_tab_id.as_deref() == Some("widgets")
            && config.sidebar.widgets_note_enabled
            && config.sidebar.widget_order.iter().any(|id| id == "note");
        let result = if note_enabled {
            match &event {
                IiPanelEvent::SaveQuickNote { text, .. } => {
                    crate::families::ii::quick_note_store::save_current_text(text)
                }
                IiPanelEvent::ClearQuickNote { .. } => {
                    crate::families::ii::quick_note_store::clear_current_text()
                }
                _ => unreachable!(),
            }
        } else {
            Err("QuickNote is not enabled for the selected view".into())
        };
        let snapshot = result.unwrap_or_else(|error| {
            crate::families::ii::quick_note_store::QuickNoteSnapshot {
                status: crate::families::ii::quick_note_store::QuickNoteStatus::Unavailable,
                title: None,
                text: None,
                current_tab: None,
                error: Some(error),
            }
        });
        if let Some(handle) = surfaces.borrow().get(&display_id).copied() {
            let _ = handle.update(cx, |view, _, cx| view.set_quick_note(snapshot, cx));
        }
        return;
    }
    match event {
        IiPanelEvent::Toggle { panel, .. } => {
            if !ii_panel_enabled(config, panel) {
                return;
            }
            match panel {
                IiPanelKind::LeftSidebar => state.left_open = !state.left_open,
                IiPanelKind::RightSidebar => {
                    state.right_open = !state.right_open;
                    if !state.right_open {
                        state.volume_mixer_open = false;
                        state.volume_device_picker_open = false;
                    }
                }
                IiPanelKind::Dashboard => state.dashboard_open = !state.dashboard_open,
            }
        }
        IiPanelEvent::Close { panel, .. } => match panel {
            IiPanelKind::LeftSidebar => state.left_open = false,
            IiPanelKind::RightSidebar => {
                state.right_open = false;
                state.volume_mixer_open = false;
                state.volume_device_picker_open = false;
            }
            IiPanelKind::Dashboard => state.dashboard_open = false,
        },
        IiPanelEvent::ToggleExpand { .. } => state.left_expanded = !state.left_expanded,
        IiPanelEvent::SelectLeftTab { tab_id, .. } => {
            if !config
                .sidebar
                .left_tabs
                .iter()
                .any(|tab| tab.enabled && tab.id == tab_id)
            {
                return;
            }
            state.selected_left_tab_id = Some(tab_id);
        }
        IiPanelEvent::ToggleVolumeMixer { .. } => {
            state.volume_mixer_open = !state.volume_mixer_open;
            if !state.volume_mixer_open {
                state.volume_device_picker_open = false;
            }
        }
        IiPanelEvent::ToggleVolumeDevicePicker { .. } => {
            state.volume_device_picker_open = !state.volume_device_picker_open;
        }
        IiPanelEvent::CloseVolumeMixer { .. } => {
            state.volume_mixer_open = false;
            state.volume_device_picker_open = false;
        }
        IiPanelEvent::SelectVolumeMixerTab { tab, .. } => {
            state.volume_mixer_tab = tab;
            state.volume_device_picker_open = false;
            remember_volume_mixer_tab(display_id, tab);
        }
        IiPanelEvent::CloseAll { .. } => state = IiPanelState::default(),
        IiPanelEvent::ToggleOverview { .. }
        | IiPanelEvent::CloseOverview { .. }
        | IiPanelEvent::SetOverviewMode { .. }
        | IiPanelEvent::SetOverviewQuery { .. }
        | IiPanelEvent::LaunchOverviewApplication { .. } => return,
        IiPanelEvent::SaveQuickNote { .. } | IiPanelEvent::ClearQuickNote { .. } => return,
        IiPanelEvent::LaunchQuickCommand { .. } => return,
    }
    if !state.selected_left_tab_id.as_ref().is_some_and(|selected| {
        config
            .sidebar
            .left_tabs
            .iter()
            .any(|tab| tab.enabled && &tab.id == selected)
    }) {
        state.selected_left_tab_id = config
            .sidebar
            .left_tab_order
            .iter()
            .find(|id| {
                config
                    .sidebar
                    .left_tabs
                    .iter()
                    .any(|tab| tab.enabled && &tab.id == *id)
            })
            .cloned();
    }
    if let Some(handle) = vertical_bars.borrow().get(&display_id).copied() {
        let left_open = state.left_open;
        let _ = handle.update(cx, |host, _, cx| host.set_left_sidebar_open(left_open, cx));
    }
    if let Some(handle) = horizontal_bars.borrow().get(&display_id).copied() {
        let left_open = state.left_open;
        let _ = handle.update(cx, |host, _, cx| host.set_left_sidebar_open(left_open, cx));
    }
    if state.any_open() {
        states.borrow_mut().insert(display_id, state.clone());
        if let Some(handle) = surfaces.borrow().get(&display_id).copied() {
            if let Err(error) = handle.update(cx, |view, _, cx| view.set_state(state.clone(), cx)) {
                eprintln!("Could not update GPUI ii panel surface for {display_id:?}: {error:#}");
                close_ii_panel_for_display(cx, surfaces, states, display_id);
            }
        } else {
            open_ii_panel_for_display(
                cx,
                surfaces,
                display_id,
                config,
                theme,
                state.clone(),
                events,
                media_actions,
                media_cache,
            );
        }
    } else {
        close_ii_panel_for_display(cx, surfaces, states, display_id);
    }
}

fn open_ii_panel_for_display(
    cx: &mut App,
    surfaces: &IiPanelSurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    state: IiPanelState,
    events: &Sender<IiPanelEvent>,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
    media_cache: &MediaStateCache,
) {
    if surfaces.borrow().contains_key(&display_id) {
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let output_size = display.bounds().size;
    let output_name = niri_output_name(display.bounds());
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size: output_size,
        })),
        app_id: Some("intoo-gpui-ii-panels".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-ii-panels".into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(Pixels::ZERO),
            exclusive_edge: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    };
    let actions = ii_panel_actions(events, display_id, media_actions);
    let config = config.clone();
    let theme = theme.clone();
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| {
            IiPanelSurface::new(output_name, output_size, config, theme, state, actions, cx)
        })
    }) {
        Ok(handle) => {
            let media = media_cache.borrow().clone();
            let _ = handle.update(cx, |view, _, cx| view.set_media_state(media, cx));
            surfaces.borrow_mut().insert(display_id, handle);
        }
        Err(error) => eprintln!("Could not create GPUI ii overlay for {display_id:?}: {error:#}"),
    }
}

fn ii_panel_actions(
    events: &Sender<IiPanelEvent>,
    display: DisplayId,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
) -> crate::families::ii::panel_surface::IiPanelActions {
    let send = |event: IiPanelEvent| {
        let sender = events.clone();
        std::sync::Arc::new(move || {
            let _ = sender.try_send(event.clone());
        }) as std::sync::Arc<dyn Fn() + Send + Sync>
    };
    let select_tab = {
        let sender = events.clone();
        std::sync::Arc::new(move |tab| {
            let _ = sender.try_send(IiPanelEvent::SelectVolumeMixerTab { display, tab });
        }) as std::sync::Arc<dyn Fn(VolumeMixerTab) + Send + Sync>
    };
    let select_left_tab = {
        let sender = events.clone();
        std::sync::Arc::new(move |tab_id| {
            let _ = sender.try_send(IiPanelEvent::SelectLeftTab { display, tab_id });
        }) as std::sync::Arc<dyn Fn(String) + Send + Sync>
    };
    let launch_quick_command = {
        let sender = events.clone();
        std::sync::Arc::new(move |command| {
            let _ = sender.try_send(IiPanelEvent::LaunchQuickCommand { display, command });
        }) as std::sync::Arc<dyn Fn(String) + Send + Sync>
    };
    let save_quick_note = {
        let sender = events.clone();
        std::sync::Arc::new(move |text| {
            let _ = sender.try_send(IiPanelEvent::SaveQuickNote { display, text });
        }) as std::sync::Arc<dyn Fn(String) + Send + Sync>
    };
    let clear_quick_note = {
        let sender = events.clone();
        std::sync::Arc::new(move || {
            let _ = sender.try_send(IiPanelEvent::ClearQuickNote { display });
        }) as std::sync::Arc<dyn Fn() + Send + Sync>
    };
    crate::families::ii::panel_surface::IiPanelActions {
        close_left: send(IiPanelEvent::Close {
            display,
            panel: IiPanelKind::LeftSidebar,
        }),
        close_right: send(IiPanelEvent::Close {
            display,
            panel: IiPanelKind::RightSidebar,
        }),
        close_dashboard: send(IiPanelEvent::Close {
            display,
            panel: IiPanelKind::Dashboard,
        }),
        expand_left: send(IiPanelEvent::ToggleExpand { display }),
        select_left_tab,
        launch_quick_command,
        save_quick_note,
        clear_quick_note,
        close_all: send(IiPanelEvent::CloseAll { display }),
        toggle_volume_device_picker: send(IiPanelEvent::ToggleVolumeDevicePicker { display }),
        close_volume_mixer: send(IiPanelEvent::CloseVolumeMixer { display }),
        select_volume_mixer_tab: select_tab,
        media_action: {
            let sender = media_actions.clone();
            std::sync::Arc::new(move |action| {
                let _ = sender.try_send(action);
            })
        },
    }
}

fn close_ii_panel_for_display(
    cx: &mut App,
    surfaces: &IiPanelSurfaceMap,
    states: &IiPanelStateMap,
    display_id: DisplayId,
) {
    states.borrow_mut().remove(&display_id);
    if let Some(handle) = surfaces.borrow_mut().remove(&display_id) {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

fn reconcile_ii_panel_for_display(
    cx: &mut App,
    surfaces: &IiPanelSurfaceMap,
    states: &IiPanelStateMap,
    vertical_bars: &VerticalSurfaceMap,
    horizontal_bars: &SurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    events: &Sender<IiPanelEvent>,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
    media_cache: &MediaStateCache,
    replace: bool,
) {
    let Some(mut state) = states.borrow().get(&display_id).cloned() else {
        if surfaces.borrow().contains_key(&display_id) {
            close_ii_panel_for_display(cx, surfaces, states, display_id);
        }
        return;
    };
    if !ii_panel_enabled(config, IiPanelKind::LeftSidebar) {
        state.left_open = false;
        state.left_expanded = false;
    }
    if !ii_panel_enabled(config, IiPanelKind::RightSidebar) {
        state.right_open = false;
    }
    if !ii_panel_enabled(config, IiPanelKind::Dashboard) {
        state.dashboard_open = false;
    }
    if !ii_panel_screen_enabled(cx, display_id, &config.sidebar.screen_list) {
        if let Some(handle) = vertical_bars.borrow().get(&display_id).copied() {
            let _ = handle.update(cx, |host, _, cx| host.set_left_sidebar_open(false, cx));
        }
        if let Some(handle) = horizontal_bars.borrow().get(&display_id).copied() {
            let _ = handle.update(cx, |host, _, cx| host.set_left_sidebar_open(false, cx));
        }
        close_ii_panel_for_display(cx, surfaces, states, display_id);
    } else if !state.left_open
        && !state.right_open
        && !state.dashboard_open
        && !state.volume_mixer_open
    {
        close_ii_panel_for_display(cx, surfaces, states, display_id);
    } else if replace {
        close_ii_panel_for_display(cx, surfaces, states, display_id);
        states.borrow_mut().insert(display_id, state.clone());
        open_ii_panel_for_display(
            cx,
            surfaces,
            display_id,
            config,
            theme,
            state,
            events,
            media_actions,
            media_cache,
        );
    } else if let Some(handle) = surfaces.borrow().get(&display_id).copied() {
        states.borrow_mut().insert(display_id, state.clone());
        let Some(display) = cx
            .displays()
            .into_iter()
            .find(|display| display.id() == display_id)
        else {
            return;
        };
        let size = display.bounds().size;
        if let Err(error) = handle.update(cx, |view, window: &mut Window, cx| {
            view.set_state(state.clone(), cx);
            window.resize(size);
        }) {
            eprintln!("Could not resize GPUI ii panel surface for {display_id:?}: {error:#}");
            close_ii_panel_for_display(cx, surfaces, states, display_id);
        }
    } else {
        states.borrow_mut().insert(display_id, state.clone());
        open_ii_panel_for_display(
            cx,
            surfaces,
            display_id,
            config,
            theme,
            state.clone(),
            events,
            media_actions,
            media_cache,
        );
    }
}

fn iris_display_enabled(
    cx: &App,
    display_id: DisplayId,
    options: &Option<serde_json::Value>,
) -> bool {
    let list: Vec<String> = options
        .as_ref()
        .and_then(|v| v.get("screenList"))
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if list.is_empty() {
        return true;
    }
    let displays = cx.displays();
    let named: Vec<_> = displays
        .iter()
        .filter_map(|d| niri_output_name(d.bounds()).map(|n| (d.id(), n)))
        .collect();
    !named
        .iter()
        .any(|(_, name)| list.iter().any(|selected| selected == name))
        || named
            .iter()
            .any(|(id, name)| *id == display_id && list.iter().any(|selected| selected == name))
}

fn reconcile_iris_displays(
    cx: &mut App,
    surfaces: &SurfaceMap,
    config: &ShellConfig,
    theme: &ThemeTokens,
    options: &Option<serde_json::Value>,
    dock_options: &Option<serde_json::Value>,
    dock_enabled: bool,
) {
    for display in cx.displays() {
        if iris_display_enabled(cx, display.id(), options) {
            open_iris_for_display(
                cx,
                surfaces,
                display.id(),
                config,
                theme,
                options.clone(),
                dock_options.clone(),
                dock_enabled,
            );
        } else {
            close_for_display(cx, surfaces, display.id());
        }
    }
}

fn open_iris_for_display(
    cx: &mut App,
    surfaces: &SurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    options: Option<serde_json::Value>,
    dock_options: Option<serde_json::Value>,
    dock_enabled: bool,
) {
    if surfaces.borrow().contains_key(&display_id) {
        return;
    }
    let Some(display) = cx.displays().into_iter().find(|d| d.id() == display_id) else {
        return;
    };
    if !IrisRuntime::can_resolve(theme, options.as_ref()) {
        eprintln!("GPUI Iris: theme/preset unresolved on {display_id:?}; skipping surface");
        return;
    }
    let display_bounds = display.bounds();
    let options_window = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size: display_bounds.size,
        })),
        app_id: Some("intoo-gpui-iris-prototype".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-iris".into(),
            layer: Layer::Top,
            // IrisBar.qml's PanelWindow fills the output and explicitly uses
            // exclusiveZone: 0; its edge and island layout are internal.
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(Pixels::ZERO),
            exclusive_edge: None,
            // OnDemand allows keyboard input only after an explicit focus
            // request (e.g. opening the Iris search palette or Orbit).
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            ..Default::default()
        }),
        ..Default::default()
    };
    let theme = theme.clone();
    let output_size = display_bounds.size;
    let output_name = niri_output_name(display_bounds);
    let composition = match (ConfiguredFamilyInputs {
        output_name,
        config: config.clone(),
        theme,
    })
    .create_composition(FamilyRuntimeInputs::Iris {
        output_size,
        options,
        dock_options,
        dock_enabled,
    }) {
        Ok(composition) => composition,
        Err(error) => {
            eprintln!("GPUI Iris plugin: could not create composition: {error:?}");
            return;
        }
    };
    match cx.open_window(options_window, move |_, cx| {
        cx.new(|cx| FamilyHost::new(composition, cx))
    }) {
        Ok(handle) => {
            surfaces.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!("Could not create GPUI Iris layer surface for {display_id:?}: {error:#}")
        }
    }
}

fn resize_iris_for_display(
    cx: &mut App,
    surfaces: &SurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    options: Option<serde_json::Value>,
    dock_options: Option<serde_json::Value>,
    dock_enabled: bool,
) {
    let Some(handle) = surfaces.borrow().get(&display_id).copied() else {
        open_iris_for_display(
            cx,
            surfaces,
            display_id,
            config,
            theme,
            options,
            dock_options,
            dock_enabled,
        );
        return;
    };
    let Some(display) = cx.displays().into_iter().find(|d| d.id() == display_id) else {
        return;
    };
    let size = display.bounds().size;
    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| window.resize(size)) {
        eprintln!("Could not resize GPUI Iris surface for {display_id:?}: {error:#}");
        close_for_display(cx, surfaces, display_id);
        open_iris_for_display(
            cx,
            surfaces,
            display_id,
            config,
            theme,
            options,
            dock_options,
            dock_enabled,
        );
    }
}

fn close_waffle_overlay(cx: &mut App, overlays: &OverlayMap, display: DisplayId) {
    if let Some(handle) = overlays.borrow_mut().remove(&display) {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

fn open_waffle_overlay(
    cx: &mut App,
    overlays: &OverlayMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    source: &Option<std::sync::Arc<XdgDesktopEntrySource>>,
    events: &Sender<StartMenuEvent>,
) {
    if overlays.borrow().contains_key(&display_id) {
        return;
    }
    if !config
        .enabled_panels
        .iter()
        .any(|panel| panel == "wStartMenu")
    {
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let size = display.bounds().size;
    let base = crate::families::waffle::runtime::taskbar_style(theme);
    let style = crate::families::waffle::runtime::start_menu_style(theme, &base);
    let source_trait = source.as_ref().map(|source| {
        source.clone()
            as std::sync::Arc<
                dyn crate::families::waffle::taskbar::start_menu_adapter::StartMenuDataSource,
            >
    });
    let mut input =
        crate::families::waffle::taskbar::start_menu_adapter::input(style, source_trait);
    let source_close = input.on_close.clone();
    let close_events = events.clone();
    input.on_close = std::sync::Arc::new(move || {
        source_close();
        let _ = close_events.try_send(StartMenuEvent::Close {
            display: display_id,
        });
    });
    let refresh_events = events.clone();
    let query = input.on_query_changed.clone();
    input.on_query_changed = std::sync::Arc::new(move |value| {
        query(value);
        let _ = refresh_events.try_send(StartMenuEvent::Refresh {
            display: display_id,
        });
    });
    let refresh_events = events.clone();
    let select = input.on_select_result.clone();
    input.on_select_result = std::sync::Arc::new(move |id| {
        select(id);
        let _ = refresh_events.try_send(StartMenuEvent::Refresh {
            display: display_id,
        });
    });
    let refresh_events = events.clone();
    let all_apps = input.on_all_apps.clone();
    input.on_all_apps = std::sync::Arc::new(move || {
        all_apps();
        let _ = refresh_events.try_send(StartMenuEvent::Refresh {
            display: display_id,
        });
    });
    let refresh_events = events.clone();
    let back = input.on_back.clone();
    input.on_back = std::sync::Arc::new(move || {
        back();
        let _ = refresh_events.try_send(StartMenuEvent::Refresh {
            display: display_id,
        });
    });
    let bottom = config.waffle.bar.bottom.unwrap_or(false);
    let anchor = crate::families::waffle::taskbar::start_menu_overlay::StartMenuOverlayAnchor {
        display_size: size,
        bar_bottom: bottom,
        left_align_apps: false,
        bar_height: px(48.0),
    };
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size,
        })),
        app_id: Some("intoo-gpui-waffle-start-menu".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-waffle-start-menu".into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(Pixels::ZERO),
            exclusive_edge: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    };
    let events = events.clone();
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| {
            crate::families::waffle::taskbar::start_menu_overlay::WaffleStartMenuOverlay::new(
                input,
                anchor,
                std::sync::Arc::new(move || {
                    let _ = events.try_send(StartMenuEvent::Close {
                        display: display_id,
                    });
                }),
                cx,
            )
        })
    }) {
        Ok(handle) => {
            overlays.borrow_mut().insert(display_id, handle);
        }
        Err(error) => eprintln!("Could not open Waffle Start Menu on {display_id:?}: {error:#}"),
    }
}

fn open_waffle_for_display(
    cx: &mut App,
    surfaces: &SurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    controller: &Sender<StartMenuEvent>,
    source: &Option<std::sync::Arc<XdgDesktopEntrySource>>,
    notification_events: &Sender<NotificationCenterEvent>,
    notification_controller: &Rc<RefCell<NotificationCenterController>>,
    task_view_events: &Sender<TaskViewEvent>,
    task_view_controller: &Rc<RefCell<TaskViewController>>,
    action_center_events: &Sender<ActionCenterEvent>,
    action_center_controller: &Rc<RefCell<ActionCenterController>>,
    clipboard_events: &Sender<ClipboardEvent>,
    clipboard_controller: &Rc<RefCell<ClipboardController>>,
) {
    if surfaces.borrow().contains_key(&display_id) {
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let height = px(48.0);
    let bottom = config.waffle.bar.bottom.unwrap_or(false);
    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: Size::new(display.bounds().size.width, height),
    };
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-waffle-prototype".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-waffle".to_string(),
            layer: Layer::Top,
            anchor: if bottom {
                Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
            } else {
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT
            },
            exclusive_zone: Some(height),
            exclusive_edge: Some(if bottom { Anchor::BOTTOM } else { Anchor::TOP }),
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            ..Default::default()
        }),
        ..Default::default()
    };
    let theme = theme.clone();
    let output_name = niri_output_name(display.bounds());
    let composition = match (ConfiguredFamilyInputs {
        output_name,
        config: config.clone(),
        theme,
    })
    .create_composition(FamilyRuntimeInputs::Waffle {
        display_id,
        controller: controller.clone(),
        notification_events: notification_events.clone(),
        notification_controller: notification_controller.clone(),
        task_view_events: task_view_events.clone(),
        task_view_controller: task_view_controller.clone(),
        action_center_events: action_center_events.clone(),
        action_center_controller: action_center_controller.clone(),
        clipboard_events: clipboard_events.clone(),
        clipboard_controller: clipboard_controller.clone(),
        source: source.clone(),
    }) {
        Ok(composition) => composition,
        Err(error) => {
            eprintln!("GPUI Waffle plugin: could not create composition: {error:?}");
            return;
        }
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| FamilyHost::new(composition, cx))
    }) {
        Ok(handle) => {
            surfaces.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!("Could not create GPUI Waffle layer surface for {display_id:?}: {error:#}");
        }
    }
}

fn resize_waffle_for_display(
    cx: &mut App,
    surfaces: &SurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    controller: &Sender<StartMenuEvent>,
    source: &Option<std::sync::Arc<XdgDesktopEntrySource>>,
    notification_events: &Sender<NotificationCenterEvent>,
    notification_controller: &Rc<RefCell<NotificationCenterController>>,
    task_view_events: &Sender<TaskViewEvent>,
    task_view_controller: &Rc<RefCell<TaskViewController>>,
    action_center_events: &Sender<ActionCenterEvent>,
    action_center_controller: &Rc<RefCell<ActionCenterController>>,
    clipboard_events: &Sender<ClipboardEvent>,
    clipboard_controller: &Rc<RefCell<ClipboardController>>,
) {
    let Some(handle) = surfaces.borrow().get(&display_id).copied() else {
        open_waffle_for_display(
            cx,
            surfaces,
            display_id,
            config,
            theme,
            controller,
            source,
            notification_events,
            notification_controller,
            task_view_events,
            task_view_controller,
            action_center_events,
            action_center_controller,
            clipboard_events,
            clipboard_controller,
        );
        return;
    };
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| {
        window.resize(Size::new(display.bounds().size.width, px(48.0)));
    }) {
        eprintln!("Could not resize GPUI Waffle layer surface for {display_id:?}: {error:#}");
        close_for_display(cx, surfaces, display_id);
        open_waffle_for_display(
            cx,
            surfaces,
            display_id,
            config,
            theme,
            controller,
            source,
            notification_events,
            notification_controller,
            task_view_events,
            task_view_controller,
            action_center_events,
            action_center_controller,
            clipboard_events,
            clipboard_controller,
        );
    }
}

fn close_notification_overlay(cx: &mut App, overlays: &NotificationOverlayMap, display: DisplayId) {
    if let Some(handle) = overlays.borrow_mut().remove(&display) {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

fn close_task_view_overlay(cx: &mut App, overlays: &TaskViewOverlayMap, display: DisplayId) {
    if let Some(handle) = overlays.borrow_mut().remove(&display) {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

fn close_action_center_overlay(
    cx: &mut App,
    overlays: &ActionCenterOverlayMap,
    display: DisplayId,
) {
    if let Some(handle) = overlays.borrow_mut().remove(&display) {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

fn close_clipboard_overlay(cx: &mut App, overlays: &ClipboardOverlayMap, display: DisplayId) {
    if let Some(handle) = overlays.borrow_mut().remove(&display) {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

fn restore_ii_overview_focus(
    cx: &mut App,
    horizontal: &SurfaceMap,
    vertical: &VerticalSurfaceMap,
    display: DisplayId,
) {
    if let Some(handle) = horizontal.borrow().get(&display).copied() {
        let _ = handle.update(cx, |view, window, cx| {
            view.focus_overview_launcher(window, cx)
        });
    } else if let Some(handle) = vertical.borrow().get(&display).copied() {
        let _ = handle.update(cx, |view, window, cx| {
            view.focus_overview_launcher(window, cx)
        });
    }
}

fn close_ii_overview(
    cx: &mut App,
    overlays: &IiOverviewOverlayMap,
    horizontal: &SurfaceMap,
    vertical: &VerticalSurfaceMap,
    display: DisplayId,
) {
    if let Some(handle) = overlays.borrow_mut().remove(&display) {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
        restore_ii_overview_focus(cx, horizontal, vertical, display);
    }
}

fn reconcile_ii_overview_for_display(
    cx: &mut App,
    overlays: &IiOverviewOverlayMap,
    horizontal: &SurfaceMap,
    vertical: &VerticalSurfaceMap,
    display: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    events: &Sender<IiPanelEvent>,
    replace: bool,
) {
    let enabled = config.panel_family.as_deref() == Some("ii")
        && config
            .enabled_panels
            .iter()
            .any(|name| name == "iiOverview");
    let saved_state = overlays
        .borrow()
        .get(&display)
        .copied()
        .and_then(|handle| handle.update(cx, |view, _, _| view.view_state()).ok());
    if !enabled {
        close_ii_overview(cx, overlays, horizontal, vertical, display);
        return;
    }
    if !replace || saved_state.is_none() {
        return;
    }

    close_ii_overview(cx, overlays, horizontal, vertical, display);
    if open_ii_overview(cx, overlays, display, theme, events) {
        if let (Some(handle), Some((mode, query))) =
            (overlays.borrow().get(&display).copied(), saved_state)
        {
            let _ = handle.update(cx, |view, _, cx| {
                view.set_mode(mode, cx);
                view.set_query(query, cx);
            });
        }
    }
}

fn open_ii_overview(
    cx: &mut App,
    overlays: &IiOverviewOverlayMap,
    display_id: DisplayId,
    theme: &ThemeTokens,
    events: &Sender<IiPanelEvent>,
) -> bool {
    if overlays.borrow().contains_key(&display_id) {
        return false;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|item| item.id() == display_id)
    else {
        return false;
    };
    let bounds = display.bounds();
    let output = niri_output_name(bounds);
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-ii-overview".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-ii-overview".into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(Pixels::ZERO),
            exclusive_edge: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    };
    let theme = theme.clone();
    let events = events.clone();
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| IiOverviewOverlay::new(display_id, output, theme, events, cx))
    }) {
        Ok(handle) => {
            overlays.borrow_mut().insert(display_id, handle);
            true
        }
        Err(error) => {
            eprintln!("Could not open ii Overview on {display_id:?}: {error:#}");
            false
        }
    }
}

fn open_clipboard_overlay(
    cx: &mut App,
    overlays: &ClipboardOverlayMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    controller: &Rc<RefCell<ClipboardController>>,
    events: &Sender<ClipboardEvent>,
    pins: &[String],
) -> bool {
    if overlays.borrow().contains_key(&display_id)
        || !config
            .enabled_panels
            .iter()
            .any(|panel| panel == "wClipboard")
    {
        return false;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return false;
    };
    let size = display.bounds().size;
    let theme = theme.clone();
    let controller = controller.clone();
    let events = events.clone();
    let pins = pins.to_vec();
    let bottom = config.waffle.bar.bottom.unwrap_or(false);
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size,
        })),
        app_id: Some("intoo-gpui-waffle-clipboard".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-waffle-clipboard".into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(Pixels::ZERO),
            exclusive_edge: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| {
            crate::families::waffle::clipboard_overlay::WaffleClipboardOverlay::new(
                theme,
                events,
                controller,
                display_id,
                bottom,
                (f32::from(size.height) - 64.).max(1.),
                pins,
                cx,
            )
        })
    }) {
        Ok(handle) => {
            overlays.borrow_mut().insert(display_id, handle);
            true
        }
        Err(error) => {
            eprintln!("Could not open Waffle Clipboard on {display_id:?}: {error:#}");
            false
        }
    }
}

fn open_action_center_overlay(
    cx: &mut App,
    overlays: &ActionCenterOverlayMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    controller: &Rc<RefCell<ActionCenterController>>,
    events: &Sender<ActionCenterEvent>,
) -> bool {
    if overlays.borrow().contains_key(&display_id)
        || !config
            .enabled_panels
            .iter()
            .any(|panel| panel == "wActionCenter")
    {
        return false;
    }
    let Some(display) = cx.displays().into_iter().find(|d| d.id() == display_id) else {
        return false;
    };
    let size = display.bounds().size;
    let theme = theme.clone();
    let events = events.clone();
    let controller = controller.clone();
    let bottom = config.waffle.bar.bottom.unwrap_or(false);
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size,
        })),
        app_id: Some("intoo-gpui-waffle-action-center".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-waffle-action-center".into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(Pixels::ZERO),
            exclusive_edge: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| {
            crate::families::waffle::action_center_overlay::WaffleActionCenterOverlay::new(
                theme,
                events,
                controller,
                display_id,
                bottom,
                (f32::from(size.height) - 64.).max(1.),
                cx,
            )
        })
    }) {
        Ok(handle) => {
            overlays.borrow_mut().insert(display_id, handle);
            true
        }
        Err(error) => {
            eprintln!("Could not open Waffle Action Center on {display_id:?}: {error:#}");
            false
        }
    }
}

fn open_task_view_overlay(
    cx: &mut App,
    overlays: &TaskViewOverlayMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    controller: &Rc<RefCell<TaskViewController>>,
    events: &Sender<TaskViewEvent>,
) -> bool {
    if overlays.borrow().contains_key(&display_id)
        || !config
            .enabled_panels
            .iter()
            .any(|panel| panel == "wTaskView")
    {
        return false;
    }
    let Some(display) = cx.displays().into_iter().find(|d| d.id() == display_id) else {
        return false;
    };
    let bounds = display.bounds();
    let mode = match config.waffle.task_view.mode.as_deref() {
        Some("carousel") => crate::families::waffle::task_view::TaskViewMode::Carousel,
        _ => crate::families::waffle::task_view::TaskViewMode::Centered,
    };
    let selected_workspace = controller.borrow().selected_workspace(display_id);
    let close_on_select = config.waffle.task_view.close_on_select.unwrap_or(false);
    let theme = theme.clone();
    let output_name = niri_output_name(bounds);
    let events = events.clone();
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-waffle-task-view".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-waffle-task-view".into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(Pixels::ZERO),
            exclusive_edge: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| {
            crate::families::waffle::task_view_overlay::WaffleTaskViewOverlay::new(
                theme,
                output_name,
                display_id,
                mode,
                close_on_select,
                selected_workspace,
                events,
                cx,
            )
        })
    }) {
        Ok(handle) => {
            overlays.borrow_mut().insert(display_id, handle);
            true
        }
        Err(error) => {
            eprintln!("Could not open Waffle Task View on {display_id:?}: {error:#}");
            false
        }
    }
}

fn open_notification_overlay(
    cx: &mut App,
    overlays: &NotificationOverlayMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    events: &Sender<NotificationCenterEvent>,
) {
    if overlays.borrow().contains_key(&display_id)
        || !config
            .enabled_panels
            .iter()
            .any(|panel| panel == "wNotificationCenter")
    {
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let size = display.bounds().size;
    let bar_height = px(48.);
    let bottom = config.waffle.bar.bottom.unwrap_or(false);
    let available_height = (size.height - bar_height - px(24.)).max(px(1.));
    let close_events = events.clone();
    let on_close: crate::families::waffle::notification_center::Action =
        std::sync::Arc::new(move || {
            let _ = close_events.try_send(NotificationCenterEvent::Close {
                display: display_id,
            });
        });
    let input = crate::families::waffle::notification_overlay::unavailable_input(
        theme,
        available_height,
        std::sync::Arc::clone(&on_close),
    );
    let anchor = crate::families::waffle::notification_overlay::NotificationOverlayAnchor {
        display_size: size,
        bar_bottom: bottom,
        bar_height,
    };
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size,
        })),
        app_id: Some("intoo-gpui-waffle-notification-center".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-waffle-notification-center".into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(Pixels::ZERO),
            exclusive_edge: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| {
            crate::families::waffle::notification_overlay::WaffleNotificationOverlay::new(
                input, anchor, on_close, cx,
            )
        })
    }) {
        Ok(handle) => {
            overlays.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!("Could not open Waffle Notification Center on {display_id:?}: {error:#}")
        }
    }
}

fn open_for_display(
    cx: &mut App,
    surfaces: &SurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    panel_events: &Sender<IiPanelEvent>,
) {
    if surfaces.borrow().contains_key(&display_id) {
        return;
    }

    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let display_bounds = display.bounds();
    let bar_height = config.bar.height.unwrap_or(48.0).clamp(24.0, 128.0) as f32;
    let bottom = config.bar.bottom.unwrap_or(false);
    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: Size::new(display_bounds.size.width, px(bar_height)),
    };

    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-prototype".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-prototype".to_string(),
            anchor: if bottom {
                Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
            } else {
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT
            },
            exclusive_zone: Some(px(bar_height)),
            exclusive_edge: Some(if bottom { Anchor::BOTTOM } else { Anchor::TOP }),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    };

    let output_name = niri_output_name(display_bounds);
    let composition = match (ConfiguredFamilyInputs {
        output_name,
        config: config.clone(),
        theme: theme.clone(),
    })
    .create_composition(FamilyRuntimeInputs::IiHorizontal {
        display_id,
        panel_events: panel_events.clone(),
    }) {
        Ok(composition) => composition,
        Err(error) => {
            eprintln!("GPUI Material plugin: could not create horizontal composition: {error:?}");
            return;
        }
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| FamilyHost::new(composition, cx))
    }) {
        Ok(handle) => {
            surfaces.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!("Could not create GPUI layer surface for {display_id:?}: {error:#}");
        }
    }
}

fn open_vertical_for_display(
    cx: &mut App,
    surfaces: &VerticalSurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    panel_events: &Sender<IiPanelEvent>,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
    media_cache: &MediaStateCache,
) {
    if surfaces.borrow().contains_key(&display_id) {
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let right = config.bar.bottom.unwrap_or(false);
    // The visual width matches QML `verticalBarWidth`; the exclusive zone
    // intentionally remains the narrower `baseVerticalBarWidth` (46 px).
    let width = px(64.0);
    let exclusive_width = px(46.0);
    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: Size::new(width, display.bounds().size.height),
    };
    let edge = if right { Anchor::RIGHT } else { Anchor::LEFT };
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-vertical-bar-prototype".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-vertical-bar".to_string(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::BOTTOM | edge,
            exclusive_zone: Some(exclusive_width),
            exclusive_edge: Some(edge),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    };
    let output_name = niri_output_name(display.bounds());
    let composition = match (ConfiguredFamilyInputs {
        output_name,
        config: config.clone(),
        theme: theme.clone(),
    })
    .create_composition(FamilyRuntimeInputs::IiVertical {
        display_id,
        panel_events: panel_events.clone(),
        media_actions: media_actions.clone(),
    }) {
        Ok(composition) => composition,
        Err(error) => {
            eprintln!("GPUI Material plugin: could not create vertical composition: {error:?}");
            return;
        }
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| FamilyHost::new(composition, cx))
    }) {
        Ok(handle) => {
            let media = media_cache.borrow().clone();
            let _ = handle.update(cx, |host, _, cx| host.set_media_state(media, cx));
            surfaces.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!("Could not create GPUI vertical layer surface for {display_id:?}: {error:#}");
        }
    }
}

fn close_vertical_for_display(cx: &mut App, surfaces: &VerticalSurfaceMap, display_id: DisplayId) {
    let Some(handle) = surfaces.borrow_mut().remove(&display_id) else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| window.remove_window()) {
        eprintln!("Could not close GPUI vertical layer surface for {display_id:?}: {error:#}");
    }
}

fn resize_vertical_for_display(
    cx: &mut App,
    surfaces: &VerticalSurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    panel_events: &Sender<IiPanelEvent>,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
    media_cache: &MediaStateCache,
) {
    let Some(handle) = surfaces.borrow().get(&display_id).copied() else {
        open_vertical_for_display(
            cx,
            surfaces,
            display_id,
            config,
            theme,
            panel_events,
            media_actions,
            media_cache,
        );
        return;
    };
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| {
        window.resize(Size::new(px(64.0), display.bounds().size.height));
    }) {
        eprintln!("Could not resize GPUI vertical layer surface for {display_id:?}: {error:#}");
        close_vertical_for_display(cx, surfaces, display_id);
        open_vertical_for_display(
            cx,
            surfaces,
            display_id,
            config,
            theme,
            panel_events,
            media_actions,
            media_cache,
        );
    }
}

fn niri_output_name(bounds: Bounds<Pixels>) -> Option<String> {
    let fresh_outputs = NIRI_OUTPUT_SNAPSHOT
        .get_or_init(|| RwLock::new(NiriOutputSnapshotState::default()))
        .read()
        .ok()
        .and_then(|snapshot| {
            snapshot
                .last_success
                .as_ref()
                .and_then(|(sampled, outputs)| {
                    (sampled.elapsed() < Duration::from_secs(1)).then(|| outputs.clone())
                })
        });
    if fresh_outputs.is_none() {
        refresh_niri_output_snapshot();
    }
    let outputs = fresh_outputs?;
    let close = |a: f64, b: f64| (a - b).abs() < 1.0;
    outputs.into_iter().find_map(|output| {
        (close(output.x, f64::from(f32::from(bounds.origin.x)))
            && close(output.y, f64::from(f32::from(bounds.origin.y)))
            && close(output.width, f64::from(f32::from(bounds.size.width)))
            && close(output.height, f64::from(f32::from(bounds.size.height))))
        .then_some(output.name)
    })
}

/// Schedule a bounded background query. Surface reconciliation only reads the
/// shared snapshot, so a stalled Niri command cannot block GPUI's event loop.
fn refresh_niri_output_snapshot() {
    if NIRI_OUTPUT_REFRESH_IN_FLIGHT.swap(true, Ordering::AcqRel) {
        return;
    }
    let attempted = Instant::now();
    if let Ok(mut snapshot) = NIRI_OUTPUT_SNAPSHOT
        .get_or_init(|| RwLock::new(NiriOutputSnapshotState::default()))
        .write()
    {
        if snapshot
            .last_attempt
            .is_some_and(|previous| previous.elapsed() < Duration::from_secs(1))
        {
            NIRI_OUTPUT_REFRESH_IN_FLIGHT.store(false, Ordering::Release);
            return;
        }
        snapshot.last_attempt = Some(attempted);
    }
    thread::spawn(|| {
        let result = read_niri_output_snapshot_bounded();
        let mut failure_report = None;
        if let Ok(mut snapshot) = NIRI_OUTPUT_SNAPSHOT
            .get_or_init(|| RwLock::new(NiriOutputSnapshotState::default()))
            .write()
        {
            match result {
                Ok(outputs) => {
                    if snapshot.consecutive_failures > 0 {
                        eprintln!("GPUI output map: Niri output query recovered");
                    }
                    snapshot.last_success = Some((Instant::now(), outputs));
                    snapshot.consecutive_failures = 0;
                    snapshot.last_error = None;
                }
                Err(error) => {
                    snapshot.consecutive_failures = snapshot.consecutive_failures.saturating_add(1);
                    snapshot.last_error = Some(error.clone());
                    let last_success_age = snapshot
                        .last_success
                        .as_ref()
                        .map(|(sampled, _)| format!("{:.1}s", sampled.elapsed().as_secs_f64()))
                        .unwrap_or_else(|| "none".into());
                    let retry_exponent = snapshot.consecutive_failures.saturating_sub(1).min(4);
                    let retry_after = Duration::from_millis(1_100 * (1_u64 << retry_exponent));
                    failure_report = Some((
                        error,
                        snapshot.consecutive_failures,
                        last_success_age,
                        retry_after,
                    ));
                }
            }
        }
        NIRI_OUTPUT_REFRESH_IN_FLIGHT.store(false, Ordering::Release);
        if let Some((error, failures, last_success_age, retry_after)) = failure_report {
            eprintln!(
                "GPUI output map: Niri query failed ({error}); consecutive failures={failures}, last success age={last_success_age}; retrying in {:.1}s; stale output routing is disabled and targets fall back to all displays",
                retry_after.as_secs_f64()
            );
            thread::spawn(move || {
                thread::sleep(retry_after);
                refresh_niri_output_snapshot();
            });
        }
        if let Some(sender) = NIRI_OUTPUT_REFRESH_SENDER.get() {
            let _ = sender.try_send(());
        }
    });
}

fn read_niri_output_snapshot_bounded() -> Result<Vec<NiriOutputGeometry>, String> {
    use std::process::{Command, Stdio};

    let mut child = Command::new("niri")
        .args(["msg", "-j", "outputs"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start niri: {error}"))?;
    let deadline = Instant::now() + Duration::from_millis(400);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(status)) => return Err(format!("niri exited with {status}")),
            Err(error) => return Err(format!("could not poll niri: {error}")),
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("niri output query timed out after 400ms".into());
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not read niri output: {error}"))?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid niri output JSON: {error}"))?;
    let outputs = value
        .as_object()
        .ok_or_else(|| "niri output JSON was not an object".to_owned())?;
    Ok(outputs
        .iter()
        .filter_map(|(name, details)| {
            let logical = details.get("logical")?;
            Some(NiriOutputGeometry {
                name: name.clone(),
                x: logical.get("x")?.as_f64()?,
                y: logical.get("y")?.as_f64()?,
                width: logical.get("width")?.as_f64()?,
                height: logical.get("height")?.as_f64()?,
            })
        })
        .collect())
}

fn open_background_for_display(
    cx: &mut App,
    backgrounds: &BackgroundMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
) {
    if config.background.backdrop.enabled.unwrap_or(true)
        && config.background.backdrop.hide_wallpaper.unwrap_or(false)
    {
        return;
    }
    if backgrounds.borrow().contains_key(&display_id) {
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let display_bounds = display.bounds();
    let output_name = niri_output_name(display_bounds);
    let wallpaper_path = wallpaper_path(config, output_name.as_deref());
    let status = match wallpaper_path.as_ref() {
        Some(path) if path.is_file() => WallpaperStatus::Ready,
        _ => WallpaperStatus::Missing,
    };
    let props = DesktopBackgroundProps {
        output_name: output_name.unwrap_or_default(),
        wallpaper_path,
        decoded_image: None,
        status,
        colors: crate::components::desktop_background::DesktopBackgroundColors {
            background: theme.colors.background,
            foreground: theme.colors.foreground,
        },
    };
    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: display_bounds.size,
    };
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-background-prototype".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-background".to_string(),
            layer: Layer::Bottom,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(px(0.)),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|_| DesktopBackground::new(props.clone()))
    }) {
        Ok(handle) => {
            backgrounds.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!("Could not create GPUI background surface for {display_id:?}: {error:#}");
        }
    }
}

fn open_backdrop_for_display(
    cx: &mut App,
    backdrops: &BackdropMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
) {
    if backdrops.borrow().contains_key(&display_id)
        || !config.background.backdrop.enabled.unwrap_or(true)
    {
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let output_name = niri_output_name(display.bounds());
    let wallpaper_path = backdrop_wallpaper_path(config, output_name.as_deref());
    let wallpaper_kind = classify_wallpaper(wallpaper_path.as_deref());
    let status = match wallpaper_path.as_ref() {
        Some(path) if path.is_file() => BackdropStatus::Ready,
        _ => BackdropStatus::Missing,
    };
    let backdrop = &config.background.backdrop;
    let vignette = backdrop
        .vignette_enabled
        .unwrap_or(false)
        .then_some(Vignette {
            intensity: backdrop.vignette_intensity.unwrap_or(1.) as f32,
            radius: backdrop.vignette_radius.unwrap_or(0.7) as f32,
        });
    let input = DesktopBackdropInput {
        output_name: output_name.unwrap_or_default(),
        wallpaper_path,
        decoded_image: None,
        status,
        wallpaper_kind,
        use_aurora_style: backdrop.use_aurora_style.unwrap_or(false),
        aurora_overlay_opacity: backdrop.aurora_overlay_opacity.unwrap_or(0.38) as f32,
        enable_animation: backdrop.enable_animation.unwrap_or(false),
        dim_percent: backdrop.dim.unwrap_or(40.) as f32,
        saturation: backdrop.saturation.unwrap_or(0.) as f32,
        contrast: backdrop.contrast.unwrap_or(0.) as f32,
        blur_radius: backdrop.blur_radius.unwrap_or(40.) as f32,
        vignette,
        reduced_effects: false,
        background: theme
            .colors
            .background
            .unwrap_or_else(|| gpui::rgba(0x101014ff).into()),
        foreground: theme
            .colors
            .foreground
            .unwrap_or_else(|| gpui::rgba(0xffffffff).into()),
    };
    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: display.bounds().size,
    };
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-backdrop-prototype".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-backdrop".to_string(),
            layer: Layer::Background,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(px(0.)),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|_| DesktopBackdrop::new(input.clone()))
    }) {
        Ok(handle) => {
            let _ = handle.update(cx, |_, window: &mut Window, _| {
                window.set_input_region(Some(&[]));
            });
            backdrops.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!("Could not create GPUI backdrop surface for {display_id:?}: {error:#}");
        }
    }
}

fn backdrop_wallpaper_path(
    config: &ShellConfig,
    output_name: Option<&str>,
) -> Option<std::path::PathBuf> {
    let backdrop = &config.background.backdrop;
    if backdrop.use_main_wallpaper.unwrap_or(true) {
        return wallpaper_path(config, output_name);
    };
    let per_output = config
        .background
        .multi_monitor_enabled
        .unwrap_or(false)
        .then(|| {
            output_name.and_then(|name| {
                config
                    .background
                    .wallpapers_by_monitor
                    .iter()
                    .find(|entry| entry.monitor.as_deref() == Some(name))
                    .and_then(|entry| entry.backdrop_path.as_deref())
            })
        })
        .flatten()
        .filter(|path| !path.is_empty());
    let configured = per_output.or(backdrop
        .wallpaper_path
        .as_deref()
        .filter(|path| !path.is_empty()));
    configured
        .and_then(|configured| {
            let path = configured.strip_prefix("file://").unwrap_or(configured);
            (!path.is_empty()).then(|| std::path::PathBuf::from(path))
        })
        .or_else(|| wallpaper_path(config, output_name))
}

fn classify_wallpaper(path: Option<&std::path::Path>) -> WallpaperKind {
    let Some(extension) = path
        .and_then(std::path::Path::extension)
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase)
    else {
        return WallpaperKind::Still;
    };
    match extension.as_str() {
        "mp4" | "webm" | "mkv" | "avi" | "mov" => WallpaperKind::Video,
        "gif" => WallpaperKind::AnimatedImage,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" => WallpaperKind::Still,
        _ => WallpaperKind::Unsupported,
    }
}

fn close_backdrop_for_display(cx: &mut App, backdrops: &BackdropMap, display_id: DisplayId) {
    let Some(handle) = backdrops.borrow_mut().remove(&display_id) else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| window.remove_window()) {
        eprintln!("Could not close GPUI backdrop surface for {display_id:?}: {error:#}");
    }
}

fn wallpaper_path(config: &ShellConfig, output_name: Option<&str>) -> Option<std::path::PathBuf> {
    let configured = if config.background.multi_monitor_enabled.unwrap_or(false) {
        output_name
            .and_then(|name| {
                config
                    .background
                    .wallpapers_by_monitor
                    .iter()
                    .find(|entry| entry.monitor.as_deref() == Some(name))
                    .and_then(|entry| entry.path.as_deref())
            })
            .filter(|path| !path.is_empty())
            .or(config.background.wallpaper_path.as_deref())
    } else {
        config.background.wallpaper_path.as_deref()
    }?;
    let path = configured.strip_prefix("file://").unwrap_or(configured);
    (!path.is_empty()).then(|| std::path::PathBuf::from(path))
}

fn close_background_for_display(cx: &mut App, backgrounds: &BackgroundMap, display_id: DisplayId) {
    let Some(handle) = backgrounds.borrow_mut().remove(&display_id) else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| window.remove_window()) {
        eprintln!("Could not close GPUI background surface for {display_id:?}: {error:#}");
    }
}

fn open_ii_dock_for_display(
    cx: &mut App,
    surfaces: &IiDockSurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
) {
    if surfaces.borrow().contains_key(&display_id)
        || !config.enabled_panels.iter().any(|p| p == "iiDock")
    {
        return;
    }
    let Some(options) = config.appearance.source_sections.get("ii.dock") else {
        return;
    };
    if !options
        .get("enable")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return;
    }
    let screens = options
        .get("screenList")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !ii_panel_screen_enabled(cx, display_id, &screens) {
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let size = display.bounds().size;
    let output = niri_output_name(display.bounds());
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size,
        })),
        app_id: Some("intoo-gpui-ii-dock".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-ii-dock".into(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(Pixels::ZERO),
            exclusive_edge: None,
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            ..Default::default()
        }),
        ..Default::default()
    };
    let config = config.clone();
    let theme = theme.clone();
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| IiDockSurface::new(output, display_id, size, config, theme, cx))
    }) {
        Ok(handle) => {
            surfaces.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!("Could not create GPUI ii Dock surface for {display_id:?}: {error:#}")
        }
    }
}

fn close_ii_dock_for_display(cx: &mut App, surfaces: &IiDockSurfaceMap, display_id: DisplayId) {
    if let Some(handle) = surfaces.borrow_mut().remove(&display_id) {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

fn resize_ii_dock_for_display(
    cx: &mut App,
    surfaces: &IiDockSurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
) {
    let enabled = config.enabled_panels.iter().any(|p| p == "iiDock")
        && config
            .appearance
            .source_sections
            .get("ii.dock")
            .and_then(|v| v.get("enable"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
    if !enabled {
        close_ii_dock_for_display(cx, surfaces, display_id);
        return;
    }
    let screens = config
        .appearance
        .source_sections
        .get("ii.dock")
        .and_then(|options| options.get("screenList"))
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !ii_panel_screen_enabled(cx, display_id, &screens) {
        close_ii_dock_for_display(cx, surfaces, display_id);
        return;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        close_ii_dock_for_display(cx, surfaces, display_id);
        return;
    };
    let size = display.bounds().size;
    if let Some(handle) = surfaces.borrow().get(&display_id).copied() {
        if handle
            .update(cx, |view, window, cx| {
                view.resize(size, cx);
                window.resize(size);
            })
            .is_err()
        {
            close_ii_dock_for_display(cx, surfaces, display_id);
        }
    } else {
        open_ii_dock_for_display(cx, surfaces, display_id, config, theme);
    }
}

fn resize_background_for_display(
    cx: &mut App,
    backgrounds: &BackgroundMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
) {
    let Some(handle) = backgrounds.borrow().get(&display_id).copied() else {
        open_background_for_display(cx, backgrounds, display_id, config, theme);
        return;
    };
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| {
        window.resize(display.bounds().size);
    }) {
        eprintln!("Could not resize GPUI background surface for {display_id:?}: {error:#}");
        close_background_for_display(cx, backgrounds, display_id);
        open_background_for_display(cx, backgrounds, display_id, config, theme);
    }
}

fn close_for_display(cx: &mut App, surfaces: &SurfaceMap, display_id: DisplayId) {
    let Some(handle) = surfaces.borrow_mut().remove(&display_id) else {
        return;
    };

    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| window.remove_window()) {
        eprintln!("Could not close GPUI layer surface for {display_id:?}: {error:#}");
    }
}

fn resize_for_display(
    cx: &mut App,
    surfaces: &SurfaceMap,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
    panel_events: &Sender<IiPanelEvent>,
) {
    let Some(handle) = surfaces.borrow().get(&display_id).copied() else {
        open_for_display(cx, surfaces, display_id, config, theme, panel_events);
        return;
    };
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };

    if let Err(error) = handle.update(cx, |_, window: &mut Window, _| {
        let height = px(config.bar.height.unwrap_or(48.0).clamp(24.0, 128.0) as f32);
        window.resize(Size::new(display.bounds().size.width, height));
    }) {
        eprintln!("Could not resize GPUI layer surface for {display_id:?}: {error:#}");
        close_for_display(cx, surfaces, display_id);
        open_for_display(cx, surfaces, display_id, config, theme, panel_events);
    }
}
