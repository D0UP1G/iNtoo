//! Minimal live Island root backed only by real shell data.
//!
//! The QML family remains the source of truth for Island's geometry, presets,
//! pieces and state contract. Palette app search uses the real XDG catalog.

use std::time::Duration;
use std::{
    collections::{BTreeMap, HashMap},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use gpui::Size;
use gpui::{
    Context, Entity, FocusHandle, Focusable, IntoElement, Render, Role, ScrollHandle, Window, div,
    prelude::*, px,
};

use crate::families::waffle::taskbar::{
    start_menu_adapter::{StartMenuDataSource, desktop_entries::XdgDesktopEntrySource},
    start_menu_search::StartMenuSearch,
};
use crate::{niri, theme::ThemeTokens};

use super::chassis::{
    self, IrisChassisGeometry, IrisChassisInput, IrisChassisStyle, IrisEdge, IslandPresentation,
};
use super::dock::{self, IrisDockEdge, IrisDockGeometry, IrisDockInput, IrisDockStyle};
use super::orbit::{self, OrbitInput, OrbitStatus, OrbitStyle, OrbitWindow, OrbitWorkspace};
use super::palette::{self, PaletteInput, PaletteStyle};
use super::palette_adapter;

pub struct IrisRuntime {
    style: IrisChassisStyle,
    clock: Option<String>,
    date: Option<String>,
    expanded: bool,
    notch: bool,
    full_width: bool,
    page_width: f32,
    position: IrisEdge,
    unavailable: bool,
    palette_style: Option<PaletteStyle>,
    palette_open: bool,
    palette_source: Option<Arc<XdgDesktopEntrySource>>,
    palette_query_action: crate::families::waffle::taskbar::start_menu::QueryAction,
    palette_editor: Option<Entity<StartMenuSearch>>,
    palette_selected: Arc<AtomicUsize>,
    palette_opener_focus: Option<FocusHandle>,
    palette_focus: Option<FocusHandle>,
    palette_scroll: ScrollHandle,
    dock: Option<IrisDockInput>,
    output_name: Option<String>,
    orbit_style: Option<OrbitStyle>,
    orbit_open: bool,
    orbit_workspaces: Vec<OrbitWorkspace>,
    orbit_selected_workspace: Option<u64>,
    orbit_selected_window: Option<u64>,
    orbit_keys_swapped: bool,
    orbit_status: OrbitStatus,
    orbit_error: Option<String>,
    orbit_opener_focus: Option<FocusHandle>,
    orbit_content_focus: Option<FocusHandle>,
}

impl IrisRuntime {
    /// Check the same palette/preset resolution used by `new`, before the
    /// surface manager commits to creating a native layer window.
    pub fn can_resolve(theme: &ThemeTokens, options: Option<&serde_json::Value>) -> bool {
        let options = options.cloned().unwrap_or_default();
        let number = |key: &str, fallback: f32| {
            options
                .get(key)
                .and_then(serde_json::Value::as_f64)
                .map(|v| v as f32)
                .unwrap_or(fallback)
        };
        let height = number("height", 42.0).clamp(32.0, 96.0);
        let page_width = number("pageWidth", 440.0).clamp(280.0, 800.0);
        IrisChassisStyle::from_theme(
            theme,
            IrisChassisGeometry {
                rest_width: number("compactWidth", (page_width * 0.46).clamp(176.0, 224.0)),
                expanded_width: page_width,
                expanded_height: number("pageHeight", 300.0).clamp(180.0, 640.0),
                bar_height: height,
            },
        )
        .is_some()
    }

    /// Returns `None` when the supplied family theme is not Island or its
    /// source preset cannot be resolved. `options` may be QML's `iris.bar`
    /// object (the existing JSON config stores this under `iris.bar`).
    pub fn new(
        theme: ThemeTokens,
        options: Option<&serde_json::Value>,
        dock_options: Option<&serde_json::Value>,
        output_size: Size<gpui::Pixels>,
        dock_enabled: bool,
        output_name: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<Self> {
        let options = options.cloned().unwrap_or_default();
        let number = |key: &str, fallback: f32| {
            options
                .get(key)
                .and_then(serde_json::Value::as_f64)
                .map(|v| v as f32)
                .unwrap_or(fallback)
                .to_owned()
        };
        let text = |key: &str, fallback: &str| -> String {
            options
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or(fallback)
                .to_owned()
        };
        let height = number("height", 42.0).clamp(32.0, 96.0);
        let page_width = number("pageWidth", 440.0).clamp(280.0, 800.0);
        let orbit_keys_swapped =
            options.get("orbitKeys").and_then(serde_json::Value::as_str) == Some("swapped");
        let layout = text("layout", "island");
        let full_width = matches!(layout.as_str(), "full" | "menubar");
        let position = match text("position", "top").as_str() {
            "bottom" => IrisEdge::Bottom,
            "left" => IrisEdge::Left,
            "right" => IrisEdge::Right,
            _ => IrisEdge::Top,
        };
        // QML's compact target is dynamically measured from the visible clock
        // and configured pieces. Until font metrics and piece adapters exist,
        // the source page width and configured bar height bound the chassis.
        let rest_width = number("compactWidth", (page_width * 0.46).clamp(176.0, 224.0));
        let style = IrisChassisStyle::from_theme(
            &theme,
            IrisChassisGeometry {
                rest_width,
                expanded_width: page_width,
                expanded_height: number("pageHeight", 300.0).clamp(180.0, 640.0),
                bar_height: height,
            },
        )?;
        let palette_source = XdgDesktopEntrySource::new().map(Arc::new);
        let palette_selected = Arc::new(AtomicUsize::new(0));
        let palette_scroll = ScrollHandle::new();
        let palette_focus = cx.focus_handle();
        let orbit_opener_focus = cx.focus_handle();
        let orbit_content_focus = cx.focus_handle();
        let palette_query_action = palette_source
            .as_ref()
            .map(|source| {
                let source = Arc::clone(source);
                let selected = Arc::clone(&palette_selected);
                Arc::new(move |query| {
                    selected.store(0, Ordering::Relaxed);
                    source.query_changed(query);
                }) as crate::families::waffle::taskbar::start_menu::QueryAction
            })
            .unwrap_or_else(|| Arc::new(|_| {}));
        let palette_editor = palette_adapter::search_editor(
            String::new(),
            Arc::clone(&palette_query_action),
            palette_focus.clone(),
            cx,
        );
        let mut view = Self {
            style,
            palette_style: PaletteStyle::from_theme(&theme, page_width, 420.0),
            clock: None,
            date: None,
            expanded: false,
            notch: options
                .get("notch")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            full_width,
            page_width,
            position,
            unavailable: false,
            palette_open: false,
            palette_source,
            palette_query_action,
            palette_editor: Some(palette_editor),
            palette_selected,
            palette_opener_focus: Some(cx.focus_handle()),
            palette_focus: Some(palette_focus),
            palette_scroll,
            dock: if dock_enabled {
                let dock_options = dock_options.cloned().unwrap_or_default();
                let island_edge = position;
                let wanted_edge = dock_options
                    .get("position")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("auto");
                let dock_edge = match wanted_edge {
                    "top" => IrisDockEdge::Top,
                    "bottom" => IrisDockEdge::Bottom,
                    "left" => IrisDockEdge::Left,
                    "right" => IrisDockEdge::Right,
                    _ => match island_edge {
                        IrisEdge::Top => IrisDockEdge::Bottom,
                        IrisEdge::Bottom => IrisDockEdge::Top,
                        IrisEdge::Left => IrisDockEdge::Right,
                        IrisEdge::Right => IrisDockEdge::Left,
                    },
                };
                let dock_edge = match (dock_edge, island_edge) {
                    (IrisDockEdge::Top, IrisEdge::Top) => IrisDockEdge::Bottom,
                    (IrisDockEdge::Bottom, IrisEdge::Bottom) => IrisDockEdge::Top,
                    (IrisDockEdge::Left, IrisEdge::Left) => IrisDockEdge::Right,
                    (IrisDockEdge::Right, IrisEdge::Right) => IrisDockEdge::Left,
                    (edge, _) => edge,
                };
                IrisDockStyle::from_theme(&theme, 1.0).map(|style| IrisDockInput {
                    geometry: IrisDockGeometry {
                        width: f32::from(output_size.width),
                        height: f32::from(output_size.height),
                        edge: dock_edge,
                        icon_size: dock_options
                            .get("iconSize")
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(40.0) as f32,
                        notch: dock_options
                            .get("notch")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                    },
                    style,
                    show_launcher: dock_options
                        .get("launcher")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true),
                    apps: Vec::new(),
                    pieces: Vec::new(),
                    on_launch: None,
                    on_activate: None,
                    on_piece_activate: None,
                })
            } else {
                None
            },
            output_name: output_name.clone(),
            orbit_style: OrbitStyle::from_theme(
                &theme,
                page_width,
                (f32::from(output_size.height) * 0.62).clamp(260.0, 640.0),
            ),
            orbit_open: false,
            orbit_workspaces: Vec::new(),
            orbit_selected_workspace: None,
            orbit_selected_window: None,
            orbit_keys_swapped,
            orbit_status: OrbitStatus::Loading,
            orbit_error: None,
            orbit_opener_focus: Some(orbit_opener_focus),
            orbit_content_focus: Some(orbit_content_focus),
        };

        let dock_windows = Arc::new(Mutex::new(Vec::<niri::Window>::new()));
        let dock_catalog = XdgDesktopEntrySource::new().map(Arc::new);
        let activate_windows = Arc::clone(&dock_windows);
        if let Some(dock_input) = view.dock.as_mut() {
            dock_input.on_activate = Some(Arc::new(move |app_id| {
                let Ok(windows) = activate_windows.lock() else {
                    return;
                };
                let app_windows = windows
                    .iter()
                    .filter(|window| {
                        window
                            .app_id
                            .as_deref()
                            .is_some_and(|id| id.eq_ignore_ascii_case(&app_id))
                    })
                    .collect::<Vec<_>>();
                if app_windows.is_empty() {
                    return;
                }
                let current = app_windows.iter().position(|window| window.focused);
                let next = current.map_or(0, |index| (index + 1) % app_windows.len());
                let _ = niri::focus_window(app_windows[next].id);
            }));
        }
        let live_dock_windows = Arc::clone(&dock_windows);
        let orbit_output = output_name.clone();
        let orbit_output_size = [f32::from(output_size.width), f32::from(output_size.height)];
        cx.spawn(async move |this, cx| {
            loop {
                let output = orbit_output.clone();
                let snapshot = cx
                    .background_executor()
                    .spawn(async move { niri::snapshot(output.as_deref()) })
                    .await;
                if this
                    .update(cx, |view, cx| {
                        if let Ok(mut windows) = live_dock_windows.lock() {
                            *windows = snapshot.windows.clone();
                        }
                        if let Some(dock) = view.dock.as_mut() {
                            dock.apps = dock_catalog
                                .as_deref()
                                .map(|catalog| dock::apps_from_windows(&snapshot.windows, catalog))
                                .unwrap_or_default();
                        }
                        view.orbit_error = snapshot.error.clone();
                        view.orbit_status = if snapshot.error.is_some() {
                            OrbitStatus::Error
                        } else if view.output_name.is_none() {
                            OrbitStatus::Unavailable
                        } else {
                            OrbitStatus::Ready
                        };
                        view.orbit_workspaces = snapshot
                            .workspaces
                            .iter()
                            .map(|workspace| OrbitWorkspace {
                                id: workspace.id,
                                index: i32::from(workspace.number),
                                name: workspace.name.clone(),
                                active: workspace.active,
                                windows: {
                                    let workspace_windows = snapshot
                                        .windows
                                        .iter()
                                        .filter(|window| window.workspace_id == Some(workspace.id))
                                        .collect::<Vec<_>>();
                                    let orbit_bounds =
                                        orbit_layout_bounds(&workspace_windows, orbit_output_size);
                                    workspace_windows
                                        .into_iter()
                                        .map(|window| OrbitWindow {
                                            id: window.id,
                                            app_id: window.app_id.clone(),
                                            app_name: None,
                                            title: window.title.clone(),
                                            workspace_id: workspace.id,
                                            floating: window.floating.unwrap_or(false),
                                            bounds: orbit_bounds.get(&window.id).copied().flatten(),
                                            preview_uri: None,
                                            icon_glyph: None,
                                            focused: window.focused,
                                            urgent: window.urgent,
                                            stashed: false,
                                            recent: false,
                                        })
                                        .collect()
                                },
                            })
                            .collect();
                        view.reconcile_orbit_selection(false);
                        if let Some(value) = snapshot.clock {
                            let mut parts = value.splitn(2, " · ");
                            view.clock = parts.next().map(str::to_owned);
                            view.date = parts.next().map(str::to_owned);
                        } else {
                            view.clock = None;
                            view.date = None;
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
        .detach();

        Some(view)
    }

    /// Renderless emergency state used only if construction fails after the
    /// surface manager's preflight. Callers must log this fallback.
    pub fn unavailable() -> Self {
        Self {
            style: IrisChassisStyle::default(),
            clock: None,
            date: None,
            expanded: false,
            notch: false,
            full_width: false,
            page_width: 0.0,
            position: IrisEdge::Top,
            unavailable: true,
            palette_style: None,
            palette_open: false,
            palette_source: None,
            palette_query_action: Arc::new(|_| {}),
            palette_editor: None,
            palette_selected: Arc::new(AtomicUsize::new(0)),
            palette_opener_focus: None,
            palette_focus: None,
            palette_scroll: ScrollHandle::new(),
            dock: None,
            output_name: None,
            orbit_style: None,
            orbit_open: false,
            orbit_workspaces: Vec::new(),
            orbit_selected_workspace: None,
            orbit_selected_window: None,
            orbit_keys_swapped: false,
            orbit_status: OrbitStatus::Unavailable,
            orbit_error: None,
            orbit_opener_focus: None,
            orbit_content_focus: None,
        }
    }

    pub fn presentation(&self) -> IslandPresentation {
        if self.expanded {
            IslandPresentation::Expanded
        } else {
            IslandPresentation::Rest
        }
    }

    fn toggle(&mut self, cx: &mut Context<Self>) {
        self.expanded = !self.expanded;
        cx.notify();
    }

    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_open = true;
        self.palette_selected.store(0, Ordering::Relaxed);
        if let Some(editor) = &self.palette_editor {
            window.focus(&editor.focus_handle(cx), cx);
        }
        cx.notify();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_open = false;
        self.palette_selected.store(0, Ordering::Relaxed);
        if let Some(source) = &self.palette_source {
            source.close();
        }
        if let Some(opener) = &self.palette_opener_focus {
            window.focus(opener, cx);
        }
        cx.notify();
    }

    fn close_orbit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.orbit_open = false;
        if let Some(opener) = &self.orbit_opener_focus {
            window.focus(opener, cx);
        }
        cx.notify();
    }

    fn select_orbit_workspace(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(workspace) = self
            .orbit_workspaces
            .iter()
            .find(|workspace| workspace.id == id)
        else {
            return;
        };
        let window_id = workspace
            .windows
            .iter()
            .find(|window| window.focused)
            .or_else(|| workspace.windows.first())
            .map(|window| window.id);
        if self.orbit_selected_workspace != Some(id) || self.orbit_selected_window != window_id {
            self.orbit_selected_workspace = Some(id);
            self.orbit_selected_window = window_id;
            cx.notify();
        }
    }

    fn select_orbit_window(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(workspace_id) = self
            .orbit_workspaces
            .iter()
            .find(|workspace| workspace.windows.iter().any(|window| window.id == id))
            .map(|workspace| workspace.id)
        else {
            return;
        };
        if self.orbit_selected_workspace != Some(workspace_id)
            || self.orbit_selected_window != Some(id)
        {
            self.orbit_selected_workspace = Some(workspace_id);
            self.orbit_selected_window = Some(id);
            cx.notify();
        }
    }

    fn reconcile_orbit_selection(&mut self, reset: bool) {
        if self.orbit_workspaces.is_empty() {
            self.orbit_selected_workspace = None;
            self.orbit_selected_window = None;
            return;
        }
        let selected_exists = self.orbit_selected_workspace.is_some_and(|id| {
            self.orbit_workspaces
                .iter()
                .any(|workspace| workspace.id == id)
        });
        if reset || !selected_exists {
            let workspace = self
                .orbit_workspaces
                .iter()
                .find(|workspace| workspace.active)
                .unwrap_or(&self.orbit_workspaces[0]);
            self.orbit_selected_workspace = Some(workspace.id);
            self.orbit_selected_window = workspace
                .windows
                .iter()
                .find(|window| window.focused)
                .or_else(|| workspace.windows.first())
                .map(|window| window.id);
            return;
        }
        let Some(workspace) = self
            .orbit_workspaces
            .iter()
            .find(|workspace| Some(workspace.id) == self.orbit_selected_workspace)
        else {
            return;
        };
        let window_exists = self
            .orbit_selected_window
            .is_some_and(|id| workspace.windows.iter().any(|window| window.id == id));
        if !window_exists {
            self.orbit_selected_window = workspace
                .windows
                .iter()
                .find(|window| window.focused)
                .or_else(|| workspace.windows.first())
                .map(|window| window.id);
        }
    }

    fn handle_orbit_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.orbit_open {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.close_orbit(window, cx);
            cx.stop_propagation();
            return;
        }
        if event.is_held && (key == "enter" || key == "return") {
            return;
        }
        let workspace_prev = if self.orbit_keys_swapped {
            "left"
        } else {
            "up"
        };
        let workspace_next = if self.orbit_keys_swapped {
            "right"
        } else {
            "down"
        };
        let window_prev = if self.orbit_keys_swapped {
            "up"
        } else {
            "left"
        };
        let window_next = if self.orbit_keys_swapped {
            "down"
        } else {
            "right"
        };
        let current_workspace = self
            .orbit_selected_workspace
            .and_then(|id| self.orbit_workspaces.iter().position(|ws| ws.id == id))
            .unwrap_or(0);
        let next_workspace = match key {
            key if key == workspace_prev => Some(current_workspace.saturating_sub(1)),
            key if key == workspace_next => {
                Some((current_workspace + 1).min(self.orbit_workspaces.len().saturating_sub(1)))
            }
            _ => None,
        };
        if let Some(index) = next_workspace {
            if let Some(workspace) = self.orbit_workspaces.get(index) {
                let workspace_id = workspace.id;
                let window_id = workspace
                    .windows
                    .iter()
                    .find(|window| window.focused)
                    .or_else(|| workspace.windows.first())
                    .map(|window| window.id);
                if self.orbit_selected_workspace != Some(workspace_id)
                    || self.orbit_selected_window != window_id
                {
                    self.orbit_selected_workspace = Some(workspace_id);
                    self.orbit_selected_window = window_id;
                    cx.notify();
                }
            }
            cx.stop_propagation();
            return;
        }
        if key == "tab" || key == "backtab" || key == window_prev || key == window_next {
            let windows = self
                .orbit_selected_workspace
                .and_then(|id| self.orbit_workspaces.iter().find(|ws| ws.id == id))
                .map(|ws| &ws.windows);
            if let Some(windows) = windows.filter(|windows| !windows.is_empty()) {
                let current = self
                    .orbit_selected_window
                    .and_then(|id| windows.iter().position(|window| window.id == id))
                    .unwrap_or(0);
                let backwards = key == "backtab"
                    || (key == "tab" && event.keystroke.modifiers.shift)
                    || key == window_prev;
                let forwards =
                    key == "tab" && !event.keystroke.modifiers.shift || key == window_next;
                let next = if backwards {
                    current.saturating_sub(1)
                } else if forwards {
                    (current + 1).min(windows.len() - 1)
                } else {
                    current
                };
                let next_window = windows.get(next).map(|window| window.id);
                if self.orbit_selected_window != next_window {
                    self.orbit_selected_window = next_window;
                    cx.notify();
                }
            }
            cx.stop_propagation();
            return;
        }
        if key == "enter" || key == "return" {
            if let Some(id) = self.orbit_selected_window {
                let _ = niri::focus_window(id);
            } else if let Some(id) = self.orbit_selected_workspace {
                if let Some(workspace) = self.orbit_workspaces.iter().find(|ws| ws.id == id) {
                    let _ = niri::focus_workspace(workspace.index.clamp(0, u8::MAX as i32) as u8);
                }
            }
            cx.stop_propagation();
        }
    }
}

/// Project Niri's reported tile geometry into normalized Orbit-card bounds.
/// Tiled windows use the real column/row allocation and tile sizes. Floating
/// windows use their output-local workspace-view position and tile size.
/// Missing or invalid fields intentionally produce `None` so Orbit can show
/// its explicitly-labelled grid fallback instead of implying a live layout.
fn orbit_layout_bounds(
    windows: &[&niri::Window],
    output_size: [f32; 2],
) -> HashMap<u64, Option<[f32; 4]>> {
    let without_geometry = || windows.iter().map(|window| (window.id, None)).collect();
    #[derive(Clone, Copy)]
    struct Tile {
        id: u64,
        col: u64,
        row: u64,
        size: [f32; 2],
    }

    let tiled = windows
        .iter()
        .filter(|window| window.floating == Some(false))
        .filter_map(|window| {
            Some(Tile {
                id: window.id,
                col: window.pos_in_scrolling_layout?[0],
                row: window.pos_in_scrolling_layout?[1],
                size: window.tile_size?,
            })
        })
        .collect::<Vec<_>>();
    let mut col_widths = BTreeMap::<u64, f32>::new();
    let mut row_heights = BTreeMap::<(u64, u64), f32>::new();
    for tile in &tiled {
        col_widths
            .entry(tile.col)
            .and_modify(|width| *width = width.max(tile.size[0]))
            .or_insert(tile.size[0]);
        row_heights
            .entry((tile.col, tile.row))
            .and_modify(|height| *height = height.max(tile.size[1]))
            .or_insert(tile.size[1]);
    }

    let mut col_offsets = BTreeMap::<u64, f32>::new();
    let mut total_width = 0.0f32;
    for (col, width) in &col_widths {
        let previous_width = total_width;
        col_offsets.insert(*col, previous_width);
        total_width += *width;
        if !total_width.is_finite() || total_width <= previous_width {
            return without_geometry();
        }
    }
    let mut row_offsets = BTreeMap::<(u64, u64), f32>::new();
    let mut tiled_height = 0.0f32;
    for col in col_widths.keys().copied() {
        let mut offset = 0.0f32;
        for ((row_col, row), height) in &row_heights {
            if *row_col == col {
                let previous_height = offset;
                row_offsets.insert((col, *row), previous_height);
                offset += *height;
                if !offset.is_finite() || offset <= previous_height {
                    return without_geometry();
                }
            }
        }
        tiled_height = tiled_height.max(offset);
    }

    let mut bounds = HashMap::with_capacity(windows.len());
    for window in windows {
        if window.floating == Some(true) {
            let placement = window
                .tile_pos_in_workspace_view
                .zip(window.tile_size)
                .filter(|_| output_size[0].is_finite() && output_size[1].is_finite())
                .filter(|_| output_size[0] > 0.0 && output_size[1] > 0.0)
                .map(|(pos, size)| {
                    [
                        pos[0] / output_size[0],
                        pos[1] / output_size[1],
                        size[0] / output_size[0],
                        size[1] / output_size[1],
                    ]
                })
                .filter(|rect| valid_orbit_bounds(*rect));
            bounds.insert(window.id, placement);
        } else if window.floating == Some(false) {
            let placement = tiled
                .iter()
                .find(|tile| tile.id == window.id)
                .and_then(|tile| {
                    if total_width <= 0.0 || tiled_height <= 0.0 {
                        return None;
                    }
                    let x = *col_offsets.get(&tile.col)?;
                    let y = *row_offsets.get(&(tile.col, tile.row))?;
                    Some([
                        x / total_width,
                        y / tiled_height,
                        tile.size[0] / total_width,
                        tile.size[1] / tiled_height,
                    ])
                })
                .filter(|rect| valid_orbit_bounds(*rect));
            bounds.insert(window.id, placement);
        } else {
            bounds.insert(window.id, None);
        }
    }
    bounds
}

fn valid_orbit_bounds([x, y, width, height]: [f32; 4]) -> bool {
    [x, y, width, height].iter().all(|value| value.is_finite())
        && x >= 0.0
        && y >= 0.0
        && width > 0.0
        && height > 0.0
        && x + width <= 1.0
        && y + height <= 1.0
}

impl Render for IrisRuntime {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.unavailable {
            return div().size_full().into_any_element();
        }
        let input = IrisChassisInput {
            style: self.style.clone(),
            edge: self.position,
            presentation: self.presentation(),
            transition: if self.expanded { 1.0 } else { 0.0 },
            notch: self.notch,
            full_width: self.full_width,
            clock: self.clock.clone(),
            date: self.date.clone(),
            primary_activity: None,
            secondary_activity: None,
            page_title: Some("Island".into()),
            page_detail: Some(
                "Application search is available; other search providers are not connected yet."
                    .into(),
            ),
            leading_pieces: Vec::new(),
            trailing_pieces: Vec::new(),
        };
        let toggle = cx.listener(|this, _, _, cx| this.toggle(cx));
        let palette_toggle = cx.listener(|this, _, window, cx| {
            if this.palette_open {
                this.close_palette(window, cx);
            } else {
                this.open_palette(window, cx);
            }
        });
        let orbit_toggle = cx.listener(|this, _, window, cx| {
            if this.orbit_open {
                this.close_orbit(window, cx);
            } else {
                this.reconcile_orbit_selection(true);
                this.orbit_open = true;
                if let Some(focus) = &this.orbit_content_focus {
                    window.focus(focus, cx);
                }
                cx.notify();
            }
        });
        let key_toggle = cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" && this.palette_open {
                this.close_palette(window, cx);
            } else if event.keystroke.key == "escape" && this.orbit_open {
                this.close_orbit(window, cx);
                cx.stop_propagation();
            } else if !event.is_held
                && (event.keystroke.key == "enter" || event.keystroke.key == "space")
            {
                this.toggle(cx);
            }
        });
        let palette_key_toggle = cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
            if !event.is_held && (event.keystroke.key == "enter" || event.keystroke.key == "space")
            {
                if this.palette_open {
                    this.close_palette(window, cx);
                } else {
                    this.open_palette(window, cx);
                }
            }
        });
        let palette_keys = cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
            if !this.palette_open || event.is_held {
                return;
            }
            let snapshot = palette_adapter::snapshot(this.palette_source.as_ref());
            if event.keystroke.key == "escape" {
                this.close_palette(window, cx);
                cx.stop_propagation();
                return;
            }
            let query_is_current = this
                .palette_editor
                .as_ref()
                .is_some_and(|editor| editor.read(cx).confirms_query(&snapshot.query));
            if !query_is_current || snapshot.status == super::palette::PaletteStatus::Loading {
                return;
            }
            let result_count = snapshot
                .groups
                .iter()
                .map(|group| group.results.len())
                .sum::<usize>();
            match event.keystroke.key.as_str() {
                "down" if result_count > 0 => {
                    let index = this.palette_selected.load(Ordering::Relaxed);
                    this.palette_selected
                        .store((index + 1).min(result_count - 1), Ordering::Relaxed);
                    cx.notify();
                }
                "up" if result_count > 0 => {
                    let index = this.palette_selected.load(Ordering::Relaxed);
                    this.palette_selected
                        .store(index.saturating_sub(1), Ordering::Relaxed);
                    cx.notify();
                }
                "enter" if result_count > 0 => {
                    let index = this
                        .palette_selected
                        .load(Ordering::Relaxed)
                        .min(result_count - 1);
                    if let Some(result) = snapshot
                        .groups
                        .iter()
                        .flat_map(|group| &group.results)
                        .nth(index)
                    {
                        if let Some(source) = &this.palette_source {
                            let _ = source.launch_app(result.id.clone());
                        }
                        this.close_palette(window, cx);
                    }
                }
                _ => return,
            }
            cx.stop_propagation();
        });
        let orbit_keys = cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
            this.handle_orbit_key(event, window, cx);
        });

        let palette_data = palette_adapter::snapshot(self.palette_source.as_ref());
        let palette_editor = self.palette_editor.clone();
        if let Some(editor) = &palette_editor {
            let query = palette_data.query.clone();
            let query_action = Arc::clone(&self.palette_query_action);
            editor.update(cx, |editor, cx| {
                editor.sync_controlled_query(query, query_action, cx);
            });
        }
        let result_action = self
            .palette_source
            .as_ref()
            .map(|source| palette_adapter::result_action(Arc::clone(source)))
            .unwrap_or_else(|| Arc::new(|_| {}));
        let selected = self.palette_selected.load(Ordering::Relaxed);
        let result_count = palette_data
            .groups
            .iter()
            .map(|group| group.results.len())
            .sum::<usize>();
        if result_count == 0 {
            self.palette_selected.store(0, Ordering::Relaxed);
        } else if selected >= result_count {
            self.palette_selected
                .store(result_count - 1, Ordering::Relaxed);
        }
        let palette_input = self.palette_style.as_ref().map(|style| PaletteInput {
            style: style.clone(),
            query: palette_data.query,
            placeholder: "Search applications".to_owned(),
            groups: palette_data.groups,
            active_index: (result_count > 0).then_some(selected.min(result_count - 1)),
            hovered_index: None,
            status: palette_data.status,
            status_message: palette_data.status_message,
            on_result: result_action,
        });
        let runtime = cx.entity();
        let on_result_dismiss = std::rc::Rc::new(move |window: &mut Window, cx: &mut gpui::App| {
            runtime.update(cx, |this, cx| this.close_palette(window, cx));
        });
        let editor_for_keys = palette_editor.clone();
        let on_palette_key = std::rc::Rc::new(
            move |event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut gpui::App| {
                if let Some(editor) = &editor_for_keys {
                    editor.update(cx, |editor, cx| editor.handle_key_down(event, window, cx));
                }
            },
        );
        let palette_scroll = self.palette_scroll.clone();
        let palette_focus = self.palette_focus.clone();
        let orbit_entity = cx.entity();
        let select_workspace_entity = orbit_entity.clone();
        let on_select_workspace = Rc::new(move |id, _window: &mut Window, cx: &mut gpui::App| {
            select_workspace_entity.update(cx, |this, cx| this.select_orbit_workspace(id, cx));
        });
        let select_window_entity = orbit_entity.clone();
        let on_select_window = Rc::new(move |id, _window: &mut Window, cx: &mut gpui::App| {
            select_window_entity.update(cx, |this, cx| this.select_orbit_window(id, cx));
        });
        let key_entity = orbit_entity.clone();
        let on_orbit_key = Rc::new(
            move |event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut gpui::App| {
                key_entity.update(cx, |this, cx| this.handle_orbit_key(event, window, cx));
            },
        );
        let orbit_input = self.orbit_style.map(|style| OrbitInput {
            style,
            output_name: self
                .output_name
                .clone()
                .unwrap_or_else(|| "Unknown output".into()),
            status: self.orbit_status,
            status_message: self.orbit_error.clone(),
            workspaces: self.orbit_workspaces.clone(),
            selected_workspace: self.orbit_selected_workspace,
            selected_window: self.orbit_selected_window,
            query: String::new(),
            previews_enabled: false,
            show_recent: false,
            show_stashed: false,
            on_select_workspace: Some(on_select_workspace),
            on_activate_workspace: Some(Arc::new(|id| {
                if let Some(workspace) = niri::snapshot(None)
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.id == id)
                {
                    let _ = niri::focus_workspace(workspace.number);
                }
            })),
            on_activate_window: Some(Arc::new(|id| {
                let _ = niri::focus_window(id);
            })),
            on_select_window: Some(on_select_window),
            on_close_window: Some(Arc::new(|id| {
                let _ = niri::close_window(id);
            })),
            on_key: Some(on_orbit_key),
        });
        let mut root = div()
            .id("iris.runtime")
            .w_full()
            .min_w(px(self.page_width.min(800.0)))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .on_key_down(palette_keys)
            .on_key_down(orbit_keys)
            .child(
                div()
                    .id("iris.runtime.chassis")
                    .w_full()
                    .focusable()
                    .tab_stop(true)
                    .role(Role::Button)
                    .aria_label(if self.expanded {
                        "Island expanded"
                    } else {
                        "Island"
                    })
                    .aria_expanded(self.expanded)
                    .on_click(toggle)
                    .on_key_down(key_toggle)
                    .child(chassis::render(input)),
            )
            .child(
                div()
                    .id("iris.runtime.palette_toggle")
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(12.0))
                    .bg(self.style.piece)
                    .text_color(self.style.foreground)
                    .focusable()
                    .tab_stop(true)
                    .track_focus(
                        self.palette_opener_focus
                            .as_ref()
                            .expect("runtime has focus handle"),
                    )
                    .role(Role::Button)
                    .aria_label(if self.palette_open {
                        "Close search palette"
                    } else {
                        "Open search palette"
                    })
                    .aria_expanded(self.palette_open)
                    .on_click(palette_toggle)
                    .on_key_down(palette_key_toggle)
                    .child(if self.palette_open {
                        "Close search"
                    } else {
                        "Search"
                    }),
            )
            .child(
                div()
                    .id("iris.runtime.orbit_toggle")
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(12.0))
                    .bg(self.style.piece)
                    .text_color(self.style.foreground)
                    .focusable()
                    .tab_stop(true)
                    .track_focus(
                        self.orbit_opener_focus
                            .as_ref()
                            .expect("runtime has Orbit focus handle"),
                    )
                    .role(Role::Button)
                    .aria_label(if self.orbit_open {
                        "Close Orbit"
                    } else {
                        "Open Orbit"
                    })
                    .aria_expanded(self.orbit_open)
                    .on_click(orbit_toggle)
                    .child(if self.orbit_open {
                        "Close Orbit"
                    } else {
                        "Orbit"
                    }),
            )
            .when(self.palette_open, |this| {
                if let (Some(input), Some(editor)) = (palette_input, palette_editor) {
                    this.child(palette::render_with_editor(
                        input,
                        Some(editor),
                        Some(palette_focus.expect("available runtime has palette focus")),
                        palette_scroll,
                        on_result_dismiss,
                        on_palette_key,
                    ))
                } else {
                    this
                }
            })
            .when(self.orbit_open, |this| {
                if let Some(input) = orbit_input {
                    this.child(
                        div()
                            .id("iris.runtime.orbit_content")
                            .track_focus(
                                self.orbit_content_focus
                                    .as_ref()
                                    .expect("runtime has Orbit content focus handle"),
                            )
                            .role(Role::Group)
                            .aria_label("Iris Orbit controls")
                            .child(orbit::render(input)),
                    )
                } else {
                    this
                }
            })
            .into_any_element();
        if let Some(dock_input) = self.dock.clone() {
            root = div()
                .id("iris.runtime.output")
                .size_full()
                .child(root)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .child(dock::render(dock_input)),
                )
                .into_any_element();
        }
        root
    }
}

/// Explicit source/runtime gaps for callers and visual acceptance reports:
/// IrisFrame wallpaper/Lume material; compositor reservation and edge gates;
/// touch/hover gestures; dock and external docking; overlay and non-app palette search;
/// transitions. The current `page_detail` communicates the unconnected search.
pub fn runtime_gaps() -> [&'static str; 5] {
    [
        "wallpaper/Lume material",
        "compositor reservation",
        "gestures and edge reveal",
        "dock/docking",
        "overlay and non-app search services",
    ]
}
