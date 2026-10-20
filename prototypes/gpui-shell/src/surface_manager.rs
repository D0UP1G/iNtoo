use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
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
    App, AppContext, Bounds, ClipboardItem, DisplayChanges, DisplayEvent, DisplayId, IntoElement,
    Pixels, Render, Size, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, layer_shell::*, point, px,
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
use crate::families::iris::background_host::{
    IrisBackgroundPreviewInput, IrisBackgroundPreviewView,
};
use crate::families::iris::background_state::{MotionRuntimeInputs, resolve_output_background};
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
type IrisBackgroundPreviewMap =
    Rc<RefCell<HashMap<DisplayId, WindowHandle<IrisBackgroundPreviewView>>>>;
type IrisOutputMappingPending = Rc<RefCell<HashMap<DisplayId, Instant>>>;
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
type MediaControlsSurfaceMap = Rc<
    RefCell<
        HashMap<
            DisplayId,
            WindowHandle<crate::components::media_controls_surface::MediaControlsSurface>,
        >,
    >,
>;
type IiNotificationPopupMap = Rc<
    RefCell<
        HashMap<
            DisplayId,
            WindowHandle<crate::families::ii::notification_popup::NotificationPopup>,
        >,
    >,
>;
type IiNotificationGrace = Rc<RefCell<HashMap<DisplayId, u64>>>;
type IiOsdSurfaceMap = Rc<RefCell<HashMap<DisplayId, WindowHandle<IiOsdSurface>>>>;
type IiSessionScreenSurfaceMap = Rc<
    RefCell<
        HashMap<
            DisplayId,
            WindowHandle<crate::families::ii::session_screen_host::SessionScreenHost>,
        >,
    >,
>;
type IiSessionScreenStateMap =
    Rc<RefCell<HashMap<DisplayId, crate::families::ii::session_screen_state::SessionScreenState>>>;
type IiSessionScreenGenerationMap = Rc<RefCell<HashMap<DisplayId, u64>>>;
type IiClipboardSurfaceMap = Rc<
    RefCell<HashMap<DisplayId, WindowHandle<crate::families::ii::clipboard_host::IiClipboardHost>>>,
>;
type IiClipboardGenerationMap = Rc<RefCell<HashMap<DisplayId, u64>>>;
type IiClipboardPinsStore = Rc<RefCell<Vec<String>>>;
type IiCheatsheetSurfaceMap = Rc<
    RefCell<
        HashMap<DisplayId, WindowHandle<crate::families::ii::cheatsheet_host::IiCheatsheetHost>>,
    >,
>;
pub type SessionActionHandler = std::sync::Arc<
    dyn Fn(crate::families::ii::session_screen::SessionAction) -> bool + Send + Sync,
>;

