//! Presentation model for Waffle's fullscreen Task View.
//!
//! The QML surface contains live workspace/window thumbnails, drag-to-move,
//! rename and context-menu operations. This slice renders the live Niri labels
//! and focus state that the GPUI snapshot exposes; it does not pretend to have
//! compositor screenshots or tile geometry. Surface placement, wallpaper,
//! preview capture and mutation services belong to the host.

use std::sync::Arc;

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, FontWeight, Hsla, IntoElement, KeyDownEvent,
    Render, Role, ScrollHandle, Window, div, prelude::*, px,
};

use super::taskbar::start_menu_search::StartMenuSearch;

pub type WorkspaceAction = Arc<dyn Fn(u8) + Send + Sync>;
pub type WindowAction = Arc<dyn Fn(u64) + Send + Sync>;
pub type QueryAction = Arc<dyn Fn(String) + Send + Sync>;
pub type CloseAction = Arc<dyn Fn() + Send + Sync>;

const MAX_WINDOW_CARDS_PER_WORKSPACE: usize = 12;
const MAX_KEYBOARD_WINDOW_SELECTIONS: usize = 24;

#[derive(Clone, Debug)]
pub struct TaskViewStyle {
    pub backdrop: Hsla,
    pub panel: Hsla,
    pub card: Hsla,
    pub card_hover: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub border: Hsla,
    pub radius: f32,
    pub scale: f32,
}

#[derive(Clone, Debug)]
pub struct TaskViewWindow {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub focused: bool,
    pub urgent: bool,
}

#[derive(Clone, Debug)]
pub struct TaskViewWorkspace {
    pub number: u8,
    pub name: Option<String>,
    pub active: bool,
    pub windows: Vec<TaskViewWindow>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskViewMode {
    Carousel,
    #[default]
    Centered,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskViewLoadState {
    #[default]
    Ready,
    Loading,
    Unavailable,
}

#[derive(Clone)]
pub struct TaskViewInput {
    pub style: TaskViewStyle,
    pub mode: TaskViewMode,
    pub load_state: TaskViewLoadState,
    pub workspaces: Vec<TaskViewWorkspace>,
    /// Optional controlled selection. When absent, the component preserves its current
    /// workspace across snapshot refreshes and falls back to the active one if it vanishes.
    pub selected_workspace: Option<u8>,
    pub close_on_select: bool,
    /// Search text mirrored from a host-owned, IME-capable text input.
    pub query: String,
    /// Host-owned editor. When absent, Task View displays a noninteractive status.
    pub search_editor: Option<Entity<StartMenuSearch>>,
    /// Selects/centers a workspace card without switching the active desktop.
    pub on_select_workspace: WorkspaceAction,
    pub on_switch_workspace: WorkspaceAction,
    pub on_focus_window: WindowAction,
    pub on_close_window: WindowAction,
    pub on_query_change: QueryAction,
    pub on_close: CloseAction,
}

pub struct WaffleTaskView {
    input: TaskViewInput,
    focus_handle: FocusHandle,
    workspace_scroll: ScrollHandle,
    selected_workspace: usize,
    selected_workspace_number: Option<u8>,
    selected_window: Option<u64>,
    query: String,
}

impl WaffleTaskView {
    pub fn new(input: TaskViewInput, cx: &mut Context<Self>) -> Self {
        let selected_workspace = workspace_slot(&input, input.selected_workspace)
            .or_else(|| {
                input
                    .workspaces
                    .iter()
                    .position(|workspace| workspace.active)
            })
            .unwrap_or(0);
        let query = input.query.clone();
        let selected_workspace_number =
            input.workspaces.get(selected_workspace).map(|ws| ws.number);
        let workspace_scroll = ScrollHandle::new();
        workspace_scroll.scroll_to_item(selected_workspace);
        Self {
            input,
            focus_handle: cx.focus_handle(),
            workspace_scroll,
            selected_workspace,
            selected_workspace_number,
            selected_window: None,
            query,
        }
    }

    pub fn set_input(&mut self, input: TaskViewInput, cx: &mut Context<Self>) {
        let preferred = input.selected_workspace.or(self.selected_workspace_number);
        self.input = input;
        self.query = self.input.query.clone();
        self.selected_workspace = workspace_slot(&self.input, preferred)
            .or_else(|| {
                self.input
                    .workspaces
                    .iter()
                    .position(|workspace| workspace.active)
            })
            .unwrap_or(0);
        self.selected_workspace_number = self
            .input
            .workspaces
            .get(self.selected_workspace)
            .map(|workspace| workspace.number);
        self.workspace_scroll
            .scroll_to_item(self.selected_workspace);
        if !self
            .input
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.windows)
            .any(|window| Some(window.id) == self.selected_window)
        {
            self.selected_window = None;
        }
        cx.notify();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn selected_workspace(&self) -> Option<u8> {
        self.selected_workspace_number
    }
}

fn workspace_slot(input: &TaskViewInput, number: Option<u8>) -> Option<usize> {
    number.and_then(|number| {
        input
            .workspaces
            .iter()
            .position(|workspace| workspace.number == number)
    })
}

impl Render for WaffleTaskView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        render(self, cx)
    }
}

