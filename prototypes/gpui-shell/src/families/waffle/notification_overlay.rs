//! Output-sized Waffle Notification Center overlay host.
//!
//! Notification and calendar adapters are not connected yet, so this host
//! supplies explicit unavailable states and no synthetic records.

use std::sync::Arc;

use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, Pixels, Render, Size, Window, div,
    prelude::*, px,
};

use super::{
    notification_center::{self, Action, DataState, NotificationCenterInput},
    runtime::{start_menu_style, taskbar_style},
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
        unread_count: 0,
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
