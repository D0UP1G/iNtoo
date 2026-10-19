//! Data-driven Waffle Notification Center presentation.
//!
//! Mirrors the QML notification pane and calendar pane. Platform notification,
//! calendar, locale, and focus/window services are supplied by the surface host.
use std::sync::Arc;

use gpui::{
    ClickEvent, Hsla, InteractiveElement, IntoElement, KeyDownEvent, Pixels, Role, div, prelude::*,
    px,
};

pub type Action = Arc<dyn Fn() + Send + Sync>;
pub type ItemAction = Arc<dyn Fn(String) + Send + Sync>;
pub type DateAction = Arc<dyn Fn(CalendarDate) + Send + Sync>;

#[derive(Clone, Debug)]
pub struct NotificationCenterStyle {
    pub panel: Hsla,
    pub surface: Hsla,
    pub input: Hsla,
    pub hover: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
    pub border: Hsla,
    pub radius: f32,
    pub scale: f32,
}

#[derive(Clone, Debug)]
pub struct NotificationItem {
    pub id: String,
    pub title: String,
    pub body: String,
    pub timestamp: String,
    /// `None` when the source has no read/unread policy.
    pub unread: Option<bool>,
    pub critical: bool,
    pub actions: Vec<NotificationAction>,
}

#[derive(Clone, Debug)]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct NotificationGroup {
    pub app_id: String,
    pub app_name: String,
    /// Caller-provided icon text (usually a short glyph); empty means no icon.
    pub app_icon: String,
    pub notifications: Vec<NotificationItem>,
    pub expanded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CalendarDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

#[derive(Clone, Debug)]
pub struct CalendarDay {
    pub date: CalendarDate,
    pub label: String,
    pub in_month: bool,
    pub today: bool,
    pub selected: bool,
    /// Event dot colors are resolved by the caller from local and synced events.
    pub event_colors: Vec<Hsla>,
}

#[derive(Clone, Debug)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub time: Option<String>,
    pub all_day: bool,
    pub source_label: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FocusControls {
    pub running: bool,
    pub focus_minutes: Option<u32>,
    pub can_decrease: bool,
    pub can_increase: bool,
    pub primary_label: String,
}

#[derive(Clone)]
pub struct NotificationCenterInput {
    pub style: NotificationCenterStyle,
    /// Available output height after the host accounts for anchors and margins.
    pub available_height: Pixels,
    pub groups: Vec<NotificationGroup>,
    pub notification_count: usize,
    /// `None` when the source has no read/unread policy.
    pub unread_count: Option<usize>,
    pub silent: bool,
    pub search_query: String,
    pub notification_state: DataState,
    pub month_label: String,
    /// Localized weekday labels ordered Sunday through Saturday.
    pub weekdays: Vec<String>,
    /// Visible calendar cells (normally six weeks / 42 days, including spillover days).
    pub calendar_days: Vec<CalendarDay>,
    pub selected_date: Option<CalendarDate>,
    pub selected_date_label: String,
    pub selected_events: Vec<CalendarEvent>,
    pub calendar_state: DataState,
    pub collapsed: bool,
    pub focus_controls: Option<FocusControls>,
    pub on_close: Action,
    pub on_toggle_silent: Action,
    pub on_clear_all: Action,
    pub on_dismiss_notification: ItemAction,
    pub on_dismiss_group: ItemAction,
    pub on_notification_action: Arc<dyn Fn(String, String) + Send + Sync>,
    pub on_toggle_group: ItemAction,
    pub on_previous_month: Action,
    pub on_next_month: Action,
    pub on_select_date: DateAction,
    pub on_toggle_calendar: Action,
    pub on_adjust_focus: Arc<dyn Fn(i32) + Send + Sync>,
    pub on_focus_action: Action,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DataState {
    #[default]
    Ready,
    Loading,
    Unavailable,
    /// Live notifications received during this GPUI process only.
    ReadySessionOnly,
}

/// Render the Waffle right-side center body. An exclusive-keyboard overlay,
/// click-away surface, and notification/calendar service wiring belong to host.
pub fn render(input: NotificationCenterInput) -> impl IntoElement {
    let s = input.style.clone();
    let close = input.on_close.clone();
    let panel_width = px(440. * s.scale);
    let notif_max_height = px(560. * s.scale);
    let header = notification_header(&input);
    let search = if input.notification_count > 3 {
        Some(search_box(&input))
    } else {
        None
    };
    let dnd =
        (input.silent && input.notification_state == DataState::Ready).then(|| dnd_banner(&input));
    let groups = input
        .groups
        .iter()
        .map(|group| group_card(group, &input))
        .collect::<Vec<_>>();
    let list = if input.notification_state == DataState::Loading && groups.is_empty() {
        status_message("Notifications", "Loading notifications…", &s)
    } else if input.notification_state == DataState::Unavailable {
        status_message("Notifications", "Notification service unavailable", &s)
    } else if groups.is_empty() {
        if input.notification_state == DataState::ReadySessionOnly {
            session_empty_state(&s)
        } else {
            empty_state(&s)
        }
    } else {
        let mut list = div()
            .id("waffle.notification-center.list")
            .flex()
            .flex_col()
            .gap(px(4. * s.scale));
        if input.notification_state == DataState::ReadySessionOnly {
            list = list.child(session_disclosure(&s));
        }
        list = list.children(groups);
        list.into_any_element()
    };
    let notification_pane = div()
        .w(panel_width)
        .max_h(notif_max_height)
        .flex()
        .flex_col()
        .gap(px(8. * s.scale))
        .rounded(px(s.radius * s.scale))
        .bg(s.panel)
        .p(px(8. * s.scale))
        .child(header)
        .children(search)
        .children(dnd)
        .child(
            div()
                .id("waffle.notification-center.list-viewport")
                .flex_1()
                .min_h(px(80. * s.scale))
                .overflow_y_scroll()
                .child(list),
        );
    let calendar = calendar_pane(&input);
    div()
        .id("waffle.notification-center")
        .w(panel_width)
        .max_h(input.available_height)
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .gap(px(12. * s.scale))
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if event.keystroke.key == "escape" {
                close();
                cx.stop_propagation();
            }
        })
        .child(notification_pane)
        .child(calendar)
}