#[derive(Clone)]
pub(crate) enum IiSessionScreenEvent {
    Toggle {
        display: DisplayId,
    },
    Open {
        display: DisplayId,
        handler: Option<SessionActionHandler>,
    },
    View {
        display: DisplayId,
        event: crate::families::ii::session_screen::SessionScreenEvent,
    },
    CloseElapsed {
        display: DisplayId,
        generation: u64,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum IiClipboardSurfaceEvent {
    Open { display: DisplayId },
    Toggle { display: DisplayId },
    Dismiss { display: DisplayId, generation: u64 },
    CloseElapsed { display: DisplayId, generation: u64 },
}

#[derive(Clone, Debug)]
pub(crate) enum IiCheatsheetEvent {
    Open {
        display: DisplayId,
    },
    Close {
        display: DisplayId,
    },
    Query {
        display: DisplayId,
        query: String,
    },
    SetPage {
        display: DisplayId,
        page: IiCheatsheetPage,
    },
    CopyElement {
        display: DisplayId,
        symbol: String,
    },
    CopyFeedbackElapsed {
        display: DisplayId,
        generation: u64,
    },
    Loaded {
        display: DisplayId,
        generation: u64,
        snapshot: crate::families::ii::cheatsheet::KeybindSnapshot,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IiCheatsheetPage {
    Keybinds,
    Elements,
}

static II_CHEATSHEET_SENDER: OnceLock<Sender<IiCheatsheetEvent>> = OnceLock::new();

/// Open the Material ii keybind page as a per-display overlay for explicit UI
/// preview/testing. No production keybind producer is connected yet.
pub fn open_ii_cheatsheet_preview(display: DisplayId) -> bool {
    II_CHEATSHEET_SENDER
        .get()
        .is_some_and(|sender| sender.try_send(IiCheatsheetEvent::Open { display }).is_ok())
}

static II_CLIPBOARD_SENDER: OnceLock<Sender<IiClipboardSurfaceEvent>> = OnceLock::new();

/// Open the ii clipboard preview on exactly the supplied output. This is an
/// opt-in test seam; no production shortcut or launcher trigger is wired yet.
pub fn open_ii_clipboard_preview(display: DisplayId) -> bool {
    II_CLIPBOARD_SENDER.get().is_some_and(|sender| {
        sender
            .try_send(IiClipboardSurfaceEvent::Open { display })
            .is_ok()
    })
}

/// Toggle the prototype clipboard presentation on one explicit output.
/// This is a callable GPUI seam only; no keyboard/IPC production event emits it yet.
pub fn toggle_ii_clipboard_preview(display: DisplayId) -> bool {
    II_CLIPBOARD_SENDER.get().is_some_and(|sender| {
        sender
            .try_send(IiClipboardSurfaceEvent::Toggle { display })
            .is_ok()
    })
}

static II_SESSION_SCREEN_SENDER: OnceLock<Sender<IiSessionScreenEvent>> = OnceLock::new();

/// Toggle the non-secure ii session menu on the explicitly supplied output.
/// Session actions remain unavailable until a real handler is injected.
pub fn toggle_ii_session_screen(display: DisplayId) -> bool {
    II_SESSION_SCREEN_SENDER.get().is_some_and(|sender| {
        sender
            .try_send(IiSessionScreenEvent::Toggle { display })
            .is_ok()
    })
}

/// Open the ii session menu on an explicitly supplied output with an injected
/// action handler. Returning `true` from the handler acknowledges that the
/// request was accepted and permits the UI's 250 ms close grace period.
pub fn open_ii_session_screen_with_handler(
    display: DisplayId,
    handler: SessionActionHandler,
) -> bool {
    II_SESSION_SCREEN_SENDER.get().is_some_and(|sender| {
        sender
            .try_send(IiSessionScreenEvent::Open {
                display,
                handler: Some(handler),
            })
            .is_ok()
    })
}

#[derive(Clone)]
struct IiOsdTrigger {
    indicator: crate::families::ii::osd::OsdIndicator,
    availability: crate::families::ii::osd::OsdAvailability,
    auto_hide: bool,
}

#[derive(Clone)]
enum IiOsdHostEvent {
    Trigger(IiOsdTrigger),
    Hide,
    Hover { display: DisplayId, entered: bool },
}

static II_OSD_EVENT_SENDER: OnceLock<async_channel::Sender<IiOsdHostEvent>> = OnceLock::new();

/// Request an ii OSD presentation from a future GPUI provider or action.
/// Returns false until the ii host has initialized or when its event loop ended.
pub(crate) fn trigger_ii_osd(
    indicator: crate::families::ii::osd::OsdIndicator,
    availability: crate::families::ii::osd::OsdAvailability,
    auto_hide: bool,
) -> bool {
    II_OSD_EVENT_SENDER.get().is_some_and(|sender| {
        sender
            .try_send(IiOsdHostEvent::Trigger(IiOsdTrigger {
                indicator,
                availability,
                auto_hide,
            }))
            .is_ok()
    })
}

pub(crate) fn hide_ii_osd() -> bool {
    II_OSD_EVENT_SENDER
        .get()
        .is_some_and(|sender| sender.try_send(IiOsdHostEvent::Hide).is_ok())
}

struct IiOsdSurface {
    input: crate::families::ii::osd::OsdInput,
}

impl Render for IiOsdSurface {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        crate::families::ii::osd::render(self.input.clone())
    }
}

impl IiOsdSurface {
    fn set_input(
        &mut self,
        input: crate::families::ii::osd::OsdInput,
        cx: &mut gpui::Context<Self>,
    ) {
        self.input = input;
        cx.notify();
    }
}

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
    last_success_attempt: Option<Instant>,
    last_attempt: Option<Instant>,
    consecutive_failures: u32,
    last_error: Option<String>,
}

static NIRI_OUTPUT_SNAPSHOT: OnceLock<RwLock<NiriOutputSnapshotState>> = OnceLock::new();
static NIRI_OUTPUT_REFRESH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static NIRI_OUTPUT_REFRESH_SENDER: OnceLock<async_channel::Sender<()>> = OnceLock::new();
static GPUI_APP_LAUNCHER: OnceLock<Sender<StartMenuEvent>> = OnceLock::new();

pub fn toggle_app_launcher(display: DisplayId) {
    if let Some(sender) = GPUI_APP_LAUNCHER.get() {
        if sender.try_send(StartMenuEvent::Toggle { display }).is_err() {
            eprintln!("GPUI app launcher: could not send toggle event");
        }
    } else {
        eprintln!("GPUI app launcher is not initialized");
    }
}

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
    start_with_iris_background_preview(cx, None);
}

/// Start the shell and optionally open an explicit Iris wallpaper preview on
/// the display selected by the caller. This is a test route only; it does not
/// infer an output from Iris's bar screen list or install the QML lifecycle.
pub fn start_with_iris_background_preview(cx: &mut App, preview_display_index: Option<usize>) {
    let surfaces: SurfaceMap = Rc::default();
    let vertical_surfaces: VerticalSurfaceMap = Rc::default();
    let backgrounds: BackgroundMap = Rc::default();
    let iris_background_previews: IrisBackgroundPreviewMap = Rc::default();
    let iris_output_mapping_pending: IrisOutputMappingPending = Rc::default();
    let backdrops: BackdropMap = Rc::default();
    let waffle_backgrounds: WaffleBackgroundMap = Rc::default();
    let waffle_backdrops: WaffleBackdropMap = Rc::default();
    let waffle_output_names: WaffleOutputNameMap = Rc::default();
    let ii_panel_surfaces: IiPanelSurfaceMap = Rc::default();
    let ii_dock_surfaces: IiDockSurfaceMap = Rc::default();
    let ii_output_names: IiOutputNameMap = Rc::default();
    let ii_overview_overlays: IiOverviewOverlayMap = Rc::default();
    let ii_panel_states: IiPanelStateMap = Rc::default();
    let ii_osd_surfaces: IiOsdSurfaceMap = Rc::default();
    let ii_osd_generation = Rc::new(Cell::new(0_u64));
    let ii_osd_active: Rc<RefCell<Option<IiOsdTrigger>>> = Rc::default();
    let (ii_osd_events, ii_osd_receiver) = async_channel::unbounded::<IiOsdHostEvent>();
    if II_OSD_EVENT_SENDER.set(ii_osd_events.clone()).is_err() {
        eprintln!("GPUI ii OSD trigger seam was already initialized");
    }
    let (media_actions, media_action_receiver) = async_channel::unbounded();
    let media_cache: MediaStateCache =
        Rc::new(RefCell::new(crate::vertical_provider::unavailable_media()));
    let media_controls_surfaces: MediaControlsSurfaceMap = Rc::default();
    let media_controls_visible = Rc::new(Cell::new(false));
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
    let overlays: OverlayMap = Rc::default();
    let notification_overlays: NotificationOverlayMap = Rc::default();
    let ii_notification_popups: IiNotificationPopupMap = Rc::default();
    let ii_notification_grace: IiNotificationGrace = Rc::default();
    let ii_notification_generation = Rc::new(Cell::new(0_u64));
    let task_view_overlays: TaskViewOverlayMap = Rc::default();
    let action_center_overlays: ActionCenterOverlayMap = Rc::default();
    let clipboard_overlays: ClipboardOverlayMap = Rc::default();
    let (waffle_events, waffle_receiver) = async_channel::unbounded::<StartMenuEvent>();
    let _ = GPUI_APP_LAUNCHER.set(waffle_events.clone());
    let (notification_events, notification_receiver) =
        async_channel::unbounded::<NotificationCenterEvent>();
    let (notification_group_events, notification_group_receiver) =
        async_channel::unbounded::<(DisplayId, String)>();
    let notification_group_expanded: Rc<RefCell<HashMap<DisplayId, HashSet<String>>>> =
        Rc::default();
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
    let mut theme =
        theme_resolver::resolve_for_config(&loaded, palette.as_ref(), metadata.as_ref());
    if loaded.config.gpui_mvp.configured {
        crate::components::top_bar::apply_mvp_theme(&mut theme, &loaded.config.gpui_mvp);
    }
    // Notification name ownership is a separate explicit opt-in from opening
    // the UI, and starts only after the selected environment/config is valid.
    let (notification_service, notification_service_receiver) =
        crate::notification_service::NotificationServiceHandle::start_if_opted_in();
    let notification_service_snapshot = Rc::new(RefCell::new(notification_service.snapshot()));
    let notification_service_shutdown = notification_service.clone();
    cx.on_app_quit(move |_| {
        let service = notification_service_shutdown.clone();
        async move { service.shutdown() }
    })
    .detach();
    let critical_config: Rc<RefCell<IiCriticalSnapshot>> =
        Rc::new(RefCell::new(IiCriticalSnapshot {
            revision: 0,
            loaded: loaded.clone(),
            theme: theme.clone(),
        }));

    if let Some(index) = preview_display_index {
        let displays = cx.displays();
        if let Some(display) = displays.get(index) {
            open_iris_background_preview_for_display(
                cx,
                &iris_background_previews,
                None,
                display.id(),
                &loaded.config,
                &theme,
            );
        } else {
            eprintln!(
                "Iris background preview: display index {index} is unavailable ({} output(s))",
                displays.len()
            );
        }
    }

    let (ii_session_screen_events, ii_session_screen_receiver) =
        async_channel::unbounded::<IiSessionScreenEvent>();
    let _ = II_SESSION_SCREEN_SENDER.set(ii_session_screen_events.clone());
    let ii_session_screen_surfaces: IiSessionScreenSurfaceMap = Rc::default();
    let ii_session_screen_states: IiSessionScreenStateMap = Rc::default();
    let ii_session_screen_generations: IiSessionScreenGenerationMap = Rc::default();
    let ii_session_screen_handlers: Rc<RefCell<HashMap<DisplayId, SessionActionHandler>>> =
        Rc::default();
    let session_screen_surfaces = ii_session_screen_surfaces.clone();
    let session_screen_states = ii_session_screen_states.clone();
    let session_screen_generations = ii_session_screen_generations.clone();
    let session_screen_handlers = ii_session_screen_handlers.clone();
    let session_screen_config = critical_config.clone();
    let session_screen_sender = ii_session_screen_events.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = ii_session_screen_receiver.recv().await {
            let timer_sender = session_screen_sender.clone();
            cx.update(|cx| {
                if let Some((display, generation)) = handle_ii_session_screen_event(
                    cx,
                    event,
                    &session_screen_surfaces,
                    &session_screen_states,
                    &session_screen_generations,
                    &session_screen_handlers,
                    &session_screen_config,
                    &session_screen_sender,
                ) {
                    let executor = cx.background_executor().clone();
                    let timer_executor = executor.clone();
                    executor
                        .spawn(async move {
                            timer_executor.timer(Duration::from_millis(250)).await;
                            timer_sender
                                .send(IiSessionScreenEvent::CloseElapsed {
                                    display,
                                    generation,
                                })
                                .await
                                .ok();
                        })
                        .detach();
                }
            });
        }
    })
    .detach();

    let (ii_clipboard_events, ii_clipboard_receiver) =
        async_channel::unbounded::<IiClipboardSurfaceEvent>();
    let _ = II_CLIPBOARD_SENDER.set(ii_clipboard_events.clone());
    let ii_clipboard_surfaces: IiClipboardSurfaceMap = Rc::default();
    let ii_clipboard_generations: IiClipboardGenerationMap = Rc::default();
    let ii_clipboard_pins: IiClipboardPinsStore =
        Rc::new(RefCell::new(config.clipboard_pinned.clone()));
    let clipboard_surfaces_for_events = ii_clipboard_surfaces.clone();
    let clipboard_generations_for_events = ii_clipboard_generations.clone();
    let clipboard_pins_for_events = ii_clipboard_pins.clone();
    let clipboard_theme_for_events = theme.clone();
    let clipboard_event_sender = ii_clipboard_events.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = ii_clipboard_receiver.recv().await {
            match event {
                IiClipboardSurfaceEvent::Open { display } => {
                    cx.update(|cx| {
                        // Invalidate any close timer before reusing or creating
                        // the output surface.
                        {
                            let mut generations = clipboard_generations_for_events.borrow_mut();
                            let generation = generations.entry(display).or_insert(0);
                            crate::families::ii::clipboard_host::next_route_generation(
                                generation,
                            );
                        }
                        let existing = clipboard_surfaces_for_events
                            .borrow()
                            .get(&display)
                            .copied();
                        if let Some(handle) = existing {
                            let _ = handle.update(cx, |view, window, cx| view.reopen(window, cx));
                        } else {
                            let pins = clipboard_pins_for_events.borrow().clone();
                            if open_ii_clipboard_surface(
                                cx,
                                &clipboard_surfaces_for_events,
                                &clipboard_generations_for_events,
                                display,
                                &clipboard_theme_for_events,
                                &clipboard_event_sender,
                                pins.clone(),
                            ) {
                                *clipboard_pins_for_events.borrow_mut() = pins;
                            }
                        }
                    });
                }
                IiClipboardSurfaceEvent::Toggle { display } => {
                    let existing = clipboard_surfaces_for_events
                        .borrow()
                        .get(&display)
                        .copied();
                    match crate::families::ii::clipboard_host::clipboard_toggle_route_action(
                        existing.is_some(),
                    ) {
                        crate::families::ii::clipboard_host::ClipboardToggleRouteAction::Dismiss => {
                            let Some(handle) = existing else {
                                continue;
                            };
                        if let Ok(host_generation) =
                            handle.update(cx, |view, _, _| view.generation())
                        {
                            let _ =
                                clipboard_event_sender.try_send(IiClipboardSurfaceEvent::Dismiss {
                                    display,
                                    generation: host_generation,
                                });
                        }
                        }
                        crate::families::ii::clipboard_host::ClipboardToggleRouteAction::Open => {
                            let _ = clipboard_event_sender
                                .try_send(IiClipboardSurfaceEvent::Open { display });
                        }
                    }
                }
                IiClipboardSurfaceEvent::Dismiss {
                    display,
                    generation: host_generation,
                } => {
                    if !ii_clipboard_host_generation_is_current(
                        cx,
                        &clipboard_surfaces_for_events,
                        display,
                        host_generation,
                    ) {
                        continue;
                    }
                    let generation = {
                        let mut generations = clipboard_generations_for_events.borrow_mut();
                        let generation = generations.entry(display).or_insert(0);
                        crate::families::ii::clipboard_host::next_route_generation(generation)
                    };
                    if let Some(handle) = clipboard_surfaces_for_events
                        .borrow()
                        .get(&display)
                        .copied()
                    {
                        let _ = handle
                            .update(cx, |view, window, cx| view.dismiss_for_grace(window, cx));
                    }
                    // Exclusive keyboard interactivity cannot be changed after
                    // layer-shell creation, so release it by destroying the
                    // window. The route generation retains the reopen grace.
                    if let Some(handle) = clipboard_surfaces_for_events
                        .borrow_mut()
                        .remove(&display)
                    {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                    let sender = clipboard_event_sender.clone();
                    let executor = cx.background_executor().clone();
                    let timer_executor = executor.clone();
                    executor
                        .spawn(async move {
                            timer_executor.timer(Duration::from_millis(250)).await;
                            sender
                                .send(IiClipboardSurfaceEvent::CloseElapsed {
                                    display,
                                    generation,
                                })
                                .await
                                .ok();
                        })
                        .detach();
                }
                IiClipboardSurfaceEvent::CloseElapsed {
                    display,
                    generation,
                } => {
                    let is_current = clipboard_generations_for_events
                        .borrow()
                        .get(&display)
                        .copied()
                        .is_some_and(|current| {
                            crate::families::ii::clipboard_host::close_timer_is_current(
                                Some(current),
                                generation,
                            )
                        });
                    if !is_current {
                        continue;
                    }
                    clipboard_generations_for_events
                        .borrow_mut()
                        .remove(&display);
                    if let Some(handle) =
                        clipboard_surfaces_for_events.borrow_mut().remove(&display)
                    {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
            }
        }
    })
    .detach();

    let (ii_cheatsheet_events, ii_cheatsheet_receiver) =
        async_channel::unbounded::<IiCheatsheetEvent>();
    let _ = II_CHEATSHEET_SENDER.set(ii_cheatsheet_events.clone());
    let ii_cheatsheet_surfaces: IiCheatsheetSurfaceMap = Rc::default();
    let cheatsheet_surfaces_for_events = ii_cheatsheet_surfaces.clone();
    let cheatsheet_runtime_state = critical_config.clone();
    let cheatsheet_events_for_host = ii_cheatsheet_events.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = ii_cheatsheet_receiver.recv().await {
            match event {
                IiCheatsheetEvent::Open { display } => {
                    eprintln!("GPUI ii Cheatsheet: received open request for {display:?}");
                    cx.update(|cx| {
                        if cheatsheet_surfaces_for_events
                            .borrow()
                            .contains_key(&display)
                        {
                            return;
                        }
                        let theme = cheatsheet_runtime_state.borrow().theme.clone();
                        if open_ii_cheatsheet_surface(
                            cx,
                            &cheatsheet_surfaces_for_events,
                            display,
                            &theme,
                            &cheatsheet_events_for_host,
                        ) {
                            eprintln!("GPUI ii Cheatsheet: opened overlay for {display:?}");
                            if let Some(handle) = cheatsheet_surfaces_for_events
                                .borrow()
                                .get(&display)
                                .copied()
                            {
                                let _ = handle.update(cx, |host, _, cx| host.refresh_provider(cx));
                            }
                        }
                    });
                }
                IiCheatsheetEvent::Close { display } => {
                    cx.update(|cx| {
                        close_ii_cheatsheet_surface(cx, &cheatsheet_surfaces_for_events, display)
                    });
                }
                IiCheatsheetEvent::Query { display, query } => {
                    if let Some(handle) = cheatsheet_surfaces_for_events
                        .borrow()
                        .get(&display)
                        .copied()
                    {
                        let _ = handle.update(cx, |host, _, cx| host.set_query(query, cx));
                    }
                }
                IiCheatsheetEvent::SetPage { display, page } => {
                    if let Some(handle) = cheatsheet_surfaces_for_events
                        .borrow()
                        .get(&display)
                        .copied()
                    {
                        let show_elements = page == IiCheatsheetPage::Elements;
                        let _ = handle.update(cx, |host, _, cx| host.set_page(show_elements, cx));
                    }
                }
                IiCheatsheetEvent::CopyElement { display, symbol } => {
                    let generation =
                        crate::families::ii::cheatsheet_host::next_copy_feedback_generation();
                    cx.update(|cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(symbol.clone()));
                        if let Some(handle) = cheatsheet_surfaces_for_events
                            .borrow()
                            .get(&display)
                            .copied()
                        {
                            let symbol = symbol.clone();
                            let _ = handle.update(cx, |host, _, cx| {
                                host.set_copied_symbol(symbol, generation, cx)
                            });
                        }
                    });
                    let events = cheatsheet_events_for_host.clone();
                    cx.spawn(async move |cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(900))
                            .await;
                        let _ = events
                            .send(IiCheatsheetEvent::CopyFeedbackElapsed {
                                display,
                                generation,
                            })
                            .await;
                    })
                    .detach();
                }
                IiCheatsheetEvent::CopyFeedbackElapsed {
                    display,
                    generation,
                } => {
                    if let Some(handle) = cheatsheet_surfaces_for_events
                        .borrow()
                        .get(&display)
                        .copied()
                    {
                        let _ = handle.update(cx, |host, _, cx| {
                            if host.is_copy_feedback_current(generation) {
                                host.clear_copy_feedback(generation, cx);
                            }
                        });
                    }
                }
                IiCheatsheetEvent::Loaded {
                    display,
                    generation,
                    snapshot,
                } => {
                    if let Some(handle) = cheatsheet_surfaces_for_events
                        .borrow()
                        .get(&display)
                        .copied()
                    {
                        let _ = handle.update(cx, |host, _, cx| {
                            host.apply_snapshot(generation, snapshot, cx)
                        });
                    }
                }
            }
        }
    })
    .detach();

    let osd_surfaces_for_events = ii_osd_surfaces.clone();
    let osd_generation_for_events = ii_osd_generation.clone();
    let osd_active_for_events = ii_osd_active.clone();
    let osd_runtime_state = critical_config.clone();
    let osd_media_actions = media_actions.clone();
    let osd_events_for_events = ii_osd_events.clone();
    cx.spawn(async move |cx| {
        while let Ok(event) = ii_osd_receiver.recv().await {
            cx.update(|cx| {
                let current = osd_runtime_state.borrow().clone();
                match event {
                    IiOsdHostEvent::Trigger(trigger) => {
                        let enabled = reconcile_ii_osd(
                            cx,
                            &osd_surfaces_for_events,
                            &trigger,
                            &current.loaded.config,
                            &current.theme,
                            &osd_events_for_events,
                            &osd_media_actions,
                        );
                        if enabled {
                            *osd_active_for_events.borrow_mut() = Some(trigger.clone());
                        } else {
                            *osd_active_for_events.borrow_mut() = None;
                        }
                        if enabled && trigger.auto_hide {
                            schedule_ii_osd_timeout(
                                cx,
                                &osd_surfaces_for_events,
                                &osd_generation_for_events,
                                &osd_active_for_events,
                                &current.loaded.config.osd,
                                &trigger,
                            );
                        } else {
                            osd_generation_for_events
                                .set(osd_generation_for_events.get().wrapping_add(1));
                        }
                    }
                    IiOsdHostEvent::Hide => {
                        osd_generation_for_events
                            .set(osd_generation_for_events.get().wrapping_add(1));
                        *osd_active_for_events.borrow_mut() = None;
                        close_all_ii_osd(cx, &osd_surfaces_for_events);
                    }
                    IiOsdHostEvent::Hover {
                        display: _,
                        entered,
                    } => {
                        if entered {
                            osd_generation_for_events
                                .set(osd_generation_for_events.get().wrapping_add(1));
                        } else if let Some(trigger) = osd_active_for_events.borrow().clone()
                            && trigger.auto_hide
                        {
                            schedule_ii_osd_timeout(
                                cx,
                                &osd_surfaces_for_events,
                                &osd_generation_for_events,
                                &osd_active_for_events,
                                &current.loaded.config.osd,
                                &trigger,
                            );
                        }
                    }
                }
            });
        }
    })
    .detach();

    let media_vertical_surfaces = vertical_surfaces.clone();
    let media_horizontal_surfaces = surfaces.clone();
    let media_panel_surfaces = ii_panel_surfaces.clone();
    let media_controls_surfaces = media_controls_surfaces.clone();
    let media_controls_visible = media_controls_visible.clone();
    let media_config_snapshot = critical_config.clone();
    let media_state_cache = media_cache.clone();
    let media_controls_action_sender = media_actions.clone();
    let media_osd_events = ii_osd_events.clone();
    let media_osd_active = ii_osd_active.clone();
    cx.spawn(async move |cx| {
        let mut provider = crate::vertical_provider::MprisProvider::default();
        let mut last_sample = Instant::now() - Duration::from_secs(2);
        loop {
            while let Ok(action) = media_action_receiver.try_recv() {
                match action {
                    crate::components::vertical_bar::MediaAction::OpenControls => {
                        if media_controls_visible.get() {
                            cx.update(|cx| {
                                close_media_controls_surfaces(cx, &media_controls_surfaces);
                            });
                            media_controls_visible.set(false);
                        } else {
                            let snapshot = media_config_snapshot.borrow().clone();
                            let media = media_state_cache.borrow().clone();
                            cx.update(|cx| {
                                open_media_controls_surfaces(
                                    cx,
                                    &media_controls_surfaces,
                                    &media,
                                    &snapshot.loaded.config,
                                    &snapshot.theme,
                                    &media_controls_action_sender,
                                );
                            });
                            media_controls_visible
                                .set(!media_controls_surfaces.borrow().is_empty());
                        }
                    }
                    crate::components::vertical_bar::MediaAction::CloseControls => {
                        cx.update(|cx| {
                            close_media_controls_surfaces(cx, &media_controls_surfaces);
                        });
                        media_controls_visible.set(false);
                    }
                    action => {
                        let before = media_state_cache.borrow().clone();
                        let is_transport_action = is_mpris_transport_action(&action);
                        provider.perform_media_action(action).await;
                        let state = provider.sample().await;
                        last_sample = Instant::now();
                        let changed = is_transport_action && media_state_changed_for_osd(&before, &state);
                        *media_state_cache.borrow_mut() = state.clone();
                        cx.update(|cx| {
                            publish_media_state(
                                cx,
                                &media_horizontal_surfaces,
                                &media_vertical_surfaces,
                                &media_panel_surfaces,
                                &media_controls_surfaces,
                                state.clone(),
                            );
                            if changed && media_osd_enabled(&media_config_snapshot.borrow().loaded.config, &state) {
                                let _ = media_osd_events.try_send(IiOsdHostEvent::Trigger(IiOsdTrigger {
                                    indicator: crate::families::ii::osd::OsdIndicator::Media {
                                        title: state.title.clone(),
                                        artist: state.artist.clone(),
                                        art_url: media_state_art_url(&state),
                                        playing: Some(state.status == crate::components::vertical_bar::MediaStatus::Playing),
                                        can_previous: state.can_previous,
                                        can_toggle: state.can_toggle_playback,
                                        can_next: state.can_next,
                                    },
                                    availability: crate::families::ii::osd::OsdAvailability::Ready,
                                    auto_hide: true,
                                }));
                            }
                        });
                    }
                }
            }

            let active_media_osd = media_osd_active
                .borrow()
                .as_ref()
                .is_some_and(|trigger| {
                    matches!(
                        &trigger.indicator,
                        crate::families::ii::osd::OsdIndicator::Media { .. }
                    )
                });
            let sample_interval = mpris_sample_interval(active_media_osd);
            if last_sample.elapsed() >= sample_interval {
                let before = media_state_cache.borrow().clone();
                let state = provider.sample().await;
                last_sample = Instant::now();
                let changed = media_state_changed_for_osd(&before, &state);
                *media_state_cache.borrow_mut() = state.clone();
                cx.update(|cx| {
                    publish_media_state(
                        cx,
                        &media_horizontal_surfaces,
                        &media_vertical_surfaces,
                        &media_panel_surfaces,
                        &media_controls_surfaces,
                        state.clone(),
                    );
                    let active = media_osd_active.borrow().clone();
                    if should_refresh_active_media_osd(
                        active.as_ref(),
                        &media_config_snapshot.borrow().loaded.config,
                        &state,
                        changed,
                    ) {
                        let _ = media_osd_events.try_send(IiOsdHostEvent::Trigger(IiOsdTrigger {
                            indicator: crate::families::ii::osd::OsdIndicator::Media {
                                title: state.title.clone(),
                                artist: state.artist.clone(),
                                art_url: media_state_art_url(&state),
                                playing: Some(
                                    state.status
                                        == crate::components::vertical_bar::MediaStatus::Playing,
                                ),
                                can_previous: state.can_previous,
                                can_toggle: state.can_toggle_playback,
                                can_next: state.can_next,
                            },
                            availability: crate::families::ii::osd::OsdAvailability::Ready,
                            auto_hide: true,
                        }));
                    }
                });
            }

            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
        }
    })
    .detach();

    let overlay_receiver = waffle_receiver.clone();
    let overlay_handles = overlays.clone();
    let overlay_controller = waffle_controller.clone();
    // Resolve launcher presentation from the live config/theme snapshot when
    // it opens. Capturing these startup clones leaves later windows stale.
    let overlay_state = critical_config.clone();
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
                    let snapshot = overlay_state.borrow().clone();
                    if !open_waffle_overlay(
                        cx,
                        &overlay_handles,
                        display,
                        &snapshot.loaded.config,
                        &snapshot.theme,
                        &overlay_source,
                        &overlay_sender,
                    ) {
                        // A failed or disabled open must not leave the
                        // per-display controller claiming that a menu exists.
                        overlay_controller.borrow_mut().close(display);
                    }
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
    let notification_pump_taskbars = surfaces.clone();
    let notification_pump_service = notification_service.clone();
    let notification_pump_snapshot = notification_service_snapshot.clone();
    let notification_service_overlays = notification_overlays.clone();
    let notification_service_state = notification_controller.clone();
    let notification_service_group_expanded = notification_group_expanded.clone();
    let notification_service_group_events = notification_group_events.clone();
    let notification_service_theme = theme.clone();
    let notification_service_events = notification_events.clone();
    let ii_popup_handles = ii_notification_popups.clone();
    let ii_popup_grace = ii_notification_grace.clone();
    let ii_popup_generation = ii_notification_generation.clone();
    let ii_popup_service = notification_service.clone();
    let ii_popup_config = critical_config.clone();
    cx.spawn(async move |cx| {
        while notification_service_receiver.recv().await.is_ok() {
            let snapshot = notification_pump_service.snapshot();
            *notification_pump_snapshot.borrow_mut() = snapshot.clone();
            cx.update(|cx| {
                reconcile_ii_notification_popups(
                    cx,
                    &ii_popup_handles,
                    &ii_popup_grace,
                    &ii_popup_generation,
                    &ii_popup_service,
                    &snapshot,
                    &ii_popup_config.borrow().loaded.config,
                    &ii_popup_config.borrow().theme,
                );
                let handles = notification_service_overlays
                    .borrow()
                    .iter()
                    .map(|(display, handle)| (*display, *handle))
                    .collect::<Vec<_>>();
                let mut dead_displays = Vec::new();
                for (display_id, handle) in handles {
                    let Some(display) = cx
                        .displays()
                        .into_iter()
                        .find(|display| display.id() == display_id)
                    else {
                        continue;
                    };
                    let size = display.bounds().size;
                    let bar_height = px(48.);
                    let available_height = (size.height - bar_height - px(24.)).max(px(1.));
                    let close_events = notification_service_events.clone();
                    let on_close: crate::families::waffle::notification_center::Action =
                        std::sync::Arc::new(move || {
                            let _ = close_events.try_send(NotificationCenterEvent::Close {
                                display: display_id,
                            });
                        });
                    let expanded_groups = notification_service_group_expanded
                        .borrow()
                        .get(&display_id)
                        .cloned()
                        .unwrap_or_default();
                    let toggle_events = notification_service_group_events.clone();
                    let on_toggle_group: crate::families::waffle::notification_center::ItemAction =
                        std::sync::Arc::new(move |app_id| {
                            let _ = toggle_events.try_send((display_id, app_id));
                        });
                    let input = crate::families::waffle::notification_overlay::session_input(
                        &notification_service_theme,
                        available_height,
                        &snapshot,
                        &notification_pump_service,
                        &expanded_groups,
                        on_toggle_group,
                        on_close,
                    );
                    if handle
                        .update(cx, |view, _, cx| view.set_input(input, cx))
                        .is_err()
                    {
                        dead_displays.push(display_id);
                    }
                }
                for display_id in dead_displays {
                    notification_service_overlays
                        .borrow_mut()
                        .remove(&display_id);
                    notification_service_state.borrow_mut().close(display_id);
                }
                let taskbars = notification_pump_taskbars
                    .borrow()
                    .values()
                    .copied()
                    .collect::<Vec<_>>();
                for taskbar in taskbars {
                    let _ = taskbar.update(cx, |host, _, cx| host.refresh(cx));
                }
            });
        }
    })
    .detach();
    let notification_group_expansion_sender = notification_group_events.clone();
    let notification_group_expanded_state = notification_group_expanded.clone();
    let notification_group_overlays = notification_overlays.clone();
    let notification_group_service = notification_service.clone();
    let notification_group_snapshot = notification_service_snapshot.clone();
    let notification_group_theme = theme.clone();
    let notification_close_events = notification_events.clone();
    cx.spawn(async move |cx| {
        while let Ok((display_id, group_id)) = notification_group_receiver.recv().await {
            let expanded_groups = {
                let mut state = notification_group_expanded_state.borrow_mut();
                let groups = state.entry(display_id).or_default();
                if !groups.insert(group_id.clone()) {
                    groups.remove(&group_id);
                }
                groups.clone()
            };
            let snapshot = notification_group_service.snapshot();
            *notification_group_snapshot.borrow_mut() = snapshot.clone();
            cx.update(|cx| {
                let Some(handle) = notification_group_overlays
                    .borrow()
                    .get(&display_id)
                    .copied()
                else {
                    return;
                };
                let Some(display) = cx
                    .displays()
                    .into_iter()
                    .find(|display| display.id() == display_id)
                else {
                    return;
                };
                let available_height =
                    (display.bounds().size.height - px(48.) - px(24.)).max(px(1.));
                let close_events = notification_close_events.clone();
                let on_close: crate::families::waffle::notification_center::Action =
                    std::sync::Arc::new(move || {
                        let _ = close_events.try_send(NotificationCenterEvent::Close {
                            display: display_id,
                        });
                    });
                let toggle_events = notification_group_expansion_sender.clone();
                let on_toggle_group: crate::families::waffle::notification_center::ItemAction =
                    std::sync::Arc::new(move |app_id| {
                        let _ = toggle_events.try_send((display_id, app_id));
                    });
                let input = crate::families::waffle::notification_overlay::session_input(
                    &notification_group_theme,
                    available_height,
                    &snapshot,
                    &notification_group_service,
                    &expanded_groups,
                    on_toggle_group,
                    on_close,
                );
                if handle
                    .update(cx, |view, _, cx| view.set_input(input, cx))
                    .is_err()
                {
                    notification_group_overlays.borrow_mut().remove(&display_id);
                }
            });
        }
    })
    .detach();
    let notification_open_service = notification_service.clone();
    let notification_open_snapshot = notification_service_snapshot.clone();
    let notification_open_group_expanded = notification_group_expanded.clone();
    let notification_open_group_events = notification_group_events.clone();
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
                    let expanded_groups = notification_open_group_expanded.borrow();
                    open_notification_overlay(
                        cx,
                        &notification_handles,
                        display,
                        &notification_config,
                        &notification_theme,
                        &notification_open_service,
                        &notification_open_snapshot.borrow(),
                        &expanded_groups,
                        &notification_open_group_events,
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
    let revision_start_menu_overlays = overlays.clone();
    let revision_start_menu_controller = waffle_controller.clone();
    let revision_start_menu_source = waffle_source.clone();
    let revision_start_menu_events = waffle_events.clone();
    let revision_ii_notification_popups = ii_notification_popups.clone();
    let revision_ii_notification_grace = ii_notification_grace.clone();
    let revision_ii_notification_generation = ii_notification_generation.clone();
    let revision_notification_service = notification_service.clone();
    let revision_ii_osd_surfaces = ii_osd_surfaces.clone();
    let revision_ii_osd_generation = ii_osd_generation.clone();
    let revision_ii_osd_active = ii_osd_active.clone();
    let revision_ii_osd_events = ii_osd_events.clone();
    let revision_ii_osd_media_actions = media_actions.clone();
    let revision_ii_clipboard_surfaces = ii_clipboard_surfaces.clone();
    let revision_ii_clipboard_pins = ii_clipboard_pins.clone();
    let revision_ii_cheatsheet_surfaces = ii_cheatsheet_surfaces.clone();
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
                let mut theme = theme_resolver::resolve_for_config(
                    &runtime_loaded,
                    palette.as_ref(),
                    metadata.as_ref(),
                );
                if runtime_loaded.config.gpui_mvp.configured {
                    crate::components::top_bar::apply_mvp_theme(
                        &mut theme,
                        &runtime_loaded.config.gpui_mvp,
                    );
                }
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
                let notification_snapshot = revision_notification_service.snapshot();
                let current = revision_state.borrow().clone();
                if previous.loaded.config.clipboard_pinned
                    != current.loaded.config.clipboard_pinned
                {
                    let pins = current.loaded.config.clipboard_pinned.clone();
                    *revision_ii_clipboard_pins.borrow_mut() = pins.clone();
                    let handles = revision_ii_clipboard_surfaces
                        .borrow()
                        .values()
                        .copied()
                        .collect::<Vec<_>>();
                    for handle in handles {
                        let pins = pins.clone();
                        let _ = handle.update(cx, |view, _, cx| view.set_shared_pins(pins, cx));
                    }
                }
                let cheatsheet_handles = revision_ii_cheatsheet_surfaces
                    .borrow()
                    .values()
                    .copied()
                    .collect::<Vec<_>>();
                for handle in cheatsheet_handles {
                    let theme = theme.clone();
                    let _ = handle.update(cx, |host, _, cx| {
                        host.set_theme(theme, cx);
                        host.refresh_provider(cx);
                    });
                }
                if let Some(trigger) = revision_ii_osd_active.borrow().clone() {
                    let enabled = reconcile_ii_osd(
                        cx,
                        &revision_ii_osd_surfaces,
                        &trigger,
                        &current.loaded.config,
                        &current.theme,
                        &revision_ii_osd_events,
                        &revision_ii_osd_media_actions,
                    );
                    revision_ii_osd_generation.set(
                        revision_ii_osd_generation.get().wrapping_add(1),
                    );
                    if !enabled {
                        *revision_ii_osd_active.borrow_mut() = None;
                    }
                    if enabled && trigger.auto_hide {
                        schedule_ii_osd_timeout(
                            cx,
                            &revision_ii_osd_surfaces,
                            &revision_ii_osd_generation,
                            &revision_ii_osd_active,
                            &current.loaded.config.osd,
                            &trigger,
                        );
                    }
                } else if current.loaded.config.panel_family.as_deref() != Some("ii")
                    || !current
                        .loaded
                        .config
                        .enabled_panels
                        .iter()
                        .any(|panel| panel == "iiOnScreenDisplay")
                {
                    close_all_ii_osd(cx, &revision_ii_osd_surfaces);
                }
                reconcile_ii_notification_popups(
                    cx,
                    &revision_ii_notification_popups,
                    &revision_ii_notification_grace,
                    &revision_ii_notification_generation,
                    &revision_notification_service,
                    &notification_snapshot,
                    &current.loaded.config,
                    &current.theme,
                );
                if waffle_family_update
                    || waffle_start_menu_config_changed(
                        &previous.loaded.config,
                        &revision_state.borrow().loaded.config,
                    )
                {
                    let snapshot = revision_state.borrow().clone();
                    refresh_open_waffle_overlays(
                        cx,
                        &revision_start_menu_overlays,
                        &revision_start_menu_controller,
                        &snapshot.loaded.config,
                        &snapshot.theme,
                        &revision_start_menu_source,
                        &revision_start_menu_events,
                    );
                }
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
                &media_actions,
                &media_cache,
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
    let output_iris_background_previews = iris_background_previews.clone();
    let output_iris_mapping_pending = iris_output_mapping_pending.clone();
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
    let output_ii_osd_surfaces = ii_osd_surfaces.clone();
    let output_ii_osd_active = ii_osd_active.clone();
    let output_ii_osd_generation = ii_osd_generation.clone();
    let output_ii_osd_events = ii_osd_events.clone();
    let output_ii_osd_media_actions = media_actions.clone();
    cx.spawn(async move |cx| {
        while niri_output_updates.recv().await.is_ok() {
            cx.update(|cx| {
                let snapshot = output_state.borrow().clone();
                let preview_displays = output_iris_background_previews
                    .borrow()
                    .keys()
                    .copied()
                    .collect::<Vec<_>>();
                let live_displays = cx
                    .displays()
                    .into_iter()
                    .map(|display| display.id())
                    .collect::<Vec<_>>();
                let (live_previews, stale_previews) =
                    split_registered_preview_displays(preview_displays, live_displays);
                for display_id in live_previews {
                    let pending_since = output_iris_mapping_pending
                        .borrow()
                        .get(&display_id)
                        .copied();
                    if pending_since.is_some_and(niri_snapshot_attempted_after) {
                        output_iris_mapping_pending.borrow_mut().remove(&display_id);
                    }
                    let pending_since = output_iris_mapping_pending
                        .borrow()
                        .get(&display_id)
                        .copied();
                    open_iris_background_preview_for_display(
                        cx,
                        &output_iris_background_previews,
                        pending_since,
                        display_id,
                        &snapshot.loaded.config,
                        &snapshot.theme,
                    );
                }
                for display_id in stale_previews {
                    output_iris_mapping_pending.borrow_mut().remove(&display_id);
                    close_iris_background_preview_for_display(
                        cx,
                        &output_iris_background_previews,
                        display_id,
                    );
                }
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
                    reconcile_active_ii_osd(
                        cx,
                        &output_ii_osd_surfaces,
                        &output_ii_osd_active,
                        &output_ii_osd_generation,
                        &snapshot.loaded.config,
                        &snapshot.theme,
                        &output_ii_osd_events,
                        &output_ii_osd_media_actions,
                    );
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

    // GPUI display events do not expose Niri connector-name changes when
    // geometry stays the same. Refresh periodically for Waffle backgrounds
    // and while an explicit Iris preview needs a current output identity.
    let periodic_waffle_output_refresh = family == "waffle";
    let iris_previews_for_refresh = iris_background_previews.clone();
    if should_poll_niri_output_map(
        periodic_waffle_output_refresh,
        iris_background_previews.borrow().len(),
    ) {
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                if !should_poll_niri_output_map(
                    periodic_waffle_output_refresh,
                    iris_previews_for_refresh.borrow().len(),
                ) {
                    break;
                }
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
    let observed_ii_notification_popups = ii_notification_popups.clone();
    let observed_ii_notification_grace = ii_notification_grace.clone();
    let observed_ii_notification_generation = ii_notification_generation.clone();
    let observed_notification_controller = notification_controller.clone();
    let observed_notification_service = notification_service.clone();
    let observed_notification_snapshot = notification_service_snapshot.clone();
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
    let observed_ii_osd_surfaces = ii_osd_surfaces.clone();
    let observed_ii_osd_active = ii_osd_active.clone();
    let observed_ii_osd_generation = ii_osd_generation.clone();
    let observed_ii_osd_events = ii_osd_events.clone();
    let observed_ii_osd_media_actions = media_actions.clone();
    let observed_ii_clipboard_surfaces = ii_clipboard_surfaces.clone();
    let observed_ii_clipboard_generations = ii_clipboard_generations.clone();
    let observed_ii_cheatsheet_surfaces = ii_cheatsheet_surfaces.clone();
    let observed_iris_background_previews = iris_background_previews.clone();
    let observed_iris_mapping_pending = iris_output_mapping_pending.clone();
    let observed_family = family.to_owned();
    cx.on_window_closed({
        let surfaces = surfaces.clone();
        let vertical_surfaces = vertical_surfaces.clone();
        let backgrounds = backgrounds.clone();
        let iris_background_previews = iris_background_previews.clone();
        let iris_output_mapping_pending = iris_output_mapping_pending.clone();
        let backdrops = backdrops.clone();
        let waffle_backgrounds = waffle_backgrounds.clone();
        let waffle_backdrops = waffle_backdrops.clone();
        let waffle_output_names = waffle_output_names.clone();
        let overlays = overlays.clone();
        let notification_overlays = notification_overlays.clone();
        let ii_notification_popups = ii_notification_popups.clone();
        let task_view_overlays = task_view_overlays.clone();
        let action_center_overlays = action_center_overlays.clone();
        let clipboard_overlays = clipboard_overlays.clone();
        let ii_panel_surfaces = ii_panel_surfaces.clone();
        let ii_dock_surfaces = ii_dock_surfaces.clone();
        let ii_panel_states = ii_panel_states.clone();
        let ii_session_screen_surfaces = ii_session_screen_surfaces.clone();
        let ii_session_screen_states = ii_session_screen_states.clone();
        let ii_session_screen_handlers = ii_session_screen_handlers.clone();
        let ii_clipboard_surfaces = ii_clipboard_surfaces.clone();
        let ii_clipboard_generations = ii_clipboard_generations.clone();
        let ii_cheatsheet_surfaces = ii_cheatsheet_surfaces.clone();
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
            let closed_clipboard_displays = ii_clipboard_surfaces
                .borrow()
                .iter()
                .filter(|(_, handle)| handle.window_id() == window_id)
                .map(|(display, _)| *display)
                .collect::<Vec<_>>();
            for display in closed_clipboard_displays {
                ii_clipboard_surfaces.borrow_mut().remove(&display);
                ii_clipboard_generations.borrow_mut().remove(&display);
            }
            backgrounds
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
            iris_background_previews
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
            iris_output_mapping_pending
                .borrow_mut()
                .retain(|display, _| iris_background_previews.borrow().contains_key(display));
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
            let removed_session_screen_outputs = ii_session_screen_surfaces
                .borrow()
                .iter()
                .filter_map(|(display, handle)| {
                    (handle.window_id() == window_id).then_some(*display)
                })
                .collect::<Vec<_>>();
            for display in removed_session_screen_outputs {
                ii_session_screen_surfaces.borrow_mut().remove(&display);
                ii_session_screen_states.borrow_mut().remove(&display);
                ii_session_screen_handlers.borrow_mut().remove(&display);
            }
            ii_cheatsheet_surfaces
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
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
            ii_notification_popups
                .borrow_mut()
                .retain(|_, handle| handle.window_id() != window_id);
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
                if !observed_iris_background_previews.borrow().is_empty() {
                    mark_iris_preview_mappings_pending(
                        &observed_iris_background_previews,
                        &observed_iris_mapping_pending,
                    );
                    refresh_niri_output_snapshot();
                }
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
                    reconcile_active_ii_osd(
                        cx,
                        &observed_ii_osd_surfaces,
                        &observed_ii_osd_active,
                        &observed_ii_osd_generation,
                        &snapshot.loaded.config,
                        &snapshot.theme,
                        &observed_ii_osd_events,
                        &observed_ii_osd_media_actions,
                    );
                    let service_snapshot = observed_notification_service.snapshot();
                    reconcile_ii_notification_popups(
                        cx,
                        &observed_ii_notification_popups,
                        &observed_ii_notification_grace,
                        &observed_ii_notification_generation,
                        &observed_notification_service,
                        &service_snapshot,
                        &snapshot.loaded.config,
                        &snapshot.theme,
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
                        &observed_media_cache,
                    );
                } else {
                    open_for_display(
                        cx,
                        &observed_surfaces,
                        display_id,
                        &observed_config,
                        &observed_theme,
                        &observed_ii_panel_events,
                        &observed_media_actions,
                        &observed_media_cache,
                    );
                }
            }
            DisplayEvent::Removed(display_id) => {
                if !observed_iris_background_previews.borrow().is_empty() {
                    mark_iris_preview_mappings_pending(
                        &observed_iris_background_previews,
                        &observed_iris_mapping_pending,
                    );
                    refresh_niri_output_snapshot();
                }
                close_iris_background_preview_for_display(
                    cx,
                    &observed_iris_background_previews,
                    display_id,
                );
                observed_iris_mapping_pending
                    .borrow_mut()
                    .remove(&display_id);
                close_ii_clipboard_surface(
                    cx,
                    &observed_ii_clipboard_surfaces,
                    &observed_ii_clipboard_generations,
                    display_id,
                );
                close_ii_cheatsheet_surface(cx, &observed_ii_cheatsheet_surfaces, display_id);
                close_ii_osd_surface(cx, &observed_ii_osd_surfaces, display_id);
                if manage_ii {
                    let snapshot = observed_ii_critical.borrow().clone();
                    reconcile_active_ii_osd(
                        cx,
                        &observed_ii_osd_surfaces,
                        &observed_ii_osd_active,
                        &observed_ii_osd_generation,
                        &snapshot.loaded.config,
                        &snapshot.theme,
                        &observed_ii_osd_events,
                        &observed_ii_osd_media_actions,
                    );
                }
                close_ii_notification_popup(cx, &observed_ii_notification_popups, display_id);
                observed_ii_notification_grace
                    .borrow_mut()
                    .remove(&display_id);
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
                if changes.contains(DisplayChanges::BOUNDS)
                    && let Some(handle) = observed_ii_cheatsheet_surfaces.borrow().get(&id).copied()
                    && let Some(display) =
                        cx.displays().into_iter().find(|display| display.id() == id)
                {
                    let output_size = display.bounds().size;
                    let _ = handle.update(cx, |host, window, cx| {
                        host.set_output_size(output_size, cx);
                        window.resize(output_size);
                    });
                }
                if changes.contains(DisplayChanges::BOUNDS)
                    && observed_iris_background_previews.borrow().contains_key(&id)
                {
                    let pending_since = Instant::now();
                    observed_iris_mapping_pending
                        .borrow_mut()
                        .insert(id, pending_since);
                    refresh_niri_output_snapshot();
                    let snapshot = observed_ii_critical.borrow().clone();
                    open_iris_background_preview_for_display(
                        cx,
                        &observed_iris_background_previews,
                        Some(pending_since),
                        id,
                        &snapshot.loaded.config,
                        &snapshot.theme,
                    );
                }
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
                    if changes.contains(DisplayChanges::BOUNDS) {
                        reconcile_active_ii_osd(
                            cx,
                            &observed_ii_osd_surfaces,
                            &observed_ii_osd_active,
                            &observed_ii_osd_generation,
                            &snapshot.loaded.config,
                            &snapshot.theme,
                            &observed_ii_osd_events,
                            &observed_ii_osd_media_actions,
                        );
                    }
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
                                    &observed_notification_service,
                                    &observed_notification_snapshot.borrow(),
                                    &notification_group_expanded.borrow(),
                                    &notification_group_events,
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
                            &observed_media_cache,
                        );
                    } else if observed_family != "ii" {
                        resize_for_display(
                            cx,
                            &observed_surfaces,
                            id,
                            &observed_config,
                            &observed_theme,
                            &observed_ii_panel_events,
                            &observed_media_actions,
                            &observed_media_cache,
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
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
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
        || previous.gpui_mvp != next.gpui_mvp
}

fn waffle_background_config_changed(previous: &ShellConfig, next: &ShellConfig) -> bool {
    previous.panel_family != next.panel_family
        || previous.enabled_panels != next.enabled_panels
        || previous.background != next.background
        || previous.waffle.background != next.waffle.background
        || previous.appearance != next.appearance
}

fn waffle_start_menu_config_changed(previous: &ShellConfig, next: &ShellConfig) -> bool {
    previous.panel_family != next.panel_family
        || previous.enabled_panels != next.enabled_panels
        || previous.appearance != next.appearance
        || previous.gpui_mvp != next.gpui_mvp
        || previous.background != next.background
        || previous.waffle.background != next.waffle.background
        || previous.bar.bottom != next.bar.bottom
        || previous.bar.height != next.bar.height
        || previous.waffle.bar.bottom != next.waffle.bar.bottom
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
            media_actions,
            media_cache,
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
        IiPanelKind::ControlPanel => "iiControlPanel",
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

fn ii_event_uses_sidebar_screen_filter(event: &IiPanelEvent) -> bool {
    !matches!(
        event,
        IiPanelEvent::Toggle {
            panel: IiPanelKind::ControlPanel,
            ..
        } | IiPanelEvent::Close {
            panel: IiPanelKind::ControlPanel,
            ..
        } | IiPanelEvent::FinishControlPanelClose { .. }
    )
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
        | IiPanelEvent::FinishControlPanelClose { display, .. }
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
    if ii_event_uses_sidebar_screen_filter(&event)
        && !ii_panel_screen_enabled(cx, display_id, &config.sidebar.screen_list)
    {
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
    if let IiPanelEvent::FinishControlPanelClose { generation, .. } = &event {
        if !state.finish_control_panel_close(*generation) {
            return;
        }
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
    let mut finish_control_panel_close_later = None;
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
                IiPanelKind::ControlPanel => {
                    finish_control_panel_close_later = state.toggle_control_panel();
                }
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
            IiPanelKind::ControlPanel => {
                finish_control_panel_close_later = state.begin_control_panel_close();
            }
        },
        IiPanelEvent::FinishControlPanelClose { .. } => {}
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
    if let Some(generation) = finish_control_panel_close_later {
        let sender = events.clone();
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let _ = sender.try_send(IiPanelEvent::FinishControlPanelClose {
                display: display_id,
                generation,
            });
        })
        .detach();
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
        let control_panel_open = state.control_panel_open;
        let _ = handle.update(cx, |host, _, cx| {
            host.set_left_sidebar_open(left_open, cx);
            host.set_control_panel_open(control_panel_open, cx);
        });
    }
    if let Some(handle) = horizontal_bars.borrow().get(&display_id).copied() {
        let left_open = state.left_open;
        let control_panel_open = state.control_panel_open;
        let _ = handle.update(cx, |host, _, cx| {
            host.set_left_sidebar_open(left_open, cx);
            host.set_control_panel_open(control_panel_open, cx);
        });
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
            // GPUI exposes keyboard interactivity only at layer-window creation.
            // The close grace calls Window::blur, clearing GPUI element focus,
            // but compositor-level Exclusive remains until this window is removed.
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

fn handle_ii_session_screen_event(
    cx: &mut App,
    event: IiSessionScreenEvent,
    surfaces: &IiSessionScreenSurfaceMap,
    states: &IiSessionScreenStateMap,
    generations: &IiSessionScreenGenerationMap,
    handlers: &Rc<RefCell<HashMap<DisplayId, SessionActionHandler>>>,
    config: &Rc<RefCell<IiCriticalSnapshot>>,
    events: &Sender<IiSessionScreenEvent>,
) -> Option<(DisplayId, u64)> {
    use crate::families::ii::session_screen::SessionScreenEvent as ViewEvent;
    use crate::families::ii::session_screen_state::SessionScreenVisibility;

    match event {
        IiSessionScreenEvent::Toggle { display } => {
            let visibility = states
                .borrow()
                .get(&display)
                .map(|state| state.visibility());
            match visibility {
                Some(SessionScreenVisibility::Open) => process_ii_session_screen_view_event(
                    cx,
                    display,
                    ViewEvent::Dismiss,
                    surfaces,
                    states,
                    generations,
                    handlers,
                ),
                Some(SessionScreenVisibility::Closing { .. }) => {
                    let snapshot = {
                        let mut states = states.borrow_mut();
                        states.get_mut(&display).map(|state| {
                            state.open();
                            state.clone()
                        })
                    };
                    if let Some(snapshot) = &snapshot {
                        generations
                            .borrow_mut()
                            .insert(display, snapshot.close_generation());
                    }
                    let handle = surfaces.borrow().get(&display).copied();
                    if let (Some(handle), Some(snapshot)) = (handle, snapshot) {
                        let _ = handle
                            .update(cx, |view, window, cx| view.set_state(&snapshot, window, cx));
                    }
                    None
                }
                _ => {
                    let surface_exists = surfaces.borrow().contains_key(&display);
                    if surface_exists {
                        close_ii_session_screen_surface(cx, display, surfaces, states, handlers);
                    }
                    open_ii_session_screen_surface(
                        cx,
                        display,
                        None,
                        surfaces,
                        states,
                        generations,
                        handlers,
                        config,
                        events,
                    );
                    None
                }
            }
        }
        IiSessionScreenEvent::Open { display, handler } => {
            close_ii_session_screen_surface(cx, display, surfaces, states, handlers);
            open_ii_session_screen_surface(
                cx,
                display,
                handler,
                surfaces,
                states,
                generations,
                handlers,
                config,
                events,
            );
            None
        }
        IiSessionScreenEvent::View { display, event } => process_ii_session_screen_view_event(
            cx,
            display,
            event,
            surfaces,
            states,
            generations,
            handlers,
        ),
        IiSessionScreenEvent::CloseElapsed {
            display,
            generation,
        } => {
            let should_close = states
                .borrow_mut()
                .get_mut(&display)
                .is_some_and(|state| state.close_timer_elapsed(generation));
            if should_close {
                close_ii_session_screen_surface(cx, display, surfaces, states, handlers);
            }
            None
        }
    }
}

fn process_ii_session_screen_view_event(
    cx: &mut App,
    display: DisplayId,
    event: crate::families::ii::session_screen::SessionScreenEvent,
    surfaces: &IiSessionScreenSurfaceMap,
    states: &IiSessionScreenStateMap,
    generations: &IiSessionScreenGenerationMap,
    handlers: &Rc<RefCell<HashMap<DisplayId, SessionActionHandler>>>,
) -> Option<(DisplayId, u64)> {
    use crate::families::ii::session_screen::SessionScreenEvent as ViewEvent;
    let Some(handle) = surfaces.borrow().get(&display).copied() else {
        return None;
    };
    let mut generation = {
        let mut states = states.borrow_mut();
        let Some(state) = states.get_mut(&display) else {
            return None;
        };
        state.handle_view_event(event)
    };
    if let ViewEvent::Activate(action) = event {
        let handler = handlers.borrow().get(&display).cloned();
        match handler {
            Some(handler) if handler(action) => {
                generation = states
                    .borrow_mut()
                    .get_mut(&display)
                    .and_then(|state| state.action_acknowledged(action));
            }
            Some(_) => eprintln!(
                "GPUI ii session screen: action {action:?} was not acknowledged; menu remains open"
            ),
            None => eprintln!(
                "GPUI ii session screen: no action handler for {action:?}; request not executed and menu remains open"
            ),
        }
    }
    let snapshot = states.borrow().get(&display).cloned();
    if let Some(snapshot) = snapshot {
        generations
            .borrow_mut()
            .insert(display, snapshot.close_generation());
        let _ = handle.update(cx, |view, window, cx| view.set_state(&snapshot, window, cx));
    }
    generation.map(|generation| (display, generation))
}

fn open_ii_session_screen_surface(
    cx: &mut App,
    display_id: DisplayId,
    handler: Option<SessionActionHandler>,
    surfaces: &IiSessionScreenSurfaceMap,
    states: &IiSessionScreenStateMap,
    generations: &IiSessionScreenGenerationMap,
    handlers: &Rc<RefCell<HashMap<DisplayId, SessionActionHandler>>>,
    config: &Rc<RefCell<IiCriticalSnapshot>>,
    events: &Sender<IiSessionScreenEvent>,
) {
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|item| item.id() == display_id)
    else {
        eprintln!("GPUI ii session screen: explicit display {display_id:?} is unavailable");
        return;
    };
    let size = display.bounds().size;
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size,
        })),
        app_id: Some("intoo-gpui-ii-session-screen".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-ii-session-screen".into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(px(0.)),
            exclusive_edge: None,
            // This is a normal session menu, never a secure lock surface.
            // Its compositor-level exclusive focus is released only by removing
            // this layer-shell window after the close grace period.
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    };
    let theme = config.borrow().theme.clone();
    let screen_events = events.clone();
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| {
            crate::families::ii::session_screen_host::SessionScreenHost::new(
                theme,
                screen_events,
                display_id,
                cx,
            )
        })
    }) {
        Ok(handle) => {
            let mut state =
                crate::families::ii::session_screen_state::SessionScreenState::default();
            state.seed_close_generation(
                generations
                    .borrow()
                    .get(&display_id)
                    .copied()
                    .unwrap_or_default(),
            );
            state.open();
            generations
                .borrow_mut()
                .insert(display_id, state.close_generation());
            if let Some(handler) = handler {
                handlers.borrow_mut().insert(display_id, handler);
            }
            let _ = handle.update(cx, |view, window, cx| view.set_state(&state, window, cx));
            states.borrow_mut().insert(display_id, state);
            surfaces.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!("Could not create GPUI ii session screen for {display_id:?}: {error:#}")
        }
    }
}

fn close_ii_session_screen_surface(
    cx: &mut App,
    display: DisplayId,
    surfaces: &IiSessionScreenSurfaceMap,
    states: &IiSessionScreenStateMap,
    handlers: &Rc<RefCell<HashMap<DisplayId, SessionActionHandler>>>,
) {
    states.borrow_mut().remove(&display);
    handlers.borrow_mut().remove(&display);
    let Some(handle) = surfaces.borrow_mut().remove(&display) else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window, _| window.remove_window()) {
        eprintln!("Could not release GPUI ii session screen focus for {display:?}: {error:#}");
    }
}

fn open_media_controls_surfaces(
    cx: &mut App,
    surfaces: &MediaControlsSurfaceMap,
    media: &crate::components::vertical_bar::MediaState,
    config: &ShellConfig,
    theme: &ThemeTokens,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
) {
    if !surfaces.borrow().is_empty() {
        return;
    }
    let displays = cx
        .displays()
        .into_iter()
        .filter(|display| ii_panel_screen_enabled(cx, display.id(), &config.media.screen_list))
        .collect::<Vec<_>>();
    let Some(keyboard_display) = displays.first().map(|display| display.id()) else {
        return;
    };
    let typography_scale = media_controls_typography_scale(config);
    let popup_width = (380.0 * typography_scale).round();
    let popup_card_height = (150.0 * typography_scale).round();
    let popup_height = crate::components::media_controls_popup::card_stack_height(
        popup_width,
        popup_card_height,
        media.players.len(),
    );

    for display in displays {
        let display_id = display.id();
        let display_bounds = display.bounds();
        let output_name =
            niri_output_name(display_bounds).unwrap_or_else(|| format!("display-{display_id:?}"));
        let output_name_for_log = output_name.clone();
        let bounds = Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size: display_bounds.size,
        };
        let options = WindowOptions {
            titlebar: None,
            display_id: Some(display_id),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("intoo-gpui-media-controls".into()),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "intoo-gpui-media-controls".into(),
                layer: Layer::Overlay,
                anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                exclusive_zone: Some(Pixels::ZERO),
                keyboard_interactivity: if display_id == keyboard_display {
                    KeyboardInteractivity::Exclusive
                } else {
                    KeyboardInteractivity::None
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let output_media = media.clone();
        let colors = crate::components::top_bar::media_popup_colors(theme);
        let action_sender = media_actions.clone();
        let dismiss_sender = media_actions.clone();
        match cx.open_window(options, move |_, cx| {
            cx.new(|cx| {
                let focus_handle = cx.focus_handle();
                let action_sender = action_sender.clone();
                let close: Rc<dyn Fn()> = Rc::new(move || {
                    let _ = dismiss_sender.try_send(
                        crate::components::vertical_bar::MediaAction::CloseControls,
                    );
                });
                let popup = crate::components::media_controls_popup::MediaControlsPopupProps {
                    media: Some(output_media),
                    colors,
                    focus_handle,
                    width_px: popup_width,
                    height_px: popup_height,
                    radius_px: 20.0,
                    on_action: Some(Rc::new(move |action| {
                        let _ = action_sender.try_send(action);
                    })),
                    on_close: Some(close.clone()),
                };
                crate::components::media_controls_surface::MediaControlsSurface::new(
                    crate::components::media_controls_surface::MediaControlsSurfaceProps {
                        output_name,
                        dock_edge: crate::components::media_controls_surface::MediaControlsDockEdge::Bottom,
                        margins: crate::components::media_controls_surface::MediaControlsMargins {
                            bottom: media_controls_dock_offset(config, typography_scale),
                            ..Default::default()
                        },
                        popup,
                        on_dismiss: close,
                    },
                )
            })
        }) {
            Ok(handle) => {
                surfaces.borrow_mut().insert(display_id, handle);
            }
            Err(error) => eprintln!(
                "Could not open GPUI media controls on {output_name_for_log}: {error:#}"
            ),
        }
    }
}

fn media_controls_typography_scale(config: &ShellConfig) -> f32 {
    config
        .appearance
        .source_sections
        .get("typography")
        .and_then(|typography| typography.get("sizeScale"))
        .and_then(serde_json::Value::as_f64)
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .map(|scale| scale.clamp(0.5, 2.0) as f32)
        .unwrap_or(1.0)
}

fn media_controls_dock_offset(config: &ShellConfig, typography_scale: f32) -> f32 {
    // QML positions the card above the dock, elevation margin, compositor gap,
    // and an additional 5px safety gap. Only elevation scales with typography.
    let dock_height = config
        .appearance
        .source_sections
        .get("ii.dock")
        .and_then(|dock| dock.get("height"))
        .and_then(serde_json::Value::as_f64)
        .filter(|height| height.is_finite() && *height >= 0.0)
        .unwrap_or(60.0);
    let elevation_and_gap = f64::from((10.0 * typography_scale).round() + 10.0);
    (dock_height + elevation_and_gap).min(f64::from(f32::MAX)) as f32
}

fn close_media_controls_surfaces(cx: &mut App, surfaces: &MediaControlsSurfaceMap) {
    let handles = std::mem::take(&mut *surfaces.borrow_mut())
        .into_values()
        .collect::<Vec<_>>();
    for handle in handles {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
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
        close_control_panel: send(IiPanelEvent::Close {
            display,
            panel: IiPanelKind::ControlPanel,
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
    // Removing the GPUI window synchronously invokes `on_window_closed`, which
    // inspects `surfaces`. End the RefCell borrow before closing the window.
    let handle = surfaces.borrow_mut().remove(&display_id);
    if let Some(handle) = handle {
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
    if !ii_panel_enabled(config, IiPanelKind::ControlPanel) {
        state.control_panel_open = false;
        state.control_panel_closing = false;
    }
    let sidebar_screen_enabled =
        ii_panel_screen_enabled(cx, display_id, &config.sidebar.screen_list);
    if !sidebar_screen_enabled {
        state.left_open = false;
        state.left_expanded = false;
        state.right_open = false;
        state.dashboard_open = false;
        state.volume_mixer_open = false;
        state.volume_device_picker_open = false;
        if let Some(handle) = vertical_bars.borrow().get(&display_id).copied() {
            let _ = handle.update(cx, |host, _, cx| host.set_left_sidebar_open(false, cx));
        }
        if let Some(handle) = horizontal_bars.borrow().get(&display_id).copied() {
            let _ = handle.update(cx, |host, _, cx| host.set_left_sidebar_open(false, cx));
        }
    }
    if !sidebar_screen_enabled && !state.control_panel_open && !state.control_panel_closing {
        close_ii_panel_for_display(cx, surfaces, states, display_id);
    } else if !state.any_open() {
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
    let handle = overlays.borrow_mut().remove(&display);
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

/// Rebuild already-open Waffle launchers from the latest theme/config snapshot
/// without toggling their per-output controller state. The data source owns
/// query and navigation state, so the new view receives its current snapshot.
fn refresh_open_waffle_overlays(
    cx: &mut App,
    overlays: &OverlayMap,
    controller: &Rc<RefCell<StartMenuController>>,
    config: &ShellConfig,
    theme: &ThemeTokens,
    source: &Option<std::sync::Arc<XdgDesktopEntrySource>>,
    events: &Sender<StartMenuEvent>,
) {
    let displays = overlays.borrow().keys().copied().collect::<Vec<_>>();
    for display in displays {
        if !matches!(
            controller.borrow().state(display),
            StartMenuState::Open { .. }
        ) {
            close_waffle_overlay(cx, overlays, display);
            continue;
        }

        close_waffle_overlay(cx, overlays, display);
        if !app_launcher_enabled(config) {
            controller.borrow_mut().close(display);
            continue;
        }
        if !open_waffle_overlay(cx, overlays, display, config, theme, source, events) {
            controller.borrow_mut().close(display);
        }
    }
}

/// The II MVP TopBar always exposes Apps. Waffle exposes Start only when its
/// panel is enabled, matching the Waffle taskbar's visibility gate.
fn app_launcher_enabled(config: &ShellConfig) -> bool {
    match config.panel_family.as_deref() {
        Some("ii") => true,
        Some("waffle") => config
            .enabled_panels
            .iter()
            .any(|panel| panel == "wStartMenu"),
        _ => false,
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
) -> bool {
    if overlays.borrow().contains_key(&display_id) {
        return true;
    }
    if !app_launcher_enabled(config) {
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
    let bottom = if config.panel_family.as_deref() == Some("waffle") {
        config.waffle.bar.bottom.unwrap_or(false)
    } else {
        config.bar.bottom.unwrap_or(false)
    };
    let anchor = crate::families::waffle::taskbar::start_menu_overlay::StartMenuOverlayAnchor {
        display_size: size,
        bar_bottom: bottom,
        left_align_apps: false,
        bar_height: px(if config.panel_family.as_deref() == Some("waffle") {
            48.0
        } else {
            config.bar.height.unwrap_or(48.0).clamp(24.0, 128.0) as f32
        }),
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
            true
        }
        Err(error) => {
            eprintln!("Could not open Waffle Start Menu on {display_id:?}: {error:#}");
            false
        }
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
    let handle = overlays.borrow_mut().remove(&display);
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

fn ii_notification_output_selected(
    output_name: Option<&str>,
    screen_list: &[String],
    has_any_match: bool,
) -> bool {
    screen_list.is_empty()
        || !has_any_match
        || output_name.is_some_and(|name| screen_list.iter().any(|selected| selected == name))
}

fn reconcile_ii_notification_popups(
    cx: &mut App,
    popups: &IiNotificationPopupMap,
    grace: &IiNotificationGrace,
    generation: &Rc<Cell<u64>>,
    service: &crate::notification_service::NotificationServiceHandle,
    snapshot: &crate::notification_service::NotificationSnapshot,
    config: &ShellConfig,
    theme: &ThemeTokens,
) {
    let enabled = config.panel_family.as_deref() == Some("ii")
        && config
            .enabled_panels
            .iter()
            .any(|panel| panel == "iiNotificationPopup")
        && snapshot.state
            == crate::notification_service::NotificationSnapshotState::ReadySessionOnly;
    if !enabled {
        close_all_ii_notification_popups(cx, popups, grace);
        return;
    }

    if snapshot.live.is_empty() {
        if popups.borrow().is_empty() {
            return;
        }
        let output_sizes = cx
            .displays()
            .into_iter()
            .map(|display| (display.id(), display.bounds().size))
            .collect::<HashMap<_, _>>();
        let empty_popups = popups
            .borrow()
            .iter()
            .map(|(display, handle)| (*display, *handle))
            .collect::<Vec<_>>();
        for (display, handle) in empty_popups {
            let Some(size) = output_sizes.get(&display).copied() else {
                close_ii_notification_popup(cx, popups, display);
                continue;
            };
            let empty_input =
                ii_notification_popup_input(size, snapshot, &config.notifications, theme, service);
            let _ = handle.update(cx, |view, window, cx| {
                view.set_input(empty_input.clone(), cx);
                window.set_input_region(Some(&[]));
            });
        }
        let next = generation.get().wrapping_add(1);
        generation.set(next);
        let displays = popups.borrow().keys().copied().collect::<Vec<_>>();
        {
            let mut pending = grace.borrow_mut();
            for display in displays {
                pending.insert(display, next);
            }
        }
        let handles = popups.clone();
        let grace_state = grace.clone();
        let generation_state = generation.clone();
        let service = service.clone();
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(450))
                .await;
            let still_empty = service.snapshot().live.is_empty();
            if !still_empty {
                return;
            }
            let _ = cx.update(|cx| {
                let expired = grace_state
                    .borrow()
                    .iter()
                    .filter_map(|(display, expected)| (*expected == next).then_some(*display))
                    .collect::<Vec<_>>();
                if generation_state.get() != next {
                    return;
                }
                for display in expired {
                    close_ii_notification_popup(cx, &handles, display);
                    grace_state.borrow_mut().remove(&display);
                }
            });
        })
        .detach();
        return;
    }

    let next = generation.get().wrapping_add(1);
    generation.set(next);
    grace.borrow_mut().clear();
    let output_entries = cx
        .displays()
        .into_iter()
        .map(|display| (display.id(), niri_output_name(display.bounds())))
        .collect::<Vec<_>>();
    let has_any_match = output_entries.iter().any(|(_, name)| {
        name.as_ref()
            .is_some_and(|name| config.notifications.screen_list.contains(name))
    });
    let targets = output_entries
        .iter()
        .filter_map(|(id, name)| {
            ii_notification_output_selected(
                name.as_deref(),
                &config.notifications.screen_list,
                has_any_match,
            )
            .then_some(*id)
        })
        .collect::<HashSet<_>>();
    let existing = popups.borrow().keys().copied().collect::<Vec<_>>();
    for display in existing {
        if !targets.contains(&display) {
            close_ii_notification_popup(cx, popups, display);
        }
    }
    for display in targets {
        let Some(output) = cx
            .displays()
            .into_iter()
            .find(|output| output.id() == display)
        else {
            continue;
        };
        let input = ii_notification_popup_input(
            output.bounds().size,
            snapshot,
            &config.notifications,
            theme,
            service,
        );
        if let Some(handle) = popups.borrow().get(&display).copied() {
            let popup_input = input.clone();
            let _ = handle.update(cx, |view, window, cx| {
                view.set_input(popup_input, cx);
                let size = notification_popup_window_size(&input);
                window.resize(size);
            });
        } else {
            open_ii_notification_popup(cx, popups, display, input);
        }
    }
}

fn close_all_ii_notification_popups(
    cx: &mut App,
    popups: &IiNotificationPopupMap,
    grace: &IiNotificationGrace,
) {
    let displays = popups.borrow().keys().copied().collect::<Vec<_>>();
    for display in displays {
        close_ii_notification_popup(cx, popups, display);
    }
    grace.borrow_mut().clear();
}

fn notification_popup_window_size(
    input: &crate::families::ii::notification_popup::NotificationPopupInput,
) -> Size<Pixels> {
    let (width, height) = notification_popup_surface_dimensions(
        input.output_width_px,
        input.output_height_px,
        input.style.width,
        input.edge_margin_px,
        input.position,
    );
    Size::new(px(width), px(height))
}

fn notification_popup_surface_dimensions(
    output_width: f32,
    output_height: f32,
    content_width: f32,
    edge_margin: f32,
    position: crate::families::ii::notification_popup::NotificationPopupPosition,
) -> (f32, f32) {
    use crate::families::ii::notification_popup::notification_popup_geometry;
    let geometry = notification_popup_geometry(
        output_width,
        output_height,
        0.0,
        content_width,
        edge_margin,
        position,
    );
    let margin = edge_margin.max(0.0);
    let width = (geometry.width + margin * 2.0).min(output_width.max(0.0));
    let height = geometry.max_height.min(output_height.max(0.0)).max(1.0);
    (width, height)
}

fn close_ii_notification_popup(cx: &mut App, popups: &IiNotificationPopupMap, display: DisplayId) {
    let Some(handle) = popups.borrow_mut().remove(&display) else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window, _| window.remove_window()) {
        eprintln!("Could not close GPUI ii notification popup on {display:?}: {error:#}");
    }
}

fn ii_notification_popup_input(
    output_size: Size<Pixels>,
    snapshot: &crate::notification_service::NotificationSnapshot,
    config: &crate::config::NotificationsConfig,
    theme: &ThemeTokens,
    service: &crate::notification_service::NotificationServiceHandle,
) -> crate::families::ii::notification_popup::NotificationPopupInput {
    use crate::families::ii::notification_popup::{
        NotificationPopupInput, NotificationPopupPosition, NotificationPopupStyle,
    };
    let mut style = NotificationPopupStyle::default();
    style.surface = theme
        .colors
        .surface_elevated
        .or(theme.colors.surface)
        .unwrap_or(style.surface);
    style.card = theme.colors.surface.unwrap_or(style.card);
    style.foreground = theme.colors.foreground.unwrap_or(style.foreground);
    style.muted = theme.colors.foreground_muted.unwrap_or(style.muted);
    style.accent = theme.colors.accent.unwrap_or(style.accent);
    style.critical_surface = theme.colors.danger.unwrap_or(style.critical_surface);
    style.border = theme.colors.border.unwrap_or(style.border);
    style.radius = theme.shape.large;
    style.card_radius = theme.shape.medium;
    style.padding = theme.spacing.md;
    style.gap = theme.spacing.sm;
    // QML Appearance.sizes.notificationPopupWidth is 410 * fontSizeScale;
    // the current GPUI theme contract has no font-size-scale token.
    style.width = 410.0;
    let dismiss_service = service.clone();
    let on_dismiss = std::sync::Arc::new(move |id, reason| dismiss_service.dismiss(id, reason));
    let timeout_service = service.clone();
    let cancel_timeout = std::sync::Arc::new(move |id| timeout_service.cancel_timeout(id));
    let action_service = service.clone();
    let on_action =
        std::sync::Arc::new(move |id, action: String| action_service.invoke_action(id, &action));
    let on_copy = std::sync::Arc::new(copy_notification_body_to_wayland_clipboard);
    NotificationPopupInput {
        snapshot: snapshot.clone(),
        position: match config.position {
            crate::config::NotificationPopupPosition::TopLeft => NotificationPopupPosition::TopLeft,
            crate::config::NotificationPopupPosition::TopRight => {
                NotificationPopupPosition::TopRight
            }
            crate::config::NotificationPopupPosition::BottomLeft => {
                NotificationPopupPosition::BottomLeft
            }
            crate::config::NotificationPopupPosition::BottomRight => {
                NotificationPopupPosition::BottomRight
            }
        },
        output_width_px: f32::from(output_size.width),
        output_height_px: f32::from(output_size.height),
        edge_margin_px: config.edge_margin as f32,
        style,
        on_dismiss,
        cancel_timeout,
        on_action,
        on_copy,
    }
}

fn copy_notification_body_to_wayland_clipboard(body: String) {
    thread::spawn(move || {
        use std::io::Write;
        let Ok(mut child) = std::process::Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            return;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.as_bytes());
        }
        let _ = child.wait();
    });
}

fn open_ii_notification_popup(
    cx: &mut App,
    popups: &IiNotificationPopupMap,
    display_id: DisplayId,
    input: crate::families::ii::notification_popup::NotificationPopupInput,
) {
    use crate::families::ii::notification_popup::NotificationPopup;
    let horizontal = if matches!(
        input.position,
        crate::families::ii::notification_popup::NotificationPopupPosition::TopLeft
            | crate::families::ii::notification_popup::NotificationPopupPosition::BottomLeft
    ) {
        Anchor::LEFT
    } else {
        Anchor::RIGHT
    };
    let vertical = if matches!(
        input.position,
        crate::families::ii::notification_popup::NotificationPopupPosition::TopLeft
            | crate::families::ii::notification_popup::NotificationPopupPosition::TopRight
    ) {
        Anchor::TOP
    } else {
        Anchor::BOTTOM
    };
    let bounds = Bounds {
        origin: point(px(0.), px(0.)),
        size: notification_popup_window_size(&input),
    };
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-ii-notification-popup".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-ii-notification-popup".into(),
            layer: Layer::Overlay,
            anchor: vertical | horizontal,
            exclusive_zone: Some(px(0.)),
            exclusive_edge: None,
            margin: Some((px(0.), px(0.), px(0.), px(0.))),
            keyboard_interactivity: KeyboardInteractivity::None,
        }),
        ..Default::default()
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|_| NotificationPopup::new(input))
    }) {
        Ok(handle) => {
            popups.borrow_mut().insert(display_id, handle);
        }
        Err(error) => {
            eprintln!("Could not open GPUI ii notification popup on {display_id:?}: {error:#}")
        }
    }
}