impl Focusable for WaffleTaskView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn render(view: &mut WaffleTaskView, cx: &mut Context<WaffleTaskView>) -> impl IntoElement {
    let input = &view.input;
    let style = input.style.clone();
    let query = view.query.to_lowercase();
    let workspace_count = input.workspaces.len();
    let selected_workspace = view.selected_workspace;
    let close = input.on_close.clone();
    let keyboard_close = close.clone();
    let backdrop_close = input.on_close.clone();
    let focus_handle = view.focus_handle.clone();
    let workspace_scroll = view.workspace_scroll.clone();
    let empty = if input.load_state == TaskViewLoadState::Loading {
        Some(("Loading workspaces…", "Niri data is being read"))
    } else if input.load_state == TaskViewLoadState::Unavailable {
        Some((
            "Task View unavailable",
            "Niri workspace data is unavailable",
        ))
    } else if input.workspaces.is_empty() {
        Some((
            "No workspaces",
            "Niri has not reported any workspaces for this output",
        ))
    } else {
        None
    };

    // Mirror the QML's output-proportional preview sizing, with a cap so the
    // search field and keyboard hints remain visible on ordinary displays.
    let preview_width = 560. * style.scale;
    let preview_height = preview_width * 0.5625;
    let workspace_cards = input
        .workspaces
        .iter()
        .enumerate()
        .map(|(slot, workspace)| {
            let is_selected = slot == selected_workspace;
            let name = workspace
                .name
                .clone()
                .unwrap_or_else(|| format!("Desktop {}", workspace.number));
            let state_label = if workspace.active {
                "current desktop"
            } else {
                "desktop"
            };
            let label = format!("{name}, {state_label}, {} windows", workspace.windows.len());
            let on_select = input.on_select_workspace.clone();
            let on_select_key = on_select.clone();
            let number = workspace.number;
            let matching_count = workspace
                .windows
                .iter()
                .filter(|window| matches_query(window, &view.query))
                .count();
            let preceding_matches = input
                .workspaces
                .iter()
                .take(slot)
                .map(|previous| {
                    previous
                        .windows
                        .iter()
                        .filter(|window| matches_query(window, &view.query))
                        .take(MAX_WINDOW_CARDS_PER_WORKSPACE)
                        .count()
                })
                .sum::<usize>();
            let window_cards = workspace
                .windows
                .iter()
                .filter(|window| matches_query(window, &view.query))
                .take(MAX_WINDOW_CARDS_PER_WORKSPACE)
                .enumerate()
                .map(|(window_index, window)| {
                    window_card(
                        window,
                        window_index,
                        slot == selected_workspace && view.selected_window == Some(window.id),
                        if view.query.trim().is_empty() {
                            slot == selected_workspace
                                && window_index < MAX_KEYBOARD_WINDOW_SELECTIONS
                        } else {
                            preceding_matches + window_index < MAX_KEYBOARD_WINDOW_SELECTIONS
                        },
                        &style,
                        input.on_focus_window.clone(),
                        input.on_close_window.clone(),
                        input.on_close.clone(),
                        input.close_on_select,
                    )
                })
                .collect::<Vec<_>>();
            let visible_window_count = matching_count;
            let centered = input.mode == TaskViewMode::Centered;
            let card_width = if centered && is_selected {
                preview_width
            } else if centered {
                (preview_width * 0.72).max(260. * style.scale)
            } else {
                (preview_width * 0.84).max(280. * style.scale)
            };
            let mut card = div()
                .id(format!("waffle.task-view.workspace.{}", workspace.number))
                .min_w(px(240. * style.scale))
                .w(px(card_width * style.scale))
                .h(px(preview_height + 78. * style.scale))
                .max_h(px(preview_height + 78. * style.scale))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap(px(8. * style.scale))
                .rounded(px(style.radius * style.scale))
                .border_1()
                .border_color(if is_selected {
                    style.accent
                } else {
                    style.border
                })
                .opacity(if centered {
                    (1. - slot.abs_diff(selected_workspace) as f32 * 0.18).max(0.55)
                } else {
                    1.
                })
                .bg(style.card)
                .p(px(12. * style.scale))
                .role(Role::Group)
                .aria_label(label)
                .opacity(if centered && !is_selected {
                    (1. - slot.abs_diff(selected_workspace) as f32 * 0.18).max(0.55)
                } else {
                    1.
                })
                .child(
                    div()
                        .id(format!(
                            "waffle.task-view.workspace.{}.select",
                            workspace.number
                        ))
                        .flex()
                        .items_center()
                        .justify_between()
                        .role(Role::Button)
                        .aria_label(format!("Select {name}"))
                        .focusable()
                        .tab_stop(true)
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if is_selected {
                                    style.accent
                                } else {
                                    style.foreground
                                })
                                .child(name),
                        )
                        .child(if workspace.active {
                            div()
                                .w(px(18. * style.scale))
                                .h(px(2. * style.scale))
                                .rounded(px(1. * style.scale))
                                .bg(style.accent)
                                .into_any_element()
                        } else {
                            div().w(px(18. * style.scale)).into_any_element()
                        })
                        .child(
                            div()
                                .rounded(px(999.))
                                .bg(style.panel)
                                .px(px(8. * style.scale))
                                .py(px(3. * style.scale))
                                .text_sm()
                                .text_color(style.muted)
                                .child(format!("{}", workspace.windows.len())),
                        )
                        .on_click(move |_, _, _| on_select(number))
                        .on_key_down(move |event, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                && !event.is_held
                            {
                                on_select_key(number);
                                cx.stop_propagation();
                            }
                        }),
                );
            card = card.child(if visible_window_count == 0 {
                div()
                    .h(px(preview_height))
                    .min_h(px(120. * style.scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px((style.radius - 4.).max(4.) * style.scale))
                    .bg(style.panel)
                    .text_sm()
                    .text_color(style.muted)
                    .child(div().text_2xl().text_color(style.border).child("◇"))
                    .child(if query.is_empty() {
                        "No open windows".to_owned()
                    } else {
                        "No matching windows".to_owned()
                    })
                    .into_any_element()
            } else {
                let mut window_list = div()
                    .id(format!(
                        "waffle.task-view.workspace.{}.windows",
                        workspace.number
                    ))
                    .h(px(preview_height))
                    .max_h(px(preview_height))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(8. * style.scale))
                    .children(window_cards);
                if visible_window_count > MAX_WINDOW_CARDS_PER_WORKSPACE {
                    window_list = window_list.child(
                        div()
                            .py(px(6. * style.scale))
                            .text_xs()
                            .text_color(style.muted)
                            .child(format!(
                                "Showing {} of {} windows; refine the host search to find more",
                                MAX_WINDOW_CARDS_PER_WORKSPACE, visible_window_count
                            )),
                    );
                }
                window_list.into_any_element()
            });
            if centered && is_selected {
                card = card.border_2().border_color(style.accent);
            }
            if centered && is_selected {
                card = card.border_2().border_color(style.accent);
            }
            card.into_any_element()
        })
        .collect::<Vec<_>>();

    let result_count = input
        .workspaces
        .iter()
        .flat_map(|workspace| &workspace.windows)
        .filter(|window| {
            query.is_empty()
                || window
                    .title
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&query)
                || window
                    .app_id
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&query)
        })
        .count();
    let result_count_label = if view.query.is_empty() {
        String::new()
    } else if result_count > MAX_KEYBOARD_WINDOW_SELECTIONS {
        format!("{MAX_KEYBOARD_WINDOW_SELECTIONS}+")
    } else {
        result_count.to_string()
    };
    let search_editor = input.search_editor.clone();
    let query_label = if view.query.is_empty() {
        "Window search input is not connected".to_owned()
    } else {
        view.query.clone()
    };
    let query_is_empty = view.query.is_empty();
    let search_accessible_label = if search_editor.is_some() {
        "Task View window search input".to_owned()
    } else if query_is_empty {
        "Window search unavailable until the host supplies an IME-capable text input".to_owned()
    } else {
        format!("Window search query: {}", view.query)
    };
    let search_content = if let Some(editor) = search_editor {
        editor.into_any_element()
    } else {
        div()
            .id("waffle.task-view.search.status")
            .text_sm()
            .text_color(if query_is_empty {
                style.muted
            } else {
                style.foreground
            })
            .child(query_label)
            .into_any_element()
    };
    let query_border = if query_is_empty {
        style.border
    } else {
        style.accent
    };
    let mode = match input.mode {
        TaskViewMode::Carousel => "Carousel",
        TaskViewMode::Centered => "Centered",
    };
    let on_key_action = input.clone();
    let view_entity = cx.entity();

    div()
        .id("waffle.task-view")
        .size_full()
        .role(Role::Group)
        .aria_label("Task View")
        .focusable()
        .tab_stop(true)
        .flex()
        .items_center()
        .justify_center()
        .bg(style.backdrop)
        .text_color(style.foreground)
        .track_focus(&focus_handle)
        .on_click(move |_, _, _| backdrop_close())
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            // Let GPUI's native Tab traversal move between real tab stops.
            // The root-level handler only owns shortcuts while the root itself
            // has focus; keys from child buttons must retain their normal path.
            if event.keystroke.key == "tab" && !focus_handle.is_focused(window) {
                return;
            }
            view_entity.update(cx, |view, cx| {
                handle_key(
                    view,
                    event,
                    &on_key_action,
                    cx,
                    focus_handle.is_focused(window),
                );
            });
        })
        .child(
            div()
                .id("waffle.task-view.content")
                .max_w_full()
                .max_h_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_start()
                .gap(px(12. * style.scale))
                .p(px(12. * style.scale))
                .overflow_y_scroll()
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("waffle.task-view.search")
                        .flex()
                        .w(px(300. * style.scale))
                        .items_center()
                        .justify_between()
                        .gap(px(10. * style.scale))
                        .rounded(px(style.radius * style.scale))
                        .border_1()
                        .border_color(query_border)
                        .bg(style.panel)
                        .px(px(14. * style.scale))
                        .py(px(10. * style.scale))
                        .role(Role::Group)
                        .aria_label(search_accessible_label)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8. * style.scale))
                                .child(div().text_color(style.accent).child("⌕"))
                                .child(search_content),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(style.muted)
                                .child(result_count_label),
                        ),
                )
                .child(if let Some((title, description)) = empty {
                    div()
                        .w(px(420. * style.scale))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(8. * style.scale))
                        .rounded(px(style.radius * style.scale))
                        .bg(style.panel)
                        .p(px(24. * style.scale))
                        .child(div().text_lg().text_color(style.foreground).child(title))
                        .child(div().text_sm().text_color(style.muted).child(description))
                        .into_any_element()
                } else {
                    div()
                        .id("waffle.task-view.workspaces")
                        .max_w_full()
                        .track_scroll(&workspace_scroll)
                        .flex()
                        .items_start()
                        .gap(px(18. * style.scale))
                        .overflow_x_scroll()
                        .px(px(24. * style.scale))
                        .children(workspace_cards)
                        .into_any_element()
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(18. * style.scale))
                        .text_xs()
                        .text_color(style.muted)
                        .child(format!("{mode} · {workspace_count} desktops"))
                        .child("← →  Desktops")
                        .child("Tab  Focus controls")
                        .child("Enter  Open")
                        .child("Del  Close window")
                        .child("Esc  Close Task View"),
                )
                .child(
                    div()
                        .id("waffle.task-view.close")
                        .role(Role::Button)
                        .aria_label("Close Task View")
                        .focusable()
                        .tab_stop(true)
                        .rounded(px(style.radius * style.scale))
                        .border_1()
                        .border_color(style.border)
                        .bg(style.panel)
                        .px(px(12. * style.scale))
                        .py(px(7. * style.scale))
                        .text_sm()
                        .child("Close")
                        .on_click(move |_, _, _| close())
                        .on_key_down(move |event, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                && !event.is_held
                            {
                                keyboard_close();
                                cx.stop_propagation();
                            }
                        }),
                ),
        )
}

