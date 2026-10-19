//! Data-driven Waffle Start Menu presentation.
//!
//! QML anchors: `modules/waffle/startMenu/WaffleStartMenu.qml`,
//! `StartMenuContent.qml`, `StartPageContent.qml`, `SearchPageContent.qml`,
//! `SearchResults.qml`, and `AllAppsContent.qml`. Desktop search, recent
//! windows, and launch operations are deliberately supplied by the caller.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use gpui::{
    App, ClickEvent, Entity, FocusHandle, Hsla, IntoElement, KeyDownEvent, Role, ScrollHandle,
    Window, div, prelude::*, px,
};

use crate::{components::material, theme::MaterialThemeContext};

use super::start_menu_search::StartMenuSearch;

pub type MenuAction = Arc<dyn Fn() + Send + Sync>;
pub type ItemAction = Arc<dyn Fn(String) + Send + Sync>;
pub type QueryAction = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuApp {
    pub id: String,
    pub name: String,
    /// Optional icon glyph or a caller-provided short visual fallback.
    pub icon: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    pub category: String,
    pub detail: String,
    pub icon: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchState {
    #[default]
    Loading,
    Ready,
    Failed,
}

#[derive(Clone, Debug)]
pub struct StartMenuStyle {
    /// Shared Material tokens provide the base surface and focus treatment;
    /// the Waffle palette below remains authoritative for family styling.
    pub material: MaterialThemeContext,
    pub background: Hsla,
    pub surface: Hsla,
    pub surface_hover: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub border: Hsla,
    pub radius: f32,
}

#[derive(Clone)]
pub struct StartMenuInput {
    pub style: StartMenuStyle,
    pub query: String,
    pub results: Vec<SearchResult>,
    pub search_state: SearchState,
    pub selected_result: usize,
    pub pinned_apps: Vec<MenuApp>,
    pub recent_apps: Vec<MenuApp>,
    pub all_apps: Vec<MenuApp>,
    pub show_all_apps: bool,
    pub on_query_changed: QueryAction,
    pub on_select_result: ItemAction,
    pub on_launch_app: ItemAction,
    pub on_all_apps: MenuAction,
    pub on_back: MenuAction,
    pub on_close: MenuAction,
}

/// Render the Waffle launcher. Search is an adapter-owned controlled value;
/// this component emits keystroke text to `on_query_changed` and never runs
/// its own desktop search or invents app records.
pub fn render(
    input: StartMenuInput,
    search_input: Entity<StartMenuSearch>,
    search_focus: FocusHandle,
    current_query: String,
    results_current: bool,
) -> impl IntoElement {
    render_sized(
        input,
        search_input,
        search_focus,
        current_query,
        results_current,
        px(600.),
        px(720.),
    )
}

pub fn render_sized(
    input: StartMenuInput,
    search_input: Entity<StartMenuSearch>,
    search_focus: FocusHandle,
    current_query: String,
    results_current: bool,
    max_width: gpui::Pixels,
    max_height: gpui::Pixels,
) -> impl IntoElement {
    let style = input.style.clone();
    let close_action = input.on_close.clone();
    let key_close_action = close_action.clone();
    let results = input.results.clone();
    let displayed_results = displayed_results(&input.results);
    let selected = clamp_to_displayed(input.selected_result, &displayed_results);
    let selected_index = Arc::new(AtomicUsize::new(selected));
    let live_selected_index = selected_index.clone();
    let query_is_active = !current_query.trim().is_empty();
    let query_is_current = results_current && input.search_state == SearchState::Ready;
    let select_action = input.on_select_result.clone();
    let launch_action = input.on_launch_app.clone();
    let on_menu_key = Arc::new(
        move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
            let key = event.keystroke.key.as_str();
            if key == "escape" {
                key_close_action();
                cx.stop_propagation();
            } else if search_focus.is_focused(window) && matches!(key, "up" | "down") {
                if query_is_active && query_is_current && !results.is_empty() {
                    let current = live_selected_index.load(Ordering::Relaxed);
                    let current_position = displayed_results
                        .iter()
                        .position(|index| *index == current)
                        .unwrap_or(0);
                    let next_position = if key == "down" {
                        (current_position + 1).min(displayed_results.len() - 1)
                    } else {
                        current_position.saturating_sub(1)
                    };
                    let next = displayed_results[next_position];
                    live_selected_index.store(next, Ordering::Relaxed);
                    select_action(results[next].id.clone());
                }
                cx.stop_propagation();
            } else if search_focus.is_focused(window) && key == "enter" {
                let selected = live_selected_index.load(Ordering::Relaxed);
                if query_is_active
                    && query_is_current
                    && !event.is_held
                    && let Some(result) = results.get(selected)
                {
                    launch_action(result.id.clone());
                    close_action();
                }
                // Enter belongs to the launcher even when the query has no result;
                // never let it fall through to an unrelated action or stale item.
                cx.stop_propagation();
            }
        },
    );
    // QML paints a footer/body layer behind a separately outlined input.
    // Material roles cover that layer model; legacy Looks/glass transparency
    // and the derived inputBg tint are not represented by these tokens yet.
    let searching = !current_query.is_empty();
    let search_background = if searching {
        style
            .material
            .colors
            .surface_container
            .unwrap_or(style.surface)
    } else {
        style
            .material
            .colors
            .surface_container_low
            .unwrap_or(style.background)
    };
    let search_input_background = style
        .material
        .colors
        .surface_container_low
        .unwrap_or(style.background);
    let search_outline = style
        .material
        .colors
        .outline_variant
        .unwrap_or(style.border);
    let search = div()
        .id("waffle.start-menu.search")
        .w_full()
        .h(px(64.))
        .px(px(32.))
        .py(px(16.))
        .flex()
        .items_center()
        .bg(search_background)
        .text_color(style.foreground)
        .child(
            material::outlined_pill_surface(
                div().w_full().h(px(32.)).flex().items_center().gap(px(12.)),
                search_input_background,
                search_outline,
                16.0,
            )
            .child(div().text_lg().text_color(style.accent).child("⌕"))
            .child(
                div()
                    .w_full()
                    .h(px(32.))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .child(search_input),
            ),
        );

    let content: gpui::AnyElement = if !current_query.is_empty() {
        search_page(&input, &current_query, results_current).into_any_element()
    } else if !results_current {
        start_page(&input).into_any_element()
    } else if input.show_all_apps {
        all_apps_page(&input).into_any_element()
    } else {
        start_page(&input).into_any_element()
    };

    let surface = material::surface(&style.material, style.background, style.radius + 4.);
    surface
        .id("waffle.start-menu")
        .w(max_width)
        .max_h(max_height)
        .flex()
        .flex_col()
        .gap(px(12.))
        .p(px(20.))
        .bg(style.background)
        .text_color(style.foreground)
        .border_1()
        .border_color(style.border)
        .on_key_down(move |event, window, cx| on_menu_key(event, window, cx))
        .child(search)
        .child(content)
}