fn close_task_view_overlay(cx: &mut App, overlays: &TaskViewOverlayMap, display: DisplayId) {
    let handle = overlays.borrow_mut().remove(&display);
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

fn close_action_center_overlay(
    cx: &mut App,
    overlays: &ActionCenterOverlayMap,
    display: DisplayId,
) {
    let handle = overlays.borrow_mut().remove(&display);
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

fn close_clipboard_overlay(cx: &mut App, overlays: &ClipboardOverlayMap, display: DisplayId) {
    let handle = overlays.borrow_mut().remove(&display);
    if let Some(handle) = handle {
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
    let handle = overlays.borrow_mut().remove(&display);
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
        restore_ii_overview_focus(cx, horizontal, vertical, display);
    }
}

fn open_ii_clipboard_surface(
    cx: &mut App,
    surfaces: &IiClipboardSurfaceMap,
    generations: &IiClipboardGenerationMap,
    display_id: DisplayId,
    theme: &ThemeTokens,
    events: &Sender<IiClipboardSurfaceEvent>,
    pins: Vec<String>,
) -> bool {
    if surfaces.borrow().contains_key(&display_id) {
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
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-ii-clipboard".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-ii-clipboard".into(),
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
        cx.new(|cx| {
            crate::families::ii::clipboard_host::IiClipboardHost::new(
                theme, display_id, pins, events, cx,
            )
        })
    }) {
        Ok(handle) => {
            surfaces.borrow_mut().insert(display_id, handle);
            generations.borrow_mut().entry(display_id).or_insert(0);
            true
        }
        Err(error) => {
            eprintln!("Could not open ii Clipboard on {display_id:?}: {error:#}");
            false
        }
    }
}

fn open_ii_cheatsheet_surface(
    cx: &mut App,
    surfaces: &IiCheatsheetSurfaceMap,
    display_id: DisplayId,
    theme: &ThemeTokens,
    events: &Sender<IiCheatsheetEvent>,
) -> bool {
    if surfaces.borrow().contains_key(&display_id) {
        return false;
    }
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return false;
    };
    let output_bounds = display.bounds();
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(output_bounds)),
        app_id: Some("intoo-gpui-ii-cheatsheet".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-ii-cheatsheet".into(),
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
    let paths = crate::families::ii::cheatsheet_host::current_provider_paths();
    let events = events.clone();
    match cx.open_window(options, move |_, cx| {
        cx.new(|cx| {
            crate::families::ii::cheatsheet_host::IiCheatsheetHost::new(
                display_id,
                output_bounds.size,
                theme,
                paths,
                events,
                cx,
            )
        })
    }) {
        Ok(handle) => {
            surfaces.borrow_mut().insert(display_id, handle);
            true
        }
        Err(error) => {
            eprintln!("Could not open ii Cheatsheet on {display_id:?}: {error:#}");
            false
        }
    }
}

