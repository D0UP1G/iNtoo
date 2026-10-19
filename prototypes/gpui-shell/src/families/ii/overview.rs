//! Data-driven presentation for the non-Orbit ii Overview.
//!
//! QML sources: `modules/overview/Overview.qml`, `OverviewWidget.qml`,
//! `OverviewNiriWidget.qml`, `OverviewAllAppsGrid.qml`, `OverviewDashboard.qml`,
//! `SearchWidget.qml`, `SearchBar.qml`, and `SearchItem.qml`. Runtime services,
//! window thumbnails, app launching, workspace switching, and search editing
//! remain host responsibilities; this module renders only supplied data.

use std::sync::Arc;

use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, KeyDownEvent, Role, SharedString, div,
    prelude::*, px,
};

pub type Action = Arc<dyn Fn() + Send + Sync>;
pub type StringAction = Arc<dyn Fn(String) + Send + Sync>;
pub type IndexAction = Arc<dyn Fn(usize) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewMode {
    Workspaces,
    Applications,
    Dashboard,
    Actions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataState {
    Ready,
    Loading,
    Empty,
    Unavailable,
    Error,
}

#[derive(Debug, Clone)]
pub struct OverviewStyle {
    pub backdrop: Hsla,
    pub surface: Hsla,
    pub card: Hsla,
    pub hover: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub border: Hsla,
    pub radius: f32,
    pub gap: f32,
}

#[derive(Debug, Clone)]
pub struct OverviewItem {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub glyph: Option<String>,
    pub selected: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OverviewGroup {
    pub id: String,
    pub title: String,
    pub items: Vec<OverviewItem>,
}

#[derive(Debug, Clone)]
pub struct DashboardSection {
    pub id: String,
    pub title: String,
    pub state: DataState,
    pub status_message: Option<String>,
    pub items: Vec<OverviewItem>,
}

#[derive(Clone)]
pub struct OverviewInput {
    pub mode: OverviewMode,
    pub state: DataState,
    pub status_message: Option<String>,
    pub query: String,
    pub groups: Vec<OverviewGroup>,
    pub applications: Vec<OverviewItem>,
    pub actions: Vec<OverviewItem>,
    pub dashboard: Vec<DashboardSection>,
    /// Dashboard provider state is independent from the workspace snapshot.
    pub dashboard_state: DataState,
    pub dashboard_status_message: Option<String>,
    pub style: OverviewStyle,
    pub on_mode: Option<Arc<dyn Fn(OverviewMode) + Send + Sync>>,
    pub on_select_item: Option<StringAction>,
    pub on_move_selection: Option<IndexAction>,
    pub on_query_change: Option<StringAction>,
    pub on_close: Option<Action>,
}

/// Render Overview body. `search_editor` is supplied by a host-owned GPUI
/// editor; keeping it external avoids coupling this reusable view to a global
/// search service or another family’s text-input implementation.
pub fn render(input: OverviewInput, search_editor: Option<AnyElement>) -> AnyElement {
    let style = input.style.clone();
    let title = match input.mode {
        OverviewMode::Workspaces => "Workspaces",
        OverviewMode::Applications => "Applications",
        OverviewMode::Dashboard => "Dashboard",
        OverviewMode::Actions => "Actions",
    };
    let body = match input.mode {
        OverviewMode::Workspaces => render_groups(
            &input.groups,
            input.state,
            input.status_message.as_deref(),
            &style,
            input.on_select_item.clone(),
            "No workspaces",
        ),
        OverviewMode::Applications => render_items(
            &input.applications,
            input.state,
            input.status_message.as_deref(),
            &style,
            input.on_select_item.clone(),
            "No applications found",
        ),
        OverviewMode::Dashboard => render_dashboard(
            &input.dashboard,
            input.dashboard_state,
            input.dashboard_status_message.as_deref(),
            &style,
        ),
        OverviewMode::Actions => render_items(
            &input.actions,
            input.state,
            input.status_message.as_deref(),
            &style,
            input.on_select_item.clone(),
            "No actions available",
        ),
    };
    let modes = [
        (OverviewMode::Workspaces, "Workspaces"),
        (OverviewMode::Applications, "Applications"),
        (OverviewMode::Dashboard, "Dashboard"),
        (OverviewMode::Actions, "Actions"),
    ];
    let mode_buttons = modes.into_iter().map(|(mode, label)| {
        let active = mode == input.mode;
        let callback = input.on_mode.clone();
        let key_callback = callback.clone();
        div()
            .id(SharedString::from(format!("ii.overview.mode.{label}")))
            .role(Role::Button)
            .aria_label(format!("Show {label}"))
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .rounded(px(style.radius.min(12.)))
            .bg(if active { style.accent } else { style.card })
            .text_color(if active {
                style.surface
            } else {
                style.foreground
            })
            .px_3()
            .py_2()
            .on_click(move |_, _, _| {
                if let Some(callback) = &callback {
                    callback(mode);
                }
            })
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if is_activation_key(event) {
                    if let Some(callback) = &key_callback {
                        callback(mode);
                    }
                    cx.stop_propagation();
                }
            })
            .child(label)
            .into_any_element()
    });
    let key_close = input.on_close.clone();
    let move_selection = input.on_move_selection.clone();
    let query_callback = input.on_query_change.clone();
    let query_for_key = input.query.clone();
    let close_button = input.on_close.clone();
    let close_key = input.on_close.clone();
    // Do not expose a search landmark or placeholder when the host has not
    // supplied both the actual editor and a query adapter.
    let search_bar = search_editor
        .filter(|_| input.on_query_change.is_some())
        .map(|editor| {
            div()
                .id("ii.overview.search")
                .role(Role::Search)
                .aria_label("Search applications, windows, and workspaces")
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(style.radius.min(12.)))
                .border_1()
                .border_color(style.border)
                .bg(style.card)
                .px_3()
                .py_2()
                .child("⌕")
                .child(editor)
                .into_any_element()
        });
    div()
        .id("ii.overview")
        .size_full()
        .role(Role::Group)
        .aria_label("Overview")
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .rounded(px(style.radius))
        .border_1()
        .border_color(style.border)
        .bg(style.surface)
        .text_color(style.foreground)
        .p_4()
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            let key = event.keystroke.key.as_str();
            match key {
                "escape" => {
                    if !query_for_key.is_empty() {
                        if let Some(callback) = &query_callback {
                            callback(String::new());
                        }
                    } else if let Some(callback) = &key_close {
                        callback();
                    }
                    cx.stop_propagation();
                }
                "up" | "left" if move_selection.is_some() => {
                    if let Some(callback) = &move_selection {
                        callback(usize::MAX);
                    }
                    cx.stop_propagation();
                }
                "down" | "right" if move_selection.is_some() => {
                    if let Some(callback) = &move_selection {
                        callback(1);
                    }
                    cx.stop_propagation();
                }
                _ => {}
            }
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xl()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .id("ii.overview.close")
                        .role(Role::Button)
                        .aria_label("Close Overview")
                        .focusable()
                        .tab_stop(true)
                        .cursor_pointer()
                        .px_3()
                        .py_2()
                        .rounded(px(style.radius.min(10.)))
                        .bg(style.card)
                        .on_click(move |_, _, _| {
                            if let Some(callback) = &close_button {
                                callback();
                            }
                        })
                        .on_key_down(move |event: &KeyDownEvent, _, cx| {
                            if is_activation_key(event) {
                                if let Some(callback) = &close_key {
                                    callback();
                                }
                                cx.stop_propagation();
                            }
                        })
                        .child("Close"),
                ),
        )
        .child(div().flex().gap(px(style.gap)).children(mode_buttons))
        .when_some(search_bar, |this, search_bar| this.child(search_bar))
        .child(body)
        .into_any_element()
}

