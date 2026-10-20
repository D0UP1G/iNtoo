//! Presentation model for Waffle's fullscreen Task View.
//!
//! The QML surface contains live workspace/window thumbnails, drag-to-move,
//! rename and context-menu operations. This slice lays out window metadata
//! using Niri tile geometry when available, with labels and focus state from
//! the snapshot; it does not capture compositor screenshots. Surface placement,
//! wallpaper, preview capture and mutation services belong to the host.

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
    pub pos_in_scrolling_layout: Option<[u64; 2]>,
    pub tile_size: Option<[f32; 2]>,
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
    /// Logical output size. Missing dimensions use the explicit 1920x1080 preview fallback.
    pub output_size: Option<[f32; 2]>,
    /// `false` means the host cannot identify an output and cannot safely scope Niri data.
    pub output_filter_available: bool,
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
    query_selection_pending: bool,
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
            query_selection_pending: !query.is_empty(),
            query,
        }
    }

    pub fn set_input(&mut self, input: TaskViewInput, cx: &mut Context<Self>) {
        let query_changed = self.query != input.query;
        let had_selected_window = self.selected_window.is_some();
        let preferred = input.selected_workspace.or(self.selected_workspace_number);
        self.input = input;
        self.query = self.input.query.clone();
        if query_changed {
            self.selected_window = None;
            self.query_selection_pending = !self.query.is_empty();
        }
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
        let selected_window_is_visible = matching_windows(self, &self.input)
            .iter()
            .any(|window| Some(window.id) == self.selected_window);
        if !selected_window_is_visible {
            self.selected_window = None;
            if had_selected_window && !self.query.is_empty() {
                self.query_selection_pending = true;
            }
        }
        if self.query_selection_pending {
            self.reconcile_query_selection();
        }
        cx.notify();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.query != query {
            let was_searching = !self.query.is_empty();
            self.query = query;
            if self.query.is_empty() {
                self.query_selection_pending = false;
                self.selected_window = was_searching
                    .then(|| {
                        self.input
                            .workspaces
                            .get(self.selected_workspace)
                            .map(|workspace| workspace.windows.iter().collect::<Vec<_>>())
                            .map(|windows| {
                                project_window_widths(&windows, MAX_WINDOW_CARDS_PER_WORKSPACE)
                                    .first()
                                    .map(|(window, _)| window.id)
                            })
                            .flatten()
                    })
                    .flatten();
            } else {
                self.selected_window = None;
                self.query_selection_pending = true;
                self.reconcile_query_selection();
            }
            cx.notify();
        }
    }

    fn reconcile_query_selection(&mut self) {
        let (selection, pending) = query_selection_transition(
            &self.input.workspaces,
            &self.query,
            self.query_selection_pending,
        );
        self.query_selection_pending = pending;
        let Some((slot, id)) = selection else {
            return;
        };
        self.selected_workspace = slot;
        self.selected_workspace_number = self
            .input
            .workspaces
            .get(slot)
            .map(|workspace| workspace.number);
        self.workspace_scroll.scroll_to_item(slot);
        self.selected_window = Some(id);
        if let Some(number) = self.selected_workspace_number {
            (self.input.on_select_workspace)(number);
        }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        render(self, window, cx)
    }
}