fn close_ii_cheatsheet_surface(
    cx: &mut App,
    surfaces: &IiCheatsheetSurfaceMap,
    display: DisplayId,
) {
    if let Some(handle) = surfaces.borrow_mut().remove(&display) {
        if let Err(error) = handle.update(cx, |_, window, _| window.remove_window()) {
            eprintln!("Could not close ii Cheatsheet for {display:?}: {error:#}");
        }
    }
}

fn close_ii_clipboard_surface(
    cx: &mut App,
    surfaces: &IiClipboardSurfaceMap,
    generations: &IiClipboardGenerationMap,
    display: DisplayId,
) {
    generations.borrow_mut().remove(&display);
    if let Some(handle) = surfaces.borrow_mut().remove(&display) {
        let _ = handle.update(cx, |_, window: &mut Window, _| window.remove_window());
    }
}

fn ii_clipboard_host_generation_is_current(
    cx: &mut gpui::AsyncApp,
    surfaces: &IiClipboardSurfaceMap,
    display: DisplayId,
    generation: u64,
) -> bool {
    surfaces
        .borrow()
        .get(&display)
        .copied()
        .and_then(|handle| {
            handle
                .update(cx, |view, _, _| view.generation() == generation)
                .ok()
        })
        .unwrap_or(false)
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
    service: &crate::notification_service::NotificationServiceHandle,
    snapshot: &crate::notification_service::NotificationSnapshot,
    expanded_groups_by_display: &HashMap<DisplayId, HashSet<String>>,
    group_events: &Sender<(DisplayId, String)>,
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
    let expanded_groups = expanded_groups_by_display
        .get(&display_id)
        .cloned()
        .unwrap_or_default();
    let toggle_events = group_events.clone();
    let on_toggle_group: crate::families::waffle::notification_center::ItemAction =
        std::sync::Arc::new(move |app_id| {
            let _ = toggle_events.try_send((display_id, app_id));
        });
    let input = crate::families::waffle::notification_overlay::session_input(
        theme,
        available_height,
        snapshot,
        service,
        &expanded_groups,
        on_toggle_group,
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
        media_actions: media_actions.clone(),
        media_state: media_cache.borrow().clone(),
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

fn niri_output_name_unique_after(
    bounds: Bounds<Pixels>,
    required_attempt_after: Option<Instant>,
) -> Option<String> {
    let fresh_outputs = NIRI_OUTPUT_SNAPSHOT
        .get_or_init(|| RwLock::new(NiriOutputSnapshotState::default()))
        .read()
        .ok()
        .and_then(|snapshot| {
            snapshot
                .last_success
                .as_ref()
                .and_then(|(sampled, outputs)| {
                    let snapshot_is_recent = sampled.elapsed() < Duration::from_secs(1);
                    let snapshot_is_after_invalidation =
                        required_attempt_after.is_none_or(|invalidated_at| {
                            snapshot_attempt_is_after(snapshot.last_success_attempt, invalidated_at)
                        });
                    (snapshot_is_recent && snapshot_is_after_invalidation).then(|| outputs.clone())
                })
        });
    if fresh_outputs.is_none() {
        refresh_niri_output_snapshot();
    }
    unique_niri_output_name_for_bounds(bounds, &fresh_outputs?)
}

fn snapshot_attempt_is_after(attempted: Option<Instant>, invalidated_at: Instant) -> bool {
    attempted.is_some_and(|attempted| attempted >= invalidated_at)
}

fn niri_snapshot_attempted_after(invalidated_at: Instant) -> bool {
    NIRI_OUTPUT_SNAPSHOT
        .get_or_init(|| RwLock::new(NiriOutputSnapshotState::default()))
        .read()
        .ok()
        .is_some_and(|snapshot| {
            snapshot.last_success.is_some()
                && snapshot_attempt_is_after(snapshot.last_success_attempt, invalidated_at)
        })
}

fn should_poll_niri_output_map(periodic_waffle_host: bool, iris_preview_count: usize) -> bool {
    periodic_waffle_host || iris_preview_count > 0
}

fn mark_iris_preview_mappings_pending(
    previews: &IrisBackgroundPreviewMap,
    pending: &IrisOutputMappingPending,
) {
    let displays = previews.borrow().keys().copied().collect::<Vec<_>>();
    let invalidated_at = Instant::now();
    let mut pending = pending.borrow_mut();
    for display in displays {
        pending.insert(display, invalidated_at);
    }
}

fn unique_niri_output_name_for_bounds(
    bounds: Bounds<Pixels>,
    outputs: &[NiriOutputGeometry],
) -> Option<String> {
    let close = |a: f64, b: f64| (a - b).abs() < 1.0;
    let mut matches = outputs.iter().filter(|output| {
        close(output.x, f64::from(f32::from(bounds.origin.x)))
            && close(output.y, f64::from(f32::from(bounds.origin.y)))
            && close(output.width, f64::from(f32::from(bounds.size.width)))
            && close(output.height, f64::from(f32::from(bounds.size.height)))
    });
    let output = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(output.name.clone())
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
    thread::spawn(move || {
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
                    snapshot.last_success_attempt = Some(attempted);
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

/// Open one explicit, preview-only Iris background surface. The selected
/// `DisplayId` is exact; the compositor output name is used only when the
/// current Niri snapshot matches this display's bounds. Without that mapping,
/// the resolver receives an empty identity and supplies an opaque fallback.
fn open_iris_background_preview_for_display(
    cx: &mut App,
    previews: &IrisBackgroundPreviewMap,
    mapping_pending_since: Option<Instant>,
    display_id: DisplayId,
    config: &ShellConfig,
    theme: &ThemeTokens,
) {
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let output_name =
        niri_output_name_unique_after(display.bounds(), mapping_pending_since).unwrap_or_default();
    let policy = resolve_output_background(config, &output_name, MotionRuntimeInputs::default());
    let input = IrisBackgroundPreviewInput {
        policy,
        background: theme
            .colors
            .background
            .unwrap_or_else(|| gpui::rgba(0x111318ff).into()),
        foreground: theme
            .colors
            .foreground
            .unwrap_or_else(|| gpui::rgba(0xf2f4f8ff).into()),
    };

    let existing_handle = previews.borrow().get(&display_id).copied();
    if let Some(handle) = existing_handle {
        let new_size = display.bounds().size;
        let replacement_input = input.clone();
        match handle.update(cx, |view, window, cx| {
            view.input = replacement_input;
            window.resize(new_size);
            cx.notify();
        }) {
            Ok(()) => return,
            Err(error) => {
                remove_preview_registration(&mut previews.borrow_mut(), &display_id);
                eprintln!(
                    "Discarding stale GPUI Iris background preview handle for {display_id:?}: {error:#}"
                );
            }
        }
    }

    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: display.bounds().size,
    };
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("intoo-gpui-iris-background-preview".to_owned()),
        // The rendered root has an opaque fallback color, so it covers any
        // lower wallpaper owner for this opt-in test display at all times.
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-iris-background-preview".to_owned(),
            layer: Layer::Bottom,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(px(0.)),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    };
    match cx.open_window(options, move |_, cx| {
        cx.new(|_| IrisBackgroundPreviewView::new(input))
    }) {
        Ok(handle) => {
            let _ = handle.update(cx, |_, window, _| window.set_input_region(Some(&[])));
            previews.borrow_mut().insert(display_id, handle);
        }
        Err(error) => eprintln!(
            "Could not create opt-in GPUI Iris background preview for {display_id:?}: {error:#}"
        ),
    }
}

fn close_iris_background_preview_for_display(
    cx: &mut App,
    previews: &IrisBackgroundPreviewMap,
    display_id: DisplayId,
) {
    let Some(handle) = remove_preview_registration(&mut previews.borrow_mut(), &display_id) else {
        return;
    };
    if let Err(error) = handle.update(cx, |_, window, _| window.remove_window()) {
        eprintln!(
            "Could not close opt-in GPUI Iris background preview for {display_id:?}: {error:#}"
        );
    }
}

fn split_registered_preview_displays<K>(
    registered: impl IntoIterator<Item = K>,
    live_displays: impl IntoIterator<Item = K>,
) -> (Vec<K>, Vec<K>)
where
    K: Copy + Eq + Hash,
{
    let live = live_displays.into_iter().collect::<HashSet<_>>();
    let mut active = Vec::new();
    let mut stale = Vec::new();
    for display in registered {
        if live.contains(&display) {
            active.push(display);
        } else {
            stale.push(display);
        }
    }
    (active, stale)
}

fn remove_preview_registration<K, V>(registrations: &mut HashMap<K, V>, key: &K) -> Option<V>
where
    K: Eq + Hash,
{
    registrations.remove(key)
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
    let handle = surfaces.borrow_mut().remove(&display_id);
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

fn ii_osd_output_selected(
    output_name: Option<&str>,
    screen_list: &[String],
    has_match: bool,
) -> bool {
    screen_list.is_empty()
        || !has_match
        || output_name.is_some_and(|name| screen_list.iter().any(|selected| selected == name))
}

fn reconcile_active_ii_osd(
    cx: &mut App,
    surfaces: &IiOsdSurfaceMap,
    active: &Rc<RefCell<Option<IiOsdTrigger>>>,
    generation: &Rc<Cell<u64>>,
    config: &ShellConfig,
    theme: &ThemeTokens,
    events: &async_channel::Sender<IiOsdHostEvent>,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
) {
    let Some(trigger) = active.borrow().clone() else {
        return;
    };
    let enabled = reconcile_ii_osd(cx, surfaces, &trigger, config, theme, events, media_actions);
    if !enabled {
        *active.borrow_mut() = None;
        generation.set(generation.get().wrapping_add(1));
    }
}

fn is_mpris_transport_action(action: &crate::components::vertical_bar::MediaAction) -> bool {
    use crate::components::vertical_bar::{MediaAction, PlayerMediaCommand};
    matches!(
        action,
        MediaAction::TogglePlayback | MediaAction::Previous | MediaAction::Next
    ) || matches!(
        action,
        MediaAction::ForPlayer {
            command: PlayerMediaCommand::TogglePlayback
                | PlayerMediaCommand::Previous
                | PlayerMediaCommand::Next,
            ..
        }
    )
}

fn media_state_changed_for_osd(
    before: &crate::components::vertical_bar::MediaState,
    after: &crate::components::vertical_bar::MediaState,
) -> bool {
    use crate::components::vertical_bar::MediaStatus;
    after.status != MediaStatus::Unavailable
        && after.status != MediaStatus::NoPlayer
        && (before.status != after.status
            || before.title != after.title
            || before.artist != after.artist
            || before.players.iter().any(|old_player| {
                after
                    .players
                    .iter()
                    .find(|player| player.service_bus_name == old_player.service_bus_name)
                    .is_some_and(|player| {
                        player.track_id != old_player.track_id
                            || player.art_url != old_player.art_url
                    })
            }))
}

fn ii_osd_trigger_enabled(config: &ShellConfig, trigger: &IiOsdTrigger) -> bool {
    config.panel_family.as_deref() == Some("ii")
        && config
            .enabled_panels
            .iter()
            .any(|panel| panel == "iiOnScreenDisplay")
        && (!matches!(
            &trigger.indicator,
            crate::families::ii::osd::OsdIndicator::Media { .. }
        ) || config.osd.media_enabled)
}

fn media_osd_enabled(
    config: &ShellConfig,
    state: &crate::components::vertical_bar::MediaState,
) -> bool {
    use crate::components::vertical_bar::MediaStatus;
    config.panel_family.as_deref() == Some("ii")
        && config
            .enabled_panels
            .iter()
            .any(|panel| panel == "iiOnScreenDisplay")
        && config.osd.media_enabled
        && !matches!(
            state.status,
            MediaStatus::Unavailable | MediaStatus::NoPlayer
        )
}

fn media_state_art_url(state: &crate::components::vertical_bar::MediaState) -> Option<String> {
    state
        .players
        .iter()
        .find(|player| {
            player.status == state.status
                && player.title == state.title
                && player.artist == state.artist
        })
        .and_then(|player| player.art_url.clone())
}

fn mpris_sample_interval(media_osd_visible: bool) -> Duration {
    if media_osd_visible {
        // Media OSD gets the configured timeout plus a 1.5s grace. At the
        // default 2s timeout, 250ms samples leave several opportunities to
        // observe track/art changes before the 3.5s visibility lease expires.
        Duration::from_millis(250)
    } else {
        Duration::from_secs(2)
    }
}

fn should_refresh_active_media_osd(
    active: Option<&IiOsdTrigger>,
    config: &ShellConfig,
    state: &crate::components::vertical_bar::MediaState,
    changed: bool,
) -> bool {
    changed
        && active.is_some_and(|trigger| {
            matches!(
                &trigger.indicator,
                crate::families::ii::osd::OsdIndicator::Media { .. }
            )
        })
        && media_osd_enabled(config, state)
}

fn publish_media_state(
    cx: &mut App,
    horizontal_surfaces: &SurfaceMap,
    vertical_surfaces: &VerticalSurfaceMap,
    panel_surfaces: &IiPanelSurfaceMap,
    controls_surfaces: &MediaControlsSurfaceMap,
    state: crate::components::vertical_bar::MediaState,
) {
    let horizontal = horizontal_surfaces
        .borrow()
        .values()
        .copied()
        .collect::<Vec<_>>();
    let vertical = vertical_surfaces
        .borrow()
        .values()
        .copied()
        .collect::<Vec<_>>();
    let panels = panel_surfaces
        .borrow()
        .values()
        .copied()
        .collect::<Vec<_>>();
    let controls = controls_surfaces
        .borrow()
        .values()
        .copied()
        .collect::<Vec<_>>();
    for handle in horizontal {
        let state = state.clone();
        let _ = handle.update(cx, |host, _, cx| host.set_media_state(state, cx));
    }
    for handle in vertical {
        let state = state.clone();
        let _ = handle.update(cx, |host, _, cx| host.set_media_state(state, cx));
    }
    for handle in panels {
        let state = state.clone();
        let _ = handle.update(cx, |view, _, cx| view.set_media_state(state, cx));
    }
    for handle in controls {
        let state = state.clone();
        let _ = handle.update(cx, |view, _, cx| view.set_media_state(state, cx));
    }
}

fn reconcile_ii_osd(
    cx: &mut App,
    surfaces: &IiOsdSurfaceMap,
    trigger: &IiOsdTrigger,
    config: &ShellConfig,
    theme: &ThemeTokens,
    events: &async_channel::Sender<IiOsdHostEvent>,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
) -> bool {
    let enabled = ii_osd_trigger_enabled(config, trigger);
    if !enabled {
        close_all_ii_osd(cx, surfaces);
        return false;
    }

    let outputs = cx.displays();
    let entries = outputs
        .iter()
        .map(|display| (display.id(), niri_output_name(display.bounds())))
        .collect::<Vec<_>>();
    let has_match = entries.iter().any(|(_, name)| {
        name.as_ref().is_some_and(|name| {
            config
                .osd
                .screen_list
                .iter()
                .any(|selected| selected == name)
        })
    });
    let targets = entries
        .iter()
        .filter_map(|(id, name)| {
            ii_osd_output_selected(name.as_deref(), &config.osd.screen_list, has_match)
                .then_some(*id)
        })
        .collect::<HashSet<_>>();

    let existing = surfaces.borrow().keys().copied().collect::<Vec<_>>();
    for display in existing {
        if !targets.contains(&display) {
            close_ii_osd_surface(cx, surfaces, display);
        }
    }
    for display in targets {
        let Some(output) = outputs.iter().find(|output| output.id() == display) else {
            continue;
        };
        let input = ii_osd_input(
            output.id(),
            output.bounds().size,
            config,
            theme,
            trigger,
            events,
            media_actions,
        );
        if let Some(handle) = surfaces.borrow().get(&display).copied() {
            let region = ii_osd_input_region(&input);
            let _ = handle.update(cx, |view, window, cx| {
                window.resize(output.bounds().size);
                view.set_input(input, cx);
                window.set_input_region(Some(&region));
            });
        } else {
            open_ii_osd_surface(cx, surfaces, display, input);
        }
    }
    true
}

fn ii_osd_input(
    display: DisplayId,
    output_size: Size<Pixels>,
    config: &ShellConfig,
    theme: &ThemeTokens,
    trigger: &IiOsdTrigger,
    events: &async_channel::Sender<IiOsdHostEvent>,
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
) -> crate::families::ii::osd::OsdInput {
    use crate::families::ii::{
        osd::OsdInput,
        sidebars::{SidebarHeaderStyle, SidebarSizeMode, SidebarStyle},
    };
    let colors = &theme.colors;
    let style = SidebarStyle {
        surface: colors
            .surface
            .or(colors.background)
            .unwrap_or_else(|| gpui::rgba(0x161b22f5).into()),
        card: colors
            .surface_elevated
            .or(colors.surface)
            .unwrap_or_else(|| gpui::rgba(0x21262df5).into()),
        hover: colors
            .selection
            .unwrap_or_else(|| gpui::rgba(0x30363ddd).into()),
        selected: colors
            .selection
            .unwrap_or_else(|| gpui::rgba(0x30363ddd).into()),
        foreground: colors.foreground.unwrap_or_else(|| gpui::white().into()),
        muted: colors
            .foreground_muted
            .unwrap_or_else(|| gpui::rgba(0xb1bac4ff).into()),
        accent: colors
            .accent
            .unwrap_or_else(|| gpui::rgba(0x79c0ffff).into()),
        border: colors
            .border
            .unwrap_or_else(|| gpui::rgba(0x30363dff).into()),
        radius: theme.shape.large,
        card_radius: theme.shape.medium,
        padding: theme.spacing.md,
        gap: theme.spacing.sm,
        width: 360.0,
        height: f32::from(output_size.height),
        compact: false,
        header_style: SidebarHeaderStyle::Profile,
        size_mode: SidebarSizeMode::Content,
    };
    let send_hide = events.clone();
    let on_dismiss = std::sync::Arc::new(move || {
        let _ = send_hide.try_send(IiOsdHostEvent::Hide);
    });
    let send_hover = events.clone();
    let on_hover_changed = std::sync::Arc::new(move |entered, is_media| {
        if is_media {
            let _ = send_hover.try_send(IiOsdHostEvent::Hover { display, entered });
        }
    });
    let send_media_action = media_actions.clone();
    let on_media_action = std::sync::Arc::new(move |action| {
        let _ = send_media_action.try_send(action);
    });
    let top_notice = matches!(
        &trigger.indicator,
        crate::families::ii::osd::OsdIndicator::KeyboardLayout { .. }
            | crate::families::ii::osd::OsdIndicator::Connection { .. }
    );
    OsdInput {
        indicator: trigger.indicator.clone(),
        availability: trigger.availability.clone(),
        style,
        output_width: f32::from(output_size.width),
        output_height: f32::from(output_size.height),
        bar_height: config.bar.height.unwrap_or(48.0).max(0.0) as f32,
        bar_bottom: config.bar.bottom.unwrap_or(false),
        top_notice,
        edge_margin: 12.0,
        on_dismiss,
        on_hover_changed,
        on_media_action,
    }
}

fn ii_osd_input_region(input: &crate::families::ii::osd::OsdInput) -> Vec<Bounds<Pixels>> {
    let media = matches!(
        &input.indicator,
        crate::families::ii::osd::OsdIndicator::Media { .. }
    );
    let geometry = crate::families::ii::osd::OsdGeometry::from_output_with_media(
        input.output_width,
        input.output_height,
        input.bar_height,
        input.bar_bottom,
        input.top_notice
            || matches!(
                &input.indicator,
                crate::families::ii::osd::OsdIndicator::KeyboardLayout { .. }
                    | crate::families::ii::osd::OsdIndicator::Connection { .. }
            ),
        input.edge_margin,
        media,
    );
    if geometry.width <= 0.0 || geometry.height <= 0.0 {
        return Vec::new();
    }
    vec![Bounds {
        origin: point(px(geometry.left), px(geometry.top)),
        size: Size::new(px(geometry.width), px(geometry.height)),
    }]
}

fn open_ii_osd_surface(
    cx: &mut App,
    surfaces: &IiOsdSurfaceMap,
    display_id: DisplayId,
    input: crate::families::ii::osd::OsdInput,
) {
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.id() == display_id)
    else {
        return;
    };
    let output_size = display.bounds().size;
    let top_notice = input.top_notice
        || matches!(
            &input.indicator,
            crate::families::ii::osd::OsdIndicator::KeyboardLayout { .. }
                | crate::families::ii::osd::OsdIndicator::Connection { .. }
        );
    let region = ii_osd_input_region(&input);
    let options = WindowOptions {
        titlebar: None,
        display_id: Some(display_id),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size: output_size,
        })),
        app_id: Some("intoo-gpui-ii-osd".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "intoo-gpui-ii-osd".into(),
            layer: if top_notice {
                Layer::Top
            } else {
                Layer::Overlay
            },
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(px(0.)),
            exclusive_edge: None,
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    };
    match cx.open_window(options, move |_, cx| cx.new(|_| IiOsdSurface { input })) {
        Ok(handle) => {
            let _ = handle.update(cx, |_, window, _| window.set_input_region(Some(&region)));
            surfaces.borrow_mut().insert(display_id, handle);
        }
        Err(error) => eprintln!("Could not create GPUI ii OSD on {display_id:?}: {error:#}"),
    }
}

fn close_all_ii_osd(cx: &mut App, surfaces: &IiOsdSurfaceMap) {
    let displays = surfaces.borrow().keys().copied().collect::<Vec<_>>();
    for display in displays {
        close_ii_osd_surface(cx, surfaces, display);
    }
}

fn close_ii_osd_surface(cx: &mut App, surfaces: &IiOsdSurfaceMap, display: DisplayId) {
    if let Some(handle) = surfaces.borrow_mut().remove(&display) {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

fn schedule_ii_osd_timeout(
    cx: &mut App,
    surfaces: &IiOsdSurfaceMap,
    generation: &Rc<Cell<u64>>,
    active: &Rc<RefCell<Option<IiOsdTrigger>>>,
    config: &crate::config::OsdConfig,
    trigger: &IiOsdTrigger,
) {
    let timeout = config.timeout_ms.saturating_add(
        if matches!(
            &trigger.indicator,
            crate::families::ii::osd::OsdIndicator::Media { .. }
                | crate::families::ii::osd::OsdIndicator::Connection { .. }
        ) {
            1_500
        } else {
            0
        },
    );
    let next = generation.get().wrapping_add(1);
    generation.set(next);
    let generation = generation.clone();
    let active = active.clone();
    let surfaces = surfaces.clone();
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_millis(timeout))
            .await;
        let _ = cx.update(|cx| {
            if generation.get() != next {
                return;
            }
            *active.borrow_mut() = None;
            close_all_ii_osd(cx, &surfaces);
        });
    })
    .detach();
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
    media_actions: &async_channel::Sender<crate::components::vertical_bar::MediaAction>,
    media_cache: &MediaStateCache,
) {
    let Some(handle) = surfaces.borrow().get(&display_id).copied() else {
        open_for_display(
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

    let media_state = media_cache.borrow().clone();
    if let Err(error) = handle.update(cx, |host, window: &mut Window, cx| {
        let height = px(config.bar.height.unwrap_or(48.0).clamp(24.0, 128.0) as f32);
        window.resize(Size::new(display.bounds().size.width, height));
        host.set_media_state(media_state, cx);
    }) {
        eprintln!("Could not resize GPUI layer surface for {display_id:?}: {error:#}");
        close_for_display(cx, surfaces, display_id);
        open_for_display(
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

#[cfg(test)]
mod launcher_policy_tests {
    use super::{
        NiriOutputGeometry, app_launcher_enabled, ii_notification_output_selected,
        ii_osd_output_selected, ii_osd_trigger_enabled, is_mpris_transport_action,
        media_controls_dock_offset, media_controls_typography_scale, media_state_changed_for_osd,
        mpris_sample_interval, notification_popup_surface_dimensions, remove_preview_registration,
        should_poll_niri_output_map, should_refresh_active_media_osd, snapshot_attempt_is_after,
        split_registered_preview_displays, unique_niri_output_name_for_bounds,
    };
    use crate::config::ShellConfig;

    #[test]
    fn ii_apps_launcher_does_not_require_waffle_panel_flag() {
        let ii = ShellConfig {
            panel_family: Some("ii".into()),
            enabled_panels: Vec::new(),
            ..ShellConfig::default()
        };
        assert!(app_launcher_enabled(&ii));
    }

    #[test]
    fn removed_preview_display_is_pruned_before_later_output_refreshes() {
        let (live, stale) = split_registered_preview_displays([10_u8, 20], [20_u8, 30]);
        assert_eq!(live, [20]);
        assert_eq!(stale, [10]);

        // The close path removes stale ids from the registered map. A later
        // Niri update may contain that output name, but it cannot recreate a
        // preview without a registered explicit DisplayId.
        let (live_after_refresh, stale_after_refresh) =
            split_registered_preview_displays(live, [10_u8, 20, 30]);
        assert_eq!(live_after_refresh, [20]);
        assert!(stale_after_refresh.is_empty());
    }

    #[test]
    fn iris_output_mapping_rejects_ambiguous_bounds_and_failed_handles_are_unregistered() {
        use gpui::{Bounds, Size, point, px};
        use std::collections::HashMap;

        let bounds = Bounds {
            origin: point(px(0.), px(0.)),
            size: Size::new(px(1920.), px(1080.)),
        };
        let output = |name: &str| NiriOutputGeometry {
            name: name.to_owned(),
            x: 0.,
            y: 0.,
            width: 1920.,
            height: 1080.,
        };
        assert_eq!(
            unique_niri_output_name_for_bounds(bounds, &[output("DP-1")]),
            Some("DP-1".into())
        );
        assert_eq!(
            unique_niri_output_name_for_bounds(bounds, &[output("DP-1"), output("HDMI-A-1")]),
            None
        );

        let mut registrations = HashMap::from([(7_u8, "stale handle")]);
        assert_eq!(
            remove_preview_registration(&mut registrations, &7),
            Some("stale handle")
        );
        assert!(!registrations.contains_key(&7));
    }

    #[test]
    fn iris_bounds_invalidation_rejects_old_snapshot_attempts_and_polls_only_when_needed() {
        use std::time::{Duration, Instant};

        let invalidated_at = Instant::now();
        assert!(!snapshot_attempt_is_after(
            Some(invalidated_at - Duration::from_millis(1)),
            invalidated_at
        ));
        assert!(snapshot_attempt_is_after(
            Some(invalidated_at + Duration::from_millis(1)),
            invalidated_at
        ));
        assert!(!snapshot_attempt_is_after(None, invalidated_at));

        assert!(!should_poll_niri_output_map(false, 0));
        assert!(should_poll_niri_output_map(false, 1));
        assert!(should_poll_niri_output_map(true, 0));
    }

    #[test]
    fn waffle_start_menu_still_requires_enabled_panel() {
        let mut waffle = ShellConfig {
            panel_family: Some("waffle".into()),
            ..ShellConfig::default()
        };
        assert!(!app_launcher_enabled(&waffle));
        waffle.enabled_panels.push("wStartMenu".into());
        assert!(app_launcher_enabled(&waffle));
    }

    #[test]
    fn media_controls_offset_tracks_configured_dock_height() {
        let mut config = ShellConfig::default();
        assert_eq!(media_controls_dock_offset(&config, 1.0), 80.0);

        config
            .appearance
            .source_sections
            .insert("ii.dock".into(), serde_json::json!({ "height": 72.0 }));
        assert_eq!(media_controls_dock_offset(&config, 1.5), 97.0);
        assert_eq!(media_controls_dock_offset(&config, 1.25), 95.0);
    }

    #[test]
    fn media_controls_scale_uses_configured_typography_range() {
        let mut config = ShellConfig::default();
        assert_eq!(media_controls_typography_scale(&config), 1.0);

        config.appearance.source_sections.insert(
            "typography".into(),
            serde_json::json!({ "sizeScale": 1.25 }),
        );
        assert_eq!(media_controls_typography_scale(&config), 1.25);

        config
            .appearance
            .source_sections
            .insert("typography".into(), serde_json::json!({ "sizeScale": 4.0 }));
        assert_eq!(media_controls_typography_scale(&config), 2.0);
    }

    #[test]
    fn ii_notification_output_filter_falls_back_to_all_outputs_when_stale() {
        let selected = vec!["DP-1".to_owned()];
        assert!(ii_notification_output_selected(
            Some("eDP-1"),
            &selected,
            false
        ));
        assert!(ii_notification_output_selected(None, &selected, false));
        assert!(ii_notification_output_selected(
            Some("DP-1"),
            &selected,
            true
        ));
        assert!(!ii_notification_output_selected(
            Some("eDP-1"),
            &selected,
            true
        ));
        assert!(ii_notification_output_selected(Some("eDP-1"), &[], false));
    }

    #[test]
    fn ii_notification_surface_dimensions_include_corner_insets_and_stay_bounded() {
        use crate::families::ii::notification_popup::NotificationPopupPosition as Position;
        for position in [
            Position::TopLeft,
            Position::TopRight,
            Position::BottomLeft,
            Position::BottomRight,
        ] {
            let (width, height) =
                notification_popup_surface_dimensions(1920.0, 1080.0, 410.0, 12.0, position);
            assert_eq!(width, 434.0);
            assert_eq!(height, 864.0);
        }
        let (small_width, small_height) =
            notification_popup_surface_dimensions(160.0, 120.0, 410.0, 8.0, Position::BottomRight);
        assert_eq!(small_width, 160.0);
        assert_eq!(small_height, 96.0);
    }

    #[test]
    fn ii_osd_output_filter_falls_back_to_all_outputs_when_stale() {
        let selected = vec!["DP-1".to_owned()];
        assert!(ii_osd_output_selected(Some("eDP-1"), &selected, false));
        assert!(ii_osd_output_selected(None, &selected, false));
        assert!(ii_osd_output_selected(Some("DP-1"), &selected, true));
        assert!(!ii_osd_output_selected(Some("eDP-1"), &selected, true));
        assert!(ii_osd_output_selected(Some("eDP-1"), &[], false));
    }

    #[test]
    fn ii_osd_active_policy_invalidates_when_family_or_panel_gate_turns_off() {
        use crate::families::ii::osd::{OsdAvailability, OsdIndicator};
        let trigger = super::IiOsdTrigger {
            indicator: OsdIndicator::Volume {
                value: Some(0.5),
                muted: Some(false),
                protection_message: None,
            },
            availability: OsdAvailability::Ready,
            auto_hide: true,
        };
        let mut config = ShellConfig {
            panel_family: Some("ii".into()),
            enabled_panels: vec!["iiOnScreenDisplay".into()],
            ..ShellConfig::default()
        };
        assert!(ii_osd_trigger_enabled(&config, &trigger));
        config.enabled_panels.clear();
        assert!(!ii_osd_trigger_enabled(&config, &trigger));
        config.enabled_panels.push("iiOnScreenDisplay".into());
        config.panel_family = Some("iris".into());
        assert!(!ii_osd_trigger_enabled(&config, &trigger));
    }

    #[test]
    fn ii_media_osd_requires_media_gate_and_only_observed_transport_changes() {
        use crate::components::vertical_bar::{
            MediaAction, MediaPlayerSnapshot, MediaState, MediaStatus, PlayerMediaCommand,
        };
        use crate::families::ii::osd::{OsdAvailability, OsdIndicator};
        let mut config = ShellConfig {
            panel_family: Some("ii".into()),
            enabled_panels: vec!["iiOnScreenDisplay".into()],
            ..ShellConfig::default()
        };
        let trigger = super::IiOsdTrigger {
            indicator: OsdIndicator::Media {
                title: Some("Track".into()),
                artist: None,
                art_url: None,
                playing: Some(true),
                can_previous: true,
                can_toggle: true,
                can_next: true,
            },
            availability: OsdAvailability::Ready,
            auto_hide: true,
        };
        assert!(ii_osd_trigger_enabled(&config, &trigger));
        config.osd.media_enabled = false;
        assert!(!ii_osd_trigger_enabled(&config, &trigger));
        assert!(is_mpris_transport_action(&MediaAction::ForPlayer {
            service_bus_name: "org.mpris.MediaPlayer2.test".into(),
            command: PlayerMediaCommand::Next,
        }));
        assert!(!is_mpris_transport_action(&MediaAction::VolumeDelta(0.1)));

        let state = |title: &str| MediaState {
            status: MediaStatus::Playing,
            title: Some(title.into()),
            artist: None,
            position_percent: None,
            can_toggle_playback: true,
            can_previous: true,
            can_next: true,
            can_change_volume: false,
            players: vec![MediaPlayerSnapshot {
                service_bus_name: "org.mpris.MediaPlayer2.test".into(),
                identity: None,
                title: Some(title.into()),
                artist: None,
                album: None,
                source_url: None,
                art_url: None,
                track_id: Some(title.into()),
                status: MediaStatus::Playing,
                position_seconds: None,
                duration_seconds: None,
                position_microseconds: None,
                duration_microseconds: None,
                can_toggle_playback: true,
                can_previous: true,
                can_next: true,
                can_seek: false,
                can_change_volume: false,
            }],
        };
        assert!(media_state_changed_for_osd(&state("old"), &state("new")));
        assert!(!media_state_changed_for_osd(&state("same"), &state("same")));
        let before_art = state("same");
        let mut after_art = state("same");
        after_art.players[0].art_url = Some("file:///cover.png".into());
        assert!(media_state_changed_for_osd(&before_art, &after_art));
        let active_trigger = super::IiOsdTrigger {
            indicator: OsdIndicator::Media {
                title: Some("same".into()),
                artist: None,
                art_url: None,
                playing: Some(true),
                can_previous: true,
                can_toggle: true,
                can_next: true,
            },
            availability: OsdAvailability::Ready,
            auto_hide: true,
        };
        config.osd.media_enabled = true;
        assert!(should_refresh_active_media_osd(
            Some(&active_trigger),
            &config,
            &after_art,
            true
        ));
        assert!(!should_refresh_active_media_osd(
            Some(&active_trigger),
            &config,
            &after_art,
            false
        ));
        assert!(!should_refresh_active_media_osd(
            None, &config, &after_art, true
        ));
        let unavailable = MediaState {
            status: MediaStatus::Unavailable,
            ..state("new")
        };
        assert!(!media_state_changed_for_osd(&state("old"), &unavailable));
    }

    #[test]
    fn mpris_polling_is_faster_only_during_media_osd_visibility_lease() {
        use std::time::Duration;
        let active_interval = mpris_sample_interval(true);
        let idle_interval = mpris_sample_interval(false);
        let default_media_lease = Duration::from_millis(2_000 + 1_500);
        assert_eq!(active_interval, Duration::from_millis(250));
        assert_eq!(idle_interval, Duration::from_secs(2));
        assert!(active_interval * 4 < default_media_lease);
    }
}