fn window_card(
    window: &TaskViewWindow,
    index: usize,
    selected: bool,
    keyboard_reachable: bool,
    style: &TaskViewStyle,
    on_focus: WindowAction,
    on_close: WindowAction,
    close_view: CloseAction,
    close_on_select: bool,
) -> impl IntoElement {
    let id = window.id;
    let key_id = window.id;
    let label = window
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .or(window.app_id.as_deref())
        .unwrap_or("Untitled window");
    let app = window
        .app_id
        .clone()
        .unwrap_or_else(|| "Application".into());
    let state = if window.focused { ", focused" } else { "" };
    let urgent = if window.urgent { " · Attention" } else { "" };
    let accessible_name = format!("{label}, {app}{state}{urgent}");
    let focus_action = on_focus.clone();
    let key_close_view = close_view.clone();
    let click_focus = on_focus.clone();
    let click_close = close_view.clone();
    let close_action = on_close.clone();
    let close_key_action = on_close.clone();
    div()
        .id(format!("waffle.task-view.window.{id}"))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8. * style.scale))
        .rounded(px((style.radius - 6.).max(4.) * style.scale))
        .border_1()
        .border_color(if selected || window.focused {
            style.accent
        } else {
            style.border
        })
        .bg(if selected {
            style.card_hover
        } else {
            style.panel
        })
        .px(px(10. * style.scale))
        .py(px(8. * style.scale))
        .role(Role::Group)
        .aria_label(accessible_name)
        .child(
            div()
                .id(format!("waffle.task-view.window.{id}.activate"))
                .flex()
                .min_w_0()
                .flex_1()
                .items_center()
                .role(Role::Button)
                .aria_label(format!("Focus {label}"))
                .aria_selected(selected)
                .focusable()
                .tab_stop(keyboard_reachable)
                .child(
                    div()
                        .flex()
                        .min_w_0()
                        .flex_col()
                        .child(
                            div()
                                .truncate()
                                .text_sm()
                                .text_color(style.foreground)
                                .child(label.to_owned()),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_xs()
                                .text_color(style.muted)
                                .child(format!("{app}{urgent}")),
                        ),
                )
                .on_click(move |_, _, _| {
                    click_focus(id);
                    if close_on_select {
                        click_close();
                    }
                })
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                        focus_action(id);
                        if close_on_select {
                            key_close_view();
                        }
                        cx.stop_propagation();
                    }
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6. * style.scale))
                .child(
                    div()
                        .text_xs()
                        .text_color(style.muted)
                        .child(format!("{}", index + 1)),
                )
                .child(
                    div()
                        .id(format!("waffle.task-view.window.{id}.close"))
                        .role(Role::Button)
                        .aria_label(format!("Close {label}"))
                        .focusable()
                        .tab_stop(keyboard_reachable)
                        .rounded(px(5.))
                        .px(px(6.))
                        .py(px(3.))
                        .text_sm()
                        .text_color(style.muted)
                        .child("×")
                        .on_click(move |_, _, cx| {
                            close_action(key_id);
                            cx.stop_propagation();
                        })
                        .on_key_down(move |event, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                && !event.is_held
                            {
                                close_key_action(key_id);
                                cx.stop_propagation();
                            }
                        }),
                ),
        )
}