/// Dashboard cards follow QML's single-column card stack. Each supplied group
/// becomes a titled section; this renderer deliberately has no service access.
fn render_dashboard(
    sections: &[DashboardSection],
    state: DataState,
    message: Option<&str>,
    style: &OverviewStyle,
) -> AnyElement {
    if sections.is_empty() {
        return state_card(
            dashboard_state_title(state),
            message.unwrap_or("The host supplied no dashboard sections"),
            style,
        );
    }
    div()
        .id("ii.overview.dashboard")
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(560.))
        .mx_auto()
        .gap(px(style.gap))
        .overflow_y_scroll()
        .children(sections.iter().map(|section| {
            let content = match section.state {
                DataState::Ready if !section.items.is_empty() => div()
                    .flex()
                    .flex_col()
                    .gap(px(style.gap / 2.))
                    .children(section.items.iter().map(|item| dashboard_card(item, style)))
                    .into_any_element(),
                DataState::Ready | DataState::Empty => dashboard_section_state(
                    &section.id,
                    "No data",
                    section
                        .status_message
                        .as_deref()
                        .unwrap_or("This section has no items"),
                    style,
                ),
                DataState::Loading => dashboard_section_state(
                    &section.id,
                    "Loading",
                    section
                        .status_message
                        .as_deref()
                        .unwrap_or("Waiting for this section's provider"),
                    style,
                ),
                DataState::Unavailable => dashboard_section_state(
                    &section.id,
                    "Unavailable",
                    section
                        .status_message
                        .as_deref()
                        .unwrap_or("No provider is connected"),
                    style,
                ),
                DataState::Error => dashboard_section_state(
                    &section.id,
                    "Unable to load",
                    section
                        .status_message
                        .as_deref()
                        .unwrap_or("The provider returned an error"),
                    style,
                ),
            };
            div()
                .id(SharedString::from(format!(
                    "ii.overview.dashboard.section.{}",
                    section.id
                )))
                .role(Role::Group)
                .aria_label(format!(
                    "{}: {}",
                    section.title,
                    dashboard_state_title(section.state)
                ))
                .flex()
                .flex_col()
                .gap(px(style.gap / 2.))
                .child(
                    div()
                        .text_sm()
                        .text_color(style.muted)
                        .child(section.title.clone()),
                )
                .child(content)
                .into_any_element()
        }))
        .into_any_element()
}

