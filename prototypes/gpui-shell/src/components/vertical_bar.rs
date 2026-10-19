//! Isolated ii vertical-bar visual component.
//!
//! QML parity sources: `modules/verticalBar/VerticalBarContent.qml` and
//! `modules/verticalBar/VerticalBar.qml`. This component receives snapshots and
//! style values from its caller; it deliberately does not invent live desktop
//! metrics or bind to a global store.

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc, time::Duration};

use crate::resource_provider::{LinuxResourceProvider, ResourceSnapshot, format_percent};
use crate::{components::material, config::ShellConfig, niri, theme::ThemeTokens};
use gpui::{
    Context, FontWeight, Hsla, InteractiveElement, KeyDownEvent, MouseButton, NavigationDirection,
    PathBuilder, Role, ScrollDelta, ShapedLine, StatefulInteractiveElement, TextAlign, TextRun,
    Window, canvas, div, point, prelude::*, px, rgba, white,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSlot {
    pub label: String,
    pub occupied: bool,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResourceValue {
    pub label: String,
    pub icon: String,
    /// `None` represents an unavailable/unreported source, not a zero value.
    pub value: Option<String>,
    pub percent: Option<f64>,
    pub warning_threshold: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaState {
    /// `Unavailable` means the MPRIS source is disconnected. `NoPlayer` means
    /// it was queried successfully but has no active track/player.
    pub status: MediaStatus,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub position_percent: Option<u8>,
    pub can_toggle_playback: bool,
    pub can_previous: bool,
    pub can_next: bool,
    pub can_change_volume: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaStatus {
    Unavailable,
    NoPlayer,
    Playing,
    Paused,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MediaAction {
    TogglePlayback,
    Previous,
    Next,
    OpenControls,
    VolumeDelta(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatteryState {
    pub status: BatteryStatus,
    pub charge: BatteryChargeState,
    /// UPower percentage in 0..=100. `None` stays unknown, never zero.
    pub percentage: Option<u8>,
    /// QML `battery.low`, expressed as a percentage in 0..=100.
    pub low_threshold: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryStatus {
    Available,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryChargeState {
    Charging,
    Discharging,
    Critical,
    Full,
    Unknown,
}

/// A live Niri taskbar entry. The window ID is the action identity; titles and
/// app IDs are labels only and never used to focus a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerticalTaskbarItem {
    pub app_id: String,
    pub app_name: String,
    pub label: String,
    pub window_ids: Vec<u64>,
    pub focused_window_id: Option<u64>,
    pub pinned: bool,
    pub active: bool,
}

/// Visual parameters are passed explicitly to keep family/theme decisions out
/// of this component and make it usable for left and right placements alike.
#[derive(Clone, Copy, Debug)]
pub struct VerticalBarStyle {
    pub surface: Hsla,
    pub active_surface: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub separator: Hsla,
    /// QML battery indicator's normal ink: Material on_background, then
    /// on_surface, then the established vertical-bar foreground.
    pub battery_ink: Hsla,
    /// Material semantic error role used only for a low, non-charging battery.
    pub battery_error: Hsla,
    /// Background-colored glyphs act as the QML text-mask foreground over the fill.
    pub battery_overlay: Hsla,
    /// Contrasting text/icon color over the semantic error fill.
    pub battery_error_overlay: Hsla,
    pub radius_px: f32,
    pub width_px: f32,
    /// QML `BarGroup.vertical` base width. This currently uses the unscaled
    /// 46 px source value; host appearance scaling is not wired yet.
    pub group_width_px: f32,
    /// QML separators use `Appearance.sizes.baseBarHeight / 3`. The config
    /// value is clamped in `live`; font-size scaling remains host-owned.
    pub base_bar_height_px: f32,
    /// QML middle `Column.spacing` between visible groups.
    pub middle_spacing_px: f32,
    /// True when QML `bar.bottom` places the vertical bar on the right edge.
    pub bar_on_right: bool,
    /// Natural edge section demand from the QML layout, reserved outside middleHost.
    pub top_section_px: f32,
    pub bottom_section_px: f32,
}

impl Default for VerticalBarStyle {
    fn default() -> Self {
        Self {
            surface: rgba(0x161b22ee).into(),
            active_surface: rgba(0x30363ddd).into(),
            foreground: white().into(),
            muted: rgba(0xb1bac4ff).into(),
            accent: rgba(0x7ee787ff).into(),
            separator: rgba(0x6e768180).into(),
            battery_ink: white().into(),
            battery_error: rgba(0xf85149ff).into(),
            battery_overlay: rgba(0x161b22ff).into(),
            battery_error_overlay: white().into(),
            radius_px: 12.0,
            width_px: 64.0,
            group_width_px: 46.0,
            base_bar_height_px: 40.0,
            middle_spacing_px: 4.0,
            bar_on_right: false,
            top_section_px: 56.0,
            bottom_section_px: 88.0,
        }
    }
}

#[derive(Clone)]
pub struct VerticalBarProps {
    /// Shared Material context remains available to other environment plugins;
    /// this component uses it for its surface and keyboard focus primitives.
    pub material: crate::theme::MaterialThemeContext,
    /// Values are formatted by the caller according to the configured locale.
    pub clock: Option<String>,
    pub date: Option<String>,
    pub workspaces: Vec<WorkspaceSlot>,
    pub resources: Vec<ResourceValue>,
    /// `None` means the MPRIS provider has not been connected; `MediaStatus`
    /// distinguishes provider-unavailable from a connected empty player list.
    pub media: Option<MediaState>,
    /// Optional MPRIS intent sink. Absent actions remain unavailable.
    pub on_media_action: Option<Rc<dyn Fn(MediaAction)>>,
    /// Mirrors `bar.modules.taskbar`; enabled with no entries shows an empty state.
    pub taskbar_enabled: bool,
    pub taskbar_items: Vec<VerticalTaskbarItem>,
    pub on_activate_taskbar_app: Option<Rc<dyn Fn(String, Vec<u64>, Option<u64>)>>,
    /// Mirrors `bar.borderless`, which controls separators between groups.
    pub borderless: bool,
    /// `None` means Battery/UPower is unavailable, as in QML's `Battery.available` gate.
    pub battery: Option<BatteryState>,
    pub style: VerticalBarStyle,
}

pub struct VerticalBar {
    pub props: VerticalBarProps,
}

impl VerticalBar {
    pub fn new(props: VerticalBarProps) -> Self {
        Self { props }
    }

    pub fn live(
        output_name: Option<String>,
        config: ShellConfig,
        theme: ThemeTokens,
        media_actions: async_channel::Sender<MediaAction>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut initial_props = VerticalBarProps::default();
        initial_props.material = theme.material.clone();
        // The ii environment consumes the same generated Material base as its
        // horizontal surfaces. Missing palette roles retain the established
        // vertical-bar fallback colors, so a partial palette stays safe.
        initial_props.style.surface = theme
            .material
            .colors
            .surface_container_low
            .or(theme.material.colors.surface)
            .unwrap_or(initial_props.style.surface);
        initial_props.style.active_surface = theme
            .material
            .colors
            .primary_container
            .unwrap_or(initial_props.style.active_surface);
        initial_props.style.foreground = theme
            .material
            .colors
            .on_surface
            .unwrap_or(initial_props.style.foreground);
        initial_props.style.battery_ink = theme
            .material
            .colors
            .on_background
            .or(theme.material.colors.on_surface)
            .unwrap_or(initial_props.style.foreground);
        initial_props.style.battery_error = theme
            .material
            .colors
            .error
            .unwrap_or(initial_props.style.battery_error);
        initial_props.style.battery_overlay = theme
            .material
            .colors
            .background
            .or(theme.material.colors.inverse_surface)
            .unwrap_or(initial_props.style.surface);
        initial_props.style.battery_error_overlay = theme
            .material
            .colors
            .on_error
            .or(theme.material.colors.background)
            .or(theme.material.colors.inverse_surface)
            .unwrap_or(initial_props.style.foreground);
        initial_props.style.muted = theme
            .material
            .colors
            .on_surface_variant
            .unwrap_or(initial_props.style.muted);
        initial_props.style.accent = theme
            .material
            .colors
            .primary
            .unwrap_or(initial_props.style.accent);
        initial_props.style.separator = theme
            .material
            .colors
            .outline_variant
            .unwrap_or(initial_props.style.separator);
        initial_props.taskbar_enabled = config.bar.modules.get("taskbar").copied().unwrap_or(false);
        initial_props.borderless = config.bar.borderless.unwrap_or(true);
        initial_props.style.bar_on_right = config.bar.bottom.unwrap_or(false);
        let catalog = crate::families::waffle::taskbar::start_menu_adapter::desktop_entries::XdgDesktopEntrySource::new().map(Arc::new);
        let pinned_apps = config
            .appearance
            .source_sections
            .get("ii.dock")
            .and_then(|options| options.get("pinnedApps"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let activation_catalog = catalog.clone();
        let focus_positions = Rc::new(RefCell::new(HashMap::<String, usize>::new()));
        let activation_positions = focus_positions.clone();
        initial_props.on_activate_taskbar_app =
            Some(Rc::new(move |app_id, window_ids, focused_id| {
                if window_ids.is_empty() {
                    if let Some(catalog) = &activation_catalog {
                        if let Err(error) = catalog.spawn_app_id(&app_id) {
                            eprintln!("ii vertical taskbar: {error}");
                        }
                    }
                    return;
                }
                let key = app_id.to_lowercase();
                let mut positions = activation_positions.borrow_mut();
                let index = focused_id
                    .and_then(|id| window_ids.iter().position(|window_id| *window_id == id))
                    .map(|index| (index + 1) % window_ids.len())
                    .or_else(|| {
                        positions
                            .get(&key)
                            .map(|index| (index + 1) % window_ids.len())
                    })
                    .unwrap_or(0);
                positions.insert(key, index);
                let _ = niri::focus_window(window_ids[index]);
            }));
        initial_props.style.base_bar_height_px =
            config.bar.height.unwrap_or(40.0).clamp(24.0, 80.0) as f32;
        let battery_low = config.battery_low.unwrap_or(20.0).clamp(0.0, 100.0);
        let media_action_tx = media_actions;
        initial_props.on_media_action = Some(Rc::new(move |action| {
            let _ = media_action_tx.try_send(action);
        }));
        let view = Self::new(initial_props);
        cx.spawn(async move |this, cx| {
            let mut resource_provider = LinuxResourceProvider::default();
            let mut desktop_provider = crate::vertical_provider::VerticalProvider::new(battery_low);
            loop {
                let output_name = output_name.clone();
                let interval = resource_provider.recommended_interval();
                let resources_config = config.bar.resources.clone();
                let resource_snapshot = resource_provider.sample();
                let battery = desktop_provider.sample().await;
                crate::families::ii::panel_surface::cache_vertical_status_rings_snapshot(
                    output_name.as_deref(),
                    &resource_snapshot,
                    battery.clone(),
                );
                let state = cx
                    .background_executor()
                    .spawn(async move {
                        let per_monitor = config.bar.workspace.per_monitor.unwrap_or(true);
                        let mut snapshot = niri::snapshot(if per_monitor {
                            output_name.as_deref()
                        } else {
                            None
                        });
                        if !config.bar.workspace.dynamic_count.unwrap_or(true) {
                            let count = config.bar.workspace.shown.unwrap_or(10).clamp(1, 20) as u8;
                            snapshot.workspaces.resize_with(usize::from(count), || {
                                niri::Workspace {
                                    id: 0,
                                    number: 0,
                                    name: None,
                                    occupied: false,
                                    active: false,
                                }
                            });
                            for (index, workspace) in snapshot.workspaces.iter_mut().enumerate() {
                                workspace.number = (index + 1) as u8;
                            }
                        }
                        snapshot
                    })
                    .await;
                if this
                    .update(cx, |view, cx| {
                        view.props.workspaces = state
                            .workspaces
                            .into_iter()
                            .map(|workspace| WorkspaceSlot {
                                label: workspace.number.to_string(),
                                occupied: workspace.occupied,
                                active: workspace.active,
                            })
                            .collect();
                        view.props.taskbar_items = vertical_taskbar_items(
                            state.windows.clone(),
                            &pinned_apps,
                            catalog.as_deref(),
                        );
                        if let Some(clock) = state.clock {
                            let mut parts = clock.splitn(2, " · ");
                            view.props.clock = parts.next().map(str::to_owned);
                            view.props.date = parts.next().map(str::to_owned);
                        } else {
                            view.props.clock = None;
                            view.props.date = None;
                        }
                        view.props.resources =
                            vertical_resource_values(&resource_snapshot, &resources_config);
                        view.props.battery = battery;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor()
                    .timer(interval.max(Duration::from_secs(2)))
                    .await;
            }
        })
        .detach();
        view
    }

    pub fn set_media_state(&mut self, media: MediaState, cx: &mut Context<Self>) {
        self.props.media = Some(media);
        cx.notify();
    }
}

fn vertical_resource_values(
    snapshot: &ResourceSnapshot,
    config: &crate::config::ResourceConfig,
) -> Vec<ResourceValue> {
    let mut values = Vec::new();
    let mut push = |shown: Option<bool>,
                    label: &str,
                    icon: &str,
                    percent: Option<f64>,
                    threshold: Option<f64>,
                    default_threshold: f64| {
        if shown.unwrap_or(true) {
            values.push(ResourceValue {
                label: label.to_owned(),
                icon: icon.to_owned(),
                value: format_percent(percent),
                percent,
                warning_threshold: threshold.unwrap_or(default_threshold).clamp(0.0, 100.0),
            });
        }
    };
    push(
        config.show_memory_indicator,
        "Memory",
        "memory",
        snapshot.memory_percent,
        config.memory_warning_threshold,
        90.0,
    );
    push(
        config.show_swap_indicator,
        "Swap",
        "swap_horiz",
        snapshot.swap_percent,
        config.swap_warning_threshold,
        90.0,
    );
    push(
        config.show_cpu_indicator,
        "CPU",
        "planner_review",
        snapshot.cpu_percent,
        config.cpu_warning_threshold,
        90.0,
    );
    // No supported GPU telemetry source is connected yet. Preserve configured
    // visibility and represent that source honestly as unavailable.
    push(
        config.show_gpu_indicator,
        "GPU",
        "memory_alt",
        None,
        config.gpu_warning_threshold,
        90.0,
    );
    values
}

impl Render for VerticalBar {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        render_vertical_bar(&self.props, window)
    }
}

impl Default for VerticalBarProps {
    fn default() -> Self {
        Self {
            material: crate::theme::MaterialThemeContext::default(),
            clock: None,
            date: None,
            workspaces: Vec::new(),
            resources: Vec::new(),
            media: None,
            on_media_action: None,
            taskbar_enabled: false,
            taskbar_items: Vec::new(),
            on_activate_taskbar_app: None,
            borderless: true,
            battery: None,
            style: VerticalBarStyle::default(),
        }
    }
}

fn taskbar_viewport_height(available_height: f32, props: &VerticalBarProps) -> f32 {
    let style = props.style;
    let middle_height =
        (available_height - style.top_section_px - style.bottom_section_px).max(0.0);
    let separator_height = 1.0;
    let clock_height = 16.0 // vertical padding
        + 16.0 // clock
        + separator_height
        + 12.0 // date
        + 8.0 // clock/date group gaps
        + props.battery.as_ref().map_or(0.0, |_| 1.0 + 44.0 + 4.0);
    let workspace_height = if props.workspaces.is_empty() {
        12.0 + 14.0
    } else {
        12.0 + props.workspaces.len() as f32 * 36.0
            + props.workspaces.len().saturating_sub(1) as f32 * 4.0
    };
    let separators = if props.borderless { 3.0 } else { 0.0 };
    let visible_groups = 3.0 + separators;
    let fixed_middle_height = clock_height
        + workspace_height
        + separators * separator_height
        + (visible_groups - 1.0).max(0.0) * style.middle_spacing_px;
    (middle_height - fixed_middle_height).max(0.0)
}

fn vertical_taskbar_items(
    mut windows: Vec<niri::Window>,
    pinned_ids: &[String],
    catalog: Option<&crate::families::waffle::taskbar::start_menu_adapter::desktop_entries::XdgDesktopEntrySource>,
) -> Vec<VerticalTaskbarItem> {
    windows.sort_by_key(|window| {
        let [column, row] = window.pos_in_scrolling_layout.unwrap_or([u64::MAX; 2]);
        (column, row, window.id)
    });
    let mut running = Vec::<VerticalTaskbarItem>::new();
    for window in windows {
        let Some(app_id) = window.app_id.filter(|app_id| !app_id.trim().is_empty()) else {
            continue;
        };
        let app_name = catalog
            .and_then(|catalog| catalog.resolve_app(&app_id))
            .map(|entry| entry.0)
            .unwrap_or_else(|| app_id.clone());
        if let Some(item) = running
            .iter_mut()
            .find(|item| item.app_id.eq_ignore_ascii_case(&app_id))
        {
            item.window_ids.push(window.id);
            if window.focused {
                item.focused_window_id = Some(window.id);
                item.active = true;
            }
        } else {
            running.push(VerticalTaskbarItem {
                app_id,
                label: app_name.clone(),
                app_name,
                window_ids: vec![window.id],
                focused_window_id: window.focused.then_some(window.id),
                pinned: false,
                active: window.focused,
            });
        }
    }

    let mut ordered = Vec::new();
    for pinned_id in pinned_ids {
        if let Some(index) = running
            .iter()
            .position(|item| item.app_id.eq_ignore_ascii_case(pinned_id))
        {
            let mut item = running.remove(index);
            item.pinned = true;
            ordered.push(item);
        } else if let Some((name, _)) = catalog.and_then(|catalog| catalog.resolve_app(pinned_id)) {
            ordered.push(VerticalTaskbarItem {
                app_id: pinned_id.clone(),
                label: name.clone(),
                app_name: name,
                window_ids: Vec::new(),
                focused_window_id: None,
                pinned: true,
                active: false,
            });
        }
    }
    ordered.extend(running);
    ordered
}

/// Vertical visual composition follows `VerticalBarContent.qml`: taskbar mode
/// moves clock/date/battery above workspaces and shows app slots below them;
/// without taskbar, resources/media occupy the middle and clock/date stay low.
pub fn render_vertical_bar(props: &VerticalBarProps, window: &mut Window) -> impl IntoElement {
    let style = props.style;
    let clock = props.clock.clone();
    let date = props.date.clone();
    let workspace_nodes = props.workspaces.iter().enumerate().map(|(index, slot)| {
        let bg = if slot.active {
            style.active_surface
        } else {
            rgba(0x00000000).into()
        };
        let fg = if slot.active {
            style.foreground
        } else if slot.occupied {
            style.muted
        } else {
            style.separator
        };
        let workspace_number = slot.label.parse::<u8>().ok();
        let button = material::focusable_button(
            div()
                .w(px(40.))
                .h(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_lg()
                .bg(bg)
                .text_color(fg),
            &props.material,
            style.accent,
        );
        let button = if let Some(number) = workspace_number {
            button
                .id(("vertical-workspace", u32::from(number)))
                .role(Role::Button)
                .focusable()
                .tab_stop(true)
        } else {
            button
                .id(("vertical-workspace-label", index))
                .aria_label(format!("Workspace {}", slot.label))
        };
        let button = if let Some(number) = workspace_number {
            let keyboard_number = number;
            button
                .aria_label(vertical_workspace_accessible_name(slot, number))
                .on_click(move |_, _, _| {
                    niri::focus_workspace(number);
                })
                .on_key_down(move |event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                        niri::focus_workspace(keyboard_number);
                        cx.stop_propagation();
                    }
                })
        } else {
            button
        };
        button.child(slot.label.clone())
    });
    let resource_nodes = props
        .resources
        .iter()
        .map(|item| {
            let value = item.value.clone().unwrap_or_else(|| "—".into());
            let warning = item
                .percent
                .is_some_and(|value| value.clamp(0.0, 100.0) >= item.warning_threshold);
            let value_color = if warning {
                rgba(0xf85149ff).into()
            } else {
                style.foreground
            };
            let percent = item.percent.map(|value| value.clamp(0.0, 100.0));
            let ring_color = if warning {
                rgba(0xf85149ff).into()
            } else {
                style.foreground
            };
            let track_color = ring_color.opacity(0.5);
            let ring = canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let center = point(bounds.origin.x + px(9.), bounds.origin.y + px(9.));
                    let radius = px(7.5);
                    let circle_path = |sweep: f64| {
                        let angle = sweep.to_radians();
                        let end = point(
                            center.x + px(angle.sin() as f32 * 15.),
                            center.y - px(angle.cos() as f32 * 15.),
                        );
                        let mut path = PathBuilder::stroke(px(2.));
                        path.move_to(point(center.x, center.y - radius));
                        path.arc_to(point(radius, radius), px(0.), sweep > 180.0, true, end);
                        path.build().ok()
                    };
                    if let Some(path) = circle_path(359.9) {
                        window.paint_path(path, track_color);
                    }
                    if let Some(percent) = percent {
                        if percent > 0.0 {
                            if let Some(path) = circle_path(percent * 3.6) {
                                window.paint_path(path, ring_color);
                            }
                        }
                    }
                },
            )
            .w(px(18.))
            .h(px(18.));
            div()
                .w(px(36.))
                .id(format!("vertical-resource-{}", item.label))
                .role(Role::ProgressIndicator)
                .aria_label(if let Some(percent) = percent {
                    format!("{}: {:.0}%", item.label, percent)
                } else {
                    format!("{}: unavailable", item.label)
                })
                .when_some(percent, |this, percent| {
                    this.aria_min_numeric_value(0.0)
                        .aria_max_numeric_value(100.0)
                        .aria_numeric_value(f64::from(percent))
                })
                .flex()
                .flex_col()
                .items_center()
                .gap(px(3.))
                .text_color(value_color)
                .child(
                    div().relative().w(px(18.)).h(px(18.)).child(ring).child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .font_family("Material Symbols Rounded")
                            .text_color(style.foreground)
                            .child(item.icon.clone()),
                    ),
                )
                .child(div().font_weight(FontWeight::MEDIUM).child(value))
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(style.muted)
                        .child(item.label.clone()),
                )
        })
        .collect::<Vec<_>>();
    let media_node =
        render_vertical_media(props.media.as_ref(), style, props.on_media_action.clone());
    let taskbar_nodes = props.taskbar_items.iter().map(|item| {
        let label = if item.label.trim().is_empty() {
            item.app_name.clone()
        } else {
            item.label.clone()
        };
        let fg = if item.active {
            style.foreground
        } else {
            style.muted
        };
        let on_activate = props.on_activate_taskbar_app.clone();
        let app_id = item.app_id.clone();
        let window_ids = item.window_ids.clone();
        let focused_window_id = item.focused_window_id;
        let button = material::focusable_button(
            div()
                .id(format!("vertical-taskbar-app-{}", item.app_id))
                .role(Role::Button)
                .aria_label(format!(
                    "{}{}",
                    label,
                    if item.active { ", focused" } else { "" }
                ))
                .focusable()
                .tab_stop(true)
                .w(px(40.))
                .h(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_lg()
                .bg(if item.active {
                    style.active_surface
                } else {
                    rgba(0x00000000).into()
                })
                .text_color(fg)
                .child(label),
            &props.material,
            style.accent,
        );
        let button = if let Some(on_activate) = on_activate.clone() {
            let app_id = app_id.clone();
            let window_ids = window_ids.clone();
            button.on_click(move |_, _, _| {
                on_activate(app_id.clone(), window_ids.clone(), focused_window_id)
            })
        } else {
            button
        };
        button.on_key_down(move |event: &KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                if let Some(on_activate) = &on_activate {
                    on_activate(app_id.clone(), window_ids.clone(), focused_window_id);
                }
                cx.stop_propagation();
            }
        })
    });

    let clock_group = || {
        div()
            .w(px(style.group_width_px))
            .p(px(8.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .child(clock.clone().unwrap_or_else(|| "--:--".into())),
            )
            .child(separator(style))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(style.muted)
                    .child(date.clone().unwrap_or_else(|| "Date unavailable".into())),
            )
            .child(
                if let Some(battery) = props
                    .battery
                    .as_ref()
                    .map(|battery| render_vertical_battery(battery, style))
                {
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(separator(style))
                        .child(battery)
                } else {
                    div()
                },
            )
    };

    let resource_stack = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(10.))
        .children(resource_nodes);
    let resources_group = div()
        .w(px(style.group_width_px))
        .p(px(8.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .child(resource_stack)
        .child(separator(style))
        .child(media_node);
    let workspaces_group = div()
        .w(px(style.group_width_px))
        .p(px(6.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .children(workspace_nodes)
        .child(if props.workspaces.is_empty() {
            div()
                .text_size(px(10.))
                .text_color(style.muted)
                .child("No workspaces")
        } else {
            div()
        });
    let taskbar_group = div()
        .w(px(style.group_width_px))
        .p(px(4.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .children(taskbar_nodes)
        .child(if props.taskbar_items.is_empty() {
            div()
                .text_size(px(10.))
                .text_color(style.muted)
                .child("No app data")
        } else {
            div()
        });

    // Keep only visible groups in the stack. QML's Column excludes invisible
    // BarGroups rather than reserving a spacing slot for them.
    let mut middle_groups = Vec::new();
    if props.taskbar_enabled {
        middle_groups.push(clock_group().into_any_element());
    }
    if !props.taskbar_enabled {
        middle_groups.push(resources_group.into_any_element());
    }
    if props.borderless {
        middle_groups.push(separator(style).into_any_element());
    }
    middle_groups.push(workspaces_group.into_any_element());
    if props.taskbar_enabled && props.borderless {
        middle_groups.push(separator(style).into_any_element());
    }
    if props.taskbar_enabled {
        // QML caps the taskbar to middleHost after fixed middle groups.
        let available_height = f32::from(window.bounds().size.height).max(0.0);
        let taskbar_cap = taskbar_viewport_height(available_height, props);
        middle_groups.push(
            div()
                .id("ii.vertical-taskbar.scroll")
                .h(px(taskbar_cap))
                .min_h_0()
                .overflow_y_scroll()
                .child(taskbar_group)
                .into_any_element(),
        );
    }
    if props.borderless {
        middle_groups.push(separator(style).into_any_element());
    }
    if !props.taskbar_enabled {
        middle_groups.push(clock_group().into_any_element());
    }

    let middle_stack = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(style.middle_spacing_px))
        .children(middle_groups);

    let bar = material::surface(&props.material, style.surface, style.radius_px)
        .w(px(style.width_px))
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .text_color(style.foreground)
        // QML centers middle content within the space between natural edge sections.
        .child(div().h(px(style.top_section_px)))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .child(middle_stack),
        )
        .child(div().h(px(style.bottom_section_px)));
    bar
}

fn vertical_workspace_accessible_name(slot: &WorkspaceSlot, number: u8) -> String {
    match (slot.active, slot.occupied) {
        (true, true) => format!("Workspace {number}, active, has windows"),
        (true, false) => format!("Workspace {number}, active, empty"),
        (false, true) => format!("Workspace {number}, has windows"),
        (false, false) => format!("Workspace {number}, empty"),
    }
}

fn render_vertical_media(
    media: Option<&MediaState>,
    style: VerticalBarStyle,
    on_action: Option<Rc<dyn Fn(MediaAction)>>,
) -> impl IntoElement {
    let Some(media) = media else {
        return div()
            .w_full()
            .h(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .id("vertical-media-status")
            .role(Role::Status)
            .aria_label("Media status unavailable")
            .font_family("Material Symbols Rounded")
            .text_color(style.muted)
            .child("music_off");
    };

    let (title, detail, icon, status_label) = match media.status {
        MediaStatus::Unavailable => (
            "Media unavailable",
            "Player service unavailable",
            "music_off",
            "unavailable",
        ),
        MediaStatus::NoPlayer => ("No media", "No active player", "music_note", "no player"),
        MediaStatus::Playing => (
            media.title.as_deref().unwrap_or("No media"),
            media.artist.as_deref().unwrap_or("Unknown artist"),
            "pause",
            "playing",
        ),
        MediaStatus::Paused => (
            media.title.as_deref().unwrap_or("No media"),
            media.artist.as_deref().unwrap_or("Unknown artist"),
            "music_note",
            "paused",
        ),
        MediaStatus::Stopped => (
            media.title.as_deref().unwrap_or("No media"),
            media.artist.as_deref().unwrap_or("Unknown artist"),
            "music_note",
            "stopped",
        ),
    };
    let progress = media.position_percent.map(|value| value.min(100));
    let has_active_player = matches!(
        media.status,
        MediaStatus::Playing | MediaStatus::Paused | MediaStatus::Stopped
    );
    let ring_color = style.accent;
    let track_color = style.muted.opacity(0.55);
    let progress_ring = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = point(bounds.origin.x + px(10.), bounds.origin.y + px(10.));
            let radius = px(8.);
            let make_path = |sweep: f64| {
                let angle = sweep.to_radians();
                let end = point(
                    center.x + px(angle.sin() as f32 * 16.),
                    center.y - px(angle.cos() as f32 * 16.),
                );
                let mut path = PathBuilder::stroke(px(1.5));
                path.move_to(point(center.x, center.y - radius));
                path.arc_to(point(radius, radius), px(0.), sweep > 180.0, true, end);
                path.build().ok()
            };
            if let Some(path) = make_path(359.9) {
                window.paint_path(path, track_color);
            }
            if let Some(progress) = progress.filter(|value| *value > 0) {
                if let Some(path) = make_path(f64::from(progress) * 3.6) {
                    window.paint_path(path, ring_color);
                }
            }
        },
    )
    .w(px(20.))
    .h(px(20.));

    let mut group = div()
        .w_full()
        .id("vertical-media-group")
        .flex()
        .flex_col()
        .items_center()
        .gap(px(3.))
        .role(Role::Group)
        .aria_label(format!("Media {status_label}: {title}, {detail}"))
        .child(
            div()
                .relative()
                .id("vertical-media-open-controls")
                .role(Role::Button)
                .aria_label("Open media controls")
                .aria_description(format!(
                    "Playback progress {}",
                    progress
                        .map(|v| format!("{v}%"))
                        .unwrap_or_else(|| "unavailable".into())
                ))
                .focusable()
                .tab_stop(true)
                .w(px(20.))
                .h(px(20.))
                .child(progress_ring)
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_family("Material Symbols Rounded")
                        .text_size(px(14.))
                        .text_color(style.foreground)
                        .child(icon),
                )
                .when_some(on_action.clone(), |this, action| {
                    let click_action = action.clone();
                    let mut this = this.on_click(move |_, _, _| {
                        click_action(MediaAction::OpenControls);
                    });
                    let key_action = action.clone();
                    this = this.on_key_down(move |event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                            && !event.is_held
                        {
                            key_action(MediaAction::OpenControls);
                            cx.stop_propagation();
                        }
                    });
                    if has_active_player && media.can_toggle_playback {
                        let middle_action = action.clone();
                        this = this.on_mouse_down(MouseButton::Middle, move |_, _, _| {
                            middle_action(MediaAction::TogglePlayback);
                        });
                    }
                    if has_active_player && media.can_previous {
                        let back_action = action.clone();
                        this = this.on_mouse_down(
                            MouseButton::Navigate(NavigationDirection::Back),
                            move |_, _, _| back_action(MediaAction::Previous),
                        );
                    }
                    if has_active_player && media.can_next {
                        let forward_action = action.clone();
                        this = this.on_mouse_down(
                            MouseButton::Navigate(NavigationDirection::Forward),
                            move |_, _, _| forward_action(MediaAction::Next),
                        );
                        let right_action = action.clone();
                        this = this.on_mouse_down(MouseButton::Right, move |_, _, _| {
                            right_action(MediaAction::Next);
                        });
                    }
                    this
                }),
        )
        .child(
            div()
                .max_w(px(style.group_width_px - 8.))
                .text_size(px(9.))
                .text_color(style.foreground)
                .child(title.to_owned()),
        )
        .child(
            div()
                .text_size(px(9.))
                .text_color(if media.status == MediaStatus::Playing {
                    style.accent
                } else {
                    style.muted
                })
                .child(status_label),
        );
    if let Some(action) = on_action.filter(|_| {
        matches!(
            media.status,
            MediaStatus::Playing | MediaStatus::Paused | MediaStatus::Stopped
        ) && media.can_change_volume
    }) {
        let volume_action = action.clone();
        group = group.on_scroll_wheel(move |event, _, _| {
            let y = match event.delta {
                ScrollDelta::Pixels(delta) => f32::from(delta.y),
                ScrollDelta::Lines(delta) => delta.y,
            };
            if y != 0.0 {
                volume_action(MediaAction::VolumeDelta(if y > 0.0 { 0.05 } else { -0.05 }));
            }
        });
    }
    group
}

fn render_vertical_battery(battery: &BatteryState, style: VerticalBarStyle) -> impl IntoElement {
    if battery.status == BatteryStatus::Unavailable {
        return div()
            .w(px(36.))
            .id("vertical-battery-group")
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .role(Role::Status)
            .aria_label("Battery status unavailable")
            .child(
                div()
                    .font_family("Material Symbols Rounded")
                    .text_size(px(16.))
                    .text_color(style.muted)
                    .child("battery_unknown"),
            )
            .child(div().text_size(px(9.)).text_color(style.muted).child("—"));
    }
    let percent = battery.percentage.map(|value| value.min(100));
    let charging = battery.charge == BatteryChargeState::Charging;
    let low_warning =
        !charging && percent.is_some_and(|value| f64::from(value) <= battery.low_threshold);
    let state_label = match battery.charge {
        BatteryChargeState::Charging => "charging",
        BatteryChargeState::Discharging => "discharging",
        BatteryChargeState::Critical => "critical",
        BatteryChargeState::Full => "full",
        BatteryChargeState::Unknown => "charge state unknown",
    };
    let display_percent = percent
        .map(|value| value.to_string())
        .unwrap_or_else(|| "—".into());
    let accessible_percent = percent
        .map(|value| format!("{value}%"))
        .unwrap_or_else(|| "unknown".into());
    let icon = if charging {
        "bolt"
    } else {
        "battery_android_full"
    };
    let ink = if low_warning {
        style.battery_error
    } else {
        style.battery_ink
    };
    let fill_overlay = if low_warning {
        style.battery_error_overlay
    } else {
        style.battery_overlay
    };
    let track = ink.opacity(0.5);
    let track_overlay =
        battery_contrast_overlay_on_track(style.battery_overlay, track, style.surface);
    let fill_overlay = battery_contrast_overlay_on_fill(fill_overlay, ink, style.surface);
    let fill_height = percent.map(|value| 40.0 * f32::from(value) / 100.0);

    let mut indicator = div()
        .relative()
        .id("vertical-battery-level")
        .w(px(21.))
        .h(px(40.))
        .role(Role::ProgressIndicator)
        .aria_orientation(gpui::Orientation::Vertical)
        .aria_label(format!("Battery level: {accessible_percent}"))
        .aria_min_numeric_value(0.0)
        .aria_max_numeric_value(100.0)
        .when_some(percent, |this, percent| {
            this.aria_numeric_value(f64::from(percent))
        })
        .rounded_full()
        .overflow_hidden()
        .bg(track)
        .child(if let Some(fill_height) = fill_height {
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(fill_height))
                .rounded_full()
                .bg(ink)
        } else {
            div()
        })
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .child(battery_glyph_canvas(icon, &display_percent, track_overlay)),
        );
    if let Some(fill_height) = fill_height {
        indicator = indicator.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(fill_height))
                .rounded_full()
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(40.))
                        .child(battery_glyph_canvas(icon, &display_percent, fill_overlay)),
                ),
        );
    }

    div()
        .w(px(21.))
        .id("vertical-battery-group")
        .flex()
        .flex_col()
        .items_center()
        .role(Role::Group)
        .aria_label(format!("Battery {accessible_percent}, {state_label}"))
        .child(indicator)
}