fn handle_key(
    view: &mut WaffleTaskView,
    event: &KeyDownEvent,
    input: &TaskViewInput,
    cx: &mut Context<WaffleTaskView>,
    root_focused: bool,
) {
    let key = event.keystroke.key.as_str();
    match key {
        "escape" => {
            if !view.query.is_empty() {
                view.query.clear();
                (input.on_query_change)(String::new());
                cx.notify();
            } else {
                (input.on_close)();
            }
        }
        "left" | "right" if !input.workspaces.is_empty() => {
            if key == "left" {
                view.selected_workspace = view.selected_workspace.saturating_sub(1);
            } else {
                view.selected_workspace =
                    (view.selected_workspace + 1).min(input.workspaces.len() - 1);
            }
            view.selected_workspace_number = input
                .workspaces
                .get(view.selected_workspace)
                .map(|workspace| workspace.number);
            if let Some(workspace) = input.workspaces.get(view.selected_workspace) {
                (input.on_select_workspace)(workspace.number);
            }
            view.workspace_scroll
                .scroll_to_item(view.selected_workspace);
            view.selected_window = None;
            cx.notify();
            cx.stop_propagation();
        }
        "tab" if root_focused => {
            let windows = matching_windows(view, input);
            if !windows.is_empty() {
                let selected_index = view
                    .selected_window
                    .and_then(|id| windows.iter().position(|window| window.id == id));
                let next = if event.keystroke.modifiers.shift {
                    selected_index.map_or(windows.len() - 1, |index| index.saturating_sub(1))
                } else {
                    selected_index.map_or(0, |index| (index + 1) % windows.len())
                };
                view.selected_window = Some(windows[next].id);
                cx.notify();
            }
        }
        "enter" if !event.is_held => {
            if let Some(window) = selected_window(view, input) {
                (input.on_focus_window)(window.id);
                if input.close_on_select {
                    (input.on_close)();
                }
            } else if let Some(workspace) = input.workspaces.get(view.selected_workspace) {
                (input.on_switch_workspace)(workspace.number);
                (input.on_close)();
            }
            cx.stop_propagation();
        }
        "delete" if root_focused && !event.is_held => {
            if let Some(window) = selected_window(view, input) {
                (input.on_close_window)(window.id);
            }
            cx.stop_propagation();
        }
        _ => {}
    }
}