fn start_page(input: &StartMenuInput) -> impl IntoElement {
    let style = input.style.clone();
    let apps = input
        .pinned_apps
        .iter()
        .take(18)
        .map(|app| app_tile(app, &style, input.on_launch_app.clone()));
    let recent = input
        .recent_apps
        .iter()
        .take(4)
        .map(|app| app_row(app, &style, input.on_launch_app.clone()));
    let all_apps = input.on_all_apps.clone();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap(px(14.))
        .child(section_heading("Pinned", "All apps  ›", all_apps, &style))
        .child(div().w_full().flex().flex_wrap().gap(px(4.)).children(apps))
        .child(div().flex_1())
        .child(div().text_lg().child("Recommended"))
        .child(if input.recent_apps.is_empty() {
            status_message(
                "No recent apps",
                "Recently used apps will appear here.",
                &style,
            )
            .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .children(recent)
                .into_any_element()
        })
}

fn search_page(
    input: &StartMenuInput,
    current_query: &str,
    results_current: bool,
) -> impl IntoElement {
    let style = input.style.clone();
    let categories = categorized_results(&input.results);
    let displayed_results = categories
        .iter()
        .flat_map(|(_, indices)| indices.iter().copied())
        .collect::<Vec<_>>();
    let selected_result = clamp_to_displayed(input.selected_result, &displayed_results);
    if !results_current {
        return match input.search_state {
            SearchState::Ready | SearchState::Loading => status_message(
                "Searching…",
                "Waiting for results for the current query.",
                &style,
            )
            .into_any_element(),
            SearchState::Failed => status_message(
                "Search unavailable",
                "Try again when the search service is available.",
                &style,
            )
            .into_any_element(),
        };
    }
    match input.search_state {
        SearchState::Ready if current_query.trim().is_empty() => {
            status_message("No results", "No matching apps were found.", &style).into_any_element()
        }
        SearchState::Loading => status_message(
            "Searching…",
            "Looking for matching apps and settings.",
            &style,
        )
        .into_any_element(),
        SearchState::Failed => status_message(
            "Search unavailable",
            "Try again when the search service is available.",
            &style,
        )
        .into_any_element(),
        SearchState::Ready if input.results.is_empty() => {
            status_message("No results", "Try a different search.", &style).into_any_element()
        }
        SearchState::Ready => {
            let mut selected_child = None;
            let mut child_index = 0;
            let mut children = Vec::new();
            for (category, indices) in categories {
                children.push(
                    div()
                        .min_h(px(36.))
                        .flex()
                        .items_center()
                        .child(div().text_lg().child(category))
                        .into_any_element(),
                );
                child_index += 1;
                for index in indices {
                    let result = &input.results[index];
                    if index == selected_result {
                        selected_child = Some(child_index);
                    }
                    children.push(
                        result_row(
                            result,
                            index == selected_result,
                            &style,
                            input.on_select_result.clone(),
                            input.on_launch_app.clone(),
                            input.on_close.clone(),
                            results_current,
                        )
                        .into_any_element(),
                    );
                    child_index += 1;
                }
            }
            let scroll_handle = ScrollHandle::new();
            if let Some(index) = selected_child {
                scroll_handle.scroll_to_item(index);
            }
            div()
                .flex()
                .flex_1()
                .min_h(px(0.))
                .gap(px(12.))
                .child(
                    div()
                        .id("waffle.start-menu.apps-scroll")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h(px(0.))
                        .gap(px(6.))
                        // Pinned GPUI's broadly supported scroll primitive;
                        // restrict input gestures to their dominant axis.
                        .overflow_scroll()
                        .restrict_scroll_to_axis()
                        .track_scroll(&scroll_handle)
                        .focusable()
                        .tab_stop(false)
                        .role(Role::ListBox)
                        .aria_label("Search results")
                        .children(children),
                )
                .child(result_preview(input.results.get(selected_result), &style))
                .into_any_element()
        }
    }
}