fn notification_header(input: &NotificationCenterInput) -> impl IntoElement {
    let s = &input.style;
    let title = if input.notification_count > 0 {
        format!("Notifications ({})", input.notification_count)
    } else {
        "Notifications".into()
    };
    let silent = input.silent;
    let toggle_silent = input.on_toggle_silent.clone();
    let clear = input.on_clear_all.clone();
    div()
        .flex()
        .items_center()
        .gap(px(8. * s.scale))
        .px(px(4. * s.scale))
        .py(px(4. * s.scale))
        .child(label(title, 18. * s.scale, s.foreground, true))
        .children(
            input
                .unread_count
                .filter(|count| *count > 0)
                .map(|count| pill(format!("{count} unread"), s).into_any_element()),
        )
        .child(div().flex_1())
        .children((input.notification_state == DataState::Ready).then(|| {
            button(
                "waffle.notification-center.dnd",
                if silent { "◉" } else { "◌" },
                if silent {
                    "Do Not Disturb is on"
                } else {
                    "Enable Do Not Disturb"
                },
                s,
                move || toggle_silent(),
            )
            .into_any_element()
        }))
        .child(if input.notification_count > 0 {
            button(
                "waffle.notification-center.clear-all",
                if input.notification_state == DataState::ReadySessionOnly {
                    "Dismiss active"
                } else {
                    "Clear all"
                },
                if input.notification_state == DataState::ReadySessionOnly {
                    "Dismiss all currently active notifications"
                } else {
                    "Clear all notifications"
                },
                s,
                move || clear(),
            )
            .into_any_element()
        } else {
            div().into_any_element()
        })
}

fn search_box(input: &NotificationCenterInput) -> impl IntoElement {
    let s = &input.style;
    let query = input.search_query.clone();
    // The host-owned editor is not connected yet, so keep this as a visual label
    // rather than presenting a decorative control as an editable text field.
    div()
        .id("waffle.notification-center.search")
        .aria_label("Search notifications; editing is not connected")
        .w_full()
        .h(px(32. * s.scale))
        .flex()
        .items_center()
        .gap(px(6. * s.scale))
        .rounded(px(16. * s.scale))
        .border_1()
        .border_color(s.border)
        .bg(s.input)
        .px(px(10. * s.scale))
        .child(label("⌕", 16. * s.scale, s.muted, false))
        .child(if query.is_empty() {
            label("Search notifications", 13. * s.scale, s.muted, false).into_any_element()
        } else {
            label(query, 13. * s.scale, s.foreground, false).into_any_element()
        })
}