fn battery_glyph_canvas(icon: &str, display_percent: &str, color: Hsla) -> impl IntoElement {
    let icon = icon.to_owned();
    let display_percent = display_percent.to_owned();
    let percent_font_size = if display_percent.len() > 2 { 11. } else { 13. };
    canvas(
        move |_, window, _| {
            let mut icon_font = gpui::font("Material Symbols Rounded");
            icon_font.weight = FontWeight::MEDIUM;
            let icon_run = TextRun {
                len: icon.len(),
                font: icon_font,
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let mut percent_font = gpui::font(".SystemUIFont");
            percent_font.weight = if display_percent.len() > 2 {
                FontWeight::MEDIUM
            } else {
                FontWeight::SEMIBOLD
            };
            let percent_run = TextRun {
                len: display_percent.len(),
                font: percent_font,
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            (
                window
                    .text_system()
                    .shape_line(icon.into(), px(16.), &[icon_run], None),
                window.text_system().shape_line(
                    display_percent.into(),
                    px(percent_font_size),
                    &[percent_run],
                    None,
                ),
            )
        },
        move |bounds, (icon_line, percent_line): (ShapedLine, ShapedLine), window, cx| {
            let width = Some(bounds.size.width);
            let _ = icon_line.paint(
                point(bounds.origin.x, bounds.origin.y + px(3.)),
                px(16.),
                TextAlign::Center,
                width,
                window,
                cx,
            );
            let _ = percent_line.paint(
                point(bounds.origin.x, bounds.origin.y + px(19.)),
                px(15.),
                TextAlign::Center,
                width,
                window,
                cx,
            );
        },
    )
    .w(px(21.))
    .h(px(40.))
}

/// Keep the Material foreground/background pair when it has sufficient
/// contrast against the fill composited over its surface token. If the surface
/// token is translucent, the compositor's wallpaper backdrop is unknown, so
/// the 4.5:1 check is relative to the token color rather than arbitrary pixels.
fn battery_contrast_overlay_on_fill(preferred: Hsla, fill: Hsla, surface: Hsla) -> Hsla {
    let composite = composite_rgb(hsla_to_rgb(fill), fill.a, hsla_to_rgb(surface), surface.a);
    if foreground_contrast_ratio(preferred, composite) >= 4.5 {
        return preferred;
    }
    strongest_contrast_rgb(composite)
}

fn battery_contrast_overlay_on_track(preferred: Hsla, track: Hsla, surface: Hsla) -> Hsla {
    let composite = composite_rgb(hsla_to_rgb(track), track.a, hsla_to_rgb(surface), surface.a);
    if foreground_contrast_ratio(preferred, composite) >= 4.5 {
        return preferred;
    }
    strongest_contrast_rgb(composite)
}

fn foreground_contrast_ratio(foreground: Hsla, background: [f32; 3]) -> f32 {
    let visible_foreground = composite_rgb(hsla_to_rgb(foreground), foreground.a, background, 1.0);
    contrast_ratio_rgb(visible_foreground, background)
}

fn strongest_contrast_rgb(background: [f32; 3]) -> Hsla {
    let black = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0,
        a: 1.0,
    };
    let white = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 1.0,
    };
    if contrast_ratio_rgb(hsla_to_rgb(black), background)
        >= contrast_ratio_rgb(hsla_to_rgb(white), background)
    {
        black
    } else {
        white
    }
}