fn categorized_results(results: &[SearchResult]) -> Vec<(String, Vec<usize>)> {
    let mut categories: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, result) in results.iter().enumerate() {
        if let Some((_, indices)) = categories
            .iter_mut()
            .find(|(name, _)| *name == result.category)
        {
            indices.push(index);
        } else {
            categories.push((result.category.clone(), vec![index]));
        }
    }
    categories
}

fn displayed_results(results: &[SearchResult]) -> Vec<usize> {
    categorized_results(results)
        .into_iter()
        .flat_map(|(_, indices)| indices)
        .collect()
}

fn clamp_to_displayed(selected: usize, displayed_results: &[usize]) -> usize {
    if displayed_results.contains(&selected) {
        selected
    } else {
        displayed_results.first().copied().unwrap_or(0)
    }
}

fn all_apps_page(input: &StartMenuInput) -> impl IntoElement {
    let style = input.style.clone();
    let on_back = input.on_back.clone();
    let scroll_handle = ScrollHandle::new();
    let apps = input
        .all_apps
        .iter()
        .map(|app| app_row(app, &style, input.on_launch_app.clone()));
    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap(px(8.))
        .child(action_button(
            "‹  Back",
            "Return to pinned apps",
            style.clone(),
            on_back,
        ))
        .child(
            div()
                .id("waffle.start-menu.results-scroll")
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.))
                .gap(px(3.))
                .overflow_scroll()
                .restrict_scroll_to_axis()
                .track_scroll(&scroll_handle)
                .children(apps),
        )
}

fn section_heading(
    title: &'static str,
    action_label: &'static str,
    action: MenuAction,
    style: &StartMenuStyle,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(div().text_lg().child(title))
        .child(action_button(
            action_label,
            "Show all installed apps",
            style.clone(),
            action,
        ))
}

fn app_tile(app: &MenuApp, style: &StartMenuStyle, launch: ItemAction) -> impl IntoElement {
    let id = app.id.clone();
    let key_id = id.clone();
    let key_launch = launch.clone();
    let label = app.name.clone();
    let icon = if app.icon.is_empty() {
        "◈"
    } else {
        app.icon.as_str()
    };
    let hover = style.surface_hover;
    let button = div()
        .id(format!("waffle.start-menu.pinned.{}", app.id))
        .w(px(88.))
        .min_h(px(76.))
        .p(px(6.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .rounded(px(6.))
        .hover(move |style| style.bg(hover));
    material::focusable_button_with_roles(button, style.accent, style.accent)
        .role(Role::Button)
        .aria_label(label.clone())
        .focusable()
        .tab_stop(true)
        .on_click(move |_: &ClickEvent, _, _| launch(id.clone()))
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if is_activate_key(event) {
                key_launch(key_id.clone());
                cx.stop_propagation();
            }
        })
        .child(div().text_2xl().child(icon.to_owned()))
        .child(div().text_xs().text_center().child(label))
}

