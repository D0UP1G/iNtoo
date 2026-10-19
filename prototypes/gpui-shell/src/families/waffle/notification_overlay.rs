//! Output-sized Waffle Notification Center overlay host.
//!
//! The host accepts live-session notification snapshots; calendar data stays
//! unavailable until its adapter is connected.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, Pixels, Render, Role, Size, Window, div,
    prelude::*, px,
};

use super::{
    notification_center::{self, Action, DataState, NotificationCenterInput},
    runtime::{start_menu_style, taskbar_style},
};
use crate::notification_service::{
    NotificationServiceHandle, NotificationSnapshot, NotificationSnapshotState,
};
use crate::theme::ThemeTokens;

#[derive(Clone, Copy, Debug)]
pub struct NotificationOverlayAnchor {
    pub display_size: Size<Pixels>,
    pub bar_bottom: bool,
    pub bar_height: Pixels,
}

pub struct WaffleNotificationOverlay {
    input: NotificationCenterInput,
    anchor: NotificationOverlayAnchor,
    on_close: Action,
    focus_handle: FocusHandle,
    needs_initial_focus: bool,
}

impl WaffleNotificationOverlay {
    pub fn new(
        input: NotificationCenterInput,
        anchor: NotificationOverlayAnchor,
        on_close: Action,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            input,
            anchor,
            on_close,
            focus_handle: cx.focus_handle(),
            needs_initial_focus: true,
        }
    }

    pub fn set_input(&mut self, input: NotificationCenterInput, cx: &mut Context<Self>) {
        self.input = input;
        cx.notify();
    }
}

impl Focusable for WaffleNotificationOverlay {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WaffleNotificationOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.needs_initial_focus {
            window.focus(&self.focus_handle, cx);
            self.needs_initial_focus = false;
        }
        let close = Arc::clone(&self.on_close);
        let display_height = f32::from(self.anchor.display_size.height);
        let display_width = f32::from(self.anchor.display_size.width);
        let available_height = (display_height - f32::from(self.anchor.bar_height) - 24.).max(1.);
        let panel_width = (display_width - 24.).clamp(1., 440.);
        let edge_gap = self.anchor.bar_height + px(8.);
        let mut panel = div()
            .absolute()
            .right(px(8.))
            .w(px(panel_width))
            .max_h(px(available_height));
        panel = if self.anchor.bar_bottom {
            panel.top(edge_gap)
        } else {
            panel.bottom(edge_gap)
        };

        div()
            .id("waffle.notification-center.overlay")
            .relative()
            .size_full()
            .role(Role::Dialog)
            .aria_label("Notification center")
            .focusable()
            .tab_stop(true)
            .child(
                div()
                    .id("waffle.notification-center.backdrop")
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .left_0()
                    .on_click(move |_, _, _| close()),
            )
            .child(panel.child(notification_center::render(self.input.clone())))
    }
}

pub fn unavailable_input(
    theme: &ThemeTokens,
    available_height: Pixels,
    on_close: Action,
) -> NotificationCenterInput {
    let bar = taskbar_style(theme);
    let style = start_menu_style(theme, &bar);
    let noop: Action = Arc::new(|| {});
    NotificationCenterInput {
        style: notification_center::NotificationCenterStyle {
            panel: style.background,
            surface: style.surface,
            input: style.surface,
            hover: style.surface_hover,
            foreground: style.foreground,
            muted: style.muted,
            accent: style.accent,
            warning: bar.accent,
            danger: bar.accent,
            border: style.border,
            radius: style.radius,
            scale: 1.,
        },
        available_height,
        groups: Vec::new(),
        notification_count: 0,
        unread_count: None,
        silent: false,
        search_query: String::new(),
        notification_state: DataState::Unavailable,
        month_label: "Calendar".into(),
        weekdays: Vec::new(),
        calendar_days: Vec::new(),
        selected_date: None,
        selected_date_label: String::new(),
        selected_events: Vec::new(),
        calendar_state: DataState::Unavailable,
        collapsed: false,
        focus_controls: None,
        on_close,
        on_toggle_silent: Arc::clone(&noop),
        on_clear_all: Arc::clone(&noop),
        on_dismiss_notification: Arc::new(|_| {}),
        on_dismiss_group: Arc::new(|_| {}),
        on_notification_action: Arc::new(|_, _| {}),
        on_toggle_group: Arc::new(|_| {}),
        on_previous_month: Arc::clone(&noop),
        on_next_month: Arc::clone(&noop),
        on_select_date: Arc::new(|_| {}),
        on_toggle_calendar: Arc::clone(&noop),
        on_adjust_focus: Arc::new(|_| {}),
        on_focus_action: noop,
    }
}

