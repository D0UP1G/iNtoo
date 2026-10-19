//! Waffle taskbar composition for the GPUI prototype.
//!
//! This is a presentation-only component. Callers provide live task, tray,
//! clock, notification, and interaction state through [`WaffleTaskbarInput`].
//! No desktop state is synthesized here.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    App, ClickEvent, Focusable, Hsla, IntoElement, KeyDownEvent, Role, ScrollHandle, Window, div,
    img, prelude::*, px,
};

pub mod niri_tasks;
#[path = "start_menu.rs"]
pub mod start_menu;
pub mod start_menu_adapter;
pub mod start_menu_overlay;
pub mod start_menu_search;

pub type TaskAction = Arc<dyn Fn(String) + Send + Sync>;
pub type ButtonAction = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskEntry {
    /// Stable desktop app id, also used as the GPUI element/accessibility id.
    pub app_id: String,
    pub label: String,
    /// Existing image resolved from the desktop entry's Icon metadata.
    pub icon_path: Option<PathBuf>,
    pub pinned: bool,
    pub active: bool,
    pub window_count: usize,
    /// Zero-based focused window position in the supplied task list.
    pub focused_window_index: Option<usize>,
    /// Number of minimized windows represented by the task provider.
    pub minimized_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayEntry {
    /// Stable tray item id from the tray adapter.
    pub id: String,
    pub label: String,
    pub visible: bool,
}

#[derive(Clone, Debug)]
pub struct WaffleTaskbarStyle {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted_foreground: Hsla,
    pub accent: Hsla,
    pub active_surface: Hsla,
    pub hover_surface: Hsla,
    pub border: Hsla,
    pub bar_height: f32,
    /// Waffle `waffles.bar.iconSize`, in logical pixels.
    pub icon_size: f32,
    pub button_radius: f32,
    pub horizontal_padding: f32,
    pub item_gap: f32,
}

#[derive(Clone)]
pub struct WaffleTaskbarInput {
    pub style: WaffleTaskbarStyle,
    pub start_open: bool,
    pub task_view_open: bool,
    /// Controlled Start Menu view model. The host owns state and service
    /// subscriptions; when absent, the taskbar remains usable without
    /// presenting fabricated launcher contents.
    pub start_menu: Option<start_menu::StartMenuInput>,
    pub tasks: Vec<TaskEntry>,
    pub tray_items: Vec<TrayEntry>,
    pub clock_time: Option<String>,
    pub clock_date: Option<String>,
    pub unread_notifications: usize,
    pub notifications_silent: bool,
    pub notifications_open: bool,
    pub action_center_open: bool,
    pub clipboard_open: bool,
    pub on_start: ButtonAction,
    pub on_task_view: ButtonAction,
    pub on_task: TaskAction,
    pub on_tray_item: TaskAction,
    pub on_notifications: ButtonAction,
    pub on_action_center: ButtonAction,
    pub on_clipboard: ButtonAction,
}

/// Render the Waffle bottom taskbar. Intended for embedding inside a GPUI
/// layer-shell surface; output sizing/anchoring and service subscriptions are
/// owned by the host/adapters.
pub fn render<T: 'static>(
    input: WaffleTaskbarInput,
    task_scroll: ScrollHandle,
    window: &mut Window,
    cx: &mut Context<T>,
) -> impl IntoElement {
    let style = input.style.clone();
    let start_action = input.on_start.clone();
    let start_active = input.start_open;
    let start_key_action = start_action.clone();
    let start_button = taskbar_button(
        "waffle.start",
        "Start",
        "⊞",
        start_active,
        &style,
        move |_, _, _| start_action(),
        move |event: &KeyDownEvent, _, _| activate_on_key(event, &start_key_action),
    );
    let task_view_action = input.on_task_view.clone();
    let task_view_key_action = task_view_action.clone();
    let task_view_button = taskbar_button(
        "waffle.task-view",
        if input.task_view_open {
            "Close Task View"
        } else {
            "Open Task View"
        },
        "▦",
        input.task_view_open,
        &style,
        move |_, _, _| task_view_action(),
        move |event: &KeyDownEvent, _, _| activate_on_key(event, &task_view_key_action),
    );

    // QML renders pinned apps without windows first, then running apps. Keep
    // supplied order within each group so the component remains a pure view.
    let mut pinned_tasks = Vec::new();
    let mut running_tasks = Vec::new();
    for task in input.tasks {
        if task.window_count > 0 {
            running_tasks.push(task);
        } else if task.pinned {
            pinned_tasks.push(task);
        }
    }
    let pinned_count = pinned_tasks.len();
    let task_count = pinned_tasks.len() + running_tasks.len();
    let has_pinned_and_running = pinned_count > 0 && !running_tasks.is_empty();
    let mut tasks = pinned_tasks
        .into_iter()
        .chain(running_tasks)
        .enumerate()
        .map(|(task_index, task)| {
            let action_id = task.app_id.clone();
            let key_id = task.app_id.clone();
            let action = input.on_task.clone();
            let key_action = action.clone();
            let key_scroll = task_scroll.clone();
            let label = if task.window_count > 1 {
                format!("{} · {}", task.label, task.window_count)
            } else {
                task.label.clone()
            };
            let accessible_label = format!(
                "{label}, {} visible {}, {} minimized{}",
                task.window_count,
                if task.window_count == 1 {
                    "window"
                } else {
                    "windows"
                },
                task.minimized_count,
                if task.active { ", active" } else { "" },
            );
            let window_count = task.window_count.min(5);
            let indicator = div()
                .flex()
                .items_center()
                .gap(px(2.))
                .children((0..window_count).map(|index| {
                    let focused = task.active && task.focused_window_index == Some(index);
                    div()
                        .w(px(if focused { 16. } else { 4. }))
                        .h(px(3.))
                        .rounded(px(2.))
                        .bg(if focused {
                            style.accent
                        } else {
                            style.muted_foreground
                        })
                }))
                .children((0..task.minimized_count.min(3)).map(|_| {
                    div()
                        .w(px(4.))
                        .h(px(3.))
                        .rounded(px(2.))
                        .bg(style.muted_foreground)
                        .opacity(0.5)
                }));
            taskbar_task_button(
                format!("waffle.task.{}", task.app_id),
                accessible_label,
                task.label,
                task.icon_path,
                task.active,
                &style,
                move |_, _, _| action(action_id.clone()),
                move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "tab" {
                        let next_index = if event.keystroke.modifiers.shift {
                            task_index.checked_sub(1)
                        } else if task_index + 1 < task_count {
                            Some(task_index + 1)
                        } else {
                            None
                        };
                        if let Some(next_index) = next_index {
                            let next_separator =
                                usize::from(has_pinned_and_running && next_index >= pinned_count);
                            key_scroll.scroll_to_item(next_index + next_separator);
                        }
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                    }
                    if is_activate_key(event) {
                        key_action(key_id.clone());
                    }
                },
                indicator,
            )
            .into_any_element()
        })
        .collect::<Vec<_>>();
    if has_pinned_and_running {
        tasks.insert(
            pinned_count,
            div()
                .mx(px(4.))
                .h(px(20.))
                .w(px(1.))
                .bg(style.border)
                .into_any_element(),
        );
    }

    let tray = input
        .tray_items
        .into_iter()
        .filter(|item| item.visible)
        .map(|item| {
            let action_id = item.id.clone();
            let key_id = item.id.clone();
            let action = input.on_tray_item.clone();
            let key_action = action.clone();
            taskbar_button(
                format!("waffle.tray.{}", item.id),
                item.label.clone(),
                item.label,
                false,
                &style,
                move |_, _, _| action(action_id.clone()),
                move |event: &KeyDownEvent, _, _| {
                    if is_activate_key(event) {
                        key_action(key_id.clone());
                    }
                },
            )
        });

    let notification_action = input.on_notifications.clone();
    let notification_key_action = notification_action.clone();
    let notification_label = if input.notifications_silent {
        format!(
            "Notifications, silent, {} unread",
            input.unread_notifications
        )
    } else {
        format!("Notifications, {} unread", input.unread_notifications)
    };
    let notification_text = if input.notifications_silent {
        "◌".to_owned()
    } else if input.unread_notifications > 0 {
        format!(
            "◉ {}",
            input.unread_notifications.min(9).to_string()
                + if input.unread_notifications > 9 {
                    "+"
                } else {
                    ""
                }
        )
    } else {
        "◉".to_owned()
    };
    let notification_button = taskbar_button(
        "waffle.notifications",
        notification_label,
        notification_text,
        input.notifications_open,
        &style,
        move |_, _, _| notification_action(),
        move |event: &KeyDownEvent, _, _| activate_on_key(event, &notification_key_action),
    );
    let action_center_action = input.on_action_center.clone();
    let action_center_key_action = action_center_action.clone();
    let action_center_button = taskbar_button(
        "waffle.action-center",
        if input.action_center_open {
            "Close quick settings"
        } else {
            "Open quick settings"
        },
        "⚙",
        input.action_center_open,
        &style,
        move |_, _, _| action_center_action(),
        move |event: &KeyDownEvent, _, _| activate_on_key(event, &action_center_key_action),
    );
    let clipboard_action = input.on_clipboard.clone();
    let clipboard_key_action = clipboard_action.clone();
    let clipboard_button = taskbar_button(
        "waffle.clipboard",
        if input.clipboard_open {
            "Close clipboard history"
        } else {
            "Open clipboard history"
        },
        "▤",
        input.clipboard_open,
        &style,
        move |_, _, _| clipboard_action(),
        move |event: &KeyDownEvent, _, _| activate_on_key(event, &clipboard_key_action),
    );

    let time = input.clock_time.unwrap_or_default();
    let date = input.clock_date.unwrap_or_default();

    let start_menu = if start_active {
        input
            .start_menu
            .map(|menu| {
                let query = menu.query.clone();
                let on_query_changed = menu.on_query_changed.clone();
                let initial_query = query.clone();
                let initial_action = on_query_changed.clone();
                let search_input =
                    window.use_keyed_state("waffle.start-menu.search-input", cx, move |_, cx| {
                        start_menu_search::StartMenuSearch::new(initial_query, initial_action, cx)
                    });
                search_input.update(cx, |search, cx| {
                    search.sync_controlled_query(query, on_query_changed, cx);
                });
                let search_focus = search_input.focus_handle(cx);
                let results_current = search_input.read(cx).confirms_query(&menu.query);
                start_menu::render(menu, search_input, search_focus, results_current)
            })
            .map(|menu| {
                div()
                    .absolute()
                    .left(px(16.))
                    .bottom(px(style.bar_height + 8.))
                    .child(menu)
                    .into_any_element()
            })
    } else {
        None
    };

    div()
        .w_full()
        .h(px(style.bar_height))
        .flex()
        .items_center()
        .gap(px(style.item_gap))
        .px(px(style.horizontal_padding))
        .bg(style.background)
        .text_color(style.foreground)
        .border_t_1()
        .border_color(style.border)
        .child(start_button)
        .child(task_view_button)
        .children(start_menu)
        .child(
            div()
                .id("waffle.taskbar.tasks")
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(style.item_gap))
                .track_scroll(&task_scroll)
                .overflow_x_scroll()
                .children(tasks),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(style.item_gap))
                .children(tray)
                .child(notification_button)
                .child(action_center_button)
                .child(clipboard_button)
                .child(
                    div()
                        .id("waffle.clock")
                        .flex()
                        .flex_col()
                        .items_end()
                        .px(px(8.))
                        .text_color(style.foreground)
                        .aria_label(format!("Time {time}, date {date}"))
                        .role(Role::Label)
                        .child(div().text_sm().child(time))
                        .child(
                            div()
                                .text_xs()
                                .text_color(style.muted_foreground)
                                .child(date),
                        ),
                ),
        )
}

