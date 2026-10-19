//! Locale-derived seven-day strip for the ii Widgets sidebar.
//!
//! The date adapter supplies the exact seven days and localized labels. This
//! renderer owns no date math and never fills in missing locale data.

use std::sync::Arc;

use gpui::{
    AnyElement, ClickEvent, FontWeight, KeyDownEvent, Role, ScrollDelta, ScrollWheelEvent,
    SharedString, div, prelude::*, px,
};

use super::{sidebars::SidebarStyle, week_strip_state::WeekStripSnapshot};

pub type OffsetAction = Arc<dyn Fn(i32) + Send + Sync>;

#[derive(Clone)]
pub struct WeekStripInput {
    pub week_offset: i32,
    pub snapshot: Option<WeekStripSnapshot>,
    pub loading: bool,
    pub error: Option<String>,
    pub style: SidebarStyle,
    pub on_offset_change: OffsetAction,
}

impl std::fmt::Debug for WeekStripInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WeekStripInput")
            .field("week_offset", &self.week_offset)
            .field("snapshot", &self.snapshot)
            .field("loading", &self.loading)
            .field("error", &self.error)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

pub fn render_week_strip(input: WeekStripInput) -> AnyElement {
    let style = input.style;
    let offset = input.week_offset;
    let previous = input.on_offset_change.clone();
    let next = input.on_offset_change.clone();
    let today = input.on_offset_change.clone();
    let wheel = input.on_offset_change.clone();
    let error = input.error.clone();
    let loading = input.loading;
    let unavailable = input.snapshot.is_none() && !loading;
    let snapshot = input.snapshot;
    let month = snapshot.as_ref().filter(|_| offset != 0).map(|snapshot| {
        div()
            .id("ii.week-strip.month")
            .px(px(4.))
            .text_xs()
            .text_color(style.accent)
            .child(snapshot.month_label.clone())
            .into_any_element()
    });
    let days = snapshot.map(|snapshot| {
        snapshot
            .days
            .into_iter()
            .take(7)
            .enumerate()
            .map(|(index, day)| {
                let day_color = if day.is_today {
                    style.accent
                } else if day.is_weekend {
                    style.muted
                } else {
                    style.foreground
                };
                div()
                    .id(SharedString::from(format!("ii.week-strip.day.{index}")))
                    .aria_label(SharedString::from(format!(
                        "{}, {}{}",
                        day.day_name,
                        day.iso_date,
                        if day.is_today { ", today" } else { "" }
                    )))
                    .flex_1()
                    .min_w_0()
                    .h(px(36.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .rounded(px(style.card_radius))
                    .when(day.is_today, |this| this.bg(style.selected))
                    .child(div().text_xs().text_color(style.muted).child(day.day_name))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(if day.is_today {
                                FontWeight::BOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(day_color)
                            .child(day.day_number.to_string()),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>()
    });

    div()
        .id("ii.week-strip")
        .aria_label("Week calendar")
        .w_full()
        .flex()
        .items_center()
        .gap(px(2.))
        .on_scroll_wheel(move |event: &ScrollWheelEvent, _, cx| {
            let y = match event.delta {
                ScrollDelta::Pixels(point) => point.y.as_f32(),
                ScrollDelta::Lines(point) => point.y,
            };
            if y > 0.0 {
                wheel(offset.saturating_sub(1));
                cx.stop_propagation();
            } else if y < 0.0 {
                wheel(offset.saturating_add(1));
                cx.stop_propagation();
            }
        })
        .child(nav_button(
            "ii.week-strip.previous",
            "Previous week",
            "‹",
            &style,
            move || previous(offset.saturating_sub(1)),
        ))
        .when_some(month, |this, month| this.child(month))
        .when(offset != 0, |this| {
            this.child(nav_button(
                "ii.week-strip.today",
                "Go to today",
                "Today",
                &style,
                move || today(0),
            ))
        })
        .when_some(days, |this, days| this.children(days))
        .when(loading, |this| {
            this.child(
                div()
                    .id("ii.week-strip.loading")
                    .role(Role::Status)
                    .aria_label("Loading week calendar")
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(style.muted)
                    .child("Loading week…"),
            )
        })
        .when(unavailable, |this| {
            this.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(style.muted)
                    .child(error.unwrap_or_else(|| "Week data unavailable".into())),
            )
        })
        .child(nav_button(
            "ii.week-strip.next",
            "Next week",
            "›",
            &style,
            move || next(offset.saturating_add(1)),
        ))
        .into_any_element()
}

fn nav_button(
    id: &'static str,
    label: &'static str,
    text: &'static str,
    style: &SidebarStyle,
    action: impl Fn() + Clone + 'static,
) -> impl IntoElement {
    let click_action = action.clone();
    let key_action = action;
    let hover = style.hover;
    let focus = style.border;
    div()
        .id(id)
        .min_w(px(24.))
        .h(px(36.))
        .px(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(style.card_radius))
        .text_color(style.muted)
        .role(Role::Button)
        .aria_label(label)
        .focusable()
        .tab_stop(true)
        .focus_visible(move |s| s.border_1().border_color(focus))
        .hover(move |s| s.bg(hover))
        .on_click(move |_: &ClickEvent, _, _| click_action())
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                key_action();
                cx.stop_propagation();
            }
        })
        .child(SharedString::from(text))
}