fn dnd_banner(input: &NotificationCenterInput) -> impl IntoElement {
    let s = &input.style;
    let turn_off = input.on_toggle_silent.clone();
    div()
        .flex()
        .items_center()
        .gap(px(8. * s.scale))
        .rounded(px(s.radius * s.scale))
        .border_1()
        .border_color(s.warning)
        .bg(s.warning.opacity(0.15))
        .p(px(8. * s.scale))
        .child(label("◉", 14. * s.scale, s.warning, true))
        .child(label(
            "Do Not Disturb is enabled",
            13. * s.scale,
            s.warning,
            false,
        ))
        .child(div().flex_1())
        .child(button(
            "waffle.notification-center.dnd-off",
            "Turn off",
            "Disable Do Not Disturb",
            s,
            move || turn_off(),
        ))
}

fn group_card(group: &NotificationGroup, input: &NotificationCenterInput) -> impl IntoElement {
    let s = &input.style;
    let app_id = group.app_id.clone();
    let toggle = input.on_toggle_group.clone();
    let dismiss = input.on_dismiss_group.clone();
    let count = group.notifications.len();
    let critical = group
        .notifications
        .iter()
        .any(|notification| notification.critical);
    let visible_items = if group.expanded {
        group.notifications.iter().rev().take(5).collect::<Vec<_>>()
    } else {
        group.notifications.iter().rev().take(1).collect::<Vec<_>>()
    };
    let item_rows = visible_items
        .into_iter()
        .map(|notification| notification_card(notification, group, input))
        .collect::<Vec<_>>();
    let app_icon = if group.app_icon.is_empty() {
        "●"
    } else {
        &group.app_icon
    };
    let mut header = div()
        .id(format!("waffle.notification-center.group.{}", group.app_id))
        .w_full()
        .flex()
        .items_center()
        .gap(px(8. * s.scale))
        .child(label(app_icon.to_owned(), 18. * s.scale, s.accent, true))
        .child(label(
            group.app_name.clone(),
            14. * s.scale,
            if critical { s.danger } else { s.foreground },
            critical,
        ))
        .child(div().flex_1());
    if count > 1 {
        header = header.child(pill(count.to_string(), s));
        let toggle_group = toggle.clone();
        let key_toggle_group = toggle.clone();
        let group_id_for_key = group.app_id.clone();
        let expansion_text = if group.expanded {
            "See fewer".to_owned()
        } else {
            format!("+{} notifications", count - 1)
        };
        header = header.child(
            div()
                .id(format!(
                    "waffle.notification-center.group-toggle.{}",
                    group.app_id
                ))
                .px(px(8. * s.scale))
                .py(px(4. * s.scale))
                .rounded(px(s.radius * s.scale))
                .text_color(s.muted)
                .child(label(expansion_text, 12. * s.scale, s.muted, false))
                .role(Role::Button)
                .aria_label(if group.expanded {
                    format!("Show fewer notifications from {}", group.app_name)
                } else {
                    format!("Show all {} notifications from {}", count, group.app_name)
                })
                .focusable()
                .tab_stop(true)
                .on_click(move |_: &ClickEvent, _, _| toggle_group(app_id.clone()))
                .on_key_down(move |event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                        key_toggle_group(group_id_for_key.clone());
                        cx.stop_propagation();
                    }
                }),
        );
    }
    let dismiss_group = group.app_id.clone();
    header = header.child(button(
        format!("waffle.notification-center.dismiss-group.{}", group.app_id),
        "×",
        format!("Dismiss all from {}", group.app_name),
        s,
        move || dismiss(dismiss_group.clone()),
    ));
    div()
        .flex()
        .flex_col()
        .gap(px(4. * s.scale))
        .px(px(10. * s.scale))
        .py(px(8. * s.scale))
        .rounded(px(s.radius * s.scale))
        .bg(s.surface)
        .child(header)
        .children(item_rows)
}

