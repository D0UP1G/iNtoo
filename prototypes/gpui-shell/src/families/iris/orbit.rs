//! Data-driven GPUI presentation for Iris Orbit workspaces and window tiles.
//!
//! Niri layout, previews, wallpaper, names, and actions are host inputs. The
//! view does not query shell state or synthesize live workspace/window data.

use std::{rc::Rc, sync::Arc};

use gpui::{
    AnyElement, FontWeight, Hsla, IntoElement, KeyDownEvent, Render, Role, SharedString, Window,
    div, prelude::*, px,
};

use crate::theme::ThemeTokens;

#[path = "style_tokens.rs"]
mod style_tokens;
use style_tokens::IrisPalette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrbitStatus {
    #[default]
    Ready,
    Loading,
    Unavailable,
    Error,
}

#[derive(Debug, Clone)]
pub struct OrbitWindow {
    pub id: u64,
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub title: Option<String>,
    pub workspace_id: u64,
    pub floating: bool,
    /// Output-local normalized placement (0..1); supplied from Niri layout.
    pub bounds: Option<[f32; 4]>,
    /// Already-resolved preview/icon for this exact window, if available.
    pub preview_uri: Option<String>,
    pub icon_glyph: Option<String>,
    pub focused: bool,
    pub urgent: bool,
    pub stashed: bool,
    pub recent: bool,
}

impl OrbitWindow {
    fn label(&self) -> String {
        self.title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .or(self.app_name.as_deref())
            .or(self.app_id.as_deref())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Window {}", self.id))
    }
}

#[derive(Debug, Clone)]
pub struct OrbitWorkspace {
    pub id: u64,
    pub index: i32,
    pub name: Option<String>,
    pub active: bool,
    pub windows: Vec<OrbitWindow>,
}

#[derive(Debug, Clone, Copy)]
pub struct OrbitStyle {
    pub palette: IrisPalette,
    pub radius: f32,
    pub tile_radius: f32,
    pub width: f32,
    pub height: f32,
    pub density: f32,
}

impl OrbitStyle {
    pub fn from_theme(theme: &ThemeTokens, width: f32, height: f32) -> Option<Self> {
        Some(Self {
            palette: IrisPalette::from_theme(theme)?,
            radius: 26.0,
            tile_radius: 12.0,
            width,
            height,
            density: theme.shape.density.clamp(0.8, 1.35),
        })
    }
}

pub type OrbitAction = Arc<dyn Fn(u64) + Send + Sync>;
pub type OrbitSelectAction = Rc<dyn Fn(u64, &mut Window, &mut gpui::App)>;
pub type OrbitKeyAction = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut gpui::App)>;

#[derive(Clone)]
pub struct OrbitInput {
    pub style: OrbitStyle,
    pub output_name: String,
    pub status: OrbitStatus,
    pub status_message: Option<String>,
    pub workspaces: Vec<OrbitWorkspace>,
    pub selected_workspace: Option<u64>,
    pub selected_window: Option<u64>,
    pub query: String,
    pub previews_enabled: bool,
    pub show_recent: bool,
    pub show_stashed: bool,
    pub on_select_workspace: Option<OrbitSelectAction>,
    pub on_activate_workspace: Option<OrbitAction>,
    pub on_select_window: Option<OrbitSelectAction>,
    pub on_activate_window: Option<OrbitAction>,
    pub on_close_window: Option<OrbitAction>,
    pub on_key: Option<OrbitKeyAction>,
}

pub struct IrisOrbitView {
    input: OrbitInput,
}

impl IrisOrbitView {
    pub fn new(input: OrbitInput) -> Self {
        Self { input }
    }
    pub fn set_input(&mut self, input: OrbitInput, cx: &mut gpui::Context<Self>) {
        self.input = input;
        cx.notify();
    }
}

impl Render for IrisOrbitView {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        render(self.input.clone())
    }
}