fn selected_window<'a>(
    view: &WaffleTaskView,
    input: &'a TaskViewInput,
) -> Option<&'a TaskViewWindow> {
    let matching = matching_windows(view, input);
    view.selected_window
        .and_then(|id| matching.iter().find(|window| window.id == id).copied())
        .or_else(|| {
            input
                .workspaces
                .get(view.selected_workspace)?
                .windows
                .iter()
                .find(|window| window.focused && matches_query(window, &view.query))
        })
}

fn matching_windows<'a>(
    view: &WaffleTaskView,
    input: &'a TaskViewInput,
) -> Vec<&'a TaskViewWindow> {
    if view.query.trim().is_empty() {
        input
            .workspaces
            .get(view.selected_workspace)
            .into_iter()
            .flat_map(|workspace| workspace.windows.iter())
            .filter(|window| matches_query(window, &view.query))
            .take(MAX_WINDOW_CARDS_PER_WORKSPACE)
            .collect()
    } else {
        input
            .workspaces
            .iter()
            .flat_map(|workspace| {
                workspace
                    .windows
                    .iter()
                    .filter(|window| matches_query(window, &view.query))
                    .take(MAX_WINDOW_CARDS_PER_WORKSPACE)
            })
            .take(MAX_KEYBOARD_WINDOW_SELECTIONS)
            .collect()
    }
}

fn matches_query(window: &TaskViewWindow, query: &str) -> bool {
    query.trim().is_empty()
        || window
            .title
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(&query.to_lowercase())
        || window
            .app_id
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(&query.to_lowercase())
}