fn notification_card(
    notification: &NotificationItem,
    group: &NotificationGroup,
    input: &NotificationCenterInput,
) -> impl IntoElement {
    let s = &input.style;
    let dismiss = input.on_dismiss_notification.clone();
    let id = notification.id.clone();
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(4. * s.scale))
        .flex_1()
        .child(label(
            notification.title.clone(),
            14. * s.scale,
            s.foreground,
            notification.unread == Some(true),
        ));
    if !notification.body.is_empty() {
        body = body.child(label(
            notification.body.clone(),
            12. * s.scale,
            s.muted,
            false,
        ));
    }
    if !notification.timestamp.is_empty() {
        body = body.child(label(
            notification.timestamp.clone(),
            11. * s.scale,
            s.muted,
            false,
        ));
    }
    let mut row = div()
        .id(format!(
            "waffle.notification-center.item.{}",
            notification.id
        ))
        .w_full()
        .flex()
        .items_start()
        .gap(px(8. * s.scale))
        .rounded(px(s.radius * s.scale))
        .px(px(8. * s.scale))
        .py(px(6. * s.scale))
        .bg(if notification.unread == Some(true) {
            s.accent.opacity(0.08)
        } else {
            s.surface
        })
        .child(if notification.unread == Some(true) {
            label("●", 8. * s.scale, s.accent, true).into_any_element()
        } else {
            div().w(px(8. * s.scale)).into_any_element()
        })
        .child(body)
        .child(button(
            format!("waffle.notification-center.dismiss.{}", notification.id),
            "×",
            format!("Dismiss notification from {}", group.app_name),
            s,
            move || dismiss(id.clone()),
        ));
    for action in &notification.actions {
        let notification_id = notification.id.clone();
        let action_id = action.id.clone();
        let callback = input.on_notification_action.clone();
        row = row.child(button(
            format!(
                "waffle.notification-center.action.{}.{}",
                notification.id, action.id
            ),
            action.label.clone(),
            action.label.clone(),
            s,
            move || callback(notification_id.clone(), action_id.clone()),
        ));
    }
    row
}

fn empty_state(s: &NotificationCenterStyle) -> gpui::AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8. * s.scale))
        .py(px(28. * s.scale))
        .child(label("◉", 38. * s.scale, s.muted, false))
        .child(label("No new notifications", 16. * s.scale, s.muted, false))
        .child(label(
            "You're all caught up!",
            13. * s.scale,
            s.muted,
            false,
        ))
        .into_any_element()
}

fn session_empty_state(s: &NotificationCenterStyle) -> gpui::AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8. * s.scale))
        .py(px(28. * s.scale))
        .child(label("◉", 38. * s.scale, s.muted, false))
        .child(label(
            "No active notifications",
            16. * s.scale,
            s.muted,
            false,
        ))
        .child(label(
            "Only currently active notifications received during this GPUI session appear here. Closed or expired items aren't retained.",
            13. * s.scale,
            s.muted,
            false,
        ))
        .into_any_element()
}

fn session_disclosure(s: &NotificationCenterStyle) -> gpui::AnyElement {
    div()
        .w_full()
        .rounded(px(s.radius * s.scale))
        .border_1()
        .border_color(s.border)
        .bg(s.input)
        .px(px(10. * s.scale))
        .py(px(6. * s.scale))
        .child(label(
            "Only currently active notifications received during this GPUI session appear here. Closed or expired items aren't retained.",
            11. * s.scale,
            s.muted,
            false,
        ))
        .into_any_element()
}

fn status_message(title: &str, detail: &str, s: &NotificationCenterStyle) -> gpui::AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6. * s.scale))
        .py(px(20. * s.scale))
        .child(label(title, 15. * s.scale, s.foreground, true))
        .child(label(detail, 13. * s.scale, s.muted, false))
        .into_any_element()
}

