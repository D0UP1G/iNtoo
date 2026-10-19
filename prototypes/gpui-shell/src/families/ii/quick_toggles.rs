//! Data-driven quick toggle presentation for the II system sidebar.
//!
//! Toggle state and mutations are supplied by the adapter. No services are
//! queried here, and empty/unavailable states never create sample switches.

use std::sync::Arc;

use gpui::{
    AnyElement, FontWeight, IntoElement, KeyDownEvent, Role, SharedString,
    StatefulInteractiveElement, Toggled, div, prelude::*, px,
};

use super::sidebars::SidebarStyle;

pub type ToggleAction = Arc<dyn Fn(String, bool) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickTogglesState {
    Loading,
    Ready,
    Empty,
    Unavailable,
    Error,
}

#[derive(Debug, Clone)]
pub struct QuickToggleItem {
    pub id: String,
    pub label: String,
    pub on: bool,
    pub detail: Option<String>,
    pub enabled: bool,
}

#[derive(Clone)]
pub struct QuickTogglesInput {
    pub state: QuickTogglesState,
    pub message: Option<String>,
    pub toggles: Vec<QuickToggleItem>,
    pub style: SidebarStyle,
    /// Receives `(toggle_id, requested_on_state)` for enabled tiles.
    pub on_toggle: ToggleAction,
}

impl std::fmt::Debug for QuickTogglesInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QuickTogglesInput")
            .field("state", &self.state)
            .field("message", &self.message)
            .field("toggles", &self.toggles)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

/// Render a compact, wrapping tile grid; long lists scroll inside the section
/// and never claim the entire sidebar height.
pub fn render_quick_toggles(input: QuickTogglesInput) -> AnyElement {
    let style = input.style;
    let toggles = input.toggles;
    let state = if input.state == QuickTogglesState::Ready && toggles.is_empty() {
        QuickTogglesState::Empty
    } else {
        input.state
    };
    let fallback = match state {
        QuickTogglesState::Loading => "Loading quick toggles…",
        QuickTogglesState::Ready => "",
        QuickTogglesState::Empty => "No quick toggles available",
        QuickTogglesState::Unavailable => "Quick toggles are unavailable",
        QuickTogglesState::Error => "Could not load quick toggles",
    };
    let message = input.message.unwrap_or_else(|| fallback.into());
    let tiles = toggles
        .into_iter()
        .map(|toggle| render_toggle(toggle, &style, input.on_toggle.clone()));

    div()
        .id("ii.sidebar.quick-toggles")
        .w_full()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(style.padding))
        .bg(style.surface)
        .when(state == QuickTogglesState::Ready, |this| {
            this.overflow_y_scroll().child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .gap(px(style.gap))
                    .children(tiles),
            )
        })
        .when(state != QuickTogglesState::Ready, |this| {
            this.child(
                div()
                    .id("ii.sidebar.quick-toggles.status")
                    .w_full()
                    .text_sm()
                    .text_color(style.muted)
                    .role(Role::Status)
                    .aria_label(SharedString::from(message.clone()))
                    .child(message),
            )
        })
        .into_any_element()
}

fn render_toggle(
    toggle: QuickToggleItem,
    style: &SidebarStyle,
    on_toggle: ToggleAction,
) -> impl IntoElement {
    let id = toggle.id.clone();
    let label = toggle.label.clone();
    let detail = toggle.detail.clone();
    let is_on = toggle.on;
    let enabled = toggle.enabled;
    let requested_state = !is_on;
    let background = if is_on { style.selected } else { style.card };
    let foreground = if enabled {
        style.foreground
    } else {
        style.muted
    };
    let indicator = if is_on { "●" } else { "○" };
    let description = match (detail, enabled) {
        (Some(detail), true) => Some(detail),
        (Some(detail), false) => Some(format!("{detail}. This toggle is unavailable.")),
        (None, false) => Some("This toggle is unavailable".into()),
        (None, true) => None,
    };
    let mut tile = div()
        .id(SharedString::from(format!(
            "ii.sidebar.quick-toggle.{}",
            id
        )))
        .w(px(76.0))
        .min_h(px(74.0))
        .p(px(style.gap))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .rounded(px(style.card_radius))
        .bg(background)
        .text_color(foreground)
        .role(Role::Switch)
        .aria_label(SharedString::from(label.clone()))
        .aria_toggled(Toggled::from(is_on))
        .when_some(description, |this, description| {
            this.aria_description(description)
        })
        .child(
            div()
                .size(px(36.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(style.card_radius))
                .bg(if is_on { style.accent } else { style.surface })
                .text_color(if is_on { style.surface } else { style.muted })
                .text_lg()
                .child(indicator),
        )
        .child(
            div()
                .w_full()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(foreground)
                .child(label),
        );

    if enabled {
        let click_id = id.clone();
        let keyboard_id = id;
        let click_callback = on_toggle.clone();
        tile = tile
            .focusable()
            .tab_stop(true)
            .focus_visible({
                let border = style.border;
                move |s| s.border_1().border_color(border)
            })
            .cursor_pointer()
            .hover({
                let hover = style.hover;
                move |s| s.bg(hover)
            })
            .on_click(move |_, _, _| {
                click_callback(click_id.clone(), requested_state);
            })
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                    on_toggle(keyboard_id.clone(), requested_state);
                    cx.stop_propagation();
                }
            });
    } else {
        tile = tile.tab_stop(false);
    }

    tile
}