fn dashboard_state_title(state: DataState) -> &'static str {
    match state {
        DataState::Ready => "Dashboard ready",
        DataState::Loading => "Loading dashboard…",
        DataState::Empty => "Dashboard is empty",
        DataState::Unavailable => "Dashboard unavailable",
        DataState::Error => "Dashboard error",
    }
}

fn dashboard_section_state(
    id: &str,
    title: &str,
    message: &str,
    style: &OverviewStyle,
) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "ii.overview.dashboard.section.{id}.state"
        )))
        .role(Role::Status)
        .aria_label(format!("{title}: {message}"))
        .w_full()
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(px(style.radius.min(16.)))
        .border_1()
        .border_color(style.border)
        .bg(style.card)
        .px_3()
        .py_2()
        .child(
            div()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_sm()
                .text_color(style.muted)
                .child(message.to_string()),
        )
        .into_any_element()
}

fn dashboard_card(item: &OverviewItem, style: &OverviewStyle) -> AnyElement {
    let mut card = div()
        .id(SharedString::from(format!(
            "ii.overview.dashboard.card.{}",
            item.id
        )))
        .role(Role::Group)
        .aria_label(accessible_label(item))
        .w_full()
        .flex()
        .items_center()
        .gap(px(12.))
        .rounded(px(style.radius.min(16.)))
        .border_1()
        .border_color(style.border)
        .bg(style.card)
        .p_3();
    if let Some(glyph) = &item.glyph {
        card = card.child(div().text_2xl().child(glyph.clone()));
    }
    let title = div()
        .font_weight(gpui::FontWeight::MEDIUM)
        .when(item.id == "local-clock", |title| title.text_2xl())
        .child(item.title.clone());
    let mut text = div().flex().flex_col().child(title);
    if let Some(subtitle) = &item.subtitle {
        text = text.child(
            div()
                .text_sm()
                .text_color(style.muted)
                .child(subtitle.clone()),
        );
    }
    if let Some(detail) = &item.detail {
        text = text.child(
            div()
                .text_xs()
                .text_color(style.muted)
                .child(detail.clone()),
        );
    }
    card.child(text).into_any_element()
}

fn render_groups(
    groups: &[OverviewGroup],
    state: DataState,
    message: Option<&str>,
    style: &OverviewStyle,
    on_select: Option<StringAction>,
    empty_label: &str,
) -> AnyElement {
    match state {
        DataState::Loading => {
            return state_card(
                "Loading…",
                message.unwrap_or("Overview data is loading"),
                style,
            );
        }
        DataState::Unavailable => {
            return state_card(
                "Unavailable",
                message.unwrap_or("The data provider is unavailable"),
                style,
            );
        }
        DataState::Error => {
            return state_card(
                "Unable to load",
                message.unwrap_or("The data provider returned an error"),
                style,
            );
        }
        DataState::Empty => {
            return state_card(
                empty_label,
                message.unwrap_or("There is nothing to show"),
                style,
            );
        }
        DataState::Ready => {}
    }
    if groups.iter().all(|group| group.items.is_empty()) {
        return state_card(
            empty_label,
            message.unwrap_or("There is nothing to show"),
            style,
        );
    }
    div()
        .id("ii.overview.groups")
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .overflow_y_scroll()
        .children(groups.iter().map(|group| {
            div()
                .id(SharedString::from(format!(
                    "ii.overview.group.{}",
                    group.id
                )))
                .role(Role::Group)
                .aria_label(group.title.clone())
                .flex()
                .flex_col()
                .gap(px(style.gap / 2.))
                .child(
                    div()
                        .text_sm()
                        .text_color(style.muted)
                        .child(group.title.clone()),
                )
                .children(
                    group
                        .items
                        .iter()
                        .map(|item| item_card(item, style, on_select.clone())),
                )
                .into_any_element()
        }))
        .into_any_element()
}