fn taskbar_button(
    id: impl Into<gpui::SharedString>,
    accessible_label: impl Into<gpui::SharedString>,
    text: impl Into<gpui::SharedString>,
    active: bool,
    style: &WaffleTaskbarStyle,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    taskbar_button_inner(
        id,
        accessible_label,
        text,
        active,
        style,
        on_click,
        on_key_down,
        None,
        None,
    )
}

fn taskbar_task_button(
    id: impl Into<gpui::SharedString>,
    accessible_label: impl Into<gpui::SharedString>,
    text: impl Into<gpui::SharedString>,
    icon_path: Option<PathBuf>,
    active: bool,
    style: &WaffleTaskbarStyle,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    indicator: impl IntoElement,
) -> impl IntoElement {
    taskbar_button_inner(
        id,
        accessible_label,
        text,
        active,
        style,
        on_click,
        on_key_down,
        Some(indicator.into_any_element()),
        icon_path,
    )
}

fn taskbar_button_inner(
    id: impl Into<gpui::SharedString>,
    accessible_label: impl Into<gpui::SharedString>,
    text: impl Into<gpui::SharedString>,
    active: bool,
    style: &WaffleTaskbarStyle,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    indicator: Option<gpui::AnyElement>,
    icon_path: Option<PathBuf>,
) -> impl IntoElement {
    let id: gpui::SharedString = id.into();
    let text: gpui::SharedString = text.into();
    let background = if active {
        style.active_surface
    } else {
        style.background
    };
    let border = style.border;
    let hover_surface = style.hover_surface;
    let button = div()
        .id(id)
        .min_h(px(36.))
        .px(px(10.))
        .rounded(px(style.button_radius))
        .flex()
        .items_center()
        .justify_center()
        .bg(background)
        .text_color(style.foreground)
        .role(Role::Button)
        .aria_label(accessible_label)
        .focusable()
        .tab_stop(true)
        .focus_visible(move |focus_style| focus_style.border_1().border_color(border))
        .hover(move |hover_style| hover_style.bg(hover_surface))
        .on_click(on_click)
        .on_key_down(on_key_down);

    if let Some(path) = icon_path {
        // QML AppButton fills the taskbar height. With the default 4px top
        // and bottom insets and 2px horizontal insets its square target is
        // bar_height - 4px wide. The icon itself follows WAppIcon.iconSize.
        let button_width = (style.bar_height - 4.0).max(style.icon_size);
        button
            .relative()
            .w(px(button_width))
            .h(px(style.bar_height))
            .p_0()
            .child(img(path).w(px(style.icon_size)).h(px(style.icon_size)))
            .children(indicator.map(|indicator| {
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(1.))
                    .flex()
                    .justify_center()
                    .child(indicator)
            }))
    } else {
        button
            .relative()
            .h(px(style.bar_height))
            .px(px(10.))
            .child(
                div()
                    .max_w(px(128.))
                    .min_w_0()
                    .truncate()
                    .child(text.to_string()),
            )
            .children(indicator.map(|indicator| {
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(1.))
                    .flex()
                    .justify_center()
                    .child(indicator)
            }))
    }
}

fn is_activate_key(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held
}

fn activate_on_key(event: &KeyDownEvent, action: &ButtonAction) {
    if is_activate_key(event) {
        action();
    }
}