fn calendar_pane(input: &NotificationCenterInput) -> impl IntoElement {
    let s = &input.style;
    let previous = input.on_previous_month.clone();
    let next = input.on_next_month.clone();
    let mut weekday_row =
        div()
            .w_full()
            .flex()
            .justify_between()
            .children(input.weekdays.iter().map(|weekday| {
                div()
                    .w(px(42. * s.scale))
                    .flex()
                    .justify_center()
                    .child(label(weekday.clone(), 11. * s.scale, s.muted, false))
            }));
    if input.weekdays.is_empty() {
        weekday_row = weekday_row.child(label("", 11., s.muted, false));
    }
    let days = input
        .calendar_days
        .iter()
        .map(|day| calendar_day(day, input))
        .collect::<Vec<_>>();
    let calendar_body = if input.calendar_state == DataState::Unavailable {
        status_message("Calendar", "Calendar data unavailable", s)
    } else if input.calendar_state == DataState::Loading && input.calendar_days.is_empty() {
        status_message("Calendar", "Loading calendar…", s)
    } else {
        div()
            .flex()
            .flex_col()
            .gap(px(6. * s.scale))
            .child(weekday_row)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_between()
                    .gap_y(px(2. * s.scale))
                    .children(days),
            )
            .into_any_element()
    };
    let title = input.month_label.clone();
    let unavailable = input.calendar_state == DataState::Unavailable;
    let prev_button = (!unavailable).then(|| {
        button(
            "waffle.notification-center.previous-month",
            "‹",
            "Previous month",
            s,
            move || previous(),
        )
        .into_any_element()
    });
    let next_button = (!unavailable).then(|| {
        button(
            "waffle.notification-center.next-month",
            "›",
            "Next month",
            s,
            move || next(),
        )
        .into_any_element()
    });
    let collapsed = input.collapsed;
    let on_collapse = input.on_toggle_calendar.clone();
    let header = div()
        .flex()
        .items_center()
        .gap(px(4. * s.scale))
        .children(prev_button)
        .child(label(title, 16. * s.scale, s.foreground, true))
        .children(next_button)
        .child(div().flex_1())
        .children((!unavailable).then(|| {
            button(
                "waffle.notification-center.calendar-collapse",
                if collapsed { "⌃" } else { "⌄" },
                if collapsed {
                    "Expand calendar"
                } else {
                    "Collapse calendar"
                },
                s,
                move || on_collapse(),
            )
            .into_any_element()
        }));
    let mut pane = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10. * s.scale))
        .rounded(px(s.radius * s.scale))
        .bg(s.panel)
        .px(px(16. * s.scale))
        .py(px(10. * s.scale))
        .child(header);
    if !input.collapsed {
        pane = pane.child(calendar_body);
        if let Some(date) = input.selected_date.as_ref() {
            pane = pane.child(selected_date_events(input, date));
        }
        pane = pane.child(focus_footer(input));
    }
    pane
}

fn calendar_day(day: &CalendarDay, input: &NotificationCenterInput) -> impl IntoElement {
    let s = &input.style;
    let selected = day.selected;
    let today = day.today;
    let enabled = day.in_month;
    let date = day.date.clone();
    let select = input.on_select_date.clone();
    let date_label = format!("{}, {}", day.label, day.date.month);
    let mut cell = div()
        .id(format!(
            "waffle.notification-center.date.{}-{}-{}",
            day.date.year, day.date.month, day.date.day
        ))
        .w(px(42. * s.scale))
        .h(px(38. * s.scale))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(2. * s.scale))
        .rounded(px(s.radius * s.scale))
        .text_color(if selected { s.panel } else { s.foreground })
        .bg(if selected { s.accent } else { s.panel })
        .opacity(if enabled { 1. } else { 0.45 })
        .child(label(
            day.label.clone(),
            13. * s.scale,
            if selected { s.panel } else { s.foreground },
            today || selected,
        ));
    if today && !selected {
        cell = cell.border_1().border_color(s.accent);
    }
    if enabled {
        let click_select = select.clone();
        let click_date = date.clone();
        cell = cell
            .role(Role::Button)
            .aria_label(date_label)
            .focusable()
            .tab_stop(true)
            .on_click(move |_: &ClickEvent, _, _| click_select(click_date.clone()))
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                    select(date.clone());
                    cx.stop_propagation();
                }
            });
    } else {
        cell = cell.aria_label(format!("{date_label}, outside displayed month"));
    }
    if !day.event_colors.is_empty() {
        cell = cell.child(
            div().flex().gap(px(2.)).children(
                day.event_colors
                    .iter()
                    .take(3)
                    .map(|color| div().w(px(4.)).h(px(4.)).rounded_full().bg(*color)),
            ),
        );
    } else {
        cell = cell.child(div().h(px(4.)));
    }
    cell
}

