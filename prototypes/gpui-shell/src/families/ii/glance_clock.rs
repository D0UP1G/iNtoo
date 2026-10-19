//! Clock-only visual from the left sidebar's Widgets glance header.
//!
//! The provider currently supplies one opaque, process-locale formatted
//! string; this component deliberately does not split or reformat it.

use gpui::{AnyElement, FontWeight, IntoElement, SharedString, div, prelude::*, px};

use super::sidebars::SidebarStyle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlanceClockState {
    Loading,
    Unavailable,
    Ready(String),
}

pub fn render_glance_clock(state: GlanceClockState, style: &SidebarStyle) -> AnyElement {
    let (label, ready) = match state {
        GlanceClockState::Loading => ("Loading clock…".to_owned(), false),
        GlanceClockState::Unavailable => ("Clock unavailable".to_owned(), false),
        GlanceClockState::Ready(label) => (label, true),
    };
    let accessible_label = if ready {
        format!("Current time and date: {label}")
    } else {
        label.clone()
    };
    div()
        .id("ii.sidebar.left.widgets.glance-clock")
        .w_full()
        .px(px(16.0))
        .pt(px(12.0))
        .pb(px(8.0))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(4.0))
        .text_color(if ready { style.accent } else { style.muted })
        .aria_label(SharedString::from(accessible_label))
        .child(
            div()
                .text_3xl()
                .font_weight(FontWeight::MEDIUM)
                .child(label),
        )
        .into_any_element()
}