pub fn render(input: OrbitInput) -> impl IntoElement {
    let p = input.style.palette;
    let mut cards = Vec::<AnyElement>::new();
    let mut workspaces = input.workspaces.clone();
    workspaces.sort_by_key(|workspace| workspace.index);
    let active = input
        .selected_workspace
        .or_else(|| workspaces.iter().find(|ws| ws.active).map(|ws| ws.id));
    for workspace in workspaces {
        let selected = active == Some(workspace.id);
        let workspace_id = workspace.id;
        let activate_workspace = input.on_activate_workspace.clone();
        let select_workspace = input.on_select_workspace.clone();
        let title = workspace
            .name
            .clone()
            .unwrap_or_else(|| format!("Workspace {}", workspace.index));
        let mut tiles = Vec::<AnyElement>::new();
        let visible_windows = workspace
            .windows
            .iter()
            .filter(|window| {
                (!window.stashed || input.show_stashed)
                    && (input.query.trim().is_empty() || {
                        let q = input.query.to_lowercase();
                        window.label().to_lowercase().contains(&q)
                            || window
                                .app_id
                                .as_deref()
                                .unwrap_or_default()
                                .to_lowercase()
                                .contains(&q)
                    })
            })
            .collect::<Vec<_>>();
        let list_fallback = visible_windows.iter().any(|window| window.bounds.is_none());
        for (position, window) in visible_windows.iter().enumerate() {
            // A workspace has one geometry mode: if any visible window lacks
            // valid metadata, grid every visible window to avoid mixing
            // synthetic fallback positions with live Niri coordinates.
            let fallback_bounds = || {
                let count = visible_windows.len().max(1) as f32;
                let rows = count.min(4.0);
                let row = position as f32 % rows;
                let column = (position as f32 / rows).floor();
                let columns = (count / rows).ceil().max(1.0);
                [
                    0.035 + column * (0.93 / columns),
                    0.13 + row * (0.82 / rows),
                    0.90 / columns,
                    0.76 / rows,
                ]
            };
            let bounds = if list_fallback {
                fallback_bounds()
            } else if let Some(bounds) = window.bounds {
                bounds
            } else {
                fallback_bounds()
            };
            let label = window.label();
            let grid_accessible_status = if list_fallback {
                ", approximate fallback grid placement"
            } else {
                ""
            };
            let is_selected = input.selected_window == Some(window.id);
            let preview = input.previews_enabled && window.preview_uri.is_some();
            let icon = window.icon_glyph.clone().unwrap_or_else(|| "▣".into());
            let mut thumb = div()
                .id(format!("iris.orbit.window.{}", window.id))
                .absolute()
                .left(px(bounds[0] * input.style.width))
                .top(px(bounds[1] * input.style.height))
                .w(px(bounds[2] * input.style.width))
                .h(px(bounds[3] * input.style.height))
                .rounded(px(input.style.tile_radius))
                .border_2()
                .border_color(if is_selected { p.accent } else { p.hairline })
                .bg(if preview { p.material_high } else { p.fill })
                .text_color(p.text)
                .flex()
                .items_end()
                .p(px(8.0 * input.style.density))
                .role(Role::Group)
                .aria_label(format!("Window tile: {label}{grid_accessible_status}"));
            let activate = input.on_activate_window.clone();
            let select = input.on_select_window.clone();
            let window_id = window.id;
            let workspace_is_selected = selected;
            let activate_button = div()
                .id(format!("iris.orbit.activate.{}", window.id))
                .absolute()
                .inset_0()
                .role(Role::Button)
                .aria_label(format!(
                    "{}{}{}{}",
                    if is_selected { "Selected: " } else { "" },
                    label,
                    if window.floating { ", floating" } else { "" },
                    grid_accessible_status
                ))
                .focusable()
                .tab_stop(true)
                .focus_visible({
                    let accent = p.accent;
                    move |style| style.border_2().border_color(accent)
                })
                .on_click({
                    let callback = activate.clone();
                    let select = select.clone();
                    move |_, window, cx| {
                        if workspace_is_selected {
                            if let Some(callback) = &callback {
                                callback(window_id);
                            }
                        } else if let Some(select) = &select {
                            select(window_id, window, cx);
                        }
                    }
                });
            if !preview {
                thumb = thumb.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(28.0))
                        .text_color(p.accent)
                        .child(icon),
                );
            }
            if list_fallback {
                thumb = thumb.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .rounded_tl(px(input.style.tile_radius))
                        .rounded_br(px(7.0))
                        .bg(p.material)
                        .px(px(5.0))
                        .py(px(3.0))
                        .text_size(px(9.0))
                        .text_color(p.preset.subtext)
                        .child("Window grid"),
                );
            }
            thumb = thumb.child(
                div()
                    .relative()
                    .w_full()
                    .rounded(px(6.0))
                    .bg(p.material)
                    .px(px(6.0))
                    .py(px(4.0))
                    .text_size(px(11.0))
                    .child(label.clone()),
            );
            if window.urgent {
                thumb = thumb.border_color(p.accent);
            }
            thumb = thumb.child(activate_button);
            if let Some(close) = input.on_close_window.clone() {
                thumb = thumb.child(
                    div()
                        .id(format!("iris.orbit.close.{}", window.id))
                        .absolute()
                        .top(px(5.0))
                        .right(px(5.0))
                        .rounded(px(10.0))
                        .bg(p.material)
                        .px(px(7.0))
                        .py(px(4.0))
                        .text_color(p.text)
                        .role(Role::Button)
                        .aria_label(format!("Close {label}"))
                        .focusable()
                        .tab_stop(true)
                        .focus_visible({
                            let accent = p.accent;
                            move |style| style.border_2().border_color(accent)
                        })
                        .on_click({
                            let close = close.clone();
                            move |_, _, _| close(window_id)
                        })
                        .on_key_down(move |event: &KeyDownEvent, _, cx| {
                            if !event.is_held && is_activate_key(event) {
                                close(window_id);
                                cx.stop_propagation();
                            }
                        })
                        .child("×"),
                );
            }
            tiles.push(thumb.into_any_element());
        }
        let mut card = div()
            .id(format!("iris.orbit.workspace.{}", workspace.id))
            .relative()
            .w(px(input.style.width))
            .h(px(input.style.height))
            .flex_shrink_0()
            .overflow_hidden()
            .rounded(px(input.style.radius))
            .border_2()
            .border_color(if selected { p.accent } else { p.hairline })
            .bg(p.material_high)
            .text_color(p.text)
            .role(Role::Group)
            .aria_label(format!(
                "{title}, {} windows{}",
                workspace.windows.len(),
                if workspace.active { ", active" } else { "" }
            ));
        if workspace.windows.is_empty() {
            card = card.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(p.preset.subtext)
                    .child("Empty workspace"),
            );
        }
        for tile in tiles {
            card = card.child(tile);
        }
        if list_fallback && !workspace.windows.is_empty() {
            card = card.child(
                div()
                    .absolute()
                    .bottom_0()
                    .right_0()
                    .rounded_tl(px(8.0))
                    .bg(p.material)
                    .px(px(8.0))
                    .py(px(5.0))
                    .text_size(px(10.0))
                    .text_color(p.preset.subtext)
                    .child("Window grid · Niri layout unavailable"),
            );
        }
        let workspace_action = activate_workspace.clone();
        let workspace_select = select_workspace.clone();
        card = card.child(
            div()
                .id(format!("iris.orbit.workspace.select.{}", workspace.id))
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bg(p.material)
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(13.0))
                .role(Role::Button)
                .aria_label(if selected {
                    format!("Selected workspace {title}")
                } else {
                    format!("Select workspace {title}")
                })
                .focusable()
                .tab_stop(true)
                .focus_visible({
                    let accent = p.accent;
                    move |style| style.border_2().border_color(accent)
                })
                .on_click({
                    let callback = workspace_action.clone();
                    let select = workspace_select.clone();
                    move |_, window, cx| {
                        if selected {
                            if let Some(callback) = &callback {
                                callback(workspace_id);
                            }
                        } else if let Some(select) = &select {
                            select(workspace_id, window, cx);
                        }
                    }
                })
                .child(title),
        );
        cards.push(card.into_any_element());
    }

    let mut body = div()
        .id("iris.orbit.surface")
        .w(px(input.style.width + 36.0))
        .max_h(px(input.style.height + 180.0))
        .rounded(px(input.style.radius))
        .bg(p.material)
        .text_color(p.text)
        .border_1()
        .border_color(p.hairline)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .focusable()
        .role(Role::Window)
        .aria_label(format!("Iris Orbit on {}", input.output_name))
        .on_key_down({
            let key = input.on_key.clone();
            move |event, window, cx| {
                if let Some(key) = &key {
                    key(event, window, cx);
                }
            }
        });
    body = body.child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(18.0))
                    .child("Orbit"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(p.preset.subtext)
                    .child(if input.query.is_empty() {
                        SharedString::from("Find a window")
                    } else {
                        SharedString::from(input.query.clone())
                    }),
            ),
    );
    let selection_announcement = input.selected_workspace.and_then(|workspace_id| {
        input
            .workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
            .map(|workspace| {
                let workspace_title = workspace
                    .name
                    .as_deref()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Workspace {}", workspace.index));
                let selected_window = input.selected_window.and_then(|window_id| {
                    workspace
                        .windows
                        .iter()
                        .find(|window| window.id == window_id)
                });
                selected_window.map_or_else(
                    || format!("Selected {workspace_title}"),
                    |window| format!("Selected {}, {workspace_title}", window.label()),
                )
            })
    });
    if let Some(announcement) = selection_announcement {
        body = body.child(
            div()
                .id("iris.orbit.selection.status")
                .text_size(px(10.0))
                .text_color(p.preset.subtext)
                .role(Role::Status)
                .aria_label(announcement.clone())
                .child(announcement),
        );
    }
    match input.status {
        OrbitStatus::Loading => {
            body = body.child(status_card("Loading Niri workspaces…", p.preset.subtext, p))
        }
        OrbitStatus::Unavailable => {
            body = body.child(status_card(
                input
                    .status_message
                    .as_deref()
                    .unwrap_or("Niri workspace data is unavailable"),
                p.preset.subtext,
                p,
            ))
        }
        OrbitStatus::Error => {
            body = body.child(status_card(
                input
                    .status_message
                    .as_deref()
                    .unwrap_or("Could not read Niri workspaces"),
                p.accent,
                p,
            ))
        }
        OrbitStatus::Ready if cards.is_empty() => {
            body = body.child(status_card(
                "No workspaces on this output",
                p.preset.subtext,
                p,
            ))
        }
        OrbitStatus::Ready => {
            body = body.child(
                div()
                    .id("iris.orbit.workspace.list")
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .overflow_y_scroll()
                    .children(cards),
            );
        }
    }
    let mut recents = input
        .workspaces
        .iter()
        .flat_map(|ws| ws.windows.iter())
        .filter(|window| window.recent && !window.stashed)
        .collect::<Vec<_>>();
    if input.show_recent && !recents.is_empty() {
        recents.sort_by_key(|window| !window.focused);
        body = body.child(
            div()
                .text_size(px(12.0))
                .text_color(p.preset.subtext)
                .child("Recent windows"),
        );
        for window in recents.into_iter().take(8) {
            let callback = input.on_activate_window.clone();
            let id = window.id;
            body = body.child(
                div()
                    .id(format!("iris.orbit.recent.{id}"))
                    .w_full()
                    .px(px(10.0))
                    .py(px(7.0))
                    .rounded(px(8.0))
                    .bg(p.material_high)
                    .role(Role::Button)
                    .aria_label(format!("Recent: {}", window.label()))
                    .child(window.label())
                    .on_click(move |_, _, _| {
                        if let Some(callback) = &callback {
                            callback(id);
                        }
                    }),
            );
        }
    }
    body
}

fn status_card(message: &str, color: Hsla, p: IrisPalette) -> impl IntoElement {
    div()
        .id("iris.orbit.status")
        .w_full()
        .rounded(px(12.0))
        .bg(p.material_high)
        .p(px(18.0))
        .text_color(color)
        .role(Role::Status)
        .child(message.to_owned())
}

fn is_activate_key(event: &KeyDownEvent) -> bool {
    event.keystroke.key == "enter" || event.keystroke.key == "space"
}
