//! Output-bound surfaces for the existing II sidebar and dashboard renderers.
//!
//! The component renders adapter supplied state. Platform services that do not
//! have GPUI adapters yet are represented as empty states, never sample data.

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
};

use gpui::{
    DisplayId, FocusHandle, IntoElement, KeyDownEvent, Role, ScrollHandle, SharedString, Window,
    div, prelude::*, px, rgba, white,
};

use super::volume_mixer;

static AUDIO_SNAPSHOT_CACHE: OnceLock<RwLock<(ProviderStatus, Option<AudioSnapshot>)>> =
    OnceLock::new();

#[derive(Clone, Debug)]
pub struct StatusRingsSnapshot {
    /// `None` means procfs could not provide a value or CPU has not had two samples yet.
    pub cpu_percent: Option<f64>,
    /// `None` means procfs could not provide a value.
    pub ram_percent: Option<f64>,
    /// `None` means there is no UPower display battery; unavailable bus is represented
    /// by `Some(BatteryState { status: Unavailable, .. })`.
    pub battery: Option<crate::components::vertical_bar::BatteryState>,
    pub sampled_at: Option<std::time::Instant>,
    /// True when no source has refreshed this output's metrics within 8 seconds.
    pub stale: bool,
}

impl Default for StatusRingsSnapshot {
    fn default() -> Self {
        Self {
            cpu_percent: None,
            ram_percent: None,
            battery: None,
            sampled_at: None,
            stale: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusRingsSource {
    VerticalBar,
    TopBar,
}

struct CachedStatusRingsSnapshot {
    snapshot: StatusRingsSnapshot,
    source: StatusRingsSource,
    published_at: std::time::Instant,
}

static STATUS_RINGS_CACHE: OnceLock<RwLock<HashMap<String, CachedStatusRingsSnapshot>>> =
    OnceLock::new();

fn status_rings_cache() -> &'static RwLock<HashMap<String, CachedStatusRingsSnapshot>> {
    STATUS_RINGS_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn cache_status_rings_snapshot(
    output_name: Option<&str>,
    resources: &crate::resource_provider::ResourceSnapshot,
    battery: Option<crate::components::vertical_bar::BatteryState>,
    source: StatusRingsSource,
) {
    let now = std::time::Instant::now();
    let snapshot = StatusRingsSnapshot {
        cpu_percent: resources.cpu_percent,
        ram_percent: resources.memory_percent,
        battery,
        sampled_at: Some(now),
        stale: false,
    };
    if let Ok(mut cached) = status_rings_cache().write() {
        cached.insert(
            output_name.unwrap_or_default().to_owned(),
            CachedStatusRingsSnapshot {
                snapshot,
                source,
                published_at: now,
            },
        );
    }
}

/// Publishes the vertical family's existing CPU/RAM and UPower sample.
pub fn cache_vertical_status_rings_snapshot(
    output_name: Option<&str>,
    resources: &crate::resource_provider::ResourceSnapshot,
    battery: Option<crate::components::vertical_bar::BatteryState>,
) {
    cache_status_rings_snapshot(
        output_name,
        resources,
        battery,
        StatusRingsSource::VerticalBar,
    );
}

/// Publishes TopBar values only while there is no recent vertical-family owner.
/// This lets either family seed the same per-display panel cache without the
/// two battery pollers racing when surfaces briefly overlap during a switch.
pub fn cache_top_status_rings_snapshot_if_unowned(
    output_name: Option<&str>,
    resources: &crate::resource_provider::ResourceSnapshot,
    battery: Option<crate::components::vertical_bar::BatteryState>,
) {
    let key = output_name.unwrap_or_default().to_owned();
    if let Ok(mut cached) = status_rings_cache().write() {
        if cached.get(&key).is_some_and(|entry| {
            entry.source == StatusRingsSource::VerticalBar
                && entry.published_at.elapsed() < std::time::Duration::from_secs(8)
        }) {
            return;
        }
        let now = std::time::Instant::now();
        cached.insert(
            key,
            CachedStatusRingsSnapshot {
                snapshot: StatusRingsSnapshot {
                    cpu_percent: resources.cpu_percent,
                    ram_percent: resources.memory_percent,
                    battery,
                    sampled_at: Some(now),
                    stale: false,
                },
                source: StatusRingsSource::TopBar,
                published_at: now,
            },
        );
    }
}

pub fn has_recent_vertical_status_rings_source(output_name: Option<&str>) -> bool {
    status_rings_cache()
        .read()
        .ok()
        .and_then(|cached| {
            cached.get(output_name.unwrap_or_default()).map(|entry| {
                entry.source == StatusRingsSource::VerticalBar
                    && entry.published_at.elapsed() < std::time::Duration::from_secs(8)
            })
        })
        .unwrap_or(false)
}

fn status_rings_snapshot(output_name: Option<&str>) -> StatusRingsSnapshot {
    let snapshot = status_rings_cache().read().ok().and_then(|cached| {
        cached.get(output_name.unwrap_or_default()).map(|entry| {
            let age = entry.published_at.elapsed();
            if age < std::time::Duration::from_secs(8) {
                entry.snapshot.clone()
            } else {
                StatusRingsSnapshot {
                    sampled_at: entry.snapshot.sampled_at,
                    stale: true,
                    ..StatusRingsSnapshot::default()
                }
            }
        })
    });
    snapshot.unwrap_or_default()
}

fn audio_snapshot_cache() -> &'static RwLock<(ProviderStatus, Option<AudioSnapshot>)> {
    AUDIO_SNAPSHOT_CACHE.get_or_init(|| RwLock::new((ProviderStatus::Connecting, None)))
}

pub fn cache_audio_snapshot(status: ProviderStatus, snapshot: Option<AudioSnapshot>) {
    if let Ok(mut cached) = audio_snapshot_cache().write() {
        *cached = (status, snapshot);
    }
}
use crate::families::ii::context_weather::ContextWeatherInput;
use crate::families::ii::context_weather_state::{
    self as context_weather_state, ContextWeatherState, ContextWeatherStatus,
};
use crate::families::ii::controls_card::ControlsCardInput;
use crate::families::ii::crypto_state::{self as crypto_state, CryptoState, CryptoStatus};
use crate::families::ii::crypto_widget::{CryptoRefreshAction, CryptoWidgetInput};
use crate::families::ii::glance_clock::{GlanceClockState, render_glance_clock};
use crate::families::ii::left_sidebar_tabs::{LeftSidebarTabsInput, render_left_sidebar_tabs};
use crate::families::ii::media_player::MediaPlayerInput;
use crate::families::ii::notification_panel::{NotificationPanelInput, NotificationPanelState};
use crate::families::ii::quick_launch::{
    QuickLaunchEntry as RenderQuickLaunchEntry, QuickLaunchInput, command_basename,
};
use crate::families::ii::quick_note::QuickNoteCard;
use crate::families::ii::quick_sliders::{QuickSlidersInput, QuickSlidersState};
use crate::families::ii::quick_toggles::{QuickTogglesInput, QuickTogglesState};
use crate::families::ii::status_rings::StatusRingsInput;
use crate::families::ii::wallpaper_gallery::{
    WallpaperGalleryEntry, WallpaperGalleryInput, WallpaperGalleryState,
};
use crate::families::ii::wallpaper_gallery_state::{
    self as wallpaper_gallery_state, WallpaperGallerySnapshot, WallpaperGalleryStatus,
};
use crate::families::ii::week_strip::{OffsetAction, WeekStripInput};
use crate::families::ii::week_strip_state::{self, WeekStripSnapshot};
use crate::families::ii::world_clock::WorldClockInput;
use crate::families::ii::world_clock_state::{self, WorldClockSnapshot};
use crate::{
    audio_provider::{AudioNodeKind, AudioSnapshot, ProviderStatus},
    config::{ShellConfig, WeatherConfig},
    families::ii::sidebars::{
        self, ContentState, DashboardCard, DashboardInput, DrawerSide, DrawerState, Profile,
        SidebarHeaderStyle, SidebarHostState, SidebarInput, SidebarSection, SidebarSizeMode,
        SidebarStyle,
    },
    families::ii::system_sidebar_header,
    niri,
    theme::ThemeTokens,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IiPanelState {
    pub left_open: bool,
    pub left_expanded: bool,
    pub selected_left_tab_id: Option<String>,
    pub right_open: bool,
    pub dashboard_open: bool,
    pub volume_mixer_open: bool,
    pub volume_device_picker_open: bool,
    pub volume_mixer_tab: volume_mixer::VolumeMixerTab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IiPanelKind {
    LeftSidebar,
    RightSidebar,
    Dashboard,
}

#[derive(Debug, Clone)]
pub enum IiPanelEvent {
    Toggle {
        display: DisplayId,
        panel: IiPanelKind,
    },
    Close {
        display: DisplayId,
        panel: IiPanelKind,
    },
    ToggleExpand {
        display: DisplayId,
    },
    SelectLeftTab {
        display: DisplayId,
        tab_id: String,
    },
    LaunchQuickCommand {
        display: DisplayId,
        command: String,
    },
    SaveQuickNote {
        display: DisplayId,
        text: String,
    },
    ClearQuickNote {
        display: DisplayId,
    },
    CloseAll {
        display: DisplayId,
    },
    ToggleOverview {
        display: DisplayId,
    },
    CloseOverview {
        display: DisplayId,
    },
    SetOverviewMode {
        display: DisplayId,
        mode: super::overview::OverviewMode,
    },
    SetOverviewQuery {
        display: DisplayId,
        query: String,
    },
    LaunchOverviewApplication {
        display: DisplayId,
        application_id: String,
    },
    ToggleVolumeMixer {
        display: DisplayId,
    },
    ToggleVolumeDevicePicker {
        display: DisplayId,
    },
    CloseVolumeMixer {
        display: DisplayId,
    },
    SelectVolumeMixerTab {
        display: DisplayId,
        tab: volume_mixer::VolumeMixerTab,
    },
}

#[derive(Clone)]
pub struct IiPanelLauncher {
    pub display: DisplayId,
    pub events: async_channel::Sender<IiPanelEvent>,
    pub left_enabled: bool,
    pub left_open: bool,
    pub right_enabled: bool,
    pub dashboard_enabled: bool,
    pub overview_enabled: bool,
    pub overview_focus: FocusHandle,
    pub volume_mixer_enabled: bool,
}

pub fn render_launchers(launcher: IiPanelLauncher) -> impl IntoElement {
    let buttons = [
        (
            IiPanelKind::LeftSidebar,
            launcher.left_enabled,
            "Left sidebar",
        ),
        (
            IiPanelKind::RightSidebar,
            launcher.right_enabled,
            "System sidebar",
        ),
        (
            IiPanelKind::Dashboard,
            launcher.dashboard_enabled,
            "Dashboard",
        ),
    ];
    let display = launcher.display;
    let overview_focus = launcher.overview_focus.clone();
    div()
        .id("ii.panel.launchers")
        .absolute()
        .top(px(4.0))
        .right(px(8.0))
        .flex()
        .gap(px(4.0))
        .children(buttons.into_iter().filter_map(|(panel, enabled, label)| {
            if !enabled {
                return None;
            }
            let events = launcher.events.clone();
            let keyboard_events = events.clone();
            let accessible = label.to_owned();
            let is_active = panel == IiPanelKind::LeftSidebar && launcher.left_open;
            let glyph = match panel {
                IiPanelKind::LeftSidebar => "◧",
                IiPanelKind::RightSidebar => "◨",
                IiPanelKind::Dashboard => "▦",
            };
            Some(
                div()
                    .id(SharedString::from(format!("ii.panel.launcher.{panel:?}")))
                    .min_w(px(28.0))
                    .h(px(28.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(if is_active {
                        rgba(0x526b91aa)
                    } else {
                        rgba(0x00000000)
                    })
                    .rounded(px(999.0))
                    .text_color(white())
                    .role(Role::Button)
                    .aria_label(SharedString::from(accessible))
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .hover(move |style| {
                        style.bg(if is_active {
                            rgba(0x6483b0cc)
                        } else {
                            rgba(0xffffff18)
                        })
                    })
                    .focus_visible(|style| style.border_1().border_color(rgba(0x8ab4f8ff)))
                    .on_click(move |_, _, _| {
                        let _ = events.try_send(IiPanelEvent::Toggle { display, panel });
                    })
                    .on_key_down(move |event: &KeyDownEvent, _, cx| {
                        if !event.is_held
                            && (event.keystroke.key == "enter" || event.keystroke.key == "space")
                        {
                            let _ =
                                keyboard_events.try_send(IiPanelEvent::Toggle { display, panel });
                            cx.stop_propagation();
                        }
                    })
                    .child(glyph),
            )
        }))
        .children(std::iter::once(()).filter_map(|_| {
            if !launcher.overview_enabled {
                return None;
            }
            let events = launcher.events.clone();
            let keyboard_events = events.clone();
            let display = launcher.display;
            Some(
                div()
                    .id("ii.panel.launcher.Overview")
                    .min_w(px(28.0))
                    .h(px(28.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba(0x00000000))
                    .rounded(px(999.0))
                    .text_color(white())
                    .role(Role::Button)
                    .aria_label("Overview")
                    .track_focus(&overview_focus)
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba(0xffffff18)))
                    .focus_visible(|style| style.border_1().border_color(rgba(0x8ab4f8ff)))
                    .on_click(move |_, _, _| {
                        let _ = events.try_send(IiPanelEvent::ToggleOverview { display });
                    })
                    .on_key_down(move |event: &KeyDownEvent, _, cx| {
                        if !event.is_held
                            && (event.keystroke.key == "enter" || event.keystroke.key == "space")
                        {
                            let _ =
                                keyboard_events.try_send(IiPanelEvent::ToggleOverview { display });
                            cx.stop_propagation();
                        }
                    })
                    .child("◫"),
            )
        }))
        .children(std::iter::once(()).filter_map(|_| {
            if !launcher.volume_mixer_enabled {
                return None;
            }
            let events = launcher.events.clone();
            let keyboard_events = events.clone();
            let display = launcher.display;
            Some(
                div()
                    .id("ii.panel.launcher.VolumeMixer")
                    .min_w(px(28.0))
                    .h(px(28.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba(0x00000000))
                    .rounded(px(999.0))
                    .text_color(white())
                    .role(Role::Button)
                    .aria_label("Volume mixer")
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba(0xffffff18)))
                    .focus_visible(|style| style.border_1().border_color(rgba(0x8ab4f8ff)))
                    .on_click(move |_, _, _| {
                        let _ = events.try_send(IiPanelEvent::ToggleVolumeMixer { display });
                    })
                    .on_key_down(move |event: &KeyDownEvent, _, cx| {
                        if !event.is_held
                            && (event.keystroke.key == "enter" || event.keystroke.key == "space")
                        {
                            let _ = keyboard_events
                                .try_send(IiPanelEvent::ToggleVolumeMixer { display });
                            cx.stop_propagation();
                        }
                    })
                    .child("◖"),
            )
        }))
}

impl IiPanelState {
    pub fn any_open(&self) -> bool {
        self.left_open || self.right_open || self.dashboard_open || self.volume_mixer_open
    }
}

#[derive(Clone)]
pub struct IiPanelActions {
    pub close_left: Arc<dyn Fn() + Send + Sync>,
    pub close_right: Arc<dyn Fn() + Send + Sync>,
    pub close_dashboard: Arc<dyn Fn() + Send + Sync>,
    pub expand_left: Arc<dyn Fn() + Send + Sync>,
    pub select_left_tab: Arc<dyn Fn(String) + Send + Sync>,
    pub launch_quick_command: Arc<dyn Fn(String) + Send + Sync>,
    pub save_quick_note: Arc<dyn Fn(String) + Send + Sync>,
    pub clear_quick_note: Arc<dyn Fn() + Send + Sync>,
    pub close_all: Arc<dyn Fn() + Send + Sync>,
    pub toggle_volume_device_picker: Arc<dyn Fn() + Send + Sync>,
    pub close_volume_mixer: Arc<dyn Fn() + Send + Sync>,
    pub select_volume_mixer_tab: Arc<dyn Fn(volume_mixer::VolumeMixerTab) + Send + Sync>,
    pub media_action: Arc<dyn Fn(crate::components::vertical_bar::MediaAction) + Send + Sync>,
}

type CryptoHistoryResult = (
    u64,
    u64,
    Vec<String>,
    u64,
    HashMap<String, Option<Vec<f32>>>,
);

type WeatherRequestSignature = (bool, Option<String>, Option<f64>, Option<f64>, bool, u64);

fn weather_request_signature(config: &WeatherConfig) -> WeatherRequestSignature {
    (
        config.enabled,
        config.city.clone(),
        config.manual_lat,
        config.manual_lon,
        config.use_uscs,
        config.fetch_interval_minutes.clamp(5, 60),
    )
}

pub struct IiPanelSurface {
    output_name: Option<String>,
    output_size: gpui::Size<gpui::Pixels>,
    config: ShellConfig,
    theme: ThemeTokens,
    state: IiPanelState,
    actions: IiPanelActions,
    snapshot: niri::Snapshot,
    snapshot_initialized: bool,
    audio_snapshot: Option<AudioSnapshot>,
    audio_status: ProviderStatus,
    pub status_rings: StatusRingsSnapshot,
    pub quick_note: crate::families::ii::quick_note_store::QuickNoteSnapshot,
    quick_note_card: gpui::Entity<QuickNoteCard>,
    week_offset: i32,
    week_snapshot: Option<WeekStripSnapshot>,
    week_error: Option<String>,
    week_offset_tx: async_channel::Sender<i32>,
    week_last_refresh: std::time::Instant,
    world_clock_snapshot: Option<WorldClockSnapshot>,
    context_weather: ContextWeatherState,
    context_weather_generation: u64,
    crypto_state: CryptoState,
    crypto_generation: u64,
    crypto_request_id: u64,
    crypto_refresh_tx: async_channel::Sender<()>,
    wallpaper_gallery_snapshot: Option<WallpaperGallerySnapshot>,
    wallpaper_gallery_generation: u64,
    wallpaper_gallery_cursor_index: usize,
    wallpaper_gallery_scroll: ScrollHandle,
    left_tab_focus_handles: HashMap<String, FocusHandle>,
    left_tabs_scroll: ScrollHandle,
    pub media: crate::components::vertical_bar::MediaState,
}

impl IiPanelSurface {
    pub fn new(
        output_name: Option<String>,
        output_size: gpui::Size<gpui::Pixels>,
        config: ShellConfig,
        theme: ThemeTokens,
        state: IiPanelState,
        actions: IiPanelActions,
        cx: &mut Context<Self>,
    ) -> Self {
        let (audio_status, audio_snapshot) = audio_snapshot_cache()
            .read()
            .map(|snapshot| (snapshot.0, snapshot.1.clone()))
            .unwrap_or((ProviderStatus::Connecting, None));
        let status_rings = status_rings_snapshot(output_name.as_deref());
        let left_tab_focus_handles = config
            .sidebar
            .left_tabs
            .iter()
            .map(|tab| (tab.id.clone(), cx.focus_handle()))
            .collect();
        let initial_note = crate::families::ii::quick_note_store::QuickNoteSnapshot::loading();
        let note_style = sidebar_style(&config, &theme, output_size);
        let note_card = cx.new(|_| {
            QuickNoteCard::new(
                initial_note.clone(),
                note_style,
                actions.save_quick_note.clone(),
                actions.clear_quick_note.clone(),
            )
        });
        let (week_offset_tx, week_offset_rx) = async_channel::unbounded::<i32>();
        let (crypto_refresh_tx, crypto_refresh_rx) = async_channel::unbounded::<()>();
        let view = Self {
            output_name,
            output_size,
            config,
            theme,
            state,
            actions,
            snapshot: niri::Snapshot::default(),
            snapshot_initialized: false,
            audio_snapshot,
            audio_status,
            status_rings,
            quick_note: initial_note,
            quick_note_card: note_card,
            week_offset: 0,
            week_snapshot: None,
            week_error: None,
            week_offset_tx,
            week_last_refresh: std::time::Instant::now(),
            world_clock_snapshot: None,
            context_weather: ContextWeatherState {
                status: ContextWeatherStatus::Loading,
                snapshot: None,
                stale: false,
                updated_at_ms: None,
            },
            context_weather_generation: 0,
            crypto_state: CryptoState {
                status: CryptoStatus::Loading,
                coins: Vec::new(),
                stale: false,
                updated_at: None,
                pending_ids: Vec::new(),
            },
            crypto_generation: 0,
            crypto_request_id: 0,
            crypto_refresh_tx,
            wallpaper_gallery_snapshot: None,
            wallpaper_gallery_generation: 0,
            wallpaper_gallery_cursor_index: 0,
            wallpaper_gallery_scroll: ScrollHandle::new(),
            left_tab_focus_handles,
            left_tabs_scroll: ScrollHandle::new(),
            media: crate::vertical_provider::unavailable_media(),
        };
        cx.spawn(async move |this, cx| {
            let snapshot = cx
                .background_executor()
                .spawn(async { crate::families::ii::quick_note_store::load() })
                .await;
            let _ = this.update(cx, |view, cx| view.set_quick_note(snapshot, cx));
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let initial = cx
                .background_executor()
                .spawn(async { week_strip_state::build_snapshot(0) })
                .await;
            if this
                .update(cx, |view, cx| {
                    if view.week_offset == 0 {
                        view.week_snapshot = initial.as_ref().ok().cloned();
                        view.week_error = initial.err();
                        view.week_last_refresh = std::time::Instant::now();
                        cx.notify();
                    }
                })
                .is_err()
            {
                return;
            }
            while let Ok(offset) = week_offset_rx.recv().await {
                if this
                    .update(cx, |view, cx| {
                        view.week_offset = offset;
                        view.week_last_refresh = std::time::Instant::now();
                        view.week_snapshot = None;
                        view.week_error = None;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                let result = cx
                    .background_executor()
                    .spawn(async move { week_strip_state::build_snapshot(offset) })
                    .await;
                if this
                    .update(cx, |view, cx| {
                        if view.week_offset == offset {
                            view.week_snapshot = result.as_ref().ok().cloned();
                            view.week_error = result.err();
                            view.week_last_refresh = std::time::Instant::now();
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let mut was_visible = false;
            let mut next_refresh = std::time::Instant::now();
            loop {
                let config = match this.update(cx, |view, _| {
                    let visible = view.state.left_open
                        && view.state.selected_left_tab_id.as_deref() == Some("widgets")
                        && view.config.sidebar.widgets_world_clock_enabled
                        && view
                            .config
                            .sidebar
                            .widget_order
                            .iter()
                            .any(|id| id == "worldclock");
                    if visible {
                        Some(view.config.sidebar.world_clock.clone())
                    } else {
                        None
                    }
                }) {
                    Ok(config) => config,
                    Err(_) => break,
                };
                let visible = config.is_some();
                if let Some(config) = config {
                    if !was_visible || std::time::Instant::now() >= next_refresh {
                        next_refresh = std::time::Instant::now()
                            + std::time::Duration::from_secs(if config.show_seconds {
                                1
                            } else {
                                30
                            });
                        let snapshot = cx
                            .background_executor()
                            .spawn(async move { world_clock_state::build_snapshot(&config) })
                            .await;
                        if this
                            .update(cx, |view, cx| {
                                view.world_clock_snapshot = Some(snapshot);
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                was_visible = visible;
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let mut was_visible = false;
            loop {
                let (visible, scan_config) = match this.update(cx, |view, _| {
                    let visible = view.state.left_open
                        && view.state.selected_left_tab_id.as_deref() == Some("widgets")
                        && view.config.sidebar.widgets_wallpaper_enabled
                        && view
                            .config
                            .sidebar
                            .widget_order
                            .iter()
                            .any(|id| id == "wallpaper");
                    let scan = if visible && !was_visible {
                        view.wallpaper_gallery_generation =
                            view.wallpaper_gallery_generation.wrapping_add(1);
                        Some((
                            view.wallpaper_gallery_generation,
                            view.config.wallpapers_directory.clone(),
                            view.config.background.wallpaper_path.clone(),
                        ))
                    } else {
                        None
                    };
                    (visible, scan)
                }) {
                    Ok(result) => result,
                    Err(_) => break,
                };
                let mut applied = true;
                if let Some((generation, directory, current_path)) = scan_config {
                    let expected_directory = directory.clone();
                    let expected_current_path = current_path.clone();
                    let snapshot = cx
                        .background_executor()
                        .spawn(async move {
                            wallpaper_gallery_state::scan_gallery(
                                directory.as_deref(),
                                current_path.as_deref(),
                            )
                        })
                        .await;
                    applied = match this.update(cx, |view, cx| {
                        let still_visible = view.state.left_open
                            && view.state.selected_left_tab_id.as_deref() == Some("widgets")
                            && view.config.sidebar.widgets_wallpaper_enabled
                            && view
                                .config
                                .sidebar
                                .widget_order
                                .iter()
                                .any(|id| id == "wallpaper")
                            && view.config.wallpapers_directory == expected_directory
                            && view.config.background.wallpaper_path == expected_current_path;
                        if view.wallpaper_gallery_generation == generation && still_visible {
                            view.wallpaper_gallery_snapshot = Some(snapshot);
                            view.wallpaper_gallery_cursor_index = 0;
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    }) {
                        Ok(applied) => applied,
                        Err(_) => break,
                    };
                }
                if !applied {
                    was_visible = false;
                } else {
                    was_visible = visible;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let mut was_visible = false;
            let mut next_refresh = std::time::Instant::now();
            let mut last_refresh_generation = None;
            let mut last_signature: Option<WeatherRequestSignature> = None;
            loop {
                let (visible, config, generation) = match this.update(cx, |view, cx| {
                    let visible = view.state.left_open
                        && view.state.selected_left_tab_id.as_deref() == Some("widgets")
                        && view.config.sidebar.widgets_context_enabled
                        && view.config.sidebar.context_show_weather
                        && view.config.bar.weather.enabled
                        && view
                            .config
                            .sidebar
                            .widget_order
                            .iter()
                            .any(|id| id == "context");
                    if visible != was_visible {
                        view.context_weather_generation =
                            view.context_weather_generation.wrapping_add(1);
                    }
                    let signature = weather_request_signature(&view.config.bar.weather);
                    if last_signature
                        .as_ref()
                        .is_some_and(|previous| *previous != signature)
                    {
                        view.context_weather_generation =
                            view.context_weather_generation.wrapping_add(1);
                        view.context_weather = ContextWeatherState {
                            status: ContextWeatherStatus::Loading,
                            snapshot: view.context_weather.snapshot.clone(),
                            stale: view.context_weather.snapshot.is_some(),
                            updated_at_ms: view.context_weather.updated_at_ms,
                        };
                        cx.notify();
                    }
                    last_signature = Some(signature);
                    (
                        visible,
                        visible.then(|| view.config.clone()),
                        view.context_weather_generation,
                    )
                }) {
                    Ok(result) => result,
                    Err(_) => break,
                };
                if let Some(config) = config {
                    if !was_visible
                        || last_refresh_generation != Some(generation)
                        || std::time::Instant::now() >= next_refresh
                    {
                        let expected_signature = weather_request_signature(&config.bar.weather);
                        let interval = config.bar.weather.fetch_interval_minutes.clamp(5, 60);
                        next_refresh = std::time::Instant::now()
                            + std::time::Duration::from_secs(interval.saturating_mul(60));
                        last_refresh_generation = Some(generation);
                        if this
                            .update(cx, |view, cx| {
                                if view.context_weather_generation == generation {
                                    view.context_weather = ContextWeatherState {
                                        status: ContextWeatherStatus::Loading,
                                        snapshot: view.context_weather.snapshot.clone(),
                                        stale: view.context_weather.snapshot.is_some(),
                                        updated_at_ms: view.context_weather.updated_at_ms,
                                    };
                                    cx.notify();
                                }
                            })
                            .is_err()
                        {
                            break;
                        }
                        // Load the cache before the bounded network request so
                        // offline first-use can render last-known conditions.
                        let cache_path = weather_cache_path();
                        let cache_config = config.clone();
                        let cached = cx
                            .background_executor()
                            .spawn(async move {
                                context_weather_state::load_cached_context_weather(
                                    &cache_config,
                                    &cache_path,
                                )
                                .await
                            })
                            .await;
                        let _ = this.update(cx, |view, cx| {
                            let still_current = view.context_weather_generation == generation
                                && weather_request_signature(&view.config.bar.weather)
                                    == expected_signature;
                            if still_current {
                                if let Some(cache_time) = cached.updated_at_ms {
                                    let current_time =
                                        view.context_weather.updated_at_ms.unwrap_or_default();
                                    if view.context_weather.snapshot.is_none()
                                        || cache_time > current_time
                                    {
                                        let mut cached = cached;
                                        cached.status = ContextWeatherStatus::Loading;
                                        view.context_weather = cached;
                                        cx.notify();
                                    }
                                }
                            }
                        });
                        let request_config = config.clone();
                        let result = cx
                            .background_executor()
                            .spawn(async move {
                                context_weather_state::load_context_weather(
                                    &request_config,
                                    &weather_cache_path(),
                                )
                                .await
                            })
                            .await;
                        if this
                            .update(cx, |view, cx| {
                                let still_visible = view.state.left_open
                                    && view.state.selected_left_tab_id.as_deref()
                                        == Some("widgets")
                                    && view.config.sidebar.widgets_context_enabled
                                    && view.config.sidebar.context_show_weather
                                    && view.config.bar.weather.enabled
                                    && view
                                        .config
                                        .sidebar
                                        .widget_order
                                        .iter()
                                        .any(|id| id == "context");
                                let same_request =
                                    weather_request_signature(&view.config.bar.weather)
                                        == expected_signature;
                                if view.context_weather_generation == generation
                                    && still_visible
                                    && same_request
                                {
                                    let keep_current = result.stale
                                        && result.snapshot.is_some()
                                        && view.context_weather.snapshot.is_some()
                                        && view.context_weather.updated_at_ms.unwrap_or_default()
                                            > result.updated_at_ms.unwrap_or_default();
                                    if keep_current {
                                        view.context_weather.status = result.status;
                                        view.context_weather.stale = true;
                                    } else if result.snapshot.is_none()
                                        && view.context_weather.snapshot.is_some()
                                    {
                                        view.context_weather.status = result.status;
                                        view.context_weather.stale = true;
                                    } else {
                                        view.context_weather = result;
                                    }
                                    cx.notify();
                                }
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                was_visible = visible;
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
            }
        })
        .detach();
        let (crypto_history_tx, crypto_history_rx) =
            async_channel::unbounded::<CryptoHistoryResult>();
        cx.spawn(async move |this, cx| {
            let mut was_visible = false;
            let mut next_refresh = std::time::Instant::now();
            let mut last_signature: Option<(Vec<String>, u64)> = None;
            let mut history_active = false;
            loop {
                if let Ok((generation, request_id, expected_coin_ids, expected_interval, results)) =
                    crypto_history_rx.try_recv()
                {
                    history_active = false;
                    let _ = this.update(cx, |view, cx| {
                        let still_visible = view.state.left_open
                            && view.state.selected_left_tab_id.as_deref() == Some("widgets")
                            && view
                                .config
                                .sidebar
                                .left_tabs
                                .iter()
                                .any(|tab| tab.id == "widgets" && tab.enabled)
                            && view.config.sidebar.widgets_crypto_enabled
                            && view
                                .config
                                .sidebar
                                .widget_order
                                .iter()
                                .any(|id| id == "crypto");
                        if view.crypto_generation == generation
                            && view.crypto_request_id == request_id
                            && still_visible
                            && view.config.sidebar.crypto_coins == expected_coin_ids
                            && view.config.sidebar.crypto_refresh_interval_seconds
                                == expected_interval
                        {
                            for coin in &mut view.crypto_state.coins {
                                if let Some(Some(samples)) = results.get(&coin.id) {
                                    coin.sparkline = samples.clone();
                                }
                            }
                            view.crypto_state
                                .pending_ids
                                .retain(|id| !results.contains_key(id));
                            cx.notify();
                        }
                    });
                }
                let (visible, config, generation) = match this.update(cx, |view, _| {
                    let widgets_tab_enabled = view
                        .config
                        .sidebar
                        .left_tabs
                        .iter()
                        .any(|tab| tab.id == "widgets" && tab.enabled);
                    let visible = view.state.left_open
                        && view.state.selected_left_tab_id.as_deref() == Some("widgets")
                        && widgets_tab_enabled
                        && view.config.sidebar.widgets_crypto_enabled
                        && view
                            .config
                            .sidebar
                            .widget_order
                            .iter()
                            .any(|id| id == "crypto");
                    if visible != was_visible {
                        view.crypto_generation = view.crypto_generation.wrapping_add(1);
                    }
                    let signature = (
                        view.config.sidebar.crypto_coins.clone(),
                        view.config.sidebar.crypto_refresh_interval_seconds,
                    );
                    if last_signature
                        .as_ref()
                        .is_some_and(|previous| *previous != signature)
                    {
                        view.crypto_generation = view.crypto_generation.wrapping_add(1);
                        view.crypto_request_id = view.crypto_request_id.wrapping_add(1);
                        view.crypto_state = CryptoState {
                            status: CryptoStatus::Loading,
                            coins: Vec::new(),
                            stale: false,
                            updated_at: None,
                            pending_ids: Vec::new(),
                        };
                        next_refresh = std::time::Instant::now();
                    }
                    last_signature = Some(signature);
                    (
                        visible,
                        visible.then(|| view.config.clone()),
                        view.crypto_generation,
                    )
                }) {
                    Ok(result) => result,
                    Err(_) => break,
                };
                let mut retry_immediately = false;
                let force = visible && crypto_refresh_rx.try_recv().is_ok();
                if let Some(config) = config {
                    if force || !was_visible || std::time::Instant::now() >= next_refresh {
                        let interval = config
                            .sidebar
                            .crypto_refresh_interval_seconds
                            .clamp(30, 300);
                        next_refresh =
                            std::time::Instant::now() + std::time::Duration::from_secs(interval);
                        let cache_path = crypto_cache_path();
                        let expected_coin_ids = config.sidebar.crypto_coins.clone();
                        let expected_interval = config.sidebar.crypto_refresh_interval_seconds;
                        let request_id = match this
                            .update(cx, |view, cx| {
                                if view.crypto_generation == generation {
                                    view.crypto_request_id = view.crypto_request_id.wrapping_add(1);
                                    // Keep any current rows visible while a manual refresh runs.
                                    view.crypto_state.status = CryptoStatus::Loading;
                                    cx.notify();
                                    Some(view.crypto_request_id)
                                } else {
                                    None
                                }
                            })
                            .ok()
                            .flatten()
                        {
                            Some(request_id) => request_id,
                            None => {
                                was_visible = visible;
                                cx.background_executor()
                                    .timer(std::time::Duration::from_secs(1))
                                    .await;
                                continue;
                            }
                        };
                        // Quote/cache I/O is bounded by one request timeout. History has its own task.
                        let result = cx
                            .background_executor()
                            .spawn(async move {
                                if force {
                                    crypto_state::refresh_crypto_quotes(&config, &cache_path).await
                                } else {
                                    crypto_state::load_crypto_quotes(&config, &cache_path).await
                                }
                            })
                            .await;
                        let applied = this
                            .update(cx, |view, cx| {
                                let widgets_tab_enabled = view
                                    .config
                                    .sidebar
                                    .left_tabs
                                    .iter()
                                    .any(|tab| tab.id == "widgets" && tab.enabled);
                                let still_visible = view.state.left_open
                                    && view.state.selected_left_tab_id.as_deref()
                                        == Some("widgets")
                                    && widgets_tab_enabled
                                    && view.config.sidebar.widgets_crypto_enabled
                                    && view
                                        .config
                                        .sidebar
                                        .widget_order
                                        .iter()
                                        .any(|id| id == "crypto");
                                let same_request_config = view.config.sidebar.crypto_coins
                                    == expected_coin_ids
                                    && view.config.sidebar.crypto_refresh_interval_seconds
                                        == expected_interval;
                                if view.crypto_generation == generation
                                    && view.crypto_request_id == request_id
                                    && still_visible
                                    && same_request_config
                                {
                                    let pending_ids = result.pending_ids.clone();
                                    view.crypto_state = result;
                                    cx.notify();
                                    Some(pending_ids)
                                } else {
                                    None
                                }
                            })
                            .ok()
                            .flatten();
                        if applied.is_none() {
                            retry_immediately = true;
                        }
                    }
                }
                if visible && !history_active {
                    let job = this
                        .update(cx, |view, _| {
                            let pending_ids = view.crypto_state.pending_ids.clone();
                            if pending_ids.is_empty() {
                                return None;
                            }
                            Some((
                                view.crypto_generation,
                                view.crypto_request_id,
                                view.config.sidebar.crypto_coins.clone(),
                                view.config.sidebar.crypto_refresh_interval_seconds,
                                pending_ids,
                            ))
                        })
                        .ok()
                        .flatten();
                    if let Some((generation, request_id, coin_ids, interval, pending_ids)) = job {
                        history_active = true;
                        let path = crypto_cache_path();
                        let tx = crypto_history_tx.clone();
                        cx.background_executor()
                            .spawn(async move {
                                let results = crypto_state::load_crypto_history(
                                    &path,
                                    &coin_ids,
                                    &pending_ids,
                                )
                                .await;
                                let _ = tx.try_send((
                                    generation, request_id, coin_ids, interval, results,
                                ));
                            })
                            .detach();
                    }
                }
                // If an in-flight result was invalidated (for example, the
                // tab closed and reopened), start a fresh load on the next tick.
                was_visible = visible && !retry_immediately;
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
            }
        })
        .detach();
        let output = view.output_name.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let output = output.clone();
                let snapshot = cx
                    .background_executor()
                    .spawn(async move { niri::snapshot(output.as_deref()) })
                    .await;
                if this
                    .update(cx, |view, cx| {
                        view.snapshot = snapshot;
                        view.snapshot_initialized = true;
                        view.status_rings = status_rings_snapshot(view.output_name.as_deref());
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                let refresh_week = this
                    .update(cx, |view, _| {
                        if view.week_last_refresh.elapsed() >= std::time::Duration::from_secs(60) {
                            view.week_last_refresh = std::time::Instant::now();
                            Some(view.week_offset)
                        } else {
                            None
                        }
                    })
                    .ok()
                    .flatten();
                if let Some(offset) = refresh_week {
                    let result = cx
                        .background_executor()
                        .spawn(async move { week_strip_state::build_snapshot(offset) })
                        .await;
                    if this
                        .update(cx, |view, cx| {
                            if view.week_offset == offset {
                                view.week_snapshot = result.as_ref().ok().cloned();
                                view.week_error = result.err();
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
            }
        })
        .detach();
        view
    }

    pub fn set_state(&mut self, state: IiPanelState, cx: &mut Context<Self>) {
        if self.state.left_open != state.left_open
            || self.state.selected_left_tab_id != state.selected_left_tab_id
        {
            self.wallpaper_gallery_generation = self.wallpaper_gallery_generation.wrapping_add(1);
            self.crypto_generation = self.crypto_generation.wrapping_add(1);
            self.context_weather_generation = self.context_weather_generation.wrapping_add(1);
        }
        self.state = state;
        cx.notify();
    }

    /// Replace weather settings and synchronously invalidate any in-flight request.
    /// The current GPUI host has no live config-reload path wired to this seam yet.
    pub fn set_weather_config(&mut self, weather: WeatherConfig, cx: &mut Context<Self>) {
        if self.config.bar.weather != weather {
            // Invalidate before storing or rendering the new settings so an old
            // response cannot pass its request identity check during the update.
            self.context_weather_generation = self.context_weather_generation.wrapping_add(1);
            self.config.bar.weather = weather;
            self.context_weather = ContextWeatherState {
                status: ContextWeatherStatus::Loading,
                snapshot: None,
                stale: false,
                updated_at_ms: None,
            };
            cx.notify();
        }
    }

    pub fn set_audio_snapshot(
        &mut self,
        status: ProviderStatus,
        snapshot: Option<AudioSnapshot>,
        cx: &mut Context<Self>,
    ) {
        cache_audio_snapshot(status, snapshot.clone());
        self.audio_status = status;
        self.audio_snapshot = snapshot;
        cx.notify();
    }

    pub fn set_media_state(
        &mut self,
        state: crate::components::vertical_bar::MediaState,
        cx: &mut Context<Self>,
    ) {
        self.media = state;
        cx.notify();
    }

    pub fn set_quick_note(
        &mut self,
        snapshot: crate::families::ii::quick_note_store::QuickNoteSnapshot,
        cx: &mut Context<Self>,
    ) {
        self.quick_note = snapshot;
        cx.notify();
    }
}

impl Render for IiPanelSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let style = sidebar_style(&self.config, &self.theme, self.output_size);
        let state = self.state.clone();
        let close_left = self.actions.close_left.clone();
        let expand_left = self.actions.expand_left.clone();
        let close_right = {
            let close_sidebar = self.actions.close_right.clone();
            let close_mixer = self.actions.close_volume_mixer.clone();
            Arc::new(move || {
                close_mixer();
                close_sidebar();
            }) as Arc<dyn Fn() + Send + Sync>
        };
        let close_dashboard = self.actions.close_dashboard.clone();
        let close_mixer = self.actions.close_volume_mixer.clone();
        let close_mixer_key = close_mixer.clone();
        let toggle_device_picker = self.actions.toggle_volume_device_picker.clone();
        let output = self.output_name.clone().unwrap_or_default();
        let snapshot = self.snapshot.clone();

        let feature_host = SidebarHostState {
            screen_name: output.clone(),
            role_open: state.left_open,
            other_role_open: state.right_open,
            // GPUI has no ShellEditSession or hold-open source yet.
            role_hold_open: false,
            role_target_output: output.clone(),
            editor_active: false,
            screen_locked: screen_is_locked(),
            fullscreen_covered: false,
            enabled_screen: true,
            feature_role: true,
            role_expanded: state.left_expanded,
        };
        let system_host = SidebarHostState {
            screen_name: output.clone(),
            role_open: state.right_open,
            other_role_open: state.left_open,
            // GPUI has no ShellEditSession or hold-open source yet.
            role_hold_open: false,
            role_target_output: output,
            editor_active: false,
            screen_locked: screen_is_locked(),
            fullscreen_covered: false,
            enabled_screen: true,
            feature_role: false,
            role_expanded: false,
        };
        let feature_side = configured_side(
            self.config.sidebar.feature_slot.as_deref(),
            DrawerSide::Left,
        );
        let system_side = configured_side(
            self.config.sidebar.system_slot.as_deref(),
            DrawerSide::Right,
        );
        let mut feature_style = style.clone();
        if state.left_expanded {
            feature_style.width =
                (feature_style.width * 1.35).min(self.output_size.width.as_f32() * 0.8);
        }
        let feature_tabs = render_left_sidebar_tabs(LeftSidebarTabsInput {
            tabs: self.config.sidebar.left_tabs.clone(),
            order: self.config.sidebar.left_tab_order.clone(),
            selected_tab_id: state.selected_left_tab_id.clone(),
            style: feature_style.clone(),
            focus_handles: self.left_tab_focus_handles.clone(),
            scroll: self.left_tabs_scroll.clone(),
            on_select: self.actions.select_left_tab.clone(),
        });
        let media_has_title = self
            .media
            .title
            .as_deref()
            .is_some_and(|title| !title.trim().is_empty());
        let media_is_live = matches!(
            self.media.status,
            crate::components::vertical_bar::MediaStatus::Playing
                | crate::components::vertical_bar::MediaStatus::Paused
                | crate::components::vertical_bar::MediaStatus::Stopped
        );
        let show_widgets_media = state.selected_left_tab_id.as_deref() == Some("widgets")
            && self.config.sidebar.widgets_media_enabled
            && self
                .config
                .sidebar
                .widget_order
                .iter()
                .any(|id| id == "media")
            && media_is_live
            && media_has_title;
        let feature_header = div()
            .w_full()
            .flex()
            .flex_col()
            .child(feature_tabs)
            .when(
                state.selected_left_tab_id.as_deref() == Some("widgets"),
                |header| {
                    let clock_state = if let Some(label) = self.snapshot.clock.clone() {
                        GlanceClockState::Ready(label)
                    } else if self.snapshot_initialized {
                        GlanceClockState::Unavailable
                    } else {
                        GlanceClockState::Loading
                    };
                    header.child(render_glance_clock(clock_state, &feature_style))
                },
            )
            .into_any_element();
        let mut feature_input = sidebar_input(
            feature_side,
            if state.left_expanded {
                DrawerState::Expanded
            } else {
                DrawerState::Open
            },
            feature_style.clone(),
            "Feature sidebar",
            "This sidebar has no connected GPUI content adapter".into(),
            close_left.clone(),
            expand_left,
        );
        let show_widgets_note = state.selected_left_tab_id.as_deref() == Some("widgets")
            && self.config.sidebar.widgets_note_enabled
            && self
                .config
                .sidebar
                .widget_order
                .iter()
                .any(|id| id == "note");
        let card_snapshot = self.quick_note_card.read(cx).snapshot().clone();
        if self.quick_note.status != crate::families::ii::quick_note_store::QuickNoteStatus::Ready
            && card_snapshot.status == crate::families::ii::quick_note_store::QuickNoteStatus::Ready
        {
            self.quick_note = card_snapshot;
        }
        self.quick_note_card.update(cx, |card, cx| {
            card.sync(
                self.quick_note.clone(),
                feature_style.clone(),
                self.actions.save_quick_note.clone(),
                self.actions.clear_quick_note.clone(),
            );
        });
        if show_widgets_media {
            feature_input.sections.insert(
                0,
                SidebarSection {
                    id: "widgets-media".into(),
                    title: "Media player".into(),
                    state: ContentState::Ready,
                    message: None,
                    items: Vec::new(),
                    notification_panel: None,
                    quick_sliders: None,
                    quick_toggles: None,
                    media_player: Some(MediaPlayerInput {
                        media: Some(self.media.clone()),
                        style: feature_style.clone(),
                        on_action: Some(self.actions.media_action.clone()),
                    }),
                    status_rings: None,
                    quick_launch: None,
                    quick_note: None,
                    week_strip: None,
                    world_clock: None,
                    wallpaper_gallery: None,
                    controls_card: None,
                    context_weather: None,
                    crypto_widget: None,
                },
            );
        }
        let show_widgets_status = state.selected_left_tab_id.as_deref() == Some("widgets")
            && self.config.sidebar.widgets_status_enabled
            && self
                .config
                .sidebar
                .widget_order
                .iter()
                .any(|id| id == "status")
            && (self.config.sidebar.status_rings_show_cpu
                || self.config.sidebar.status_rings_show_ram
                || self.config.sidebar.status_rings_show_battery);
        if show_widgets_status {
            // Respect ordering among the migrated rows only. Other configured
            // widget IDs are not rendered yet and therefore have no placement.
            let status_before_media = self
                .config
                .sidebar
                .widget_order
                .iter()
                .position(|id| id == "status")
                .zip(
                    self.config
                        .sidebar
                        .widget_order
                        .iter()
                        .position(|id| id == "media"),
                )
                .is_some_and(|(status, media)| status < media);
            let status_index = usize::from(show_widgets_media && !status_before_media);
            feature_input.sections.insert(
                status_index,
                SidebarSection {
                    id: "widgets-status-rings".into(),
                    title: "Status".into(),
                    state: ContentState::Ready,
                    message: None,
                    items: Vec::new(),
                    notification_panel: None,
                    quick_sliders: None,
                    quick_toggles: None,
                    media_player: None,
                    status_rings: Some(StatusRingsInput {
                        cpu_percent: self.status_rings.cpu_percent,
                        ram_percent: self.status_rings.ram_percent,
                        battery: self.status_rings.battery.clone(),
                        stale: self.status_rings.stale && self.status_rings.sampled_at.is_some(),
                        show_cpu: self.config.sidebar.status_rings_show_cpu,
                        show_ram: self.config.sidebar.status_rings_show_ram,
                        show_battery: self.config.sidebar.status_rings_show_battery,
                        style: feature_style.clone(),
                    }),
                    quick_launch: None,
                    quick_note: None,
                    week_strip: None,
                    world_clock: None,
                    wallpaper_gallery: None,
                    controls_card: None,
                    context_weather: None,
                    crypto_widget: None,
                },
            );
        }
        let show_widgets_launch = state.selected_left_tab_id.as_deref() == Some("widgets")
            && self.config.sidebar.widgets_launch_enabled
            && self
                .config
                .sidebar
                .widget_order
                .iter()
                .any(|id| id == "launch");
        if show_widgets_launch {
            let shortcuts =
                self.config
                    .sidebar
                    .quick_launch
                    .iter()
                    .filter_map(|shortcut| {
                        let command = shortcut.cmd.as_deref()?.trim();
                        if command.is_empty() {
                            return None;
                        }
                        let executable_name = command_basename(command)?.to_lowercase();
                        let running = self.snapshot.windows.iter().any(|window| {
                            window.app_id.as_deref().is_some_and(|app_id| {
                                app_id.to_lowercase().contains(&executable_name)
                            }) || window.title.as_deref().is_some_and(|title| {
                                title.to_lowercase().contains(&executable_name)
                            })
                        });
                        Some(RenderQuickLaunchEntry {
                            command: command.to_owned(),
                            name: shortcut
                                .name
                                .clone()
                                .filter(|name| !name.trim().is_empty())
                                .unwrap_or_else(|| executable_name.clone()),
                            icon: shortcut
                                .icon
                                .clone()
                                .filter(|icon| !icon.trim().is_empty())
                                .unwrap_or_else(|| "apps".to_owned()),
                            running,
                        })
                    })
                    .collect::<Vec<_>>();
            let launch_rank = self
                .config
                .sidebar
                .widget_order
                .iter()
                .position(|id| id == "launch")
                .unwrap_or(usize::MAX);
            let insert_at = feature_input
                .sections
                .iter()
                .take_while(|section| {
                    matches!(
                        section.id.as_str(),
                        "widgets-media" | "widgets-status-rings"
                    )
                })
                .filter(|section| {
                    let widget_id = if section.id == "widgets-media" {
                        "media"
                    } else {
                        "status"
                    };
                    self.config
                        .sidebar
                        .widget_order
                        .iter()
                        .position(|id| id == widget_id)
                        .is_some_and(|rank| rank < launch_rank)
                })
                .count();
            feature_input.sections.insert(
                insert_at,
                SidebarSection {
                    id: "widgets-quick-launch".into(),
                    title: "Quick launch".into(),
                    state: ContentState::Ready,
                    message: None,
                    items: Vec::new(),
                    notification_panel: None,
                    quick_sliders: None,
                    quick_toggles: None,
                    media_player: None,
                    status_rings: None,
                    quick_launch: Some(QuickLaunchInput {
                        entries: shortcuts,
                        style: feature_style.clone(),
                        on_launch: self.actions.launch_quick_command.clone(),
                    }),
                    quick_note: None,
                    week_strip: None,
                    world_clock: None,
                    wallpaper_gallery: None,
                    controls_card: None,
                    context_weather: None,
                    crypto_widget: None,
                },
            );
        }
        if show_widgets_note {
            let note_rank = self
                .config
                .sidebar
                .widget_order
                .iter()
                .position(|id| id == "note")
                .unwrap_or(usize::MAX);
            let insert_at = feature_input
                .sections
                .iter()
                .take_while(|section| {
                    matches!(
                        section.id.as_str(),
                        "widgets-media" | "widgets-status-rings" | "widgets-quick-launch"
                    )
                })
                .filter(|section| {
                    let widget_id = match section.id.as_str() {
                        "widgets-media" => "media",
                        "widgets-status-rings" => "status",
                        _ => "launch",
                    };
                    self.config
                        .sidebar
                        .widget_order
                        .iter()
                        .position(|id| id == widget_id)
                        .is_some_and(|rank| rank < note_rank)
                })
                .count();
            feature_input.sections.insert(
                insert_at,
                SidebarSection {
                    id: "widgets-quick-note".into(),
                    title: "Quick note".into(),
                    state: ContentState::Ready,
                    message: None,
                    items: Vec::new(),
                    notification_panel: None,
                    quick_sliders: None,
                    quick_toggles: None,
                    media_player: None,
                    status_rings: None,
                    quick_launch: None,
                    quick_note: Some(self.quick_note_card.clone()),
                    week_strip: None,
                    world_clock: None,
                    wallpaper_gallery: None,
                    controls_card: None,
                    context_weather: None,
                    crypto_widget: None,
                },
            );
        }
        let show_widgets_week = state.selected_left_tab_id.as_deref() == Some("widgets")
            && self.config.sidebar.widgets_week_enabled
            && self
                .config
                .sidebar
                .widget_order
                .iter()
                .any(|id| id == "week");
        if show_widgets_week {
            let week_rank = self
                .config
                .sidebar
                .widget_order
                .iter()
                .position(|id| id == "week")
                .unwrap_or(usize::MAX);
            let insert_at = feature_input
                .sections
                .iter()
                .take_while(|section| {
                    matches!(
                        section.id.as_str(),
                        "widgets-media"
                            | "widgets-status-rings"
                            | "widgets-quick-launch"
                            | "widgets-quick-note"
                    )
                })
                .filter(|section| {
                    let widget_id = match section.id.as_str() {
                        "widgets-media" => "media",
                        "widgets-status-rings" => "status",
                        "widgets-quick-launch" => "launch",
                        _ => "note",
                    };
                    self.config
                        .sidebar
                        .widget_order
                        .iter()
                        .position(|id| id == widget_id)
                        .is_some_and(|rank| rank < week_rank)
                })
                .count();
            let offset_sender = self.week_offset_tx.clone();
            let on_offset_change: OffsetAction = Arc::new(move |offset| {
                let _ = offset_sender.try_send(offset);
            });
            feature_input.sections.insert(
                insert_at,
                SidebarSection {
                    id: "widgets-week-strip".into(),
                    title: "Week".into(),
                    state: if self.week_error.is_some() {
                        ContentState::Error
                    } else if self.week_snapshot.is_some() {
                        ContentState::Ready
                    } else {
                        ContentState::Loading
                    },
                    message: self.week_error.clone(),
                    items: Vec::new(),
                    notification_panel: None,
                    quick_sliders: None,
                    quick_toggles: None,
                    media_player: None,
                    status_rings: None,
                    quick_launch: None,
                    quick_note: None,
                    week_strip: Some(WeekStripInput {
                        week_offset: self.week_offset,
                        snapshot: self.week_snapshot.clone(),
                        loading: self.week_snapshot.is_none() && self.week_error.is_none(),
                        error: self.week_error.clone(),
                        style: feature_style.clone(),
                        on_offset_change,
                    }),
                    world_clock: None,
                    wallpaper_gallery: None,
                    controls_card: None,
                    context_weather: None,
                    crypto_widget: None,
                },
            );
        }
        let show_widgets_world_clock = state.selected_left_tab_id.as_deref() == Some("widgets")
            && self.config.sidebar.widgets_world_clock_enabled
            && self
                .config
                .sidebar
                .widget_order
                .iter()
                .any(|id| id == "worldclock");
        if show_widgets_world_clock {
            // Order only among widgets that currently have GPUI renderers.
            // Configured widgets without a renderer have no visible position yet.
            let world_clock_rank = self
                .config
                .sidebar
                .widget_order
                .iter()
                .position(|id| id == "worldclock")
                .unwrap_or(usize::MAX);
            let insert_at = feature_input
                .sections
                .iter()
                .filter_map(|section| {
                    let widget_id = match section.id.as_str() {
                        "widgets-media" => Some("media"),
                        "widgets-status-rings" => Some("status"),
                        "widgets-quick-launch" => Some("launch"),
                        "widgets-quick-note" => Some("note"),
                        "widgets-week-strip" => Some("week"),
                        _ => None,
                    }?;
                    self.config
                        .sidebar
                        .widget_order
                        .iter()
                        .position(|id| id == widget_id)
                        .filter(|rank| *rank < world_clock_rank)
                })
                .count();
            feature_input.sections.insert(
                insert_at,
                SidebarSection {
                    id: "widgets-world-clock".into(),
                    title: "World clock".into(),
                    state: if self.world_clock_snapshot.is_some() {
                        ContentState::Ready
                    } else {
                        ContentState::Loading
                    },
                    message: None,
                    items: Vec::new(),
                    notification_panel: None,
                    quick_sliders: None,
                    quick_toggles: None,
                    media_player: None,
                    status_rings: None,
                    quick_launch: None,
                    quick_note: None,
                    week_strip: None,
                    world_clock: Some(WorldClockInput {
                        snapshot: self.world_clock_snapshot.clone(),
                        auto: self.config.sidebar.world_clock.timezones.is_empty(),
                        style: feature_style.clone(),
                    }),
                    wallpaper_gallery: None,
                    controls_card: None,
                    context_weather: None,
                    crypto_widget: None,
                },
            );
        }
        let show_context_weather = state.selected_left_tab_id.as_deref() == Some("widgets")
            && self.config.sidebar.widgets_context_enabled
            && self.config.sidebar.context_show_weather
            && self.config.bar.weather.enabled
            && self
                .config
                .sidebar
                .widget_order
                .iter()
                .any(|id| id == "context");
        if show_context_weather {
            let context_rank = self
                .config
                .sidebar
                .widget_order
                .iter()
                .position(|id| id == "context")
                .unwrap_or(usize::MAX);
            let insert_at = feature_input
                .sections
                .iter()
                .filter_map(|section| {
                    let widget_id = match section.id.as_str() {
                        "widgets-media" => Some("media"),
                        "widgets-status-rings" => Some("status"),
                        "widgets-quick-launch" => Some("launch"),
                        "widgets-quick-note" => Some("note"),
                        "widgets-week-strip" => Some("week"),
                        "widgets-world-clock" => Some("worldclock"),
                        _ => None,
                    }?;
                    self.config
                        .sidebar
                        .widget_order
                        .iter()
                        .position(|id| id == widget_id)
                        .filter(|rank| *rank < context_rank)
                })
                .count();
            let weather_status = match &self.context_weather.status {
                ContextWeatherStatus::Ready => ContentState::Ready,
                ContextWeatherStatus::Loading => ContentState::Loading,
                ContextWeatherStatus::Unavailable(_) => ContentState::Error,
            };
            let weather_message = match &self.context_weather.status {
                ContextWeatherStatus::Unavailable(message) => Some(message.clone()),
                _ => None,
            };
            feature_input.sections.insert(
                insert_at,
                SidebarSection {
                    id: "widgets-context-weather".into(),
                    title: "Weather".into(),
                    state: weather_status,
                    message: weather_message,
                    items: Vec::new(),
                    notification_panel: None,
                    quick_sliders: None,
                    quick_toggles: None,
                    media_player: None,
                    status_rings: None,
                    quick_launch: None,
                    quick_note: None,
                    week_strip: None,
                    world_clock: None,
                    wallpaper_gallery: None,
                    controls_card: None,
                    context_weather: Some(ContextWeatherInput {
                        state: self.context_weather.clone(),
                        style: feature_style.clone(),
                    }),
                    crypto_widget: None,
                },
            );
        }
        let show_widgets_wallpaper = state.selected_left_tab_id.as_deref() == Some("widgets")
            && self.config.sidebar.widgets_wallpaper_enabled
            && self
                .config
                .sidebar
                .widget_order
                .iter()
                .any(|id| id == "wallpaper");
        if show_widgets_wallpaper {
            // Place among migrated widgets only; unsupported configured IDs do
            // not have a rendered slot yet.
            let wallpaper_rank = self
                .config
                .sidebar
                .widget_order
                .iter()
                .position(|id| id == "wallpaper")
                .unwrap_or(usize::MAX);
            let insert_at = feature_input
                .sections
                .iter()
                .filter_map(|section| {
                    let widget_id = match section.id.as_str() {
                        "widgets-media" => Some("media"),
                        "widgets-status-rings" => Some("status"),
                        "widgets-quick-launch" => Some("launch"),
                        "widgets-quick-note" => Some("note"),
                        "widgets-week-strip" => Some("week"),
                        "widgets-world-clock" => Some("worldclock"),
                        "widgets-context-weather" => Some("context"),
                        _ => None,
                    }?;
                    self.config
                        .sidebar
                        .widget_order
                        .iter()
                        .position(|id| id == widget_id)
                        .filter(|rank| *rank < wallpaper_rank)
                })
                .count();
            let (gallery_state, entries, mut gallery_note) =
                match self.wallpaper_gallery_snapshot.as_ref() {
                    None => (WallpaperGalleryState::Loading, Vec::new(), None),
                    Some(snapshot) => {
                        let state = match snapshot.status {
                            WallpaperGalleryStatus::Loading => WallpaperGalleryState::Loading,
                            WallpaperGalleryStatus::Unavailable => WallpaperGalleryState::Error,
                            WallpaperGalleryStatus::Empty | WallpaperGalleryStatus::Ready => {
                                WallpaperGalleryState::Ready
                            }
                        };
                        let entries = snapshot
                            .items
                            .iter()
                            .map(|item| WallpaperGalleryEntry {
                                path: item.path.clone(),
                                current: item.is_current,
                                preview_supported: item.preview_supported,
                                preview_unsupported_reason: item.preview_unsupported_reason.clone(),
                            })
                            .collect();
                        let note = snapshot.error.clone();
                        (state, entries, note)
                    }
                };
            if self
                .wallpaper_gallery_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.truncated)
            {
                gallery_note.get_or_insert_with(|| {
                    "Wallpaper preview list is limited to recent files".into()
                });
            }
            let gallery_entity = cx.entity();
            let on_cursor_change = std::rc::Rc::new(move |index, cx: &mut gpui::App| {
                let _ = gallery_entity.update(cx, |view, cx| {
                    view.wallpaper_gallery_cursor_index = index;
                    cx.notify();
                });
            });
            feature_input.sections.insert(
                insert_at,
                SidebarSection {
                    id: "widgets-wallpaper-gallery".into(),
                    title: "Wallpapers".into(),
                    state: match gallery_state {
                        WallpaperGalleryState::Loading => ContentState::Loading,
                        WallpaperGalleryState::Ready => ContentState::Ready,
                        WallpaperGalleryState::Error => ContentState::Error,
                    },
                    message: gallery_note.clone(),
                    items: Vec::new(),
                    notification_panel: None,
                    quick_sliders: None,
                    quick_toggles: None,
                    media_player: None,
                    status_rings: None,
                    quick_launch: None,
                    quick_note: None,
                    week_strip: None,
                    world_clock: None,
                    wallpaper_gallery: Some(WallpaperGalleryInput {
                        state: gallery_state,
                        entries,
                        error: gallery_note,
                        show_header: self.config.sidebar.quick_wallpaper_show_header,
                        style: feature_style.clone(),
                        cursor_index: self.wallpaper_gallery_cursor_index,
                        scroll_handle: self.wallpaper_gallery_scroll.clone(),
                        on_cursor_change,
                    }),
                    controls_card: None,
                    context_weather: None,
                    crypto_widget: None,
                },
            );
        }
        let show_widgets_controls = state.selected_left_tab_id.as_deref() == Some("widgets")
            && self.config.sidebar.widgets_controls_enabled
            && self
                .config
                .sidebar
                .widget_order
                .iter()
                .any(|id| id == "controls");
        if show_widgets_controls {
            // Respect order across currently implemented Widgets sections.
            let controls_rank = self
                .config
                .sidebar
                .widget_order
                .iter()
                .position(|id| id == "controls")
                .unwrap_or(usize::MAX);
            let insert_at = feature_input
                .sections
                .iter()
                .filter_map(|section| {
                    let widget_id = match section.id.as_str() {
                        "widgets-media" => Some("media"),
                        "widgets-status-rings" => Some("status"),
                        "widgets-quick-launch" => Some("launch"),
                        "widgets-quick-note" => Some("note"),
                        "widgets-week-strip" => Some("week"),
                        "widgets-world-clock" => Some("worldclock"),
                        "widgets-wallpaper-gallery" => Some("wallpaper"),
                        "widgets-context-weather" => Some("context"),
                        _ => None,
                    }?;
                    self.config
                        .sidebar
                        .widget_order
                        .iter()
                        .position(|id| id == widget_id)
                        .filter(|rank| *rank < controls_rank)
                })
                .count();
            feature_input.sections.insert(
                insert_at,
                SidebarSection {
                    id: "widgets-controls-card".into(),
                    title: "Controls".into(),
                    state: ContentState::Ready,
                    message: Some("Runtime control state and actions are unavailable".into()),
                    items: Vec::new(),
                    notification_panel: None,
                    quick_sliders: None,
                    quick_toggles: None,
                    media_player: None,
                    status_rings: None,
                    quick_launch: None,
                    quick_note: None,
                    week_strip: None,
                    world_clock: None,
                    wallpaper_gallery: None,
                    controls_card: Some(ControlsCardInput {
                        show_dark_mode: self.config.sidebar.controls_card_show_dark_mode,
                        show_dnd: self.config.sidebar.controls_card_show_dnd,
                        show_night_light: self.config.sidebar.controls_card_show_night_light,
                        show_game_mode: self.config.sidebar.controls_card_show_game_mode,
                        show_network: self.config.sidebar.controls_card_show_network,
                        show_bluetooth: self.config.sidebar.controls_card_show_bluetooth,
                        show_settings: self.config.sidebar.controls_card_show_settings,
                        show_lock: self.config.sidebar.controls_card_show_lock,
                        style: feature_style.clone(),
                    }),
                    context_weather: None,
                    crypto_widget: None,
                },
            );
        }
        let show_widgets_crypto = state.selected_left_tab_id.as_deref() == Some("widgets")
            && self.config.sidebar.widgets_crypto_enabled
            && self
                .config
                .sidebar
                .widget_order
                .iter()
                .any(|id| id == "crypto");
        if show_widgets_crypto {
            let crypto_rank = self
                .config
                .sidebar
                .widget_order
                .iter()
                .position(|id| id == "crypto")
                .unwrap_or(usize::MAX);
            let insert_at = feature_input
                .sections
                .iter()
                .filter_map(|section| {
                    let widget_id = match section.id.as_str() {
                        "widgets-media" => Some("media"),
                        "widgets-status-rings" => Some("status"),
                        "widgets-quick-launch" => Some("launch"),
                        "widgets-quick-note" => Some("note"),
                        "widgets-week-strip" => Some("week"),
                        "widgets-world-clock" => Some("worldclock"),
                        "widgets-context-weather" => Some("context"),
                        "widgets-wallpaper-gallery" => Some("wallpaper"),
                        "widgets-controls-card" => Some("controls"),
                        _ => None,
                    }?;
                    self.config
                        .sidebar
                        .widget_order
                        .iter()
                        .position(|id| id == widget_id)
                        .filter(|rank| *rank < crypto_rank)
                })
                .count();
            let refresh_sender = self.crypto_refresh_tx.clone();
            let on_refresh: CryptoRefreshAction = Arc::new(move || {
                let _ = refresh_sender.try_send(());
            });
            let crypto_state = self.crypto_state.clone();
            let crypto_content_state = if self.config.sidebar.crypto_coins.is_empty() {
                ContentState::Empty
            } else {
                match &crypto_state.status {
                    CryptoStatus::Loading => ContentState::Loading,
                    CryptoStatus::Ready => ContentState::Ready,
                    CryptoStatus::Unavailable(_) => ContentState::Error,
                }
            };
            let crypto_message = match &crypto_state.status {
                CryptoStatus::Unavailable(message) => Some(message.clone()),
                _ if self.config.sidebar.crypto_coins.is_empty() => {
                    Some("No coins configured".to_owned())
                }
                _ => None,
            };
            feature_input.sections.insert(
                insert_at,
                SidebarSection {
                    id: "widgets-crypto".into(),
                    title: "Crypto".into(),
                    state: crypto_content_state,
                    message: crypto_message,
                    items: Vec::new(),
                    notification_panel: None,
                    quick_sliders: None,
                    quick_toggles: None,
                    media_player: None,
                    status_rings: None,
                    quick_launch: None,
                    quick_note: None,
                    week_strip: None,
                    world_clock: None,
                    wallpaper_gallery: None,
                    controls_card: None,
                    context_weather: None,
                    crypto_widget: Some(CryptoWidgetInput {
                        state: crypto_state,
                        style: feature_style.clone(),
                        on_refresh,
                        refresh_enabled: !self.config.sidebar.crypto_coins.is_empty(),
                    }),
                },
            );
        }
        let mut system_style = style.clone();
        system_style.width = self
            .config
            .sidebar
            .system_width
            .unwrap_or(460.0)
            .clamp(280.0, 900.0) as f32;
        system_style.header_style = match self.config.sidebar.right_header_style.as_deref() {
            Some("classic") => SidebarHeaderStyle::Classic,
            _ => SidebarHeaderStyle::Profile,
        };
        let mut system_input = sidebar_input(
            system_side,
            DrawerState::Open,
            system_style.clone(),
            "System sidebar",
            "System controls and notification adapters are not connected".into(),
            close_right.clone(),
            Arc::new(|| {}),
        );
        system_input.profile = None;
        let mut section_order = self.config.sidebar.right_section_order.clone();
        if self.config.sidebar.compact {
            let control_order = &self.config.sidebar.right_controls_section_order;
            let controls = ["sliders", "toggles", "media"];
            if control_order.iter().any(|section| section == "media") {
                // Standard section order has no media slot. Add one beside the
                // control slots so compact ordering can move it deterministically.
                let after_toggles = section_order
                    .iter()
                    .position(|section| section == "toggles")
                    .map(|index| index + 1)
                    .unwrap_or(section_order.len());
                section_order.insert(after_toggles, "media".into());
            }
            let ordered_controls = control_order
                .iter()
                .filter(|section| controls.contains(&section.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            let mut merged = Vec::with_capacity(section_order.len());
            let mut inserted_controls = false;
            for section in section_order {
                if controls.contains(&section.as_str()) {
                    if !inserted_controls {
                        merged.extend(ordered_controls.iter().cloned());
                        inserted_controls = true;
                    }
                } else {
                    merged.push(section);
                }
            }
            section_order = merged;
        }
        system_input.sections = section_order
            .iter()
            .map(|section_id| {
                if section_id == "system" {
                    SidebarSection {
                        id: "system-header".into(),
                        title: String::new(),
                        state: ContentState::Ready,
                        message: None,
                        items: Vec::new(),
                        notification_panel: None,
                        quick_sliders: None,
                        quick_toggles: None,
                        media_player: None,
                        status_rings: None,
                        quick_launch: None,
                        quick_note: None,
                        week_strip: None,
                        world_clock: None,
                        wallpaper_gallery: None,
                        controls_card: None,
                        context_weather: None,
                        crypto_widget: None,
                    }
                } else {
                    let (title, message) = match section_id.as_str() {
                        "sliders" => ("Quick sliders", "Slider controls are unavailable"),
                        "toggles" => ("Quick toggles", "Toggle controls are unavailable"),
                        "notifications" => ("Notifications", "Notification service is unavailable"),
                        "media" => ("Media player", "Media player is unavailable"),
                        "widgets" => ("Widgets", "Widget services are unavailable"),
                        _ => ("Unavailable", "Sidebar section is unavailable"),
                    };
                    SidebarSection {
                        id: section_id.clone(),
                        title: title.into(),
                        state: ContentState::Empty,
                        message: Some(message.into()),
                        items: Vec::new(),
                        notification_panel: (section_id == "notifications").then(|| {
                            NotificationPanelInput {
                                state: NotificationPanelState::Unavailable,
                                message: Some("Notification service is unavailable".into()),
                                notifications: Vec::new(),
                                style: system_style.clone(),
                                on_action: Arc::new(|_, _| {}),
                            }
                        }),
                        quick_sliders: (section_id == "sliders").then(|| QuickSlidersInput {
                            state: QuickSlidersState::Unavailable,
                            message: Some("Slider controls are unavailable".into()),
                            sliders: Vec::new(),
                            style: system_style.clone(),
                            on_value_change: Arc::new(|_, _| {}),
                        }),
                        quick_toggles: (section_id == "toggles").then(|| QuickTogglesInput {
                            state: QuickTogglesState::Unavailable,
                            message: Some("Toggle controls are unavailable".into()),
                            toggles: Vec::new(),
                            style: system_style.clone(),
                            on_toggle: Arc::new(|_, _| {}),
                        }),
                        media_player: (section_id == "media").then(|| MediaPlayerInput {
                            media: Some(self.media.clone()),
                            style: system_style.clone(),
                            on_action: Some(self.actions.media_action.clone()),
                        }),
                        status_rings: None,
                        quick_launch: None,
                        quick_note: None,
                        week_strip: None,
                        world_clock: None,
                        wallpaper_gallery: None,
                        controls_card: None,
                        context_weather: None,
                        crypto_widget: None,
                    }
                }
            })
            .collect();
        let dashboard = dashboard_input(&self.config, &style, snapshot);
        let dashboard_enabled = dashboard_is_enabled(&self.config);
        let dashboard_open = state.dashboard_open && dashboard_enabled;
        let background = self
            .theme
            .colors
            .background
            .or(self.theme.colors.surface)
            .unwrap_or_else(|| rgba(0x0b0d10d8).into());
        let volume_mixer = volume_mixer_input(
            state.volume_mixer_open,
            self.audio_status,
            self.audio_snapshot.as_ref(),
            &self.theme,
            state.volume_device_picker_open,
            toggle_device_picker,
            close_mixer.clone(),
            state.volume_mixer_tab,
            self.actions.select_volume_mixer_tab.clone(),
        );
        let escape_close_left = close_left.clone();
        let escape_close_right = close_right.clone();
        let escape_close_dashboard = close_dashboard.clone();

        div()
            .id("ii.panel.surface")
            .size_full()
            .relative()
            .focusable()
            .tab_stop(true)
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if event.keystroke.key.eq_ignore_ascii_case("escape") {
                    if state.volume_mixer_open {
                        close_mixer_key();
                    } else if dashboard_open {
                        escape_close_dashboard();
                    } else if state.right_open {
                        escape_close_right();
                    } else if state.left_open {
                        escape_close_left();
                    } else {
                        return;
                    }
                    cx.stop_propagation();
                }
            })
            .when(dashboard_open, |root| {
                let close = close_dashboard.clone();
                root.child(
                    div()
                        .id("ii.dashboard.backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x00000088))
                        .on_click(move |_, _, _| close()),
                )
                .child(
                    div()
                        .id("ii.dashboard.panel")
                        .relative()
                        .absolute()
                        .top(px(48.0))
                        .bottom(px(24.0))
                        .left(px(24.0))
                        .right(px(24.0))
                        .max_w(px(
                            self.output_size.width.as_f32() * dashboard_width_ratio(&self.config)
                        ))
                        .mx_auto()
                        .bg(background)
                        .rounded(px(style.radius))
                        .shadow_lg()
                        .child(sidebars::render_dashboard(dashboard))
                        .child({
                            let keyboard_close = close_dashboard.clone();
                            div()
                                .id("ii.dashboard.close")
                                .absolute()
                                .top(px(10.0))
                                .right(px(10.0))
                                .size(px(32.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(style.card)
                                .rounded(px(16.0))
                                .text_color(style.foreground)
                                .aria_label(SharedString::from("Close dashboard"))
                                .focusable()
                                .tab_stop(true)
                                .on_click(move |_, _, _| close_dashboard())
                                .on_key_down(move |event: &KeyDownEvent, _, cx| {
                                    if event.keystroke.key == "enter"
                                        || event.keystroke.key == "space"
                                    {
                                        keyboard_close();
                                        cx.stop_propagation();
                                    }
                                })
                                .child("×")
                        }),
                )
            })
            .when(state.left_open, |root| {
                root.child(
                    sidebars::render_host_with_header(
                        &feature_host,
                        feature_input,
                        Some(feature_header),
                    )
                    .into_any_element(),
                )
            })
            .when(state.right_open, |root| {
                let header = system_sidebar_header::render(
                    system_sidebar_header::host_input(system_style.header_style),
                    &system_style,
                );
                root.child(
                    sidebars::render_host_with_header(&system_host, system_input, Some(header))
                        .into_any_element(),
                )
            })
            .when(state.volume_mixer_open, |root| {
                let close = self.actions.close_volume_mixer.clone();
                root.child(
                    div()
                        .id("ii.volume_mixer.backdrop")
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgba(0x00000088))
                        .on_click(move |_, _, _| close())
                        .child(
                            div()
                                .id("ii.volume_mixer.dialog")
                                .relative()
                                .w(px(440.))
                                .h(px(600.))
                                .max_h(px(self.output_size.height.as_f32() - 32.))
                                .p(px(18.))
                                .bg(style.surface)
                                .rounded(px(style.radius))
                                .shadow_lg()
                                .on_click(|_, _, cx| cx.stop_propagation())
                                .child(volume_mixer::render(volume_mixer)),
                        ),
                )
            })
            .when(!state.any_open(), |root| {
                root.child(div().id("ii.panel.closed").size_full())
            })
    }
}

fn volume_mixer_input(
    shown: bool,
    provider_status: ProviderStatus,
    snapshot: Option<&AudioSnapshot>,
    theme: &ThemeTokens,
    device_picker_open: bool,
    toggle_device_picker: Arc<dyn Fn() + Send + Sync>,
    close_volume_mixer: Arc<dyn Fn() + Send + Sync>,
    selected_tab: volume_mixer::VolumeMixerTab,
    select_tab: Arc<dyn Fn(volume_mixer::VolumeMixerTab) + Send + Sync>,
) -> volume_mixer::VolumeMixerInput {
    let colors = &theme.colors;
    let surface = colors
        .surface
        .or(colors.background)
        .unwrap_or_else(|| rgba(0x161b22f5).into());
    let elevated = colors
        .surface_elevated
        .or(colors.surface)
        .unwrap_or_else(|| rgba(0x21262df5).into());
    let foreground = colors.foreground.unwrap_or_else(|| rgba(0xe6edf3ff).into());
    let muted = colors
        .foreground_muted
        .unwrap_or_else(|| rgba(0x8b949eff).into());
    let accent = colors.accent.unwrap_or_else(|| rgba(0x58a6ffff).into());
    let border = colors.border.unwrap_or_else(|| rgba(0x6e768180).into());
    let nodes = snapshot
        .map(|snapshot| snapshot.nodes.as_slice())
        .unwrap_or_default();
    let is_output = selected_tab == volume_mixer::VolumeMixerTab::Output;
    let is_music = selected_tab == volume_mixer::VolumeMixerTab::Music;
    let devices = nodes
        .iter()
        .filter(|node| {
            if is_output {
                matches!(&node.kind, AudioNodeKind::OutputDevice)
            } else if selected_tab == volume_mixer::VolumeMixerTab::Input {
                matches!(&node.kind, AudioNodeKind::InputDevice)
            } else {
                false
            }
        })
        .map(|node| volume_mixer::AudioDevice {
            id: node.global_id.to_string(),
            name: node
                .description
                .clone()
                .or_else(|| node.name.clone())
                .unwrap_or_else(|| format!("PipeWire node {}", node.global_id)),
            selected: None,
            available: true,
        })
        .collect();
    let expected_stream_kind = if is_output {
        AudioNodeKind::OutputStream
    } else {
        AudioNodeKind::InputStream
    };
    let streams = nodes
        .iter()
        .filter(|node| !is_music && node.kind == expected_stream_kind)
        .map(|node| {
            // SPA channel volumes are intentionally not collapsed into one value.
            // Show the node level only when the provider supplied that scalar.
            let volume = node.volume;
            volume_mixer::AppStream {
                id: node.global_id.to_string(),
                name: node
                    .application_name
                    .clone()
                    .or_else(|| node.description.clone())
                    .or_else(|| node.name.clone())
                    .unwrap_or_else(|| format!("Audio stream {}", node.global_id)),
                icon: if is_output { "♪" } else { "●" }.into(),
                volume,
                max_volume: 1.,
                muted: node.muted,
                available: true,
            }
        })
        .collect::<Vec<_>>();
    let streams_state = if is_music {
        volume_mixer::MixerDataState::Unavailable
    } else {
        match provider_status {
            ProviderStatus::Connecting => volume_mixer::MixerDataState::Loading,
            ProviderStatus::Unavailable => volume_mixer::MixerDataState::Unavailable,
            ProviderStatus::Ready if streams.is_empty() => volume_mixer::MixerDataState::Ready,
            ProviderStatus::Ready => volume_mixer::MixerDataState::Ready,
        }
    };
    volume_mixer::VolumeMixerInput {
        selected_tab,
        dialog_shown: shown,
        current_device_name: None,
        devices,
        device_picker_open,
        streams,
        streams_state,
        streams_status_message: Some("PipeWire data is read-only in this prototype".into()),
        read_only: true,
        on_close: close_volume_mixer,
        on_select_tab: select_tab,
        style: volume_mixer::VolumeMixerStyle {
            surface,
            elevated,
            hover: colors.selection.unwrap_or(elevated),
            selected: colors.selection.unwrap_or(elevated),
            foreground,
            muted,
            accent,
            danger: colors.danger.unwrap_or(accent),
            border,
            radius: theme.shape.medium,
            small_radius: theme.shape.small,
            spacing: 12.,
        },
        on_toggle_device_picker: toggle_device_picker,
        on_select_device: None,
        on_set_volume: None,
        on_toggle_mute: None,
    }
}

fn sidebar_input(
    side: DrawerSide,
    drawer: DrawerState,
    style: SidebarStyle,
    title: &str,
    empty_message: String,
    on_close: Arc<dyn Fn() + Send + Sync>,
    on_expand: Arc<dyn Fn() + Send + Sync>,
) -> SidebarInput {
    SidebarInput {
        side,
        drawer,
        title: title.into(),
        profile: profile_from_environment(),
        navigation: Vec::new(),
        actions: Vec::new(),
        sections: vec![SidebarSection {
            id: "adapter-status".into(),
            title: "Available content".into(),
            state: ContentState::Empty,
            message: Some(empty_message),
            items: Vec::new(),
            notification_panel: None,
            quick_sliders: None,
            quick_toggles: None,
            media_player: None,
            status_rings: None,
            quick_launch: None,
            quick_note: None,
            week_strip: None,
            world_clock: None,
            wallpaper_gallery: None,
            controls_card: None,
            context_weather: None,
            crypto_widget: None,
        }],
        style,
        on_close,
        on_expand,
        on_navigation: Arc::new(|_| {}),
        on_action: Arc::new(|_| {}),
        on_section_item: Arc::new(|_| {}),
        on_retry: Arc::new(|_| {}),
    }
}

fn profile_from_environment() -> Option<Profile> {
    let name = std::env::var("USER").ok().filter(|name| !name.is_empty())?;
    Some(Profile {
        identity: name.clone(),
        name,
        avatar: None,
        banner: None,
        status: None,
    })
}

fn sidebar_style(
    config: &ShellConfig,
    theme: &ThemeTokens,
    output_size: gpui::Size<gpui::Pixels>,
) -> SidebarStyle {
    let colors = &theme.colors;
    let feature_width = config
        .sidebar
        .feature_width
        .unwrap_or(460.0)
        .clamp(280.0, 900.0) as f32;
    SidebarStyle {
        surface: colors
            .surface
            .or(colors.background)
            .unwrap_or_else(|| rgba(0x161b22f5).into()),
        card: colors
            .surface_elevated
            .or(colors.surface)
            .unwrap_or_else(|| rgba(0x21262df5).into()),
        hover: colors.selection.unwrap_or_else(|| rgba(0x30363ddd).into()),
        selected: colors.selection.unwrap_or_else(|| rgba(0x30363ddd).into()),
        foreground: colors.foreground.unwrap_or_else(|| white().into()),
        muted: colors
            .foreground_muted
            .unwrap_or_else(|| rgba(0xb1bac4ff).into()),
        accent: colors.accent.unwrap_or_else(|| rgba(0x79c0ffff).into()),
        border: colors.border.unwrap_or_else(|| rgba(0x30363dff).into()),
        radius: theme.shape.large,
        card_radius: theme.shape.medium,
        padding: theme.spacing.md,
        gap: theme.spacing.sm,
        width: feature_width,
        height: if config.sidebar.size_mode.as_deref() == Some("custom") {
            config
                .sidebar
                .custom_height
                .unwrap_or(output_size.height.as_f32() as f64)
                .clamp(320.0, output_size.height.as_f32() as f64) as f32
        } else {
            output_size.height.as_f32()
        },
        compact: config.sidebar.compact,
        header_style: config
            .sidebar
            .right_header_style
            .as_deref()
            .map(|style| {
                if style == "classic" {
                    SidebarHeaderStyle::Classic
                } else {
                    SidebarHeaderStyle::Profile
                }
            })
            .unwrap_or(SidebarHeaderStyle::Profile),
        size_mode: if config.sidebar.size_mode.as_deref() == Some("custom") {
            SidebarSizeMode::Custom
        } else if config.sidebar.size_mode.as_deref() == Some("content") {
            SidebarSizeMode::Content
        } else {
            SidebarSizeMode::Full
        },
    }
}

fn dashboard_input(
    config: &ShellConfig,
    style: &SidebarStyle,
    snapshot: niri::Snapshot,
) -> DashboardInput {
    let zones = &config.dashboard.layout;
    let make_column = |ids: &[String]| {
        ids.iter()
            .map(|id| {
                let (title, glyph) = dashboard_card_identity(id);
                let (state, message, lines) = match id.as_str() {
                    "welcome" => {
                        let lines = [snapshot.clock.clone(), snapshot.active_window.clone()]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>();
                        if lines.is_empty() {
                            (
                                ContentState::Error,
                                Some(
                                    snapshot
                                        .error
                                        .clone()
                                        .unwrap_or_else(|| "Niri data unavailable".into()),
                                ),
                                lines,
                            )
                        } else {
                            (ContentState::Ready, None, lines)
                        }
                    }
                    "clock" => match snapshot.clock.clone() {
                        Some(value) => (ContentState::Ready, None, vec![value]),
                        None => (
                            ContentState::Error,
                            Some("System clock unavailable".into()),
                            vec![],
                        ),
                    },
                    _ => (
                        ContentState::Error,
                        Some("GPUI service adapter unavailable".into()),
                        Vec::new(),
                    ),
                };
                DashboardCard {
                    id: id.clone(),
                    title: title.into(),
                    subtitle: None,
                    glyph: glyph.into(),
                    state,
                    message,
                    lines,
                }
            })
            .collect()
    };
    DashboardInput {
        title: Some("Dashboard".into()),
        columns: [
            make_column(&zones.left),
            make_column(&zones.center),
            make_column(&zones.right),
        ],
        style: style.clone(),
    }
}

fn dashboard_card_identity(id: &str) -> (&'static str, &'static str) {
    match id {
        "welcome" => ("Welcome", "◉"),
        "clock" => ("Clock", "◷"),
        "system" => ("System", "▤"),
        "github" => ("GitHub", "◇"),
        "notifications" => ("Notifications", "♧"),
        "todo" => ("To do", "☷"),
        "media" => ("Media", "♫"),
        "weather" => ("Weather", "☼"),
        "calendar" => ("Calendar", "▦"),
        other => {
            eprintln!("GPUI ii dashboard: unknown configured card `{other}` omitted");
            ("Unavailable", "·")
        }
    }
}

fn dashboard_is_enabled(config: &ShellConfig) -> bool {
    config
        .enabled_panels
        .iter()
        .any(|panel| panel == "iiDashboard")
        && config.dashboard.enabled
}

fn crypto_cache_path() -> std::path::PathBuf {
    let state_root = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/state"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".local/state"));
    state_root.join("user/crypto_cache.json")
}

fn weather_cache_path() -> std::path::PathBuf {
    let state_root = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/state"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".local/state"));
    state_root.join("user/weather-cache.json")
}

fn dashboard_width_ratio(config: &ShellConfig) -> f32 {
    config.dashboard.width_ratio.unwrap_or(0.62).clamp(0.4, 0.9) as f32
}

fn configured_side(slot: Option<&str>, fallback: DrawerSide) -> DrawerSide {
    match slot {
        Some("left") => DrawerSide::Left,
        Some("right") => DrawerSide::Right,
        _ => fallback,
    }
}

fn screen_is_locked() -> bool {
    // No compositor lock-state adapter exists in the current GPUI prototype.
    false
}