impl Focusable for WaffleTaskView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn render(
    view: &mut WaffleTaskView,
    window: &Window,
    cx: &mut Context<WaffleTaskView>,
) -> impl IntoElement {
    let input = &view.input;
    let style = input.style.clone();
    let query = view.query.to_lowercase();
    let workspace_count = input.workspaces.len();
    let [output_width, output_height] =
        valid_output_size(input.output_size).unwrap_or([1920.0, 1080.0]);
    let preview_width = (0.38 * output_width).min(600.0) * style.scale;
    let preview_height = preview_width * output_height / output_width;
    let selected_workspace = view.selected_workspace;
    let close = input.on_close.clone();
    let keyboard_close = close.clone();
    let backdrop_close = input.on_close.clone();
    let focus_handle = view.focus_handle.clone();
    let workspace_scroll = view.workspace_scroll.clone();
    let empty = if !input.output_filter_available {
        Some((
            "Task View unavailable",
            "The current output could not be identified, so windows from other outputs are hidden",
        ))
    } else if input.load_state == TaskViewLoadState::Loading {
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

    // Use the output-proportional preview bounds from the QML reference. If
    // GPUI cannot provide a usable size, the 1920x1080 fallback is explicit.
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
            let focus_color = style.accent;
            let number = workspace.number;
            let matching = matching_windows_for_workspace(workspace, &view.query);
            let matching_count = matching.len();
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
            let projected = project_window_widths(&matching, MAX_WINDOW_CARDS_PER_WORKSPACE);
            let window_cards = projected
                .iter()
                .take(MAX_WINDOW_CARDS_PER_WORKSPACE)
                .enumerate()
                .map(|(window_index, (window, width))| {
                    window_card(
                        window,
                        window_index,
                        preview_width * width,
                        preview_height,
                        slot == selected_workspace && view.selected_window == Some(window.id),
                        if view.query.is_empty() {
                            window_index < MAX_WINDOW_CARDS_PER_WORKSPACE
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
                .w(px(card_width))
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
                        .focus_visible(move |focus_style| {
                            focus_style.border_2().border_color(focus_color)
                        })
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
                        if slot == selected_workspace {
                            "No open windows".to_owned()
                        } else {
                            "Select this workspace to view its windows".to_owned()
                        }
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
                    .flex_row()
                    .gap(px(8. * style.scale))
                    .children(window_cards);
                if visible_window_count > MAX_WINDOW_CARDS_PER_WORKSPACE {
                    window_list = window_list.child(
                        div()
                            .py(px(6. * style.scale))
                            .text_xs()
                            .text_color(style.muted)
                            .child(format!(
                                "Showing first {} of {} windows in visual order",
                                MAX_WINDOW_CARDS_PER_WORKSPACE, visible_window_count
                            )),
                    );
                }
                window_list.into_any_element()
            });
            if centered && is_selected {
                card = card.border_2().border_color(style.accent);
            }
            card.into_any_element()
        })
        .collect::<Vec<_>>();

    let result_count = search_match_count(&input.workspaces, &query);
    let selection_cap_reached = result_count > MAX_KEYBOARD_WINDOW_SELECTIONS
        || input.workspaces.iter().any(|workspace| {
            workspace
                .windows
                .iter()
                .filter(|window| matches_query(window, &query))
                .count()
                > MAX_WINDOW_CARDS_PER_WORKSPACE
        });
    let result_count_label = result_count.to_string();
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
    let search_focused = input
        .search_editor
        .as_ref()
        .is_some_and(|editor| editor.read(cx).focus_handle(cx).is_focused(window));
    let query_border = if search_focused || !query_is_empty {
        style.accent
    } else {
        style.border
    };
    let close_focus_color = style.accent;
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
            view_entity.update(cx, |view, cx| {
                handle_key(view, event, &on_key_action, window, cx);
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
                        .border_color(query_border)
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
                        .gap(px(48. * style.scale))
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
                        .child(if valid_output_size(input.output_size).is_none() {
                            "Output geometry unavailable · using 1920 × 1080"
                        } else {
                            ""
                        })
                        .child("← →  Desktops")
                        .child("Tab  Cycle windows")
                        .child("Ctrl+Tab  Focus controls")
                        .child("Tab  Next control when focused")
                        .child(if selection_cap_reached {
                            format!("Tab cycles up to {MAX_KEYBOARD_WINDOW_SELECTIONS} visible windows (first {MAX_WINDOW_CARDS_PER_WORKSPACE} per desktop)")
                        } else {
                            String::new()
                        })
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
                        .focus_visible(move |focus_style| {
                            focus_style.border_2().border_color(close_focus_color)
                        })
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
    width: f32,
    height: f32,
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
    let focus_color = style.accent;
    div()
        .id(format!("waffle.task-view.window.{id}"))
        .flex()
        .w(px(width.max(1.0)))
        .h(px(height))
        .flex_shrink_0()
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
                .focus_visible(move |focus_style| focus_style.border_2().border_color(focus_color))
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
                        .focus_visible(move |focus_style| {
                            focus_style.border_2().border_color(focus_color)
                        })
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
    window: &mut Window,
    cx: &mut Context<WaffleTaskView>,
) {
    let key = event.keystroke.key.as_str();
    let search_focused = input
        .search_editor
        .as_ref()
        .is_some_and(|editor| editor.read(cx).focus_handle(cx).is_focused(window));
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
        "tab" | "backtab" => {
            let action = task_view_tab_action(
                key,
                event.keystroke.modifiers.shift,
                event.keystroke.modifiers.control,
                event.keystroke.modifiers.platform,
                event.keystroke.modifiers.alt,
                search_focused,
            );
            match action {
                TaskViewTabAction::CycleWindows { previous } => {
                    let windows = matching_windows(view, input);
                    if !windows.is_empty() {
                        let selected_index = view
                            .selected_window
                            .and_then(|id| windows.iter().position(|window| window.id == id));
                        if let Some(next) =
                            next_window_index(selected_index, windows.len(), previous)
                        {
                            view.selected_window = Some(windows[next].id);
                            cx.notify();
                        }
                    }
                }
                TaskViewTabAction::FocusNext => window.focus_next(cx),
                TaskViewTabAction::FocusPrevious => window.focus_prev(cx),
                TaskViewTabAction::Propagate => return,
            }
            cx.stop_propagation();
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
        "delete"
            if !event.is_held
                && !event.keystroke.modifiers.shift
                && !event.keystroke.modifiers.control
                && !event.keystroke.modifiers.alt
                && !event.keystroke.modifiers.platform =>
        {
            if let Some(window) = selected_window(view, input) {
                (input.on_close_window)(window.id);
            }
            cx.stop_propagation();
        }
        _ => {}
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskViewTabAction {
    CycleWindows { previous: bool },
    FocusNext,
    FocusPrevious,
    Propagate,
}

fn task_view_tab_action(
    key: &str,
    shift: bool,
    control: bool,
    platform: bool,
    alt: bool,
    search_focused: bool,
) -> TaskViewTabAction {
    let previous = key == "backtab" || shift;
    if alt {
        return TaskViewTabAction::Propagate;
    }
    if search_focused && !control && !platform {
        TaskViewTabAction::CycleWindows { previous }
    } else if previous {
        TaskViewTabAction::FocusPrevious
    } else {
        TaskViewTabAction::FocusNext
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
    matching_windows_for_selection(&input.workspaces, view.selected_workspace, &view.query)
}

fn matching_windows_for_selection<'a>(
    workspaces: &'a [TaskViewWorkspace],
    selected_workspace: usize,
    query: &str,
) -> Vec<&'a TaskViewWindow> {
    if query.is_empty() {
        let windows = workspaces
            .get(selected_workspace)
            .into_iter()
            .flat_map(|workspace| workspace.windows.iter())
            .filter(|window| matches_query(window, query))
            .collect::<Vec<_>>();
        project_window_widths(&windows, MAX_WINDOW_CARDS_PER_WORKSPACE)
            .into_iter()
            .map(|(window, _)| window)
            .collect()
    } else {
        workspaces
            .iter()
            .flat_map(|workspace| {
                let windows = workspace
                    .windows
                    .iter()
                    .filter(|window| matches_query(window, query))
                    .collect::<Vec<_>>();
                project_window_widths(&windows, MAX_WINDOW_CARDS_PER_WORKSPACE)
                    .into_iter()
                    .map(|(window, _)| window)
                    .collect::<Vec<_>>()
            })
            .take(MAX_KEYBOARD_WINDOW_SELECTIONS)
            .collect()
    }
}

fn matches_query(window: &TaskViewWindow, query: &str) -> bool {
    query.is_empty()
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

fn valid_output_size(size: Option<[f32; 2]>) -> Option<[f32; 2]> {
    size.filter(|[width, height]| {
        width.is_finite() && height.is_finite() && *width > 0.0 && *height > 0.0
    })
}

fn matching_windows_for_workspace<'a>(
    workspace: &'a TaskViewWorkspace,
    query: &str,
) -> Vec<&'a TaskViewWindow> {
    workspace
        .windows
        .iter()
        .filter(|window| matches_query(window, query))
        .collect()
}

fn first_search_window(workspaces: &[TaskViewWorkspace], query: &str) -> Option<(usize, u64)> {
    workspaces.iter().enumerate().find_map(|(slot, workspace)| {
        let matches = matching_windows_for_workspace(workspace, query);
        project_window_widths(&matches, MAX_WINDOW_CARDS_PER_WORKSPACE)
            .first()
            .map(|(window, _)| (slot, window.id))
    })
}

fn query_selection_transition(
    workspaces: &[TaskViewWorkspace],
    query: &str,
    pending: bool,
) -> (Option<(usize, u64)>, bool) {
    if !pending || query.is_empty() {
        return (None, pending);
    }
    match first_search_window(workspaces, query) {
        Some(selection) => (Some(selection), false),
        None => (None, true),
    }
}

fn search_match_count(workspaces: &[TaskViewWorkspace], query: &str) -> usize {
    workspaces
        .iter()
        .flat_map(|workspace| &workspace.windows)
        .filter(|window| matches_query(window, query))
        .count()
}

fn next_window_index(selected: Option<usize>, count: usize, previous: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    if previous {
        Some(selected.map_or(
            count - 1,
            |index| if index == 0 { count - 1 } else { index - 1 },
        ))
    } else {
        Some(selected.map_or(0, |index| (index + 1) % count))
    }
}

/// Order windows by Niri's reported column/row and use their tile widths as
/// proportions. When any required geometry is absent or invalid, preserve the
/// supplied order and give every window an equal share; no coordinates are
/// synthesized for that fallback.
fn project_window_widths<'a>(
    windows: &[&'a TaskViewWindow],
    limit: usize,
) -> Vec<(&'a TaskViewWindow, f32)> {
    if windows.is_empty() {
        return Vec::new();
    }
    let mut ordered = windows.to_vec();
    let valid_geometry = windows.iter().all(|window| {
        window.pos_in_scrolling_layout.is_some()
            && window.tile_size.is_some_and(|[width, height]| {
                width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
            })
    });
    if !valid_geometry {
        let share = 1.0 / ordered.len() as f32;
        ordered.truncate(limit);
        return ordered.into_iter().map(|window| (window, share)).collect();
    }
    let total_width = ordered
        .iter()
        .filter_map(|window| window.tile_size.map(|size| size[0]))
        .sum::<f32>();
    if !total_width.is_finite() || total_width <= 0.0 {
        let share = 1.0 / ordered.len() as f32;
        ordered.truncate(limit);
        return ordered.into_iter().map(|window| (window, share)).collect();
    }
    let visible_len = ordered.len().min(limit);
    if visible_len < ordered.len() {
        ordered.select_nth_unstable_by(visible_len, |left, right| {
            left.pos_in_scrolling_layout
                .cmp(&right.pos_in_scrolling_layout)
        });
    }
    ordered.truncate(visible_len);
    ordered.sort_by_key(|window| window.pos_in_scrolling_layout);
    ordered
        .into_iter()
        .map(|window| {
            (
                window,
                window.tile_size.map_or(0.0, |size| size[0]) / total_width,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u64, position: Option<[u64; 2]>, size: Option<[f32; 2]>) -> TaskViewWindow {
        TaskViewWindow {
            id,
            title: None,
            app_id: None,
            focused: false,
            urgent: false,
            pos_in_scrolling_layout: position,
            tile_size: size,
        }
    }

    #[test]
    fn task_view_uses_niri_order_and_proportional_tile_widths() {
        let a = window(1, Some([1, 0]), Some([300.0, 800.0]));
        let b = window(2, Some([0, 0]), Some([700.0, 800.0]));
        let projected = project_window_widths(&[&a, &b], MAX_WINDOW_CARDS_PER_WORKSPACE);

        assert_eq!(
            projected
                .iter()
                .map(|(window, _)| window.id)
                .collect::<Vec<_>>(),
            [2, 1]
        );
        assert!((projected[0].1 - 0.7).abs() < 0.001);
        assert!((projected[1].1 - 0.3).abs() < 0.001);
    }

    #[test]
    fn task_view_uses_uniform_widths_without_complete_geometry() {
        let a = window(1, Some([1, 0]), Some([300.0, 800.0]));
        let b = window(2, None, None);
        let projected = project_window_widths(&[&a, &b], MAX_WINDOW_CARDS_PER_WORKSPACE);

        assert_eq!(
            projected
                .iter()
                .map(|(window, _)| window.id)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(projected[0].1, 0.5);
        assert_eq!(projected[1].1, 0.5);
    }

    #[test]
    fn task_view_validates_or_falls_back_output_size() {
        assert_eq!(
            valid_output_size(Some([1600.0, 900.0])),
            Some([1600.0, 900.0])
        );
        assert_eq!(valid_output_size(Some([0.0, 900.0])), None);
        assert_eq!(valid_output_size(Some([f32::NAN, 900.0])), None);
        assert_eq!(
            valid_output_size(None).unwrap_or([1920.0, 1080.0]),
            [1920.0, 1080.0]
        );
    }

    #[test]
    fn task_view_limits_layout_work_to_visible_cards_but_counts_all_matches() {
        let windows = (0..30)
            .map(|index| {
                let mut window = window(index, Some([index, 0]), Some([100.0, 800.0]));
                window.title = Some(format!("Window {index}"));
                window
            })
            .collect::<Vec<_>>();
        let mut references = windows.iter().collect::<Vec<_>>();
        references.reverse();
        let visible = project_window_widths(&references, MAX_WINDOW_CARDS_PER_WORKSPACE);
        assert_eq!(visible.len(), MAX_WINDOW_CARDS_PER_WORKSPACE);
        assert_eq!(
            visible
                .iter()
                .map(|(window, _)| window.id)
                .collect::<Vec<_>>(),
            (0..MAX_WINDOW_CARDS_PER_WORKSPACE as u64).collect::<Vec<_>>()
        );

        let workspace = TaskViewWorkspace {
            number: 1,
            name: None,
            active: true,
            windows,
        };
        assert_eq!(search_match_count(&[workspace], "window"), 30);
    }

    #[test]
    fn empty_query_counts_all_workspaces_but_keyboard_selection_stays_scoped() {
        let mut first = window(1, Some([0, 0]), Some([500.0, 800.0]));
        first.title = Some("First".to_owned());
        let mut second = window(2, Some([0, 0]), Some([500.0, 800.0]));
        second.title = Some("Second".to_owned());
        let workspaces = vec![
            TaskViewWorkspace {
                number: 1,
                name: None,
                active: true,
                windows: vec![first],
            },
            TaskViewWorkspace {
                number: 2,
                name: None,
                active: false,
                windows: vec![second],
            },
        ];

        assert_eq!(search_match_count(&workspaces, ""), 2);
        assert_eq!(
            matching_windows_for_selection(&workspaces, 1, "")
                .iter()
                .map(|window| window.id)
                .collect::<Vec<_>>(),
            [2]
        );
    }

    #[test]
    fn task_view_pending_search_selects_first_match_after_snapshot_arrives() {
        let (before_snapshot, pending) = query_selection_transition(&[], "window", true);
        assert_eq!(before_snapshot, None);
        assert!(pending);

        let mut first = window(1, Some([0, 0]), Some([600.0, 800.0]));
        first.title = Some("Window A".to_owned());
        let mut second = window(2, Some([1, 0]), Some([400.0, 800.0]));
        second.title = Some("Window B".to_owned());
        let workspace = TaskViewWorkspace {
            number: 3,
            name: None,
            active: true,
            windows: vec![second, first],
        };
        let (after_snapshot, pending) = query_selection_transition(&[workspace], "window", pending);
        assert_eq!(after_snapshot, Some((0, 1)));
        assert!(!pending);
    }

    #[test]
    fn task_view_tab_navigation_wraps_in_visual_order() {
        assert_eq!(next_window_index(None, 3, false), Some(0));
        assert_eq!(next_window_index(None, 3, true), Some(2));
        assert_eq!(next_window_index(Some(2), 3, false), Some(0));
        assert_eq!(next_window_index(Some(0), 3, true), Some(2));
        assert_eq!(next_window_index(None, 0, false), None);
    }

    #[test]
    fn task_view_tabs_cycle_windows_from_search_and_traverse_controls() {
        assert_eq!(
            task_view_tab_action("tab", false, false, false, false, true),
            TaskViewTabAction::CycleWindows { previous: false }
        );
        assert_eq!(
            task_view_tab_action("backtab", false, false, false, false, true),
            TaskViewTabAction::CycleWindows { previous: true }
        );
        assert_eq!(
            task_view_tab_action("tab", true, false, false, false, false),
            TaskViewTabAction::FocusPrevious
        );
        assert_eq!(
            task_view_tab_action("tab", false, false, false, false, false),
            TaskViewTabAction::FocusNext
        );
        assert_eq!(
            task_view_tab_action("tab", false, true, false, false, true),
            TaskViewTabAction::FocusNext
        );
        assert_eq!(
            task_view_tab_action("tab", false, false, false, true, true),
            TaskViewTabAction::Propagate
        );
    }
}