fn app_row(app: &MenuApp, style: &StartMenuStyle, launch: ItemAction) -> impl IntoElement {
    let id = app.id.clone();
    let key_id = id.clone();
    let key_launch = launch.clone();
    let label = app.name.clone();
    let icon = if app.icon.is_empty() {
        "◈"
    } else {
        app.icon.as_str()
    };
    let hover = style.surface_hover;
    let focus = style.accent;
    let button = div()
        .id(format!("waffle.start-menu.app.{}", app.id))
        .w_full()
        .min_h(px(44.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(px(6.))
        .hover(move |style| style.bg(hover));
    material::focusable_button_with_roles(button, focus, focus)
        .role(Role::Button)
        .aria_label(label.clone())
        .focusable()
        .tab_stop(true)
        .on_click(move |_: &ClickEvent, _, _| launch(id.clone()))
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if is_activate_key(event) {
                key_launch(key_id.clone());
                cx.stop_propagation();
            }
        })
        .child(div().text_lg().child(icon.to_owned()))
        .child(div().flex_1().child(label))
}

fn result_row(
    result: &SearchResult,
    selected: bool,
    style: &StartMenuStyle,
    select: ItemAction,
    launch: ItemAction,
    close: MenuAction,
    results_current: bool,
) -> impl IntoElement {
    let id = result.id.clone();
    let click_id = id.clone();
    let label = result.name.clone();
    let icon = if result.icon.is_empty() {
        "⌕"
    } else {
        result.icon.as_str()
    };
    let bg = if selected {
        style.surface_hover
    } else {
        style.surface
    };
    let focus = style.accent;
    let button = div()
        .id(format!("waffle.start-menu.result.{}", result.id))
        .w_full()
        .min_h(px(48.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(px(6.))
        .bg(bg)
        .hover(move |style| style.bg(bg));
    material::focusable_button_with_roles(button, focus, focus)
        .role(Role::ListBoxOption)
        .aria_label(format!("{}, {}", result.name, result.category))
        .aria_selected(selected)
        .tab_stop(false)
        .on_click(move |_: &ClickEvent, _, _| {
            if results_current {
                select(click_id.clone());
                launch(click_id.clone());
                close();
            }
        })
        .child(div().text_lg().child(icon.to_owned()))
        .child(div().flex_1().child(label))
        .child(
            div()
                .text_xs()
                .text_color(style.muted)
                .child(result.detail.clone()),
        )
}

fn is_activate_key(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held
}

fn result_preview(result: Option<&SearchResult>, style: &StartMenuStyle) -> impl IntoElement {
    let style = style.clone();
    if let Some(result) = result {
        material::surface(&style.material, style.surface, style.radius)
            .w(px(220.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .p(px(16.))
            .bg(style.surface)
            .child(
                div().text_2xl().child(
                    if result.icon.is_empty() {
                        "⌕"
                    } else {
                        &result.icon
                    }
                    .to_owned(),
                ),
            )
            .child(
                div()
                    .id("waffle.start-menu.selected-result-status")
                    .focusable()
                    .tab_stop(false)
                    .role(Role::Status)
                    .text_lg()
                    .text_center()
                    .aria_label(format!("Selected result: {}", result.name))
                    .child(result.name.clone()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(style.muted)
                    .child(result.category.clone()),
            )
    } else {
        div().w(px(220.)).child("")
    }
}

fn status_message(
    title: &'static str,
    detail: &'static str,
    style: &StartMenuStyle,
) -> impl IntoElement {
    div()
        .id("waffle.start-menu.search-status")
        .focusable()
        .tab_stop(false)
        .role(Role::Status)
        .aria_label(title)
        .flex()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .child(div().text_lg().child(title))
        .child(div().text_sm().text_color(style.muted).child(detail))
}

fn action_button(
    label: &'static str,
    accessible_name: &'static str,
    style: StartMenuStyle,
    action: MenuAction,
) -> impl IntoElement {
    let focus = style.accent;
    let hover = style.surface_hover;
    let key_action = action.clone();
    let button = div()
        .id(format!("waffle.start-menu.action.{label}"))
        .min_h(px(32.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .hover(move |style| style.bg(hover));
    material::focusable_button_with_roles(button, focus, focus)
        .role(Role::Button)
        .aria_label(accessible_name)
        .focusable()
        .tab_stop(true)
        .on_click(move |_: &ClickEvent, _, _| action())
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                key_action();
                cx.stop_propagation();
            }
        })
        .child(label)
}