fn render_items(
    items: &[OverviewItem],
    state: DataState,
    message: Option<&str>,
    style: &OverviewStyle,
    on_select: Option<StringAction>,
    empty_label: &str,
) -> AnyElement {
    match state {
        DataState::Loading => {
            return state_card(
                "Loading…",
                message.unwrap_or("Overview data is loading"),
                style,
            );
        }
        DataState::Unavailable => {
            return state_card(
                "Unavailable",
                message.unwrap_or("The data provider is unavailable"),
                style,
            );
        }
        DataState::Error => {
            return state_card(
                "Unable to load",
                message.unwrap_or("The data provider returned an error"),
                style,
            );
        }
        DataState::Empty => {
            return state_card(
                empty_label,
                message.unwrap_or("There is nothing to show"),
                style,
            );
        }
        DataState::Ready => {}
    }
    if items.is_empty() {
        return state_card(
            empty_label,
            message.unwrap_or("There is nothing to show"),
            style,
        );
    }
    div()
        .id("ii.overview.items")
        .flex()
        .flex_wrap()
        .gap(px(style.gap))
        .overflow_y_scroll()
        .children(
            items
                .iter()
                .map(|item| item_card(item, style, on_select.clone())),
        )
        .into_any_element()
}

fn item_card(
    item: &OverviewItem,
    style: &OverviewStyle,
    on_select: Option<StringAction>,
) -> AnyElement {
    let callback = on_select;
    let key_callback = callback.clone();
    let id = item.id.clone();
    let key_id = id.clone();
    let mut card = div()
        .id(SharedString::from(format!("ii.overview.item.{}", item.id)))
        .role(Role::Button)
        .aria_label(accessible_label(item))
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(12.))
        .min_w(px(180.))
        .rounded(px(style.radius.min(16.)))
        .border_1()
        .border_color(if item.selected {
            style.accent
        } else {
            style.border
        })
        .bg(if item.selected {
            style.hover
        } else {
            style.card
        })
        .p_3()
        .on_click(move |_, _, _| {
            if let Some(callback) = &callback {
                callback(id.clone());
            }
        })
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if is_activation_key(event) {
                if let Some(callback) = &key_callback {
                    callback(key_id.clone());
                }
                cx.stop_propagation();
            }
        });
    if let Some(glyph) = &item.glyph {
        card = card.child(div().text_2xl().child(glyph.clone()));
    }
    let mut text = div().flex().flex_col().child(
        div()
            .font_weight(gpui::FontWeight::MEDIUM)
            .child(item.title.clone()),
    );
    if let Some(subtitle) = &item.subtitle {
        text = text.child(
            div()
                .text_sm()
                .text_color(style.muted)
                .child(subtitle.clone()),
        );
    }
    if let Some(detail) = &item.detail {
        text = text.child(
            div()
                .text_xs()
                .text_color(style.muted)
                .child(detail.clone()),
        );
    }
    card.child(text).into_any_element()
}

fn accessible_label(item: &OverviewItem) -> String {
    let mut label = item.title.clone();
    if let Some(subtitle) = &item.subtitle {
        label.push_str(", ");
        label.push_str(subtitle);
    }
    if item.selected {
        label.push_str(", selected");
    }
    label
}

fn state_card(title: &str, message: &str, style: &OverviewStyle) -> AnyElement {
    div()
        .id("ii.overview.state")
        .role(Role::Status)
        .aria_label(format!("{title}: {message}"))
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .rounded(px(style.radius))
        .border_1()
        .border_color(style.border)
        .bg(style.card)
        .p_6()
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_sm()
                .text_color(style.muted)
                .child(message.to_string()),
        )
        .into_any_element()
}

fn is_activation_key(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held
}