fn selected_date_events(input: &NotificationCenterInput, date: &CalendarDate) -> gpui::AnyElement {
    let s = &input.style;
    let mut content = div()
        .flex()
        .flex_col()
        .gap(px(4. * s.scale))
        .border_t_1()
        .border_color(s.border)
        .pt(px(8. * s.scale))
        .child(label(
            if input.selected_date_label.is_empty() {
                format!("{} / {} / {}", date.month, date.day, date.year)
            } else {
                input.selected_date_label.clone()
            },
            14. * s.scale,
            s.foreground,
            true,
        ));
    if input.selected_events.is_empty() {
        content = content.child(label("No events", 12. * s.scale, s.muted, false));
    } else {
        for event in &input.selected_events {
            let mut row = div()
                .flex()
                .items_center()
                .gap(px(8. * s.scale))
                .child(label(
                    event.title.clone(),
                    12. * s.scale,
                    s.foreground,
                    false,
                ));
            if let Some(time) = &event.time {
                row = row.child(label(time.clone(), 11. * s.scale, s.muted, false));
            } else if event.all_day {
                row = row.child(label("All day", 11. * s.scale, s.muted, false));
            }
            if let Some(source) = &event.source_label {
                row = row.child(label(source.clone(), 10. * s.scale, s.muted, false));
            }
            content = content.child(row);
        }
    }
    content.into_any_element()
}

fn focus_footer(input: &NotificationCenterInput) -> gpui::AnyElement {
    let s = &input.style;
    let Some(controls) = input.focus_controls.as_ref() else {
        return div().into_any_element();
    };
    let mut row = div()
        .flex()
        .items_center()
        .gap(px(6. * s.scale))
        .border_t_1()
        .border_color(s.border)
        .pt(px(8. * s.scale));
    if let Some(minutes) = controls.focus_minutes.filter(|_| !controls.running) {
        let dec = input.on_adjust_focus.clone();
        let inc = input.on_adjust_focus.clone();
        let can_decrease = controls.can_decrease;
        let can_increase = controls.can_increase;
        row = row
            .child(button(
                "waffle.notification-center.focus-minus",
                "−",
                "Decrease focus time by five minutes",
                s,
                move || {
                    if can_decrease {
                        dec(-5)
                    }
                },
            ))
            .child(label(
                format!("{minutes} mins"),
                13. * s.scale,
                s.muted,
                false,
            ))
            .child(button(
                "waffle.notification-center.focus-plus",
                "+",
                "Increase focus time by five minutes",
                s,
                move || {
                    if can_increase {
                        inc(5)
                    }
                },
            ));
    } else if controls.running {
        row = row.child(label("Focusing", 14. * s.scale, s.foreground, true));
    }
    row = row.child(div().flex_1());
    let focus_action = input.on_focus_action.clone();
    row = row.child(button(
        "waffle.notification-center.focus-action",
        controls.primary_label.clone(),
        controls.primary_label.clone(),
        s,
        move || focus_action(),
    ));
    row.into_any_element()
}

fn pill(text: String, s: &NotificationCenterStyle) -> impl IntoElement {
    div()
        .px(px(7. * s.scale))
        .py(px(2. * s.scale))
        .rounded(px(99. * s.scale))
        .bg(s.input)
        .child(label(text, 11. * s.scale, s.foreground, false))
}

fn button(
    id: impl Into<String>,
    text: impl Into<String>,
    accessible_name: impl Into<String>,
    s: &NotificationCenterStyle,
    action: impl Fn() + Send + Sync + 'static,
) -> impl IntoElement {
    let action: Action = Arc::new(action);
    let key_action = action.clone();
    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .px(px(8. * s.scale))
        .py(px(4. * s.scale))
        .rounded(px(s.radius * s.scale))
        .bg(s.surface)
        .text_color(s.foreground)
        .hover({
            let hover = s.hover;
            move |style| style.bg(hover)
        })
        .child(label(text.into(), 12. * s.scale, s.foreground, false))
        .role(Role::Button)
        .aria_label(accessible_name.into())
        .focusable()
        .tab_stop(true)
        .on_click(move |_: &ClickEvent, _, _| action())
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                key_action();
                cx.stop_propagation();
            }
        })
}

fn label(text: impl Into<String>, size: f32, color: Hsla, strong: bool) -> impl IntoElement {
    let mut element = div()
        .text_size(px(size))
        .text_color(color)
        .child(text.into());
    if strong {
        element = element.font_weight(gpui::FontWeight::SEMIBOLD);
    }
    element
}
