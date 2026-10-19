//! Data-driven notification list presentation for the II right sidebar.
//!
//! The panel owns no notification service state. The adapter supplies each
//! notification and receives action identifiers selected by the user.

use std::sync::Arc;

use gpui::{
    AnyElement, FontWeight, IntoElement, KeyDownEvent, Role, SharedString,
    StatefulInteractiveElement, div, prelude::*, px,
};

use super::sidebars::SidebarStyle;

pub type NotificationAction = Arc<dyn Fn(String, String) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationPanelState {
    Loading,
    Empty,
    Unavailable,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}

#[derive(Debug, Clone)]
pub struct NotificationActionItem {
    /// Stable service action key; passed to `on_action` unchanged.
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct NotificationItem {
    pub id: String,
    pub app: String,
    pub title: String,
    pub body: String,
    pub urgency: NotificationUrgency,
    pub actions: Vec<NotificationActionItem>,
}

#[derive(Clone)]
pub struct NotificationPanelInput {
    pub state: NotificationPanelState,
    pub message: Option<String>,
    pub notifications: Vec<NotificationItem>,
    pub style: SidebarStyle,
    /// Receives `(notification_id, action_id)`.
    pub on_action: NotificationAction,
}

impl std::fmt::Debug for NotificationPanelInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NotificationPanelInput")
            .field("state", &self.state)
            .field("message", &self.message)
            .field("notifications", &self.notifications)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

/// Renders adapter-provided notification data. Ready lists scroll vertically;
/// all other states render an explicit message without placeholder entries.
pub fn render_notification_panel(input: NotificationPanelInput) -> AnyElement {
    let style = input.style;
    let notifications = input.notifications;
    let state = if input.state == NotificationPanelState::Ready && notifications.is_empty() {
        NotificationPanelState::Empty
    } else {
        input.state
    };
    let fallback = match state {
        NotificationPanelState::Loading => "Loading notifications…",
        NotificationPanelState::Empty => "All caught up",
        NotificationPanelState::Unavailable => "Notifications are unavailable",
        NotificationPanelState::Ready => "",
    };
    let message = input.message.unwrap_or_else(|| fallback.into());
    let cards = notifications.into_iter().map(|notification| {
        render_notification_card(notification, &style, input.on_action.clone())
    });

    div()
        .id("ii.sidebar.notifications")
        .w_full()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(style.padding))
        .bg(style.surface)
        .when(state == NotificationPanelState::Ready, |this| {
            this.overflow_y_scroll().children(cards)
        })
        .when(state != NotificationPanelState::Ready, |this| {
            this.items_center().justify_center().child(
                div()
                    .id("ii.sidebar.notifications.status")
                    .text_sm()
                    .text_color(style.muted)
                    .role(Role::Status)
                    .aria_label(SharedString::from(message.clone()))
                    .child(message),
            )
        })
        .into_any_element()
}

fn render_notification_card(
    notification: NotificationItem,
    style: &SidebarStyle,
    on_action: NotificationAction,
) -> impl IntoElement {
    let critical = notification.urgency == NotificationUrgency::Critical;
    let card = if critical { style.selected } else { style.card };
    let app = notification.app.clone();
    let title = notification.title.clone();
    let body = notification.body.clone();
    let notification_id = notification.id.clone();
    let action_title = notification.title.clone();
    let card_label = format!(
        "Notification from {}: {}. {}{}",
        notification.app,
        notification.title,
        notification.body,
        if critical { " Critical urgency." } else { "" }
    );
    let actions = notification.actions.into_iter().map(move |action| {
        let click_notification_id = notification_id.clone();
        let key_notification_id = notification_id.clone();
        let action_id_click = action.id.clone();
        let action_id_key = action.id.clone();
        let callback_click = on_action.clone();
        let callback_key = on_action.clone();
        let accessible = format!("{}: {}", action.label, action_title);
        div()
            .id(SharedString::from(format!(
                "ii.sidebar.notification.{}.action.{}",
                notification_id, action.id
            )))
            .min_h(px(34.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .rounded(px(style.card_radius))
            .bg(style.surface)
            .text_sm()
            .text_color(style.foreground)
            .role(Role::Button)
            .aria_label(SharedString::from(accessible))
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .hover({
                let hover = style.hover;
                move |s| s.bg(hover)
            })
            .on_click(move |_, _, _| {
                callback_click(click_notification_id.clone(), action_id_click.clone())
            })
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                    callback_key(key_notification_id.clone(), action_id_key.clone());
                    cx.stop_propagation();
                }
            })
            .child(action.label)
    });

    div()
        .id(SharedString::from(format!(
            "ii.sidebar.notification.{}",
            notification.id
        )))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(style.padding))
        .bg(card)
        .rounded(px(style.card_radius))
        .text_color(style.foreground)
        .role(Role::Group)
        .aria_label(card_label)
        .when(critical, |this| {
            this.border_l_2().border_color(style.accent)
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(style.gap))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(div().text_xs().text_color(style.muted).child(app)),
        )
        .when(!body.is_empty(), |this| {
            this.child(div().w_full().text_sm().text_color(style.muted).child(body))
        })
        .children(actions)
}