/// Builds a presentation snapshot from notifications received in this process.
/// This projection deliberately has no persisted history, unread/read state,
/// Do Not Disturb setting, or fabricated timestamps.
pub fn session_input(
    theme: &ThemeTokens,
    available_height: Pixels,
    snapshot: &NotificationSnapshot,
    service: &NotificationServiceHandle,
    expanded_groups: &HashSet<String>,
    on_toggle_group: notification_center::ItemAction,
    on_close: Action,
) -> NotificationCenterInput {
    let mut input = unavailable_input(theme, available_height, on_close);
    input.notification_state = match snapshot.state {
        NotificationSnapshotState::Loading => DataState::Loading,
        NotificationSnapshotState::Unavailable => DataState::Unavailable,
        NotificationSnapshotState::ReadySessionOnly => DataState::ReadySessionOnly,
    };
    let live_items = if snapshot.state == NotificationSnapshotState::ReadySessionOnly {
        snapshot.live.as_slice()
    } else {
        &[]
    };
    input.notification_count = live_items.len();
    input.unread_count = None;
    input.silent = false;

    let mut grouped = HashMap::<String, Vec<notification_center::NotificationItem>>::new();
    let mut latest_id = HashMap::<String, u32>::new();
    let mut ids_by_app = HashMap::<String, Vec<u32>>::new();
    let mut app_names_by_key = HashMap::<String, String>::new();
    let mut app_icons_by_key = HashMap::<String, String>::new();
    for live in live_items {
        let app_name = live.app_name.clone();
        let group_key = notification_group_key(&live.app_name, &live.sender);
        app_names_by_key
            .entry(group_key.clone())
            .or_insert_with(|| app_name.clone());
        if !live.app_icon.is_empty() {
            app_icons_by_key.insert(group_key.clone(), live.app_icon.clone());
        }
        grouped
            .entry(group_key.clone())
            .or_default()
            .push(notification_center::NotificationItem {
                id: live.id.to_string(),
                title: live.summary.clone(),
                body: live.body.clone(),
                timestamp: String::new(),
                unread: None,
                critical: live.urgency
                    == crate::notification_service::NotificationUrgency::Critical,
                actions: live
                    .actions
                    .iter()
                    .map(|(id, label)| notification_center::NotificationAction {
                        id: id.clone(),
                        label: label.clone(),
                    })
                    .collect(),
            });
        latest_id
            .entry(group_key.clone())
            .and_modify(|current| *current = (*current).max(live.id))
            .or_insert(live.id);
        ids_by_app.entry(group_key).or_default().push(live.id);
    }

    let mut app_names = grouped.keys().cloned().collect::<Vec<_>>();
    app_names.sort_by_key(|app_name| {
        std::cmp::Reverse(latest_id.get(app_name).copied().unwrap_or_default())
    });
    input.groups = app_names
        .into_iter()
        .map(|group_key| {
            let mut notifications = grouped.remove(&group_key).unwrap_or_default();
            notifications.reverse();
            let app_name = app_names_by_key.remove(&group_key).unwrap_or_default();
            let app_icon = app_icons_by_key.remove(&group_key).unwrap_or_default();
            notification_center::NotificationGroup {
                app_id: group_key.clone(),
                app_name: if app_name.is_empty() {
                    "Unknown application".into()
                } else {
                    app_name.clone()
                },
                app_icon,
                notifications,
                expanded: expanded_groups.contains(&group_key),
            }
        })
        .collect();

    input.on_toggle_group = on_toggle_group;
    let live_ids = live_items.iter().map(|item| item.id).collect::<Vec<_>>();
    let valid_live_ids = Arc::new(live_ids.iter().copied().collect::<HashSet<_>>());
    let clear_service = service.clone();
    input.on_clear_all = Arc::new(move || {
        for id in &live_ids {
            clear_service.dismiss(*id, 2);
        }
    });
    let grouped_ids = Arc::new(ids_by_app);
    let dismiss_group_service = service.clone();
    input.on_dismiss_group = Arc::new(move |app_id| {
        if let Some(ids) = grouped_ids.get(&app_id) {
            for id in ids {
                dismiss_group_service.dismiss(*id, 2);
            }
        }
    });
    let dismiss_service = service.clone();
    let dismiss_ids = valid_live_ids.clone();
    input.on_dismiss_notification = Arc::new(move |id| {
        if let Ok(id) = id.parse::<u32>() {
            if dismiss_ids.contains(&id) {
                dismiss_service.dismiss(id, 2);
            }
        }
    });
    let action_service = service.clone();
    let action_ids = valid_live_ids;
    input.on_notification_action = Arc::new(move |id, action_id| {
        if let Ok(id) = id.parse::<u32>() {
            if action_ids.contains(&id) {
                let _ = action_service.invoke_action(id, &action_id);
            }
        }
    });

    input
}

fn notification_group_key(app_name: &str, sender: &str) -> String {
    if app_name.is_empty() {
        // Unlike QML's shared empty-name bucket, sender scope avoids mass-dismiss across unrelated apps.
        format!("sender:{sender}")
    } else {
        format!("app:{app_name}")
    }
}
