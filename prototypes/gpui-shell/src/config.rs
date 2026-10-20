//! Typed, defensive reader for the existing iNtoo JSON configuration.
//!
//! This intentionally reads only the II bar and shared appearance inputs needed
//! by the first GPUI surfaces. Colors stay as source strings/JSON values: the
//! QML theme resolver has not been ported, so this adapter never invents them.

use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

const DEFAULT_CONFIG: &str = include_str!("../../../defaults/config.json");

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigLoad {
    pub config: ShellConfig,
    pub source: ConfigSource,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Runtime,
    BundledDefaults,
    BundledDefaultsAfterError,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShellConfig {
    pub panel_family: Option<String>,
    pub enabled_panels: Vec<String>,
    pub bar: BarConfig,
    pub media: MediaConfig,
    pub notifications: NotificationsConfig,
    pub osd: OsdConfig,
    pub control_panel: ControlPanelConfig,
    pub appearance: AppearanceConfig,
    pub background: BackgroundConfig,
    /// Iris desktop/background inputs, kept separate from II and Waffle surfaces.
    pub iris_background: IrisBackgroundConfig,
    /// Configured `wallpapers.directory`; empty or absent uses Pictures/Wallpapers.
    pub wallpapers_directory: Option<String>,
    pub waffle: WaffleConfig,
    pub clipboard_pinned: Vec<String>,
    pub sidebar: SidebarConfig,
    pub dashboard: DashboardConfig,
    /// QML `battery.low` warning threshold, expressed as a percentage.
    pub battery_low: Option<f64>,
    /// Settings owned by the standalone GPUI MVP and namespaced in `gpuiMvp`.
    pub gpui_mvp: crate::mvp_settings::MvpSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaConfig {
    /// QML `media.screenList`; an empty or unmatched list means all outputs.
    pub screen_list: Vec<String>,
    /// QML `media.filterDuplicatePlayers`.
    pub filter_duplicate_players: bool,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            screen_list: Vec::new(),
            filter_duplicate_players: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NotificationPopupPosition {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationsConfig {
    pub position: NotificationPopupPosition,
    pub screen_list: Vec<String>,
    pub edge_margin: u32,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            position: NotificationPopupPosition::TopRight,
            screen_list: Vec::new(),
            edge_margin: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsdConfig {
    /// QML `osd.screenList`; empty or fully stale lists target all outputs.
    pub screen_list: Vec<String>,
    pub timeout_ms: u64,
    pub media_enabled: bool,
}

impl Default for OsdConfig {
    fn default() -> Self {
        Self {
            screen_list: Vec::new(),
            timeout_ms: 2_000,
            media_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlPanelStyle {
    Panel,
    Island,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlPanelSection {
    Profile,
    DateTime,
    Media,
    Wallpaper,
    Weather,
    System,
    Sliders,
    QuickActions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlPanelConfig {
    pub keep_loaded: bool,
    pub compact_mode: bool,
    pub style: ControlPanelStyle,
    pub show_media_section: bool,
    pub show_weather_section: bool,
    pub show_wallpaper_section: bool,
    pub show_system_section: bool,
    pub show_sliders_section: bool,
    pub show_quick_actions_section: bool,
    pub show_wallpaper_scheme_chips: bool,
}

impl Default for ControlPanelConfig {
    fn default() -> Self {
        Self {
            keep_loaded: false,
            compact_mode: true,
            style: ControlPanelStyle::Panel,
            show_media_section: true,
            show_weather_section: true,
            show_wallpaper_section: true,
            show_system_section: true,
            show_sliders_section: true,
            show_quick_actions_section: true,
            show_wallpaper_scheme_chips: false,
        }
    }
}

impl ControlPanelConfig {
    /// QML keeps Profile and DateTime visible and follows this fixed order for
    /// optional content sections.
    pub fn visible_sections(&self) -> Vec<ControlPanelSection> {
        let mut sections = vec![ControlPanelSection::Profile, ControlPanelSection::DateTime];
        for (enabled, section) in [
            (self.show_media_section, ControlPanelSection::Media),
            (self.show_wallpaper_section, ControlPanelSection::Wallpaper),
            (self.show_weather_section, ControlPanelSection::Weather),
            (self.show_system_section, ControlPanelSection::System),
            (self.show_sliders_section, ControlPanelSection::Sliders),
            (
                self.show_quick_actions_section,
                ControlPanelSection::QuickActions,
            ),
        ] {
            if enabled {
                sections.push(section);
            }
        }
        sections
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SidebarConfig {
    pub screen_list: Vec<String>,
    pub feature_width: Option<f64>,
    pub system_width: Option<f64>,
    pub custom_height: Option<f64>,
    pub size_mode: Option<String>,
    pub compact: bool,
    pub feature_slot: Option<String>,
    pub system_slot: Option<String>,
    pub right_header_style: Option<String>,
    pub right_section_order: Vec<String>,
    pub right_controls_section_order: Vec<String>,
    pub left_tabs: Vec<SidebarLeftTab>,
    pub left_tab_order: Vec<String>,
    /// QML `sidebar.widgets.media` visibility switch (defaults to true).
    pub widgets_media_enabled: bool,
    /// QML `sidebar.widgets.context` visibility switch (defaults to true).
    pub widgets_context_enabled: bool,
    /// QML `sidebar.widgets.widgetOrder`, normalized with missing widget IDs appended.
    pub widget_order: Vec<String>,
    /// QML `sidebar.widgets.status` visibility switch (defaults to true).
    pub widgets_status_enabled: bool,
    pub widgets_week_enabled: bool,
    pub widgets_launch_enabled: bool,
    pub widgets_note_enabled: bool,
    /// QML `sidebar.widgets.controls` visibility switch (defaults to true).
    pub widgets_controls_enabled: bool,
    /// QML `sidebar.widgets.worldClock` visibility switch (defaults to false in bundled config).
    pub widgets_world_clock_enabled: bool,
    /// QML `sidebar.widgets.crypto` visibility switch (defaults to false).
    pub widgets_crypto_enabled: bool,
    /// QML `sidebar.widgets.crypto_settings.refreshInterval`, in seconds.
    pub crypto_refresh_interval_seconds: u64,
    /// QML `sidebar.widgets.crypto_settings.coins`, preserving configured order.
    pub crypto_coins: Vec<String>,
    /// QML `sidebar.widgets.contextShowWeather` visibility switch.
    pub context_show_weather: bool,
    pub world_clock: WorldClockConfig,
    pub widgets_wallpaper_enabled: bool,
    pub quick_wallpaper_show_header: bool,
    pub status_rings_show_cpu: bool,
    pub status_rings_show_ram: bool,
    pub status_rings_show_battery: bool,
    /// QML `sidebar.widgets.controlsCard.show*` presentation gates.
    pub controls_card_show_dark_mode: bool,
    pub controls_card_show_dnd: bool,
    pub controls_card_show_night_light: bool,
    pub controls_card_show_game_mode: bool,
    pub controls_card_show_network: bool,
    pub controls_card_show_bluetooth: bool,
    pub controls_card_show_settings: bool,
    pub controls_card_show_lock: bool,
    /// Configured QML `sidebar.widgets.quickLaunch` entries in stored order.
    pub quick_launch: Vec<QuickLaunchEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldClockConfig {
    pub timezones: Vec<String>,
    pub show_seconds: bool,
    pub use_24_hour: bool,
    pub show_date: bool,
    pub highlight_local: bool,
}

impl Default for WorldClockConfig {
    fn default() -> Self {
        Self {
            timezones: Vec::new(),
            show_seconds: false,
            use_24_hour: true,
            show_date: true,
            highlight_local: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickLaunchEntry {
    pub icon: Option<String>,
    pub name: Option<String>,
    pub cmd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarLeftTab {
    pub id: String,
    pub enabled: bool,
}

const SIDEBAR_RIGHT_SECTION_ORDER: [&str; 5] =
    ["system", "sliders", "toggles", "notifications", "widgets"];

const SIDEBAR_RIGHT_CONTROLS_SECTION_ORDER: [&str; 5] =
    ["sliders", "toggles", "devices", "media", "quickActions"];
const SIDEBAR_LEFT_TAB_ORDER: [&str; 10] = [
    "widgets",
    "ai",
    "translator",
    "anime",
    "animeSchedule",
    "wallhaven",
    "news",
    "ytmusic",
    "tools",
    "software",
];

const SIDEBAR_WIDGET_ORDER: [&str; 10] = [
    "media",
    "week",
    "context",
    "note",
    "launch",
    "controls",
    "status",
    "crypto",
    "wallpaper",
    "worldclock",
];

/// Mirrors DraggableWidgetContainer: keep recognized saved IDs in order and
/// append IDs missing from the saved list in the QML default order.
pub fn normalize_sidebar_widget_order(saved: &[String]) -> Vec<String> {
    let mut order = Vec::with_capacity(SIDEBAR_WIDGET_ORDER.len());
    for id in saved {
        if SIDEBAR_WIDGET_ORDER.contains(&id.as_str()) && !order.contains(id) {
            order.push(id.clone());
        }
    }
    for id in SIDEBAR_WIDGET_ORDER {
        if !order.iter().any(|saved| saved == id) {
            order.push(id.to_owned());
        }
    }
    order
}

pub fn normalize_sidebar_left_tab_order(saved: &[String]) -> Vec<String> {
    let mut order = Vec::with_capacity(SIDEBAR_LEFT_TAB_ORDER.len());
    for id in saved {
        if SIDEBAR_LEFT_TAB_ORDER.contains(&id.as_str()) && !order.contains(id) {
            order.push(id.clone());
        }
    }
    for id in SIDEBAR_LEFT_TAB_ORDER {
        if !order.iter().any(|saved| saved == id) {
            order.push(id.to_owned());
        }
    }
    order
}

/// Sanitizes `sidebar.right.sectionOrder` like SidebarRightContent: discard
/// unknown/duplicate values, then append the standard right sidebar defaults.
pub fn normalize_sidebar_right_section_order(saved: &[String]) -> Vec<String> {
    let mut order = Vec::with_capacity(SIDEBAR_RIGHT_SECTION_ORDER.len());
    for id in saved {
        if SIDEBAR_RIGHT_SECTION_ORDER.contains(&id.as_str()) && !order.contains(id) {
            order.push(id.clone());
        }
    }
    for id in SIDEBAR_RIGHT_SECTION_ORDER {
        if !order.iter().any(|saved| saved == id) {
            order.push(id.to_owned());
        }
    }
    order
}

/// Sanitizes compact sidebar controls independently from standard section order.
/// Explicit lists retain their membership and order; an empty list receives the
/// QML default order, including media.
pub fn normalize_sidebar_right_controls_section_order(saved: &[String]) -> Vec<String> {
    if saved.is_empty() {
        return SIDEBAR_RIGHT_CONTROLS_SECTION_ORDER
            .iter()
            .map(|id| (*id).to_owned())
            .collect();
    }
    let mut order = Vec::with_capacity(SIDEBAR_RIGHT_CONTROLS_SECTION_ORDER.len());
    for id in saved {
        if SIDEBAR_RIGHT_CONTROLS_SECTION_ORDER.contains(&id.as_str()) && !order.contains(id) {
            order.push(id.clone());
        }
    }
    order
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DashboardConfig {
    pub enabled: bool,
    pub width_ratio: Option<f64>,
    pub layout: DashboardLayout,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DashboardLayout {
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaffleConfig {
    pub bar: WaffleBarConfig,
    pub background: WaffleBackgroundConfig,
    pub task_view: WaffleTaskViewConfig,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaffleBackgroundConfig {
    pub use_main_wallpaper: Option<bool>,
    pub wallpaper_path: Option<String>,
    pub hide_when_fullscreen: Option<bool>,
    pub backdrop: WaffleBackdropConfig,
}

/// Waffle owns its backdrop namespace independently of the shared/ii backdrop.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaffleBackdropConfig {
    pub enabled: Option<bool>,
    pub hide_wallpaper: Option<bool>,
    pub use_main_wallpaper: Option<bool>,
    pub wallpaper_path: Option<String>,
    pub blur_radius: Option<f64>,
    pub dim: Option<f64>,
    pub saturation: Option<f64>,
    pub contrast: Option<f64>,
    pub vignette_enabled: Option<bool>,
    pub vignette_intensity: Option<f64>,
    pub vignette_radius: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaffleTaskViewConfig {
    pub mode: Option<String>,
    pub close_on_select: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaffleBarConfig {
    pub bottom: Option<bool>,
    pub screen_list: Vec<String>,
    /// Source `waffles.bar.iconSize`, in logical pixels before monitor scaling.
    pub icon_size: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackgroundConfig {
    pub wallpaper_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub fill_mode: Option<String>,
    pub use_main_wallpaper: Option<bool>,
    pub enable_animation: Option<bool>,
    pub pause_animation_on_battery: Option<bool>,
    pub video_pause: Option<String>,
    pub hide_when_fullscreen: Option<bool>,
    pub transition: BackgroundTransitionConfig,
    pub effects: BackgroundEffectsConfig,
    pub parallax: BackgroundParallaxConfig,
    pub multi_monitor_enabled: Option<bool>,
    pub wallpapers_by_monitor: Vec<WallpaperOutputConfig>,
    pub backdrop: BackdropConfig,
}

/// The desktop-widget background and overview backdrop are sourced from the
/// shared `background` tree, while the menu and module gate live under `iris`.
/// Raw source objects are retained alongside typed fields so new QML keys are
/// not discarded before their GPUI adapter is implemented.
#[derive(Debug, Clone, PartialEq)]
pub struct IrisBackgroundConfig {
    pub source: ConfigSource,
    pub desktop_widgets_enabled: bool,
    pub desktop_menu_wallpaper: bool,
    pub desktop_menu_items: Vec<String>,
    pub raw_runtime_sections: BTreeMap<String, Value>,
    pub raw_default_sections: BTreeMap<String, Value>,
}

impl Default for IrisBackgroundConfig {
    fn default() -> Self {
        Self {
            source: ConfigSource::BundledDefaults,
            desktop_widgets_enabled: true,
            desktop_menu_wallpaper: true,
            desktop_menu_items: vec![
                "widgets",
                "customize",
                "screenshot",
                "terminal",
                "settings",
                "restart",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            raw_runtime_sections: BTreeMap::new(),
            raw_default_sections: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackgroundTransitionConfig {
    pub enabled: Option<bool>,
    pub kind: Option<String>,
    pub direction: Option<String>,
    pub duration_ms: Option<u64>,
    pub bezier: Vec<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackgroundEffectsConfig {
    pub blur_enabled: Option<bool>,
    pub blur_radius: Option<f64>,
    pub thumbnail_blur_strength: Option<f64>,
    pub animated_blur_enabled: Option<bool>,
    pub dim: Option<f64>,
    pub dynamic_dim: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackgroundParallaxConfig {
    pub enabled: Option<bool>,
    pub axis: Option<String>,
    pub vertical: Option<bool>,
    pub auto_vertical: Option<bool>,
    pub workspace_enabled: Option<bool>,
    pub workspace_shift: Option<f64>,
    pub workspace_zoom: Option<f64>,
    pub zoom: Option<f64>,
    pub sidebar_enabled: Option<bool>,
    pub panel_shift: Option<f64>,
    pub widgets_factor: Option<f64>,
    pub widget_depth: Option<f64>,
    pub pause_during_transitions: Option<bool>,
    pub transition_settle_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackdropConfig {
    pub enabled: Option<bool>,
    pub hide_wallpaper: Option<bool>,
    pub use_main_wallpaper: Option<bool>,
    pub wallpaper_path: Option<String>,
    pub thumbnail_path: Option<String>,
    /// Legacy persisted `fillMode`; rendering currently always covers output.
    pub fill_mode: Option<String>,
    pub blur_radius: Option<f64>,
    pub dim: Option<f64>,
    pub saturation: Option<f64>,
    pub contrast: Option<f64>,
    pub vignette_enabled: Option<bool>,
    pub vignette_intensity: Option<f64>,
    pub vignette_radius: Option<f64>,
    pub enable_animation: Option<bool>,
    pub enable_animated_blur: Option<bool>,
    pub use_aurora_style: Option<bool>,
    pub aurora_overlay_opacity: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WallpaperOutputConfig {
    pub monitor: Option<String>,
    pub path: Option<String>,
    pub backdrop_path: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MediaPopupMode {
    #[default]
    Dock,
    Bar,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarConfig {
    /// `bottom` and `vertical` are the source configuration's placement flags.
    pub bottom: Option<bool>,
    pub vertical: Option<bool>,
    pub height: Option<f64>,
    /// QML `bar.screenList`; an empty list targets every output and a stale
    /// list falls back to every output.
    pub screen_list: Vec<String>,
    /// QML `bar.appearanceStyle`; retained so critical surfaces are rebuilt
    /// when their composition root changes style.
    pub appearance_style: Option<String>,
    pub show_background: Option<bool>,
    pub borderless: Option<bool>,
    /// QML `bar.verbose`, defaulting to true.
    pub verbose: bool,
    /// QML `bar.media.maxWidth`, normalized to the media component's minimum.
    pub media_max_width: f64,
    /// QML `bar.media.popupMode`; defaults to the full-screen dock controls.
    pub media_popup_mode: MediaPopupMode,
    pub modules: BTreeMap<String, bool>,
    pub resources: ResourceConfig,
    pub layout: BarLayout,
    pub workspace: WorkspaceConfig,
    pub weather: WeatherConfig,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            bottom: None,
            vertical: None,
            height: None,
            screen_list: Vec::new(),
            appearance_style: None,
            show_background: None,
            borderless: None,
            verbose: true,
            media_max_width: 220.0,
            media_popup_mode: MediaPopupMode::Dock,
            modules: BTreeMap::new(),
            resources: ResourceConfig::default(),
            layout: BarLayout::default(),
            workspace: WorkspaceConfig::default(),
            weather: WeatherConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherConfig {
    pub enabled: bool,
    pub city: Option<String>,
    pub manual_lat: Option<f64>,
    pub manual_lon: Option<f64>,
    pub use_uscs: bool,
    /// QML `bar.weather.fetchInterval`, in minutes, clamped to the 5..=60 slider.
    pub fetch_interval_minutes: u64,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            city: None,
            manual_lat: None,
            manual_lon: None,
            use_uscs: false,
            fetch_interval_minutes: 10,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceConfig {
    pub show_memory_indicator: Option<bool>,
    pub show_temp_indicator: Option<bool>,
    pub show_swap_indicator: Option<bool>,
    pub show_cpu_indicator: Option<bool>,
    pub show_gpu_indicator: Option<bool>,
    pub memory_warning_threshold: Option<f64>,
    pub temp_caution_threshold: Option<f64>,
    pub temp_warning_threshold: Option<f64>,
    pub swap_warning_threshold: Option<f64>,
    pub cpu_warning_threshold: Option<f64>,
    pub gpu_warning_threshold: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BarLayout {
    pub left: Vec<String>,
    pub center_left: Vec<String>,
    pub center: Vec<String>,
    pub center_right: Vec<String>,
    pub right: Vec<String>,
    pub spacer_mode: SpacerMode,
    /// Minimum QML `bar.layout.spacerWidth`, clamped to zero or above.
    pub spacer_width: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpacerMode {
    #[default]
    Auto,
    Fill,
    Fixed,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkspaceConfig {
    pub always_show_numbers: Option<bool>,
    pub monochrome_icons: Option<bool>,
    pub per_monitor: Option<bool>,
    pub dynamic_count: Option<bool>,
    pub scroll_behavior: Option<String>,
    pub invert_scroll: Option<bool>,
    pub show_app_icons: Option<bool>,
    pub shown: Option<u64>,
    pub use_nerd_font: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppearanceConfig {
    pub global_style: Option<String>,
    pub theme: Option<String>,
    pub style_preset: Option<String>,
    /// Island selects its own visual preset independently of appearance.theme.
    pub island_preset: Option<String>,
    pub transparency_enabled: Option<bool>,
    pub transparency_automatic: Option<bool>,
    pub background_transparency: Option<f64>,
    pub content_transparency: Option<f64>,
    /// Full source sections for later family-aware theme resolution.
    pub source_sections: BTreeMap<String, Value>,
}

impl ConfigLoad {
    /// Read an existing runtime file; when it does not exist, use the checked-in
    /// defaults. A malformed file is diagnosed and falls back safely as well.
    pub fn from_path(path: impl AsRef<Path>) -> Self {
        match fs::read_to_string(path.as_ref()) {
            Ok(text) => Self::from_bytes(text.as_bytes()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::from_defaults(),
            Err(error) => Self::fallback(format!("cannot read runtime config: {error}")),
        }
    }

    /// Parse one already-read file image. The config watcher uses this entry
    /// point so its content hash and parsed snapshot refer to the same bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match serde_json::from_slice::<Value>(bytes) {
            Ok(value) if value.is_object() => Self::from_value(value, ConfigSource::Runtime),
            Ok(_) => Self::fallback("runtime config root must be a JSON object"),
            Err(error) => Self::fallback(format!("malformed runtime config: {error}")),
        }
    }

    pub fn from_defaults() -> Self {
        match serde_json::from_str::<Value>(DEFAULT_CONFIG) {
            Ok(value) => Self::from_value(value, ConfigSource::BundledDefaults),
            Err(error) => Self {
                config: ShellConfig::default(),
                source: ConfigSource::BundledDefaultsAfterError,
                diagnostics: vec![format!("bundled defaults are invalid JSON: {error}")],
            },
        }
    }

    fn fallback(message: impl Into<String>) -> Self {
        let mut loaded = Self::from_defaults();
        loaded.source = ConfigSource::BundledDefaultsAfterError;
        loaded.diagnostics.insert(0, message.into());
        loaded
    }

    fn from_value(root: Value, source: ConfigSource) -> Self {
        let mut diagnostics = Vec::new();
        let (gpui_mvp, gpui_mvp_diagnostics) = crate::mvp_settings::MvpSettings::from_json(&root);
        diagnostics.extend(gpui_mvp_diagnostics);
        let defaults = serde_json::from_str::<Value>(DEFAULT_CONFIG).unwrap_or(Value::Null);
        let bar = section(&root, "bar").or_else(|| section(&defaults, "bar"));
        let appearance = section(&root, "appearance").or_else(|| section(&defaults, "appearance"));
        let runtime_background = section(&root, "background");
        let default_background = section(&defaults, "background");
        let background = runtime_background.or(default_background);
        let battery = section(&root, "battery").or_else(|| section(&defaults, "battery"));
        let media = section(&root, "media").or_else(|| section(&defaults, "media"));
        let control_panel = section(&root, "controlPanel");
        let default_control_panel = section(&defaults, "controlPanel");
        let mut cfg = ShellConfig::default();
        cfg.iris_background = iris_background_config(&root, &defaults, source, &mut diagnostics);
        cfg.control_panel.keep_loaded = bool_with_default(
            control_panel,
            default_control_panel,
            "keepLoaded",
            "controlPanel.keepLoaded",
            false,
            &mut diagnostics,
        );
        cfg.control_panel.compact_mode = bool_with_default(
            control_panel,
            default_control_panel,
            "compactMode",
            "controlPanel.compactMode",
            true,
            &mut diagnostics,
        );
        cfg.control_panel.show_media_section = bool_with_default(
            control_panel,
            default_control_panel,
            "showMediaSection",
            "controlPanel.showMediaSection",
            true,
            &mut diagnostics,
        );
        cfg.control_panel.show_weather_section = bool_with_default(
            control_panel,
            default_control_panel,
            "showWeatherSection",
            "controlPanel.showWeatherSection",
            true,
            &mut diagnostics,
        );
        cfg.control_panel.show_wallpaper_section = bool_with_default(
            control_panel,
            default_control_panel,
            "showWallpaperSection",
            "controlPanel.showWallpaperSection",
            true,
            &mut diagnostics,
        );
        cfg.control_panel.show_system_section = bool_with_default(
            control_panel,
            default_control_panel,
            "showSystemSection",
            "controlPanel.showSystemSection",
            true,
            &mut diagnostics,
        );
        cfg.control_panel.show_sliders_section = bool_with_default(
            control_panel,
            default_control_panel,
            "showSlidersSection",
            "controlPanel.showSlidersSection",
            true,
            &mut diagnostics,
        );
        cfg.control_panel.show_quick_actions_section = bool_with_default(
            control_panel,
            default_control_panel,
            "showQuickActionsSection",
            "controlPanel.showQuickActionsSection",
            true,
            &mut diagnostics,
        );
        cfg.control_panel.show_wallpaper_scheme_chips = bool_with_default(
            control_panel,
            default_control_panel,
            "showWallpaperSchemeChips",
            "controlPanel.showWallpaperSchemeChips",
            false,
            &mut diagnostics,
        );
        cfg.control_panel.style = match control_panel
            .and_then(|value| string_at(value, "style", "controlPanel.style", &mut diagnostics))
            .or_else(|| {
                default_control_panel.and_then(|value| {
                    string_at(
                        value,
                        "style",
                        "defaults.controlPanel.style",
                        &mut diagnostics,
                    )
                })
            })
            .as_deref()
        {
            None | Some("panel") => ControlPanelStyle::Panel,
            Some("island") => ControlPanelStyle::Island,
            Some(_) => {
                diagnostics.push("controlPanel.style must be panel or island".into());
                ControlPanelStyle::Panel
            }
        };
        cfg.media.screen_list = string_list(
            media.and_then(|value| value.get("screenList")),
            "media.screenList",
            &mut diagnostics,
        );
        cfg.media.filter_duplicate_players = media
            .and_then(|value| {
                bool_at(
                    value,
                    "filterDuplicatePlayers",
                    "media.filterDuplicatePlayers",
                    &mut diagnostics,
                )
            })
            .unwrap_or(true);
        let notifications = root.get("notifications");
        let default_notifications = defaults.get("notifications");
        cfg.notifications.screen_list = string_list(
            notifications
                .and_then(|value| value.get("screenList"))
                .or_else(|| default_notifications.and_then(|value| value.get("screenList"))),
            "notifications.screenList",
            &mut diagnostics,
        );
        cfg.notifications.position = match notifications
            .and_then(|value| {
                string_at(
                    value,
                    "position",
                    "notifications.position",
                    &mut diagnostics,
                )
            })
            .or_else(|| {
                default_notifications.and_then(|value| {
                    string_at(
                        value,
                        "position",
                        "defaults.notifications.position",
                        &mut diagnostics,
                    )
                })
            })
            .as_deref()
        {
            Some("topLeft") => NotificationPopupPosition::TopLeft,
            Some("bottomLeft") => NotificationPopupPosition::BottomLeft,
            Some("bottomRight") => NotificationPopupPosition::BottomRight,
            None | Some("topRight") => NotificationPopupPosition::TopRight,
            Some(_) => {
                diagnostics.push(
                    "notifications.position must be topRight, topLeft, bottomRight, or bottomLeft"
                        .into(),
                );
                NotificationPopupPosition::TopRight
            }
        };
        cfg.notifications.edge_margin = notifications
            .and_then(|value| {
                number_at(
                    value,
                    "edgeMargin",
                    "notifications.edgeMargin",
                    &mut diagnostics,
                )
            })
            .or_else(|| {
                default_notifications.and_then(|value| {
                    number_at(
                        value,
                        "edgeMargin",
                        "defaults.notifications.edgeMargin",
                        &mut diagnostics,
                    )
                })
            })
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|value| value.min(256.0) as u32)
            .unwrap_or(4);
        let osd = root.get("osd");
        let default_osd = defaults.get("osd");
        cfg.osd.screen_list = string_list(
            osd.and_then(|value| value.get("screenList"))
                .or_else(|| default_osd.and_then(|value| value.get("screenList"))),
            "osd.screenList",
            &mut diagnostics,
        );
        let configured_osd_timeout = osd
            .and_then(|value| number_at(value, "timeout", "osd.timeout", &mut diagnostics))
            .filter(|value| value.is_finite() && *value >= 0.0);
        let default_osd_timeout = default_osd
            .and_then(|value| number_at(value, "timeout", "defaults.osd.timeout", &mut diagnostics))
            .filter(|value| value.is_finite() && *value >= 0.0);
        cfg.osd.timeout_ms = configured_osd_timeout
            .or(default_osd_timeout)
            .map(|value| value.min(60_000.0) as u64)
            .unwrap_or(2_000);
        cfg.osd.media_enabled = bool_with_default(
            osd,
            default_osd,
            "mediaEnabled",
            "osd.mediaEnabled",
            true,
            &mut diagnostics,
        );
        cfg.gpui_mvp = gpui_mvp;
        cfg.battery_low = battery
            .and_then(|value| number_at(value, "low", "battery.low", &mut diagnostics))
            .or_else(|| {
                defaults
                    .get("battery")
                    .and_then(|value| number_at(value, "low", "battery.low", &mut diagnostics))
            });
        cfg.panel_family = string_at(&root, "panelFamily", "panelFamily", &mut diagnostics)
            .or_else(|| string_at(&defaults, "panelFamily", "panelFamily", &mut diagnostics));
        cfg.enabled_panels = if root.get("enabledPanels").is_some() {
            string_list(root.get("enabledPanels"), "enabledPanels", &mut diagnostics)
        } else {
            string_list(
                defaults.get("enabledPanels"),
                "enabledPanels",
                &mut diagnostics,
            )
        };
        let sidebar = root.get("sidebar").or_else(|| defaults.get("sidebar"));
        let wallpapers = root
            .get("wallpapers")
            .or_else(|| defaults.get("wallpapers"));
        cfg.wallpapers_directory = wallpapers
            .and_then(|value| {
                string_at(value, "directory", "wallpapers.directory", &mut diagnostics)
            })
            .filter(|directory| !directory.trim().is_empty());
        if let Some(sidebar) = sidebar {
            cfg.sidebar.widgets_context_enabled = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "context",
                "sidebar.widgets.context",
                true,
                &mut diagnostics,
            );
            cfg.sidebar.widgets_media_enabled = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "media",
                "sidebar.widgets.media",
                true,
                &mut diagnostics,
            );
            cfg.sidebar.widgets_status_enabled = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "status",
                "sidebar.widgets.status",
                true,
                &mut diagnostics,
            );
            cfg.sidebar.widgets_week_enabled = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "week",
                "sidebar.widgets.week",
                true,
                &mut diagnostics,
            );
            cfg.sidebar.widgets_launch_enabled = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "launch",
                "sidebar.widgets.launch",
                false,
                &mut diagnostics,
            );
            cfg.sidebar.widgets_controls_enabled = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "controls",
                "sidebar.widgets.controls",
                true,
                &mut diagnostics,
            );
            cfg.sidebar.widgets_note_enabled = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "note",
                "sidebar.widgets.note",
                false,
                &mut diagnostics,
            );
            cfg.sidebar.widgets_world_clock_enabled = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "worldClock",
                "sidebar.widgets.worldClock",
                true,
                &mut diagnostics,
            );
            cfg.sidebar.widgets_crypto_enabled = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "crypto",
                "sidebar.widgets.crypto",
                false,
                &mut diagnostics,
            );
            let root_crypto_settings = root
                .get("sidebar")
                .and_then(|value| value.get("widgets"))
                .and_then(|value| value.get("crypto_settings"))
                .filter(|value| !value.is_null());
            let default_crypto_settings = defaults
                .get("sidebar")
                .and_then(|value| value.get("widgets"))
                .and_then(|value| value.get("crypto_settings"));
            cfg.sidebar.crypto_refresh_interval_seconds = root_crypto_settings
                .and_then(|value| {
                    number_at(
                        value,
                        "refreshInterval",
                        "sidebar.widgets.crypto_settings.refreshInterval",
                        &mut diagnostics,
                    )
                })
                .or_else(|| {
                    default_crypto_settings.and_then(|value| {
                        number_at(
                            value,
                            "refreshInterval",
                            "sidebar.widgets.crypto_settings.refreshInterval",
                            &mut diagnostics,
                        )
                    })
                })
                .map(|seconds| {
                    ((seconds.clamp(30.0, 300.0) - 30.0) / 30.0).round() as u64 * 30 + 30
                })
                .unwrap_or(60);
            cfg.sidebar.crypto_coins = root_crypto_settings
                .and_then(|value| value.get("coins"))
                .or_else(|| default_crypto_settings.and_then(|value| value.get("coins")))
                .map(|value| crypto_coin_ids(value, &mut diagnostics))
                .unwrap_or_default();
            cfg.sidebar.context_show_weather = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "contextShowWeather",
                "sidebar.widgets.contextShowWeather",
                true,
                &mut diagnostics,
            );
            cfg.sidebar.world_clock = world_clock_config(
                root.get("sidebar"),
                defaults.get("sidebar"),
                &mut diagnostics,
            );
            cfg.sidebar.widgets_wallpaper_enabled = nested_sidebar_bool(
                root.get("sidebar"),
                defaults.get("sidebar"),
                "widgets",
                "wallpaper",
                "sidebar.widgets.wallpaper",
                false,
                &mut diagnostics,
            );
            cfg.sidebar.quick_wallpaper_show_header = nested_sidebar_bool(
                root.get("sidebar").and_then(|value| value.get("widgets")),
                defaults
                    .get("sidebar")
                    .and_then(|value| value.get("widgets")),
                "quickWallpaper",
                "showHeader",
                "sidebar.widgets.quickWallpaper.showHeader",
                true,
                &mut diagnostics,
            );
            for (field, key) in [
                (
                    &mut cfg.sidebar.controls_card_show_dark_mode,
                    "showDarkMode",
                ),
                (&mut cfg.sidebar.controls_card_show_dnd, "showDnd"),
                (
                    &mut cfg.sidebar.controls_card_show_night_light,
                    "showNightLight",
                ),
                (
                    &mut cfg.sidebar.controls_card_show_game_mode,
                    "showGameMode",
                ),
                (&mut cfg.sidebar.controls_card_show_network, "showNetwork"),
                (
                    &mut cfg.sidebar.controls_card_show_bluetooth,
                    "showBluetooth",
                ),
                (&mut cfg.sidebar.controls_card_show_settings, "showSettings"),
                (&mut cfg.sidebar.controls_card_show_lock, "showLock"),
            ] {
                *field = nested_sidebar_bool(
                    root.get("sidebar").and_then(|value| value.get("widgets")),
                    defaults
                        .get("sidebar")
                        .and_then(|value| value.get("widgets")),
                    "controlsCard",
                    key,
                    &format!("sidebar.widgets.controlsCard.{key}"),
                    true,
                    &mut diagnostics,
                );
            }
            let shortcuts_source = root
                .get("sidebar")
                .and_then(|value| value.get("widgets"))
                .and_then(|value| value.get("quickLaunch"))
                .filter(|value| !value.is_null())
                .or_else(|| {
                    defaults
                        .get("sidebar")
                        .and_then(|value| value.get("widgets"))
                        .and_then(|value| value.get("quickLaunch"))
                });
            cfg.sidebar.quick_launch = quick_launch_entries(
                shortcuts_source,
                "sidebar.widgets.quickLaunch",
                &mut diagnostics,
            );
            for (field, key) in [
                (&mut cfg.sidebar.status_rings_show_cpu, "showCpu"),
                (&mut cfg.sidebar.status_rings_show_ram, "showRam"),
                (&mut cfg.sidebar.status_rings_show_battery, "showBattery"),
            ] {
                *field = root
                    .get("sidebar")
                    .and_then(|value| value.get("widgets"))
                    .and_then(|value| value.get("statusRings"))
                    .and_then(|value| {
                        bool_at(
                            value,
                            key,
                            &format!("sidebar.widgets.statusRings.{key}"),
                            &mut diagnostics,
                        )
                    })
                    .or_else(|| {
                        defaults
                            .get("sidebar")
                            .and_then(|value| value.get("widgets"))
                            .and_then(|value| value.get("statusRings"))
                            .and_then(|value| {
                                bool_at(
                                    value,
                                    key,
                                    &format!("sidebar.widgets.statusRings.{key}"),
                                    &mut diagnostics,
                                )
                            })
                    })
                    .unwrap_or(true);
            }
            let widget_order_source = root
                .get("sidebar")
                .and_then(|value| value.get("widgets"))
                .and_then(|value| value.get("widgetOrder"))
                .or_else(|| {
                    defaults
                        .get("sidebar")
                        .and_then(|value| value.get("widgets"))
                        .and_then(|value| value.get("widgetOrder"))
                });
            cfg.sidebar.widget_order = normalize_sidebar_widget_order(&string_list(
                widget_order_source,
                "sidebar.widgets.widgetOrder",
                &mut diagnostics,
            ));
            cfg.sidebar.right_header_style = sidebar.get("right").and_then(|right| {
                string_at(
                    right,
                    "headerStyle",
                    "sidebar.right.headerStyle",
                    &mut diagnostics,
                )
            });
            cfg.sidebar.right_section_order = sidebar
                .get("right")
                .map(|right| {
                    normalize_sidebar_right_section_order(&string_list(
                        right.get("sectionOrder"),
                        "sidebar.right.sectionOrder",
                        &mut diagnostics,
                    ))
                })
                .unwrap_or_else(|| normalize_sidebar_right_section_order(&[]));
            cfg.sidebar.right_controls_section_order = sidebar
                .get("right")
                .map(|right| {
                    normalize_sidebar_right_controls_section_order(&string_list(
                        right.get("controlsSectionOrder"),
                        "sidebar.right.controlsSectionOrder",
                        &mut diagnostics,
                    ))
                })
                .unwrap_or_else(|| normalize_sidebar_right_controls_section_order(&[]));
            cfg.sidebar.screen_list = string_list(
                sidebar.get("screenList"),
                "sidebar.screenList",
                &mut diagnostics,
            );
            cfg.sidebar.size_mode =
                string_at(sidebar, "sizeMode", "sidebar.sizeMode", &mut diagnostics);
            if let Some(layout) = sidebar.get("shellLayout") {
                let feature = layout.get("feature");
                let system = layout.get("system");
                cfg.sidebar.feature_slot = feature.and_then(|value| {
                    string_at(
                        value,
                        "slot",
                        "sidebar.shellLayout.feature.slot",
                        &mut diagnostics,
                    )
                });
                cfg.sidebar.system_slot = system.and_then(|value| {
                    string_at(
                        value,
                        "slot",
                        "sidebar.shellLayout.system.slot",
                        &mut diagnostics,
                    )
                });
                cfg.sidebar.feature_width = feature.and_then(|value| {
                    number_at(
                        value,
                        "width",
                        "sidebar.shellLayout.feature.width",
                        &mut diagnostics,
                    )
                });
                cfg.sidebar.system_width = system.and_then(|value| {
                    number_at(
                        value,
                        "width",
                        "sidebar.shellLayout.system.width",
                        &mut diagnostics,
                    )
                });
                cfg.sidebar.custom_height = feature.and_then(|value| {
                    number_at(
                        value,
                        "customHeight",
                        "sidebar.shellLayout.feature.customHeight",
                        &mut diagnostics,
                    )
                });
                cfg.sidebar.size_mode = feature
                    .and_then(|value| {
                        string_at(
                            value,
                            "sizeMode",
                            "sidebar.shellLayout.feature.sizeMode",
                            &mut diagnostics,
                        )
                    })
                    .or(cfg.sidebar.size_mode.clone());
            }
            cfg.sidebar.compact = string_at(sidebar, "layout", "sidebar.layout", &mut diagnostics)
                .is_some_and(|layout| layout == "compact");
        }
        let root_sidebar = root.get("sidebar");
        let defaults_sidebar = defaults.get("sidebar");
        let left_order_source = root_sidebar
            .and_then(|value| value.get("left"))
            .and_then(|value| value.get("tabOrder"))
            .or_else(|| {
                defaults_sidebar
                    .and_then(|value| value.get("left"))
                    .and_then(|value| value.get("tabOrder"))
            });
        cfg.sidebar.left_tab_order = normalize_sidebar_left_tab_order(&string_list(
            left_order_source,
            "sidebar.left.tabOrder",
            &mut diagnostics,
        ));
        let policies = root.get("policies").or_else(|| defaults.get("policies"));
        let ai_policy = policies
            .and_then(|value| number_at(value, "ai", "policies.ai", &mut diagnostics))
            .unwrap_or(0.0);
        let weeb_policy = policies
            .and_then(|value| number_at(value, "weeb", "policies.weeb", &mut diagnostics))
            .unwrap_or(0.0);
        cfg.sidebar.left_tabs = vec![
            left_tab(
                "widgets",
                nested_sidebar_bool(
                    root_sidebar,
                    defaults_sidebar,
                    "widgets",
                    "enable",
                    "sidebar.widgets.enable",
                    true,
                    &mut diagnostics,
                ),
            ),
            left_tab("ai", ai_policy != 0.0),
            left_tab(
                "translator",
                nested_sidebar_bool(
                    root_sidebar,
                    defaults_sidebar,
                    "translator",
                    "enable",
                    "sidebar.translator.enable",
                    false,
                    &mut diagnostics,
                ),
            ),
            left_tab("anime", weeb_policy != 0.0 && weeb_policy != 2.0),
            left_tab(
                "animeSchedule",
                nested_sidebar_bool(
                    root_sidebar,
                    defaults_sidebar,
                    "animeSchedule",
                    "enable",
                    "sidebar.animeSchedule.enable",
                    false,
                    &mut diagnostics,
                ),
            ),
            left_tab(
                "wallhaven",
                nested_sidebar_bool(
                    root_sidebar,
                    defaults_sidebar,
                    "wallhaven",
                    "enable",
                    "sidebar.wallhaven.enable",
                    true,
                    &mut diagnostics,
                ),
            ),
            left_tab(
                "news",
                nested_sidebar_bool(
                    root_sidebar,
                    defaults_sidebar,
                    "news",
                    "enable",
                    "sidebar.news.enable",
                    true,
                    &mut diagnostics,
                ),
            ),
            left_tab(
                "ytmusic",
                nested_sidebar_bool(
                    root_sidebar,
                    defaults_sidebar,
                    "ytmusic",
                    "enable",
                    "sidebar.ytmusic.enable",
                    false,
                    &mut diagnostics,
                ),
            ),
            left_tab(
                "tools",
                nested_sidebar_bool(
                    root_sidebar,
                    defaults_sidebar,
                    "tools",
                    "enable",
                    "sidebar.tools.enable",
                    false,
                    &mut diagnostics,
                ),
            ),
            left_tab(
                "software",
                nested_sidebar_bool(
                    root_sidebar,
                    defaults_sidebar,
                    "software",
                    "enable",
                    "sidebar.software.enable",
                    false,
                    &mut diagnostics,
                ),
            ),
        ];
        let dashboard = root.get("dashboard").or_else(|| defaults.get("dashboard"));
        if let Some(dashboard) = dashboard {
            cfg.dashboard.enabled =
                bool_at(dashboard, "enable", "dashboard.enable", &mut diagnostics).unwrap_or(true);
            cfg.dashboard.width_ratio = number_at(
                dashboard,
                "widthRatio",
                "dashboard.widthRatio",
                &mut diagnostics,
            );
            if let Some(layout) = dashboard.get("layout") {
                cfg.dashboard.layout.left = string_list(
                    layout.get("left"),
                    "dashboard.layout.left",
                    &mut diagnostics,
                );
                cfg.dashboard.layout.center = string_list(
                    layout.get("center"),
                    "dashboard.layout.center",
                    &mut diagnostics,
                );
                cfg.dashboard.layout.right = string_list(
                    layout.get("right"),
                    "dashboard.layout.right",
                    &mut diagnostics,
                );
            }
        }
        if let Some(bar) = bar {
            let default_bar = defaults.get("bar");
            cfg.bar.screen_list = match bar.get("screenList") {
                Some(value) => string_list(Some(value), "bar.screenList", &mut diagnostics),
                None => string_list(
                    default_bar.and_then(|value| value.get("screenList")),
                    "defaults.bar.screenList",
                    &mut diagnostics,
                ),
            };
            cfg.bar.appearance_style = string_at(
                bar,
                "appearanceStyle",
                "bar.appearanceStyle",
                &mut diagnostics,
            )
            .or_else(|| {
                default_bar.and_then(|value| {
                    string_at(
                        value,
                        "appearanceStyle",
                        "defaults.bar.appearanceStyle",
                        &mut diagnostics,
                    )
                })
            });
            cfg.bar.verbose = bool_at(bar, "verbose", "bar.verbose", &mut diagnostics)
                .or_else(|| {
                    default_bar.and_then(|value| {
                        bool_at(value, "verbose", "defaults.bar.verbose", &mut diagnostics)
                    })
                })
                .unwrap_or(true);
            let media = bar
                .get("media")
                .filter(|value| !value.is_null())
                .or_else(|| default_bar.and_then(|value| value.get("media")));
            cfg.bar.media_max_width = media
                .and_then(|value| {
                    number_at(value, "maxWidth", "bar.media.maxWidth", &mut diagnostics)
                })
                .or_else(|| {
                    default_bar
                        .and_then(|value| value.get("media"))
                        .and_then(|value| {
                            number_at(
                                value,
                                "maxWidth",
                                "defaults.bar.media.maxWidth",
                                &mut diagnostics,
                            )
                        })
                })
                .unwrap_or(220.0)
                .max(120.0);
            let popup_mode = media
                .and_then(|value| {
                    string_at(value, "popupMode", "bar.media.popupMode", &mut diagnostics)
                })
                .or_else(|| {
                    default_bar
                        .and_then(|value| value.get("media"))
                        .and_then(|value| {
                            string_at(
                                value,
                                "popupMode",
                                "defaults.bar.media.popupMode",
                                &mut diagnostics,
                            )
                        })
                });
            cfg.bar.media_popup_mode = match popup_mode.as_deref() {
                None | Some("dock") => MediaPopupMode::Dock,
                Some("bar") => MediaPopupMode::Bar,
                Some(_) => {
                    diagnostics.push("bar.media.popupMode must be one of: dock, bar".into());
                    MediaPopupMode::Dock
                }
            };
            let weather = bar
                .get("weather")
                .or_else(|| defaults.get("bar").and_then(|value| value.get("weather")));
            if let Some(weather) = weather {
                cfg.bar.weather.enabled =
                    bool_at(weather, "enable", "bar.weather.enable", &mut diagnostics)
                        .unwrap_or(true);
                cfg.bar.weather.city =
                    string_at(weather, "city", "bar.weather.city", &mut diagnostics);
                cfg.bar.weather.manual_lat = number_at(
                    weather,
                    "manualLat",
                    "bar.weather.manualLat",
                    &mut diagnostics,
                );
                cfg.bar.weather.manual_lon = number_at(
                    weather,
                    "manualLon",
                    "bar.weather.manualLon",
                    &mut diagnostics,
                );
                cfg.bar.weather.use_uscs =
                    bool_at(weather, "useUSCS", "bar.weather.useUSCS", &mut diagnostics)
                        .unwrap_or(false);
                cfg.bar.weather.fetch_interval_minutes = number_at(
                    weather,
                    "fetchInterval",
                    "bar.weather.fetchInterval",
                    &mut diagnostics,
                )
                .filter(|value| value.is_finite())
                .map(|value| value.round() as u64)
                .unwrap_or(10)
                .clamp(5, 60);
            }
            cfg.bar.bottom = bool_at(bar, "bottom", "bar.bottom", &mut diagnostics);
            cfg.bar.vertical = bool_at(bar, "vertical", "bar.vertical", &mut diagnostics);
            cfg.bar.height = number_at(bar, "height", "bar.height", &mut diagnostics);
            cfg.bar.show_background = bool_at(
                bar,
                "showBackground",
                "bar.showBackground",
                &mut diagnostics,
            );
            cfg.bar.borderless = bool_at(bar, "borderless", "bar.borderless", &mut diagnostics);
            cfg.bar.modules = bool_map(bar.get("modules"), "bar.modules", &mut diagnostics);
            let mut merged_resources = defaults
                .get("bar")
                .and_then(|defaults_bar| defaults_bar.get("resources"))
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            if let Some(runtime_resources) = bar.get("resources").and_then(Value::as_object) {
                merged_resources.extend(runtime_resources.clone());
            }
            {
                let resources = Value::Object(merged_resources);
                cfg.bar.resources = ResourceConfig {
                    show_memory_indicator: bool_at(
                        &resources,
                        "showMemoryIndicator",
                        "bar.resources.showMemoryIndicator",
                        &mut diagnostics,
                    ),
                    show_temp_indicator: bool_at(
                        &resources,
                        "showTempIndicator",
                        "bar.resources.showTempIndicator",
                        &mut diagnostics,
                    ),
                    show_swap_indicator: bool_at(
                        &resources,
                        "showSwapIndicator",
                        "bar.resources.showSwapIndicator",
                        &mut diagnostics,
                    ),
                    show_cpu_indicator: bool_at(
                        &resources,
                        "showCpuIndicator",
                        "bar.resources.showCpuIndicator",
                        &mut diagnostics,
                    ),
                    show_gpu_indicator: bool_at(
                        &resources,
                        "showGpuIndicator",
                        "bar.resources.showGpuIndicator",
                        &mut diagnostics,
                    ),
                    memory_warning_threshold: number_at(
                        &resources,
                        "memoryWarningThreshold",
                        "bar.resources.memoryWarningThreshold",
                        &mut diagnostics,
                    ),
                    temp_caution_threshold: number_at(
                        &resources,
                        "tempCautionThreshold",
                        "bar.resources.tempCautionThreshold",
                        &mut diagnostics,
                    ),
                    temp_warning_threshold: number_at(
                        &resources,
                        "tempWarningThreshold",
                        "bar.resources.tempWarningThreshold",
                        &mut diagnostics,
                    ),
                    swap_warning_threshold: number_at(
                        &resources,
                        "swapWarningThreshold",
                        "bar.resources.swapWarningThreshold",
                        &mut diagnostics,
                    ),
                    cpu_warning_threshold: number_at(
                        &resources,
                        "cpuWarningThreshold",
                        "bar.resources.cpuWarningThreshold",
                        &mut diagnostics,
                    ),
                    gpu_warning_threshold: number_at(
                        &resources,
                        "gpuWarningThreshold",
                        "bar.resources.gpuWarningThreshold",
                        &mut diagnostics,
                    ),
                };
            }
            let runtime_layout = bar.get("layout");
            let default_layout = defaults
                .get("bar")
                .and_then(|defaults_bar| defaults_bar.get("layout"));
            if runtime_layout.is_some_and(|layout| !layout.is_object()) {
                diagnostics.push("bar.layout must be an object".into());
            }
            let spacer_mode = runtime_layout
                .and_then(|layout| {
                    string_at(
                        layout,
                        "spacerMode",
                        "bar.layout.spacerMode",
                        &mut diagnostics,
                    )
                })
                .or_else(|| {
                    default_layout.and_then(|layout| {
                        string_at(
                            layout,
                            "spacerMode",
                            "defaults.bar.layout.spacerMode",
                            &mut diagnostics,
                        )
                    })
                })
                .unwrap_or_else(|| "auto".into());
            cfg.bar.layout.spacer_mode = match spacer_mode.as_str() {
                "auto" => SpacerMode::Auto,
                "fill" => SpacerMode::Fill,
                "fixed" => SpacerMode::Fixed,
                _ => {
                    diagnostics.push(
                        "bar.layout.spacerMode must be one of auto, fill, fixed; using auto".into(),
                    );
                    SpacerMode::Auto
                }
            };
            cfg.bar.layout.spacer_width = runtime_layout
                .and_then(|layout| {
                    number_at(
                        layout,
                        "spacerWidth",
                        "bar.layout.spacerWidth",
                        &mut diagnostics,
                    )
                })
                .or_else(|| {
                    default_layout.and_then(|layout| {
                        number_at(
                            layout,
                            "spacerWidth",
                            "defaults.bar.layout.spacerWidth",
                            &mut diagnostics,
                        )
                    })
                })
                .unwrap_or(0.0)
                .clamp(0.0, 480.0);
            if let Some(layout) = runtime_layout.and_then(Value::as_object) {
                cfg.bar.layout.left =
                    string_list(layout.get("left"), "bar.layout.left", &mut diagnostics);
                cfg.bar.layout.center_left = string_list(
                    layout.get("centerLeft"),
                    "bar.layout.centerLeft",
                    &mut diagnostics,
                );
                cfg.bar.layout.center =
                    string_list(layout.get("center"), "bar.layout.center", &mut diagnostics);
                cfg.bar.layout.center_right = string_list(
                    layout.get("centerRight"),
                    "bar.layout.centerRight",
                    &mut diagnostics,
                );
                cfg.bar.layout.right =
                    string_list(layout.get("right"), "bar.layout.right", &mut diagnostics);
            }
            if let Some(workspace) = bar.get("workspaces") {
                let w = &mut cfg.bar.workspace;
                w.always_show_numbers = bool_at(
                    workspace,
                    "alwaysShowNumbers",
                    "bar.workspaces.alwaysShowNumbers",
                    &mut diagnostics,
                );
                w.monochrome_icons = bool_at(
                    workspace,
                    "monochromeIcons",
                    "bar.workspaces.monochromeIcons",
                    &mut diagnostics,
                );
                w.per_monitor = bool_at(
                    workspace,
                    "perMonitor",
                    "bar.workspaces.perMonitor",
                    &mut diagnostics,
                );
                w.dynamic_count = bool_at(
                    workspace,
                    "dynamicCount",
                    "bar.workspaces.dynamicCount",
                    &mut diagnostics,
                );
                w.scroll_behavior = string_at(
                    workspace,
                    "scrollBehavior",
                    "bar.workspaces.scrollBehavior",
                    &mut diagnostics,
                );
                w.invert_scroll = bool_at(
                    workspace,
                    "invertScroll",
                    "bar.workspaces.invertScroll",
                    &mut diagnostics,
                );
                w.show_app_icons = bool_at(
                    workspace,
                    "showAppIcons",
                    "bar.workspaces.showAppIcons",
                    &mut diagnostics,
                );
                w.shown = u64_at(workspace, "shown", "bar.workspaces.shown", &mut diagnostics);
                w.use_nerd_font = bool_at(
                    workspace,
                    "useNerdFont",
                    "bar.workspaces.useNerdFont",
                    &mut diagnostics,
                );
            }
        }
        cfg.clipboard_pinned = string_list(
            root.get("clipboard")
                .and_then(|v| v.get("pinned"))
                .or_else(|| defaults.get("clipboard").and_then(|v| v.get("pinned"))),
            "clipboard.pinned",
            &mut diagnostics,
        );
        // Waffle keeps its bar settings under `waffles.bar`, separately from
        // the ii `bar` section. Resolve each setting against bundled defaults
        // so sparse user configs retain the source defaults.
        let waffle_bar = root.get("waffles").and_then(|value| value.get("bar"));
        let default_waffle_bar = defaults.get("waffles").and_then(|value| value.get("bar"));
        cfg.waffle.bar.bottom = waffle_bar
            .and_then(|value| bool_at(value, "bottom", "waffles.bar.bottom", &mut diagnostics))
            .or_else(|| {
                default_waffle_bar.and_then(|value| {
                    bool_at(
                        value,
                        "bottom",
                        "defaults.waffles.bar.bottom",
                        &mut diagnostics,
                    )
                })
            });
        cfg.waffle.bar.screen_list = match waffle_bar.and_then(|value| value.get("screenList")) {
            Some(value) => string_list(Some(value), "waffles.bar.screenList", &mut diagnostics),
            None => string_list(
                default_waffle_bar.and_then(|value| value.get("screenList")),
                "defaults.waffles.bar.screenList",
                &mut diagnostics,
            ),
        };
        cfg.waffle.bar.icon_size = number_at(
            waffle_bar.unwrap_or(&Value::Null),
            "iconSize",
            "waffles.bar.iconSize",
            &mut diagnostics,
        )
        .or_else(|| {
            number_at(
                default_waffle_bar.unwrap_or(&Value::Null),
                "iconSize",
                "defaults.waffles.bar.iconSize",
                &mut diagnostics,
            )
        });
        let waffle_background = root
            .get("waffles")
            .and_then(|value| value.get("background"));
        let default_waffle_background = defaults
            .get("waffles")
            .and_then(|value| value.get("background"));
        cfg.waffle.background.use_main_wallpaper = waffle_background
            .and_then(|value| {
                bool_at(
                    value,
                    "useMainWallpaper",
                    "waffles.background.useMainWallpaper",
                    &mut diagnostics,
                )
            })
            .or_else(|| {
                default_waffle_background.and_then(|value| {
                    bool_at(
                        value,
                        "useMainWallpaper",
                        "defaults.waffles.background.useMainWallpaper",
                        &mut diagnostics,
                    )
                })
            });
        cfg.waffle.background.wallpaper_path = waffle_background
            .and_then(|value| {
                string_at(
                    value,
                    "wallpaperPath",
                    "waffles.background.wallpaperPath",
                    &mut diagnostics,
                )
            })
            .or_else(|| {
                default_waffle_background.and_then(|value| {
                    string_at(
                        value,
                        "wallpaperPath",
                        "defaults.waffles.background.wallpaperPath",
                        &mut diagnostics,
                    )
                })
            });
        cfg.waffle.background.hide_when_fullscreen = waffle_background
            .and_then(|value| {
                bool_at(
                    value,
                    "hideWhenFullscreen",
                    "waffles.background.hideWhenFullscreen",
                    &mut diagnostics,
                )
            })
            .or_else(|| {
                default_waffle_background.and_then(|value| {
                    bool_at(
                        value,
                        "hideWhenFullscreen",
                        "defaults.waffles.background.hideWhenFullscreen",
                        &mut diagnostics,
                    )
                })
            });
        let waffle_backdrop = waffle_background.and_then(|value| value.get("backdrop"));
        let default_waffle_backdrop =
            default_waffle_background.and_then(|value| value.get("backdrop"));
        macro_rules! waffle_backdrop_bool {
            ($field:ident, $key:literal) => {
                cfg.waffle.background.backdrop.$field = waffle_backdrop
                    .and_then(|value| {
                        bool_at(
                            value,
                            $key,
                            concat!("waffles.background.backdrop.", $key),
                            &mut diagnostics,
                        )
                    })
                    .or_else(|| {
                        default_waffle_backdrop.and_then(|value| {
                            bool_at(
                                value,
                                $key,
                                concat!("defaults.waffles.background.backdrop.", $key),
                                &mut diagnostics,
                            )
                        })
                    });
            };
        }
        macro_rules! waffle_backdrop_number {
            ($field:ident, $key:literal) => {
                cfg.waffle.background.backdrop.$field = waffle_backdrop
                    .and_then(|value| {
                        number_at(
                            value,
                            $key,
                            concat!("waffles.background.backdrop.", $key),
                            &mut diagnostics,
                        )
                    })
                    .or_else(|| {
                        default_waffle_backdrop.and_then(|value| {
                            number_at(
                                value,
                                $key,
                                concat!("defaults.waffles.background.backdrop.", $key),
                                &mut diagnostics,
                            )
                        })
                    });
            };
        }
        macro_rules! waffle_backdrop_string {
            ($field:ident, $key:literal) => {
                cfg.waffle.background.backdrop.$field = waffle_backdrop
                    .and_then(|value| {
                        string_at(
                            value,
                            $key,
                            concat!("waffles.background.backdrop.", $key),
                            &mut diagnostics,
                        )
                    })
                    .or_else(|| {
                        default_waffle_backdrop.and_then(|value| {
                            string_at(
                                value,
                                $key,
                                concat!("defaults.waffles.background.backdrop.", $key),
                                &mut diagnostics,
                            )
                        })
                    });
            };
        }
        waffle_backdrop_bool!(enabled, "enable");
        waffle_backdrop_bool!(hide_wallpaper, "hideWallpaper");
        waffle_backdrop_bool!(use_main_wallpaper, "useMainWallpaper");
        waffle_backdrop_bool!(vignette_enabled, "vignetteEnabled");
        waffle_backdrop_number!(blur_radius, "blurRadius");
        waffle_backdrop_number!(dim, "dim");
        waffle_backdrop_number!(saturation, "saturation");
        waffle_backdrop_number!(contrast, "contrast");
        waffle_backdrop_number!(vignette_intensity, "vignetteIntensity");
        waffle_backdrop_number!(vignette_radius, "vignetteRadius");
        waffle_backdrop_string!(wallpaper_path, "wallpaperPath");
        let waffle_task_view = root.get("waffles").and_then(|value| value.get("taskView"));
        let default_waffle_task_view = defaults
            .get("waffles")
            .and_then(|value| value.get("taskView"));
        cfg.waffle.task_view.mode = waffle_task_view
            .and_then(|value| string_at(value, "mode", "waffles.taskView.mode", &mut diagnostics))
            .or_else(|| {
                default_waffle_task_view.and_then(|value| {
                    string_at(
                        value,
                        "mode",
                        "defaults.waffles.taskView.mode",
                        &mut diagnostics,
                    )
                })
            });
        cfg.waffle.task_view.close_on_select = waffle_task_view
            .and_then(|value| {
                bool_at(
                    value,
                    "closeOnSelect",
                    "waffles.taskView.closeOnSelect",
                    &mut diagnostics,
                )
            })
            .or_else(|| {
                default_waffle_task_view.and_then(|value| {
                    bool_at(
                        value,
                        "closeOnSelect",
                        "defaults.waffles.taskView.closeOnSelect",
                        &mut diagnostics,
                    )
                })
            });
        if let Some(appearance) = appearance {
            cfg.appearance.global_style = string_at(
                appearance,
                "globalStyle",
                "appearance.globalStyle",
                &mut diagnostics,
            );
            cfg.appearance.theme =
                string_at(appearance, "theme", "appearance.theme", &mut diagnostics);
            // `globalStyle` is the II/Waffle visual style. Iris has a separate
            // preset under iris.appearance.preset (themeId is a distinct ID).
            cfg.appearance.style_preset = cfg.appearance.global_style.clone();
            if let Some(transparency) = appearance.get("transparency") {
                cfg.appearance.transparency_enabled = bool_at(
                    transparency,
                    "enable",
                    "appearance.transparency.enable",
                    &mut diagnostics,
                );
                cfg.appearance.transparency_automatic = bool_at(
                    transparency,
                    "automatic",
                    "appearance.transparency.automatic",
                    &mut diagnostics,
                );
                cfg.appearance.background_transparency = number_at(
                    transparency,
                    "backgroundTransparency",
                    "appearance.transparency.backgroundTransparency",
                    &mut diagnostics,
                );
                cfg.appearance.content_transparency = number_at(
                    transparency,
                    "contentTransparency",
                    "appearance.transparency.contentTransparency",
                    &mut diagnostics,
                );
            }
            for key in ["palette", "aurora", "angel", "typography", "island", "iris"] {
                if let Some(value) = appearance.get(key) {
                    cfg.appearance
                        .source_sections
                        .insert(key.to_owned(), value.clone());
                }
            }
        }
        if let Some(background) = background {
            cfg.background.wallpaper_path = string_field(
                runtime_background,
                default_background,
                "wallpaperPath",
                "background.wallpaperPath",
                &mut diagnostics,
            );
            cfg.background.thumbnail_path = string_field(
                runtime_background,
                default_background,
                "thumbnailPath",
                "background.thumbnailPath",
                &mut diagnostics,
            );
            cfg.background.fill_mode = string_field(
                runtime_background,
                default_background,
                "fillMode",
                "background.fillMode",
                &mut diagnostics,
            )
            .or_else(|| Some("fill".into()));
            cfg.background.use_main_wallpaper = bool_field(
                runtime_background,
                default_background,
                "useMainWallpaper",
                "background.useMainWallpaper",
                &mut diagnostics,
            );
            cfg.background.enable_animation = bool_field(
                runtime_background,
                default_background,
                "enableAnimation",
                "background.enableAnimation",
                &mut diagnostics,
            );
            cfg.background.pause_animation_on_battery = bool_field(
                runtime_background,
                default_background,
                "pauseAnimationOnBattery",
                "background.pauseAnimationOnBattery",
                &mut diagnostics,
            );
            cfg.background.video_pause = string_field(
                runtime_background,
                default_background,
                "videoPause",
                "background.videoPause",
                &mut diagnostics,
            );
            cfg.background.hide_when_fullscreen = bool_field(
                runtime_background,
                default_background,
                "hideWhenFullscreen",
                "background.hideWhenFullscreen",
                &mut diagnostics,
            );
            cfg.background.transition = parse_background_transition(
                runtime_background,
                default_background,
                &mut diagnostics,
            );
            cfg.background.effects =
                parse_background_effects(runtime_background, default_background, &mut diagnostics);
            cfg.background.parallax =
                parse_background_parallax(runtime_background, default_background, &mut diagnostics);
            let runtime_backdrop = runtime_background.and_then(|value| value.get("backdrop"));
            let default_backdrop = default_background.and_then(|value| value.get("backdrop"));
            let backdrop = merged_object(runtime_backdrop, default_backdrop);
            if backdrop.is_some() {
                cfg.background.backdrop = BackdropConfig {
                    enabled: bool_field(
                        runtime_backdrop,
                        default_backdrop,
                        "enable",
                        "background.backdrop.enable",
                        &mut diagnostics,
                    ),
                    hide_wallpaper: bool_field(
                        runtime_backdrop,
                        default_backdrop,
                        "hideWallpaper",
                        "background.backdrop.hideWallpaper",
                        &mut diagnostics,
                    ),
                    use_main_wallpaper: bool_field(
                        runtime_backdrop,
                        default_backdrop,
                        "useMainWallpaper",
                        "background.backdrop.useMainWallpaper",
                        &mut diagnostics,
                    ),
                    wallpaper_path: string_field(
                        runtime_backdrop,
                        default_backdrop,
                        "wallpaperPath",
                        "background.backdrop.wallpaperPath",
                        &mut diagnostics,
                    ),
                    thumbnail_path: string_field(
                        runtime_backdrop,
                        default_backdrop,
                        "thumbnailPath",
                        "background.backdrop.thumbnailPath",
                        &mut diagnostics,
                    ),
                    fill_mode: string_field(
                        runtime_backdrop,
                        default_backdrop,
                        "fillMode",
                        "background.backdrop.fillMode",
                        &mut diagnostics,
                    )
                    .or_else(|| Some("fill".into())),
                    blur_radius: number_field(
                        runtime_backdrop,
                        default_backdrop,
                        "blurRadius",
                        "background.backdrop.blurRadius",
                        &mut diagnostics,
                    ),
                    dim: number_field(
                        runtime_backdrop,
                        default_backdrop,
                        "dim",
                        "background.backdrop.dim",
                        &mut diagnostics,
                    ),
                    saturation: number_field(
                        runtime_backdrop,
                        default_backdrop,
                        "saturation",
                        "background.backdrop.saturation",
                        &mut diagnostics,
                    ),
                    contrast: number_field(
                        runtime_backdrop,
                        default_backdrop,
                        "contrast",
                        "background.backdrop.contrast",
                        &mut diagnostics,
                    ),
                    vignette_enabled: bool_field(
                        runtime_backdrop,
                        default_backdrop,
                        "vignetteEnabled",
                        "background.backdrop.vignetteEnabled",
                        &mut diagnostics,
                    ),
                    vignette_intensity: number_field(
                        runtime_backdrop,
                        default_backdrop,
                        "vignetteIntensity",
                        "background.backdrop.vignetteIntensity",
                        &mut diagnostics,
                    ),
                    vignette_radius: number_field(
                        runtime_backdrop,
                        default_backdrop,
                        "vignetteRadius",
                        "background.backdrop.vignetteRadius",
                        &mut diagnostics,
                    ),
                    enable_animation: bool_field(
                        runtime_backdrop,
                        default_backdrop,
                        "enableAnimation",
                        "background.backdrop.enableAnimation",
                        &mut diagnostics,
                    ),
                    enable_animated_blur: bool_field(
                        runtime_backdrop,
                        default_backdrop,
                        "enableAnimatedBlur",
                        "background.backdrop.enableAnimatedBlur",
                        &mut diagnostics,
                    ),
                    use_aurora_style: bool_field(
                        runtime_backdrop,
                        default_backdrop,
                        "useAuroraStyle",
                        "background.backdrop.useAuroraStyle",
                        &mut diagnostics,
                    ),
                    aurora_overlay_opacity: number_field(
                        runtime_backdrop,
                        default_backdrop,
                        "auroraOverlayOpacity",
                        "background.backdrop.auroraOverlayOpacity",
                        &mut diagnostics,
                    ),
                };
            }
            if let Some(multi_monitor) = background.get("multiMonitor") {
                cfg.background.multi_monitor_enabled = bool_at(
                    multi_monitor,
                    "enable",
                    "background.multiMonitor.enable",
                    &mut diagnostics,
                );
            }
            if let Some(entries) = background.get("wallpapersByMonitor") {
                if let Some(entries) = entries.as_array() {
                    cfg.background.wallpapers_by_monitor = entries
                        .iter()
                        .filter_map(|entry| {
                            let monitor = string_at(
                                entry,
                                "monitor",
                                "background.wallpapersByMonitor.monitor",
                                &mut diagnostics,
                            );
                            let path = string_at(
                                entry,
                                "path",
                                "background.wallpapersByMonitor.path",
                                &mut diagnostics,
                            );
                            let backdrop_path = string_at(
                                entry,
                                "backdropPath",
                                "background.wallpapersByMonitor.backdropPath",
                                &mut diagnostics,
                            );
                            Some(WallpaperOutputConfig {
                                monitor,
                                path,
                                backdrop_path,
                            })
                        })
                        .collect();
                } else {
                    diagnostics.push("background.wallpapersByMonitor must be an array".into());
                }
            }
        }
        if let Some(island_appearance) = root.get("iris").and_then(|v| v.get("appearance")) {
            cfg.appearance
                .source_sections
                .insert("iris.appearance".into(), island_appearance.clone());
            cfg.appearance.island_preset = string_at(
                island_appearance,
                "preset",
                "iris.appearance.preset",
                &mut diagnostics,
            );
            if let Some(theme_id) = string_at(
                island_appearance,
                "themeId",
                "iris.appearance.themeId",
                &mut diagnostics,
            ) {
                cfg.appearance
                    .source_sections
                    .insert("iris.themeId".into(), Value::String(theme_id));
            }
        }
        if let Some(iris_bar) = root.get("iris").and_then(|v| v.get("bar")) {
            cfg.appearance
                .source_sections
                .insert("iris.bar".into(), iris_bar.clone());
        } else if let Some(iris_bar) = defaults.get("iris").and_then(|v| v.get("bar")) {
            cfg.appearance
                .source_sections
                .insert("iris.bar".into(), iris_bar.clone());
        }
        if let Some(iris_dock) = root.get("iris").and_then(|v| v.get("dock")) {
            cfg.appearance
                .source_sections
                .insert("iris.dock".into(), iris_dock.clone());
        } else if let Some(iris_dock) = defaults.get("iris").and_then(|v| v.get("dock")) {
            cfg.appearance
                .source_sections
                .insert("iris.dock".into(), iris_dock.clone());
        }
        let mut ii_dock = defaults
            .get("dock")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(runtime_dock) = root.get("dock").and_then(Value::as_object) {
            ii_dock.extend(runtime_dock.clone());
        }
        cfg.appearance
            .source_sections
            .insert("ii.dock".into(), Value::Object(ii_dock));
        Self {
            config: cfg,
            source,
            diagnostics,
        }
    }
}

/// Resolve the same canonical/legacy config path chosen by scripts/lib/config-path.sh.
pub fn runtime_path() -> std::path::PathBuf {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".config"));
    let new_dir = config_home.join("inir");
    let old_dir = config_home.join("illogical-impulse");
    let old_is_symlink = fs::symlink_metadata(&old_dir)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false);

    let selected = if old_is_symlink && new_dir.is_dir() {
        if !new_dir.join("config.json").is_file() && old_dir.join("config.json").is_file() {
            old_dir
        } else {
            new_dir
        }
    } else if old_dir.is_dir() {
        old_dir
    } else if new_dir.is_dir() {
        new_dir
    } else {
        new_dir
    };
    selected.join("config.json")
}

fn section<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.get(key).filter(|v| v.is_object())
}

fn merged_object(runtime: Option<&Value>, defaults: Option<&Value>) -> Option<Value> {
    let mut merged = defaults?.as_object()?.clone();
    if let Some(runtime) = runtime.and_then(Value::as_object) {
        merged.extend(runtime.clone());
    }
    Some(Value::Object(merged))
}

fn selected_field<'a>(
    runtime: Option<&'a Value>,
    defaults: Option<&'a Value>,
    key: &str,
) -> Option<&'a Value> {
    runtime
        .and_then(|value| value.get(key))
        .or_else(|| defaults.and_then(|value| value.get(key)))
}

fn bool_field(
    runtime: Option<&Value>,
    defaults: Option<&Value>,
    key: &str,
    path: &str,
    diagnostics: &mut Vec<String>,
) -> Option<bool> {
    let runtime_value = runtime.and_then(|value| value.get(key));
    if let Some(value) = runtime_value {
        match value.as_bool() {
            Some(value) => return Some(value),
            None => diagnostics.push(format!("{path} must be a boolean")),
        }
    }
    let default_value = defaults.and_then(|value| value.get(key));
    default_value.and_then(|value| match value.as_bool() {
        Some(value) => Some(value),
        None => {
            diagnostics.push(format!("defaults.{path} must be a boolean"));
            None
        }
    })
}

fn string_field(
    runtime: Option<&Value>,
    defaults: Option<&Value>,
    key: &str,
    path: &str,
    diagnostics: &mut Vec<String>,
) -> Option<String> {
    let runtime_value = runtime.and_then(|value| value.get(key));
    if let Some(value) = runtime_value {
        match value.as_str() {
            Some(value) => return Some(value.to_owned()),
            None => diagnostics.push(format!("{path} must be a string")),
        }
    }
    defaults
        .and_then(|value| value.get(key))
        .and_then(|value| match value.as_str() {
            Some(value) => Some(value.to_owned()),
            None => {
                diagnostics.push(format!("defaults.{path} must be a string"));
                None
            }
        })
}

fn number_field(
    runtime: Option<&Value>,
    defaults: Option<&Value>,
    key: &str,
    path: &str,
    diagnostics: &mut Vec<String>,
) -> Option<f64> {
    let parse = |value: &Value| value.as_f64().filter(|number| number.is_finite());
    if let Some(value) = runtime.and_then(|value| value.get(key)) {
        if let Some(number) = parse(value) {
            return Some(number);
        }
        diagnostics.push(format!("{path} must be a finite number"));
    }
    defaults
        .and_then(|value| value.get(key))
        .and_then(|value| match parse(value) {
            Some(number) => Some(number),
            None => {
                diagnostics.push(format!("defaults.{path} must be a finite number"));
                None
            }
        })
}

fn u64_field(
    runtime: Option<&Value>,
    defaults: Option<&Value>,
    key: &str,
    path: &str,
    diagnostics: &mut Vec<String>,
) -> Option<u64> {
    if let Some(value) = runtime.and_then(|value| value.get(key)) {
        if let Some(number) = value.as_u64() {
            return Some(number);
        }
        diagnostics.push(format!("{path} must be a nonnegative integer"));
    }
    defaults
        .and_then(|value| value.get(key))
        .and_then(|value| match value.as_u64() {
            Some(number) => Some(number),
            None => {
                diagnostics.push(format!("defaults.{path} must be a nonnegative integer"));
                None
            }
        })
}

fn iris_background_config(
    root: &Value,
    defaults: &Value,
    source: ConfigSource,
    diagnostics: &mut Vec<String>,
) -> IrisBackgroundConfig {
    let runtime_iris = root.get("iris");
    let default_iris = defaults.get("iris");
    let runtime_modules = runtime_iris.and_then(|value| value.get("modules"));
    let default_modules = default_iris.and_then(|value| value.get("modules"));
    let runtime_menu = runtime_iris.and_then(|value| value.get("desktopMenu"));
    let default_menu = default_iris.and_then(|value| value.get("desktopMenu"));
    let desktop_widgets_enabled = bool_field(
        runtime_modules,
        default_modules,
        "desktopWidgets",
        "iris.modules.desktopWidgets",
        diagnostics,
    )
    .unwrap_or(true);
    let desktop_menu_wallpaper = bool_field(
        runtime_menu,
        default_menu,
        "wallpaper",
        "iris.desktopMenu.wallpaper",
        diagnostics,
    )
    .unwrap_or(true);
    let menu_items_value = selected_field(runtime_menu, default_menu, "items");
    let desktop_menu_items = match menu_items_value {
        Some(value) if value.is_array() => {
            string_list(Some(value), "iris.desktopMenu.items", diagnostics)
        }
        Some(_) => {
            diagnostics.push("iris.desktopMenu.items must be an array".into());
            default_menu
                .and_then(|menu| menu.get("items"))
                .map(|items| {
                    string_list(Some(items), "defaults.iris.desktopMenu.items", diagnostics)
                })
                .unwrap_or_default()
        }
        None => Vec::new(),
    };
    let mut raw_runtime_sections = BTreeMap::new();
    let mut raw_default_sections = BTreeMap::new();
    for (key, runtime, default) in [
        (
            "background",
            root.get("background"),
            defaults.get("background"),
        ),
        ("iris.modules", runtime_modules, default_modules),
        ("iris.desktopMenu", runtime_menu, default_menu),
    ] {
        if source == ConfigSource::Runtime {
            if let Some(value) = runtime {
                raw_runtime_sections.insert(key.to_owned(), value.clone());
            }
        }
        if let Some(value) = default {
            raw_default_sections.insert(key.to_owned(), value.clone());
        }
    }
    IrisBackgroundConfig {
        source,
        desktop_widgets_enabled,
        desktop_menu_wallpaper,
        desktop_menu_items,
        raw_runtime_sections,
        raw_default_sections,
    }
}

fn parse_background_transition(
    runtime: Option<&Value>,
    defaults: Option<&Value>,
    diagnostics: &mut Vec<String>,
) -> BackgroundTransitionConfig {
    let rt = runtime.and_then(|value| value.get("transition"));
    let df = defaults.and_then(|value| value.get("transition"));
    let bezier_value = selected_field(rt, df, "bezier");
    let bezier = match bezier_value.and_then(Value::as_array) {
        Some(items) => items
            .iter()
            .enumerate()
            .filter_map(
                |(index, value)| match value.as_f64().filter(|n| n.is_finite()) {
                    Some(n) => Some(n),
                    None => {
                        diagnostics.push(format!(
                            "background.transition.bezier[{index}] must be a finite number"
                        ));
                        None
                    }
                },
            )
            .collect(),
        None => Vec::new(),
    };
    BackgroundTransitionConfig {
        enabled: bool_field(
            rt,
            df,
            "enable",
            "background.transition.enable",
            diagnostics,
        ),
        kind: string_field(rt, df, "type", "background.transition.type", diagnostics),
        direction: string_field(
            rt,
            df,
            "direction",
            "background.transition.direction",
            diagnostics,
        ),
        duration_ms: u64_field(
            rt,
            df,
            "duration",
            "background.transition.duration",
            diagnostics,
        ),
        bezier,
    }
}

fn parse_background_effects(
    runtime: Option<&Value>,
    defaults: Option<&Value>,
    diagnostics: &mut Vec<String>,
) -> BackgroundEffectsConfig {
    let rt = runtime.and_then(|value| value.get("effects"));
    let df = defaults.and_then(|value| value.get("effects"));
    BackgroundEffectsConfig {
        blur_enabled: bool_field(
            rt,
            df,
            "enableBlur",
            "background.effects.enableBlur",
            diagnostics,
        ),
        blur_radius: number_field(
            rt,
            df,
            "blurRadius",
            "background.effects.blurRadius",
            diagnostics,
        ),
        thumbnail_blur_strength: number_field(
            rt,
            df,
            "thumbnailBlurStrength",
            "background.effects.thumbnailBlurStrength",
            diagnostics,
        ),
        animated_blur_enabled: bool_field(
            rt,
            df,
            "enableAnimatedBlur",
            "background.effects.enableAnimatedBlur",
            diagnostics,
        ),
        dim: number_field(rt, df, "dim", "background.effects.dim", diagnostics),
        dynamic_dim: number_field(
            rt,
            df,
            "dynamicDim",
            "background.effects.dynamicDim",
            diagnostics,
        ),
    }
}

fn parse_background_parallax(
    runtime: Option<&Value>,
    defaults: Option<&Value>,
    diagnostics: &mut Vec<String>,
) -> BackgroundParallaxConfig {
    let rt = runtime.and_then(|value| value.get("parallax"));
    let df = defaults.and_then(|value| value.get("parallax"));
    BackgroundParallaxConfig {
        enabled: bool_field(rt, df, "enable", "background.parallax.enable", diagnostics),
        axis: string_field(rt, df, "axis", "background.parallax.axis", diagnostics),
        vertical: bool_field(
            rt,
            df,
            "vertical",
            "background.parallax.vertical",
            diagnostics,
        ),
        auto_vertical: bool_field(
            rt,
            df,
            "autoVertical",
            "background.parallax.autoVertical",
            diagnostics,
        ),
        workspace_enabled: bool_field(
            rt,
            df,
            "enableWorkspace",
            "background.parallax.enableWorkspace",
            diagnostics,
        ),
        workspace_shift: number_field(
            rt,
            df,
            "workspaceShift",
            "background.parallax.workspaceShift",
            diagnostics,
        ),
        workspace_zoom: number_field(
            rt,
            df,
            "workspaceZoom",
            "background.parallax.workspaceZoom",
            diagnostics,
        ),
        zoom: number_field(rt, df, "zoom", "background.parallax.zoom", diagnostics),
        sidebar_enabled: bool_field(
            rt,
            df,
            "enableSidebar",
            "background.parallax.enableSidebar",
            diagnostics,
        ),
        panel_shift: number_field(
            rt,
            df,
            "panelShift",
            "background.parallax.panelShift",
            diagnostics,
        ),
        widgets_factor: number_field(
            rt,
            df,
            "widgetsFactor",
            "background.parallax.widgetsFactor",
            diagnostics,
        ),
        widget_depth: number_field(
            rt,
            df,
            "widgetDepth",
            "background.parallax.widgetDepth",
            diagnostics,
        ),
        pause_during_transitions: bool_field(
            rt,
            df,
            "pauseDuringTransitions",
            "background.parallax.pauseDuringTransitions",
            diagnostics,
        ),
        transition_settle_ms: u64_field(
            rt,
            df,
            "transitionSettleMs",
            "background.parallax.transitionSettleMs",
            diagnostics,
        ),
    }
}

fn bool_with_default(
    runtime: Option<&Value>,
    defaults: Option<&Value>,
    key: &str,
    path: &str,
    fallback: bool,
    diagnostics: &mut Vec<String>,
) -> bool {
    runtime
        .and_then(|value| bool_at(value, key, path, diagnostics))
        .or_else(|| {
            defaults.and_then(|value| bool_at(value, key, &format!("defaults.{path}"), diagnostics))
        })
        .unwrap_or(fallback)
}

fn bool_at(v: &Value, key: &str, path: &str, d: &mut Vec<String>) -> Option<bool> {
    v.get(key)
        .map(|x| match x.as_bool() {
            Some(n) => Some(n),
            None => {
                d.push(format!("{path} must be a boolean"));
                None
            }
        })
        .flatten()
}

fn left_tab(id: &str, enabled: bool) -> SidebarLeftTab {
    SidebarLeftTab {
        id: id.to_owned(),
        enabled,
    }
}

fn nested_sidebar_bool(
    root: Option<&Value>,
    defaults: Option<&Value>,
    section: &str,
    key: &str,
    path: &str,
    default: bool,
    diagnostics: &mut Vec<String>,
) -> bool {
    root.and_then(|sidebar| sidebar.get(section))
        .and_then(|value| bool_at(value, key, path, diagnostics))
        .or_else(|| {
            defaults
                .and_then(|sidebar| sidebar.get(section))
                .and_then(|value| bool_at(value, key, path, diagnostics))
        })
        .unwrap_or(default)
}

fn world_clock_config(
    root: Option<&Value>,
    defaults: Option<&Value>,
    diagnostics: &mut Vec<String>,
) -> WorldClockConfig {
    let root_settings = root
        .and_then(|sidebar| sidebar.get("widgets"))
        .and_then(|widgets| widgets.get("worldClock_settings"));
    let default_settings = defaults
        .and_then(|sidebar| sidebar.get("widgets"))
        .and_then(|widgets| widgets.get("worldClock_settings"));
    let pick = |key: &str| {
        root_settings
            .and_then(|settings| settings.get(key))
            .filter(|value| !value.is_null())
            .or_else(|| default_settings.and_then(|settings| settings.get(key)))
    };
    let bool_setting = |key: &str, fallback: bool, diagnostics: &mut Vec<String>| {
        pick(key).map_or(fallback, |value| match value.as_bool() {
            Some(value) => value,
            None => {
                diagnostics.push(format!(
                    "sidebar.widgets.worldClock_settings.{key} must be a boolean"
                ));
                fallback
            }
        })
    };
    let timezones = match pick("timezones") {
        Some(value) if value.is_array() => string_list(
            Some(value),
            "sidebar.widgets.worldClock_settings.timezones",
            diagnostics,
        ),
        Some(_) => {
            diagnostics
                .push("sidebar.widgets.worldClock_settings.timezones must be an array".into());
            Vec::new()
        }
        None => Vec::new(),
    };
    WorldClockConfig {
        timezones,
        show_seconds: bool_setting("showSeconds", false, diagnostics),
        use_24_hour: bool_setting("use24Hour", true, diagnostics),
        show_date: bool_setting("showDate", true, diagnostics),
        highlight_local: bool_setting("highlightLocal", true, diagnostics),
    }
}

fn number_at(v: &Value, key: &str, path: &str, d: &mut Vec<String>) -> Option<f64> {
    v.get(key)
        .map(|x| match x.as_f64() {
            Some(n) if n.is_finite() => Some(n),
            _ => {
                d.push(format!("{path} must be a finite number"));
                None
            }
        })
        .flatten()
}
fn u64_at(v: &Value, key: &str, path: &str, d: &mut Vec<String>) -> Option<u64> {
    v.get(key)
        .map(|x| match x.as_u64() {
            Some(n) => Some(n),
            None => {
                d.push(format!("{path} must be a nonnegative integer"));
                None
            }
        })
        .flatten()
}
fn string_at(v: &Value, key: &str, path: &str, d: &mut Vec<String>) -> Option<String> {
    v.get(key)
        .map(|x| match x.as_str() {
            Some(s) => Some(s.to_owned()),
            None => {
                d.push(format!("{path} must be a string"));
                None
            }
        })
        .flatten()
}
fn string_list(v: Option<&Value>, path: &str, d: &mut Vec<String>) -> Vec<String> {
    let Some(v) = v else { return Vec::new() };
    let Some(items) = v.as_array() else {
        d.push(format!("{path} must be an array"));
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| match item.as_str() {
            Some(s) => Some(s.to_owned()),
            None => {
                d.push(format!("{path}[{i}] must be a string"));
                None
            }
        })
        .collect()
}

fn crypto_coin_ids(value: &Value, diagnostics: &mut Vec<String>) -> Vec<String> {
    let Some(ids) = value.as_array() else {
        diagnostics.push("sidebar.widgets.crypto_settings.coins must be an array".into());
        return Vec::new();
    };
    let mut result = Vec::new();
    for (index, value) in ids.iter().enumerate() {
        let Some(id) = value.as_str() else {
            diagnostics.push(format!(
                "sidebar.widgets.crypto_settings.coins[{index}] must be a string"
            ));
            continue;
        };
        if id.is_empty()
            || id.len() > 80
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            diagnostics.push(format!(
                "sidebar.widgets.crypto_settings.coins[{index}] must be a lowercase CoinGecko ID"
            ));
            continue;
        }
        if result.iter().any(|existing| existing == id) {
            diagnostics.push(format!(
                "sidebar.widgets.crypto_settings.coins[{index}] duplicates {id}"
            ));
            continue;
        }
        result.push(id.to_owned());
    }
    result
}

fn quick_launch_entries(
    v: Option<&Value>,
    path: &str,
    d: &mut Vec<String>,
) -> Vec<QuickLaunchEntry> {
    let Some(v) = v else { return Vec::new() };
    let Some(entries) = v.as_array() else {
        d.push(format!("{path} must be an array"));
        return Vec::new();
    };
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let Some(object) = entry.as_object() else {
                d.push(format!("{path}[{index}] must be an object"));
                return None;
            };
            let mut field = |key: &str| -> Option<String> {
                match object.get(key) {
                    None | Some(Value::Null) => None,
                    Some(Value::String(value)) => Some(value.clone()),
                    Some(_) => {
                        d.push(format!("{path}[{index}].{key} must be a string"));
                        None
                    }
                }
            };
            Some(QuickLaunchEntry {
                icon: field("icon"),
                name: field("name"),
                cmd: field("cmd"),
            })
        })
        .collect()
}

fn bool_map(v: Option<&Value>, path: &str, d: &mut Vec<String>) -> BTreeMap<String, bool> {
    let Some(v) = v else { return BTreeMap::new() };
    let Some(values) = v.as_object() else {
        d.push(format!("{path} must be an object"));
        return BTreeMap::new();
    };
    values
        .iter()
        .filter_map(|(key, value)| match value.as_bool() {
            Some(enabled) => Some((key.clone(), enabled)),
            None => {
                d.push(format!("{path}.{key} must be a boolean"));
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigLoad, ConfigSource, ControlPanelSection, ControlPanelStyle, MediaPopupMode,
        NotificationPopupPosition, SpacerMode,
    };

    #[test]
    fn notification_popup_configuration_defaults_and_overrides_are_typed() {
        let defaults = ConfigLoad::from_defaults();
        assert_eq!(
            defaults.config.notifications.position,
            NotificationPopupPosition::TopRight
        );
        assert!(defaults.config.notifications.screen_list.is_empty());
        assert_eq!(defaults.config.notifications.edge_margin, 4);

        let configured = ConfigLoad::from_bytes(
            br#"{"notifications":{"position":"bottomLeft","screenList":["DP-1"],"edgeMargin":12}}"#,
        );
        assert_eq!(
            configured.config.notifications.position,
            NotificationPopupPosition::BottomLeft
        );
        assert_eq!(configured.config.notifications.screen_list, ["DP-1"]);
        assert_eq!(configured.config.notifications.edge_margin, 12);
    }

    #[test]
    fn invalid_notification_position_and_margin_fall_back_safely() {
        let configured =
            ConfigLoad::from_bytes(br#"{"notifications":{"position":"center","edgeMargin":-1}}"#);
        assert_eq!(
            configured.config.notifications.position,
            NotificationPopupPosition::TopRight
        );
        assert_eq!(configured.config.notifications.edge_margin, 4);
        assert!(
            configured
                .diagnostics
                .iter()
                .any(|item| item.contains("notifications.position"))
        );
    }

    #[test]
    fn osd_configuration_reads_timeout_screen_list_and_media_gate() {
        let defaults = ConfigLoad::from_defaults();
        assert_eq!(defaults.config.osd.timeout_ms, 1_000);
        assert!(defaults.config.osd.media_enabled);
        assert!(defaults.config.osd.screen_list.is_empty());

        let configured = ConfigLoad::from_bytes(
            br#"{"osd":{"timeout":2750,"screenList":["DP-1"],"mediaEnabled":false}}"#,
        );
        assert_eq!(configured.config.osd.timeout_ms, 2_750);
        assert_eq!(configured.config.osd.screen_list, ["DP-1"]);
        assert!(!configured.config.osd.media_enabled);
    }

    #[test]
    fn invalid_osd_timeout_falls_back_without_fabricating_screen_targets() {
        let configured = ConfigLoad::from_bytes(
            br#"{"osd":{"timeout":-5,"screenList":[7],"mediaEnabled":"yes"}}"#,
        );
        assert_eq!(configured.config.osd.timeout_ms, 1_000);
        assert!(configured.config.osd.screen_list.is_empty());
        assert!(configured.config.osd.media_enabled);
        assert!(
            configured
                .diagnostics
                .iter()
                .any(|item| item.contains("osd.screenList"))
        );
        assert!(
            configured
                .diagnostics
                .iter()
                .any(|item| item.contains("osd.mediaEnabled"))
        );
    }

    #[test]
    fn control_panel_defaults_and_optional_section_order_match_qml() {
        let loaded = ConfigLoad::from_defaults();
        assert!(loaded.config.control_panel.compact_mode);
        assert!(!loaded.config.control_panel.keep_loaded);
        assert_eq!(loaded.config.control_panel.style, ControlPanelStyle::Panel);
        assert_eq!(
            loaded.config.control_panel.visible_sections(),
            [
                ControlPanelSection::Profile,
                ControlPanelSection::DateTime,
                ControlPanelSection::Media,
                ControlPanelSection::Wallpaper,
                ControlPanelSection::Weather,
                ControlPanelSection::System,
                ControlPanelSection::Sliders,
                ControlPanelSection::QuickActions,
            ]
        );
    }

    #[test]
    fn control_panel_runtime_overrides_flags_style_and_preserves_section_order() {
        let loaded = ConfigLoad::from_bytes(
            br#"{"controlPanel":{"keepLoaded":true,"compactMode":false,"style":"island","showMediaSection":false,"showWallpaperSection":false,"showSystemSection":false,"showWallpaperSchemeChips":true}}"#,
        );
        let config = &loaded.config.control_panel;
        assert!(config.keep_loaded);
        assert!(!config.compact_mode);
        assert_eq!(config.style, ControlPanelStyle::Island);
        assert!(config.show_wallpaper_scheme_chips);
        assert_eq!(
            config.visible_sections(),
            [
                ControlPanelSection::Profile,
                ControlPanelSection::DateTime,
                ControlPanelSection::Weather,
                ControlPanelSection::Sliders,
                ControlPanelSection::QuickActions,
            ]
        );
    }

    #[test]
    fn malformed_control_panel_options_fall_back_to_defaults_with_diagnostics() {
        let loaded = ConfigLoad::from_bytes(
            br#"{"controlPanel":{"keepLoaded":"yes","style":"glass","showWeatherSection":0}}"#,
        );
        assert!(!loaded.config.control_panel.keep_loaded);
        assert!(loaded.config.control_panel.show_weather_section);
        assert_eq!(loaded.config.control_panel.style, ControlPanelStyle::Panel);
        assert!(loaded.diagnostics.iter().any(|item| {
            item == "controlPanel.keepLoaded must be a boolean"
                || item == "defaults.controlPanel.keepLoaded must be a boolean"
        }));
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|item| item == "controlPanel.style must be panel or island")
        );
    }

    #[test]
    fn bar_spacer_uses_bundled_defaults_and_parses_modes() {
        let defaults = ConfigLoad::from_defaults();
        assert_eq!(defaults.config.bar.layout.spacer_mode, SpacerMode::Auto);
        assert_eq!(defaults.config.bar.layout.spacer_width, 0.0);

        for (mode, expected) in [
            ("auto", SpacerMode::Auto),
            ("fill", SpacerMode::Fill),
            ("fixed", SpacerMode::Fixed),
        ] {
            let json =
                format!(r#"{{"bar":{{"layout":{{"spacerMode":"{mode}","spacerWidth":18}}}}}}"#);
            let loaded = ConfigLoad::from_bytes(json.as_bytes());
            assert_eq!(loaded.config.bar.layout.spacer_mode, expected);
            assert_eq!(loaded.config.bar.layout.spacer_width, 18.0);
        }
    }

    #[test]
    fn bar_spacer_malformed_values_fall_back_safely() {
        let loaded = ConfigLoad::from_bytes(
            br#"{"bar":{"layout":{"spacerMode":"stretch","spacerWidth":-12}}}"#,
        );
        assert_eq!(loaded.config.bar.layout.spacer_mode, SpacerMode::Auto);
        assert_eq!(loaded.config.bar.layout.spacer_width, 0.0);
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("spacerMode must be one of"))
        );
    }

    #[test]
    fn bar_spacer_width_clamps_large_finite_values_before_float_conversion() {
        let loaded = ConfigLoad::from_bytes(br#"{"bar":{"layout":{"spacerWidth":1e300}}}"#);
        assert_eq!(loaded.config.bar.layout.spacer_width, 480.0);
        assert!(loaded.config.bar.layout.spacer_width.is_finite());
    }

    #[test]
    fn bar_verbose_and_media_width_use_bundled_defaults_and_runtime_overrides() {
        let defaults = ConfigLoad::from_defaults();
        assert!(defaults.config.bar.verbose);
        assert_eq!(defaults.config.bar.media_max_width, 220.0);

        let runtime =
            ConfigLoad::from_bytes(br#"{"bar":{"verbose":false,"media":{"maxWidth":320}}}"#);
        assert!(!runtime.config.bar.verbose);
        assert_eq!(runtime.config.bar.media_max_width, 320.0);

        // Each missing runtime field falls back independently.
        let partial = ConfigLoad::from_bytes(br#"{"bar":{"verbose":false}}"#);
        assert!(!partial.config.bar.verbose);
        assert_eq!(partial.config.bar.media_max_width, 220.0);
    }

    #[test]
    fn bar_media_width_enforces_qml_minimum_and_invalid_values_fall_back() {
        let too_narrow = ConfigLoad::from_bytes(br#"{"bar":{"media":{"maxWidth":42}}}"#);
        assert_eq!(too_narrow.config.bar.media_max_width, 120.0);

        let invalid =
            ConfigLoad::from_bytes(br#"{"bar":{"verbose":"yes","media":{"maxWidth":"wide"}}}"#);
        assert!(invalid.config.bar.verbose);
        assert_eq!(invalid.config.bar.media_max_width, 220.0);
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|item| item == "bar.verbose must be a boolean")
        );
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|item| item == "bar.media.maxWidth must be a finite number")
        );
    }

    #[test]
    fn media_popup_mode_defaults_to_dock_and_accepts_runtime_override() {
        let defaults = ConfigLoad::from_defaults();
        assert_eq!(defaults.config.bar.media_popup_mode, MediaPopupMode::Dock);

        let bar = ConfigLoad::from_bytes(br#"{"bar":{"media":{"popupMode":"bar"}}}"#);
        assert_eq!(bar.config.bar.media_popup_mode, MediaPopupMode::Bar);

        let dock = ConfigLoad::from_bytes(br#"{"bar":{"media":{"popupMode":"dock"}}}"#);
        assert_eq!(dock.config.bar.media_popup_mode, MediaPopupMode::Dock);
    }

    #[test]
    fn malformed_media_popup_mode_reports_diagnostic_and_falls_back_to_dock() {
        let loaded = ConfigLoad::from_bytes(br#"{"bar":{"media":{"popupMode":"screen"}}}"#);
        assert_eq!(loaded.config.bar.media_popup_mode, MediaPopupMode::Dock);
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|item| item == "bar.media.popupMode must be one of: dock, bar")
        );
    }

    #[test]
    fn media_output_filter_defaults_and_runtime_overrides_follow_qml_contract() {
        let defaults = ConfigLoad::from_defaults();
        assert!(defaults.config.media.screen_list.is_empty());
        assert!(defaults.config.media.filter_duplicate_players);

        let configured = ConfigLoad::from_bytes(
            br#"{"media":{"screenList":["DP-1","eDP-1"],"filterDuplicatePlayers":false}}"#,
        );
        assert_eq!(configured.config.media.screen_list, ["DP-1", "eDP-1"]);
        assert!(!configured.config.media.filter_duplicate_players);
    }

    #[test]
    fn waffle_background_uses_bundled_defaults() {
        let loaded = ConfigLoad::from_defaults();
        let background = &loaded.config.waffle.background;

        assert_eq!(loaded.source, ConfigSource::BundledDefaults);
        assert_eq!(background.use_main_wallpaper, Some(true));
        assert_eq!(background.wallpaper_path.as_deref(), Some(""));
        assert_eq!(background.hide_when_fullscreen, Some(true));
        assert_eq!(background.backdrop.enabled, Some(true));
        assert_eq!(background.backdrop.hide_wallpaper, Some(false));
        assert_eq!(background.backdrop.use_main_wallpaper, Some(true));
        assert_eq!(background.backdrop.wallpaper_path.as_deref(), Some(""));
        assert_eq!(background.backdrop.dim, Some(35.));
        assert_eq!(background.backdrop.vignette_enabled, Some(false));
        assert_eq!(background.backdrop.vignette_intensity, Some(0.5));
        assert_eq!(background.backdrop.vignette_radius, Some(0.7));
    }

    #[test]
    fn waffle_background_overrides_are_independent_from_shared_backdrop() {
        let loaded = ConfigLoad::from_bytes(
            br#"{
                "background": {
                    "wallpaperPath": "/global.jpg",
                    "backdrop": {
                        "enable": false,
                        "dim": 9
                    }
                },
                "waffles": {
                    "background": {
                        "useMainWallpaper": false,
                        "wallpaperPath": "/waffle.jpg",
                        "hideWhenFullscreen": false,
                        "backdrop": {
                            "enable": false,
                            "hideWallpaper": true,
                            "useMainWallpaper": false,
                            "wallpaperPath": "/waffle-backdrop.jpg",
                            "blurRadius": 17,
                            "dim": 21,
                            "saturation": 12,
                            "contrast": -4,
                            "vignetteEnabled": true,
                            "vignetteIntensity": 0.6,
                            "vignetteRadius": 0.8
                        }
                    }
                }
            }"#,
        );
        let waffle = &loaded.config.waffle.background;

        assert_eq!(waffle.use_main_wallpaper, Some(false));
        assert_eq!(waffle.wallpaper_path.as_deref(), Some("/waffle.jpg"));
        assert_eq!(waffle.hide_when_fullscreen, Some(false));
        assert_eq!(waffle.backdrop.enabled, Some(false));
        assert_eq!(waffle.backdrop.hide_wallpaper, Some(true));
        assert_eq!(waffle.backdrop.use_main_wallpaper, Some(false));
        assert_eq!(
            waffle.backdrop.wallpaper_path.as_deref(),
            Some("/waffle-backdrop.jpg")
        );
        assert_eq!(waffle.backdrop.blur_radius, Some(17.));
        assert_eq!(waffle.backdrop.dim, Some(21.));
        assert_eq!(waffle.backdrop.saturation, Some(12.));
        assert_eq!(waffle.backdrop.contrast, Some(-4.));
        assert_eq!(waffle.backdrop.vignette_enabled, Some(true));
        assert_eq!(waffle.backdrop.vignette_intensity, Some(0.6));
        assert_eq!(waffle.backdrop.vignette_radius, Some(0.8));

        assert_eq!(
            loaded.config.background.wallpaper_path.as_deref(),
            Some("/global.jpg")
        );
        assert_eq!(loaded.config.background.backdrop.enabled, Some(false));
        assert_eq!(loaded.config.background.backdrop.dim, Some(9.));
        assert_eq!(waffle.backdrop.dim, Some(21.));
    }

    #[test]
    fn sparse_waffle_background_falls_back_to_bundled_defaults() {
        let loaded = ConfigLoad::from_bytes(
            br#"{"waffles":{"background":{"backdrop":{"wallpaperPath":"/custom.jpg","dim":12}}}}"#,
        );
        let background = &loaded.config.waffle.background;

        assert_eq!(background.use_main_wallpaper, Some(true));
        assert_eq!(background.hide_when_fullscreen, Some(true));
        assert_eq!(background.backdrop.enabled, Some(true));
        assert_eq!(background.backdrop.hide_wallpaper, Some(false));
        assert_eq!(background.backdrop.use_main_wallpaper, Some(true));
        assert_eq!(
            background.backdrop.wallpaper_path.as_deref(),
            Some("/custom.jpg")
        );
        assert_eq!(background.backdrop.blur_radius, Some(32.));
        assert_eq!(background.backdrop.dim, Some(12.));
        assert_eq!(background.backdrop.vignette_enabled, Some(false));
    }

    #[test]
    fn iris_background_defaults_and_source_sections_match_bundled_schema() {
        let loaded = ConfigLoad::from_defaults();
        let iris = &loaded.config.iris_background;
        let background = &loaded.config.background;

        assert_eq!(iris.source, ConfigSource::BundledDefaults);
        assert!(iris.desktop_widgets_enabled);
        assert!(iris.desktop_menu_wallpaper);
        assert_eq!(
            iris.desktop_menu_items,
            [
                "widgets",
                "customize",
                "screenshot",
                "terminal",
                "settings",
                "restart"
            ]
        );
        assert!(iris.raw_runtime_sections.is_empty());
        assert!(iris.raw_default_sections.contains_key("background"));
        assert!(iris.raw_default_sections.contains_key("iris.modules"));
        assert!(iris.raw_default_sections.contains_key("iris.desktopMenu"));

        assert_eq!(background.wallpaper_path.as_deref(), Some(""));
        assert_eq!(background.thumbnail_path.as_deref(), Some(""));
        assert_eq!(background.fill_mode.as_deref(), Some("fill"));
        assert_eq!(background.enable_animation, Some(true));
        assert_eq!(background.pause_animation_on_battery, Some(true));
        assert_eq!(background.video_pause.as_deref(), Some("covered"));
        assert_eq!(background.hide_when_fullscreen, Some(true));
        assert_eq!(background.transition.enabled, Some(true));
        assert_eq!(background.transition.kind.as_deref(), Some("random"));
        assert_eq!(background.transition.direction.as_deref(), Some("right"));
        assert_eq!(background.transition.duration_ms, Some(800));
        assert_eq!(background.transition.bezier, [0.54, 0.0, 0.34, 0.99]);
        assert_eq!(background.effects.blur_enabled, Some(true));
        assert_eq!(background.effects.blur_radius, Some(22.));
        assert_eq!(background.effects.dim, Some(30.));
        assert_eq!(background.effects.dynamic_dim, Some(20.));
        assert_eq!(background.parallax.enabled, Some(false));
        assert_eq!(background.parallax.axis.as_deref(), Some("vertical"));
        assert_eq!(background.parallax.workspace_enabled, Some(true));
        assert_eq!(background.parallax.pause_during_transitions, Some(true));
        assert_eq!(background.backdrop.enabled, Some(true));
        assert_eq!(background.backdrop.wallpaper_path.as_deref(), Some(""));
        assert_eq!(background.backdrop.fill_mode.as_deref(), Some("fill"));
        assert_eq!(background.backdrop.blur_radius, Some(40.));
        assert_eq!(background.backdrop.dim, Some(40.));
        assert_eq!(background.backdrop.saturation, Some(0.));
        assert_eq!(background.backdrop.contrast, Some(0.));
        assert_eq!(background.backdrop.vignette_intensity, Some(1.));
    }

    #[test]
    fn iris_background_reads_runtime_fields_and_retains_unmapped_source_keys() {
        let loaded = ConfigLoad::from_bytes(br#"{
            "iris": {
                "modules": {"desktopWidgets": false, "futureModule": {"enabled": true}},
                "desktopMenu": {"wallpaper": false, "items": ["terminal", "settings"], "futureMenuKey": 9}
            },
            "background": {
                "wallpaperPath": "/wall.jpg", "thumbnailPath": "/wall-thumb.jpg", "fillMode": "fit",
                "enableAnimation": false, "pauseAnimationOnBattery": false, "videoPause": "fullscreen", "hideWhenFullscreen": false,
                "transition": {"enable": false, "type": "wipe", "direction": "left", "duration": 450, "bezier": [0.1, 0.2, 0.8, 0.9]},
                "effects": {"enableBlur": true, "blurRadius": 19, "thumbnailBlurStrength": 42, "enableAnimatedBlur": true, "dim": 13, "dynamicDim": 7},
                "parallax": {"enable": true, "axis": "horizontal", "vertical": true, "autoVertical": true, "enableWorkspace": false, "workspaceShift": 0.4, "workspaceZoom": 1.3, "zoom": 1.2, "enableSidebar": false, "panelShift": 0.2, "widgetsFactor": 1.4, "widgetDepth": 0.7, "pauseDuringTransitions": false, "transitionSettleMs": 310},
                "backdrop": {"enable": false, "useMainWallpaper": false, "wallpaperPath": "/overview.jpg", "thumbnailPath": "/overview-thumb.jpg", "fillMode": "center", "blurRadius": 51, "dim": 24, "saturation": 0.25, "contrast": -0.1, "vignetteEnabled": true, "vignetteIntensity": 0.6, "vignetteRadius": 0.8, "enableAnimation": true, "enableAnimatedBlur": true, "futureEffect": "kept"}
            }
        }"#);
        let iris = &loaded.config.iris_background;
        let background = &loaded.config.background;

        assert_eq!(loaded.source, ConfigSource::Runtime);
        assert_eq!(iris.source, ConfigSource::Runtime);
        assert!(!iris.desktop_widgets_enabled);
        assert!(!iris.desktop_menu_wallpaper);
        assert_eq!(iris.desktop_menu_items, ["terminal", "settings"]);
        assert_eq!(
            iris.raw_runtime_sections["iris.modules"]["futureModule"]["enabled"],
            true
        );
        assert_eq!(
            iris.raw_runtime_sections["iris.desktopMenu"]["futureMenuKey"],
            9
        );
        assert_eq!(background.wallpaper_path.as_deref(), Some("/wall.jpg"));
        assert_eq!(background.fill_mode.as_deref(), Some("fit"));
        assert_eq!(background.enable_animation, Some(false));
        assert_eq!(background.pause_animation_on_battery, Some(false));
        assert_eq!(background.video_pause.as_deref(), Some("fullscreen"));
        assert_eq!(background.hide_when_fullscreen, Some(false));
        assert_eq!(background.transition.kind.as_deref(), Some("wipe"));
        assert_eq!(background.transition.duration_ms, Some(450));
        assert_eq!(background.transition.bezier, [0.1, 0.2, 0.8, 0.9]);
        assert_eq!(background.effects.blur_radius, Some(19.));
        assert_eq!(background.effects.dynamic_dim, Some(7.));
        assert_eq!(background.parallax.axis.as_deref(), Some("horizontal"));
        assert_eq!(background.parallax.transition_settle_ms, Some(310));
        assert_eq!(
            background.backdrop.wallpaper_path.as_deref(),
            Some("/overview.jpg")
        );
        assert_eq!(
            background.backdrop.thumbnail_path.as_deref(),
            Some("/overview-thumb.jpg")
        );
        assert_eq!(background.backdrop.fill_mode.as_deref(), Some("center"));
        assert_eq!(background.backdrop.blur_radius, Some(51.));
        assert_eq!(background.backdrop.saturation, Some(0.25));
        assert_eq!(background.backdrop.contrast, Some(-0.1));
        assert_eq!(background.backdrop.vignette_enabled, Some(true));
        assert_eq!(background.backdrop.vignette_intensity, Some(0.6));
        assert_eq!(background.backdrop.vignette_radius, Some(0.8));
        assert_eq!(
            iris.raw_runtime_sections["background"]["backdrop"]["futureEffect"],
            "kept"
        );
    }

    #[test]
    fn sparse_and_invalid_iris_background_fields_use_bundled_defaults() {
        let sparse = ConfigLoad::from_bytes(br#"{"background":{"backdrop":{"dim":12}}}"#);
        assert_eq!(sparse.config.background.wallpaper_path.as_deref(), Some(""));
        assert_eq!(sparse.config.background.enable_animation, Some(true));
        assert_eq!(sparse.config.background.transition.duration_ms, Some(800));
        assert_eq!(sparse.config.background.backdrop.blur_radius, Some(40.));
        assert_eq!(sparse.config.background.backdrop.dim, Some(12.));
        assert_eq!(sparse.config.iris_background.desktop_widgets_enabled, true);
        assert_eq!(sparse.config.iris_background.desktop_menu_items.len(), 6);

        let invalid = ConfigLoad::from_bytes(br#"{"iris":{"modules":{"desktopWidgets":"false"},"desktopMenu":{"wallpaper":5,"items":"none"}},"background":{"wallpaperPath":17,"enableAnimation":"no","transition":{"duration":-1},"backdrop":{"saturation":"high"}}}"#);
        assert_eq!(
            invalid.config.background.wallpaper_path.as_deref(),
            Some("")
        );
        assert_eq!(invalid.config.iris_background.desktop_widgets_enabled, true);
        assert_eq!(invalid.config.iris_background.desktop_menu_wallpaper, true);
        assert_eq!(invalid.config.iris_background.desktop_menu_items.len(), 6);
        assert_eq!(invalid.config.background.enable_animation, Some(true));
        assert_eq!(invalid.config.background.transition.duration_ms, Some(800));
        assert_eq!(invalid.config.background.backdrop.saturation, Some(0.));
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|item| item.contains("iris.modules.desktopWidgets"))
        );
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|item| item.contains("iris.desktopMenu.items"))
        );
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|item| item.contains("background.transition.duration"))
        );
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|item| item.contains("background.wallpaperPath"))
        );
    }
}
