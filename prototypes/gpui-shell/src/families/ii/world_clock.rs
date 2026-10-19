//! Static WorldClock cards for the ii Widgets sidebar.
//!
//! All clock values and timezone decisions come from `world_clock_state`; this
//! renderer only presents the supplied snapshot and its explicit errors.

use gpui::{
    AnyElement, FontWeight, Role, SharedString, StatefulInteractiveElement, div, prelude::*, px,
};

use super::{sidebars::SidebarStyle, world_clock_state::WorldClockSnapshot};

#[derive(Clone)]
pub struct WorldClockInput {
    pub snapshot: Option<WorldClockSnapshot>,
    pub auto: bool,
    pub style: SidebarStyle,
}

impl std::fmt::Debug for WorldClockInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorldClockInput")
            .field("snapshot", &self.snapshot)
            .field("auto", &self.auto)
            .field("style", &self.style)
            .finish()
    }
}

pub fn render_world_clock(input: WorldClockInput) -> AnyElement {
    let style = input.style;
    let auto = input
        .snapshot
        .as_ref()
        .map_or(input.auto, |snapshot| snapshot.auto);
    let show_date = input
        .snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.show_date);
    let loading = input.snapshot.is_none();
    let highlight_local = input
        .snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.highlight_local);
    let system_timezone_error = input
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.system_timezone_error.clone());
    let no_rows = input
        .snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.rows.is_empty());
    let rows = input.snapshot.map(|snapshot| {
        snapshot
            .rows
            .into_iter()
            .map(|row| {
                let accented_local = row.is_local && highlight_local;
                let time = row.time.clone();
                let unavailable = row.error.is_some() || time.is_none();
                let offset_and_date = row.utc_offset.as_ref().map(|offset| {
                    let mut label = format!("GMT{offset}");
                    if show_date {
                        if let Some(date) = row.date.as_ref() {
                            label.push_str("  ·  ");
                            label.push_str(date);
                        }
                    }
                    label
                });
                let delta = match row.day_delta {
                    Some(1) => Some("Tomorrow"),
                    Some(-1) => Some("Yesterday"),
                    _ => None,
                };
                let accessible = accessible_row_label(
                    &row.city,
                    &row.timezone,
                    row.is_local,
                    row.is_daytime,
                    row.time.as_deref(),
                    row.utc_offset.as_deref(),
                    if show_date { row.date.as_deref() } else { None },
                    delta,
                    row.error.as_deref(),
                );
                let time_color = if accented_local {
                    style.accent
                } else {
                    style.foreground
                };
                let day_symbol = match row.is_daytime {
                    Some(true) | None => "light_mode",
                    Some(false) => "dark_mode",
                };
                let day_color = match row.is_daytime {
                    Some(true) => style.accent,
                    Some(false) | None => style.muted,
                };
                div()
                    .id(SharedString::from(format!(
                        "ii.world-clock.{}",
                        row.timezone
                    )))
                    .role(Role::Group)
                    .aria_label(SharedString::from(accessible))
                    .w_full()
                    .min_w_0()
                    .min_h(px(58.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(style.card_radius))
                    .bg(style.surface)
                    .child(
                        div()
                            .w(px(3.))
                            .h(px(40.))
                            .rounded(px(2.))
                            .bg(if accented_local {
                                style.accent
                            } else {
                                style.surface.opacity(0.)
                            }),
                    )
                    .child(
                        div()
                            .size(px(30.))
                            .rounded(px(15.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(if accented_local {
                                style.accent.opacity(0.14)
                            } else {
                                style.card
                            })
                            .text_color(day_color)
                            .font_family("Material Symbols Rounded")
                            .text_size(px(16.))
                            .child(day_symbol),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(style.foreground)
                                            .child(row.city.clone()),
                                    )
                                    .when(accented_local, |this| {
                                        this.child(
                                            div()
                                                .px(px(5.))
                                                .py(px(1.))
                                                .rounded(px(8.))
                                                .bg(style.accent.opacity(0.16))
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(style.accent)
                                                .child("LOCAL"),
                                        )
                                    }),
                            )
                            .when_some(offset_and_date, |this, label| {
                                this.child(
                                    div()
                                        .min_w_0()
                                        .text_xs()
                                        .text_color(style.muted)
                                        .child(label),
                                )
                            })
                            .when_some(row.error.clone(), |this, error| {
                                this.child(
                                    div()
                                        .min_w_0()
                                        .text_xs()
                                        .text_color(style.muted)
                                        .child(error),
                                )
                            }),
                    )
                    .child(
                        div()
                            .min_w(px(56.))
                            .flex()
                            .flex_col()
                            .items_end()
                            .when_some(time, |this, time| {
                                this.child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(time_color)
                                        .child(time),
                                )
                            })
                            .when(unavailable, |this| {
                                this.child(
                                    div().text_xs().text_color(style.muted).child("Unavailable"),
                                )
                            })
                            .when_some(delta, |this, delta| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(style.accent)
                                        .child(delta),
                                )
                            }),
                    )
            })
            .collect::<Vec<_>>()
    });

    div()
        .id("ii.world-clock.card")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(style.padding))
        .bg(style.card)
        .rounded(px(style.card_radius))
        .text_color(style.foreground)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .text_color(style.accent)
                        .font_family("Material Symbols Rounded")
                        .text_size(px(16.))
                        .child("language"),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(style.foreground)
                        .child("World Clock"),
                )
                .child(div().flex_1())
                .when(auto, |this| {
                    this.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .px(px(7.))
                            .py(px(3.))
                            .rounded(px(12.))
                            .border_1()
                            .border_color(style.border.opacity(0.5))
                            .text_xs()
                            .text_color(style.muted)
                            .child(
                                div()
                                    .font_family("Material Symbols Rounded")
                                    .text_size(px(11.))
                                    .child("my_location"),
                            )
                            .child("Auto"),
                    )
                }),
        )
        .when_some(system_timezone_error, |this, error| {
            this.child(
                div()
                    .id("ii.world-clock.local-timezone-error")
                    .role(Role::Status)
                    .text_xs()
                    .text_color(style.muted)
                    .child(format!("Local timezone unavailable: {error}")),
            )
        })
        .when(loading, |this| {
            this.child(
                div()
                    .id("ii.world-clock.loading")
                    .role(Role::Status)
                    .text_sm()
                    .text_color(style.muted)
                    .child("Loading world clock…"),
            )
        })
        .when(no_rows, |this| {
            this.child(
                div()
                    .id("ii.world-clock.empty")
                    .role(Role::Status)
                    .text_sm()
                    .text_color(style.muted)
                    .child("No timezones to show"),
            )
        })
        .when_some(rows, |this, rows| this.children(rows))
        .into_any_element()
}

fn accessible_row_label(
    city: &str,
    timezone: &str,
    local: bool,
    daytime: Option<bool>,
    time: Option<&str>,
    offset: Option<&str>,
    date: Option<&str>,
    delta: Option<&str>,
    error: Option<&str>,
) -> String {
    let mut label = format!("{city}, {timezone}");
    if local {
        label.push_str(", local time");
    }
    if let Some(time) = time {
        label.push_str(": ");
        label.push_str(time);
    }
    if let Some(offset) = offset {
        label.push_str(", GMT");
        label.push_str(offset);
    }
    if let Some(date) = date {
        label.push_str(", ");
        label.push_str(date);
    }
    if let Some(delta) = delta {
        label.push_str(", ");
        label.push_str(delta);
    }
    if let Some(daytime) = daytime {
        label.push_str(if daytime { ", daytime" } else { ", nighttime" });
    }
    if let Some(error) = error {
        label.push_str(", unavailable: ");
        label.push_str(error);
    } else if time.is_none() {
        label.push_str(", unavailable");
    }
    label
}