fn contrast_ratio_rgb(foreground: [f32; 3], background: [f32; 3]) -> f32 {
    let foreground = relative_luminance(foreground);
    let background = relative_luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

fn composite_rgb(
    foreground: [f32; 3],
    foreground_alpha: f32,
    background: [f32; 3],
    background_alpha: f32,
) -> [f32; 3] {
    let foreground_alpha = foreground_alpha.clamp(0.0, 1.0);
    let background_alpha = background_alpha.clamp(0.0, 1.0);
    let alpha = foreground_alpha + background_alpha * (1.0 - foreground_alpha);
    if alpha == 0.0 {
        return [0.0; 3];
    }
    std::array::from_fn(|index| {
        (foreground[index] * foreground_alpha
            + background[index] * background_alpha * (1.0 - foreground_alpha))
            / alpha
    })
}

fn relative_luminance(rgb: [f32; 3]) -> f32 {
    let linear = rgb.map(|channel| {
        let channel = channel.clamp(0.0, 1.0);
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
}

fn hsla_to_rgb(color: Hsla) -> [f32; 3] {
    if color.s == 0.0 {
        return [color.l; 3];
    }
    let q = if color.l < 0.5 {
        color.l * (1.0 + color.s)
    } else {
        color.l + color.s - color.l * color.s
    };
    let p = 2.0 * color.l - q;
    [
        hue_to_rgb(p, q, color.h + 1.0 / 3.0),
        hue_to_rgb(p, q, color.h),
        hue_to_rgb(p, q, color.h - 1.0 / 3.0),
    ]
}

fn hue_to_rgb(p: f32, q: f32, mut hue: f32) -> f32 {
    if hue < 0.0 {
        hue += 1.0;
    }
    if hue > 1.0 {
        hue -= 1.0;
    }
    if hue < 1.0 / 6.0 {
        p + (q - p) * 6.0 * hue
    } else if hue < 0.5 {
        q
    } else if hue < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - hue) * 6.0
    } else {
        p
    }
}

fn separator(style: VerticalBarStyle) -> gpui::Div {
    let margin = (style.base_bar_height_px.max(0.0) / 3.0).max(0.0);
    let line_width = (style.group_width_px - margin * 2.0).max(0.0);
    div()
        .w(px(line_width))
        .mx(px(margin))
        .h(px(1.))
        .bg(style.separator)
}

#[allow(dead_code)]
fn _surface_state_notes() {
    // `VerticalBar.qml`: panel is gated by barOpen, unlocked, and not editing;
    // side is selected by bar.bottom; auto-hide, screen filtering/fallback,
    // layer-shell reservation and Super/hover reveal belong to the surface host.
    // `VerticalBarContent.qml`: taskbar mode replaces the resource group with
    // vertical app slots and places clock/date/battery in a second group.
}
