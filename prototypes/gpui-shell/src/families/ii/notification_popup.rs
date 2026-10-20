//! Material II live notification popup.
//!
//! The host owns the notification service, output selection, lock/game-mode
//! policy, popup lifetime, and clipboard implementation. This component only
//! renders the service snapshot it receives and emits live notification IDs
//! and action IDs back to that host.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use gpui::{
    AnyElement, Bounds, Context, Hsla, Image, ImageFormat, IntoElement, MouseButton, ObjectFit,
    Pixels, Render, Role, SharedString, Size, Window, div, hsla, img, point, prelude::*, px,
};

use crate::notification_service::{
    LiveNotification, NotificationImage, NotificationSnapshot, NotificationSnapshotState,
    NotificationUrgency,
};

pub type DismissNotification = Arc<dyn Fn(u32, u32) + Send + Sync>;
pub type CancelNotificationTimeout = Arc<dyn Fn(u32) + Send + Sync>;
pub type InvokeNotificationAction = Arc<dyn Fn(u32, String) -> bool + Send + Sync>;
pub type CopyNotificationBody = Arc<dyn Fn(String) + Send + Sync>;

const USER_DISMISSED_REASON: u32 = 2;
const WINDOW_HEIGHT_BUFFER: f32 = 16.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotificationPopupPosition {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

impl NotificationPopupPosition {
    pub fn from_config(value: &str) -> Self {
        match value {
            "topLeft" => Self::TopLeft,
            "bottomLeft" => Self::BottomLeft,
            "bottomRight" => Self::BottomRight,
            _ => Self::TopRight,
        }
    }

    fn is_left(self) -> bool {
        matches!(self, Self::TopLeft | Self::BottomLeft)
    }

    fn is_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }
}

#[derive(Clone, Debug)]
pub struct NotificationPopupStyle {
    pub surface: Hsla,
    pub card: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub critical_surface: Hsla,
    pub critical_foreground: Hsla,
    pub border: Hsla,
    pub radius: f32,
    pub card_radius: f32,
    pub padding: f32,
    pub gap: f32,
    pub width: f32,
}

impl Default for NotificationPopupStyle {
    fn default() -> Self {
        Self {
            surface: hsla(0.67, 0.025, 0.12, 0.97),
            card: hsla(0.67, 0.03, 0.18, 1.0),
            foreground: hsla(0.67, 0.03, 0.94, 1.0),
            muted: hsla(0.67, 0.025, 0.72, 1.0),
            accent: hsla(0.58, 0.55, 0.76, 1.0),
            critical_surface: hsla(0.0, 0.65, 0.3, 1.0),
            critical_foreground: hsla(0.0, 0.0, 1.0, 1.0),
            border: hsla(0.67, 0.025, 0.34, 1.0),
            radius: 24.0,
            card_radius: 16.0,
            padding: 12.0,
            gap: 8.0,
            width: 380.0,
        }
    }
}

#[derive(Clone)]
pub struct NotificationPopupInput {
    pub snapshot: NotificationSnapshot,
    pub position: NotificationPopupPosition,
    pub output_width_px: f32,
    pub output_height_px: f32,
    pub edge_margin_px: f32,
    pub style: NotificationPopupStyle,
    /// Dismiss with the freedesktop user-dismiss reason (2).
    pub on_dismiss: DismissNotification,
    /// Cancels the timeout for one notification when its app group is hovered.
    /// The host should forward this to `NotificationServiceHandle::cancel_timeout`.
    pub cancel_timeout: CancelNotificationTimeout,
    /// Receives a live notification ID and the exact service action ID.
    /// The host should forward this to `NotificationServiceHandle::invoke_action`.
    pub on_action: InvokeNotificationAction,
    /// Clipboard access remains a host concern; this gets the plain body text.
    pub on_copy: CopyNotificationBody,
}

pub struct NotificationPopup {
    input: NotificationPopupInput,
    expanded_apps: HashSet<String>,
}

impl NotificationPopup {
    pub fn new(input: NotificationPopupInput) -> Self {
        Self {
            input,
            expanded_apps: HashSet::new(),
        }
    }

    pub fn set_input(&mut self, input: NotificationPopupInput, cx: &mut Context<Self>) {
        self.input = input;
        let app_names = group_live_notifications(&self.input.snapshot.live)
            .into_iter()
            .map(|group| group.app_name)
            .collect::<HashSet<_>>();
        self.expanded_apps.retain(|app| app_names.contains(app));
        cx.notify();
    }
}

impl Render for NotificationPopup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input_region = popup_list_input_region(
            &self.input,
            &self.expanded_apps,
            window.bounds().size.width,
            window.bounds().size.height,
        );
        window.set_input_region(Some(&input_region));
        render_notification_popup(&self.input, &self.expanded_apps, cx.entity())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NotificationGroup {
    app_name: String,
    app_icon: String,
    has_critical: bool,
    latest_id: u32,
    notifications: Vec<LiveNotification>,
}

/// Groups live items by app and orders groups newest-first. The service
/// snapshot is currently ordered by ID, so the highest ID is the available
/// recency proxy until the service exposes its QML timestamp.
fn group_live_notifications(notifications: &[LiveNotification]) -> Vec<NotificationGroup> {
    let mut groups: Vec<NotificationGroup> = Vec::new();
    for notification in notifications {
        let app_name = notification.app_name.trim();
        let app_name = if app_name.is_empty() {
            "Unknown application"
        } else {
            app_name
        };
        if let Some(group) = groups.iter_mut().find(|group| group.app_name == app_name) {
            group.latest_id = group.latest_id.max(notification.id);
            group.has_critical |= notification.urgency == NotificationUrgency::Critical;
            group.notifications.push(notification.clone());
        } else {
            groups.push(NotificationGroup {
                app_name: app_name.to_owned(),
                app_icon: notification.app_icon.clone(),
                has_critical: notification.urgency == NotificationUrgency::Critical,
                latest_id: notification.id,
                notifications: vec![notification.clone()],
            });
        }
    }
    groups.sort_by(|left, right| right.latest_id.cmp(&left.latest_id));
    groups
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotificationPopupGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub max_height: f32,
}

/// Compute an output-relative window rectangle using the configured corner,
/// QML's 80%-of-output cap (or 600px when output height is unavailable), and
/// its resize safety buffer.
pub fn notification_popup_geometry(
    output_width_px: f32,
    output_height_px: f32,
    content_height_px: f32,
    requested_width_px: f32,
    edge_margin_px: f32,
    position: NotificationPopupPosition,
) -> NotificationPopupGeometry {
    let output_width = valid_dimension(output_width_px, 0.0);
    let output_height = valid_dimension(output_height_px, 0.0);
    let margin = valid_dimension(edge_margin_px, 0.0);
    let width =
        valid_dimension(requested_width_px, 380.0).min((output_width - margin * 2.0).max(0.0));
    let max_height = if output_height_px.is_finite() && output_height_px > 0.0 {
        output_height * 0.8
    } else {
        600.0
    };
    let desired_height =
        valid_dimension(content_height_px, 0.0) + margin * 2.0 + WINDOW_HEIGHT_BUFFER;
    let height = desired_height.min(max_height);
    let x = if position.is_left() {
        margin
    } else {
        (output_width - width - margin).max(0.0)
    };
    let y = if position.is_top() {
        margin
    } else {
        (output_height - height - margin).max(0.0)
    };
    NotificationPopupGeometry {
        x,
        y,
        width,
        height,
        max_height,
    }
}

fn valid_dimension(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        fallback.max(0.0)
    }
}

fn render_notification_popup(
    input: &NotificationPopupInput,
    expanded_apps: &HashSet<String>,
    entity: gpui::Entity<NotificationPopup>,
) -> AnyElement {
    let groups = group_live_notifications(&input.snapshot.live);
    if groups.is_empty() || input.snapshot.state != NotificationSnapshotState::ReadySessionOnly {
        return div().into_any_element();
    }

    let style = input.style.clone();
    let width = valid_dimension(input.style.width, 380.0).min(
        (valid_dimension(input.output_width_px, 0.0)
            - valid_dimension(input.edge_margin_px, 0.0) * 2.0)
            .max(0.0),
    );
    let max_window_height = notification_popup_geometry(
        input.output_width_px,
        input.output_height_px,
        0.0,
        width,
        input.edge_margin_px,
        input.position,
    )
    .max_height;
    let margin = valid_dimension(input.edge_margin_px, 0.0);
    let max_content_height = (max_window_height - margin * 2.0 - WINDOW_HEIGHT_BUFFER).max(0.0);
    let mut popup = div()
        .id("ii.notifications.popup")
        .absolute()
        .w(px(width))
        .max_h(px(max_content_height))
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .overflow_y_scroll();
    popup = match input.position {
        NotificationPopupPosition::TopLeft => popup.top(px(margin)).left(px(margin)),
        NotificationPopupPosition::TopRight => popup.top(px(margin)).right(px(margin)),
        NotificationPopupPosition::BottomLeft => popup.bottom(px(margin)).left(px(margin)),
        NotificationPopupPosition::BottomRight => popup.bottom(px(margin)).right(px(margin)),
    };

    let dismiss = input.on_dismiss.clone();
    let cancel_timeout = input.cancel_timeout.clone();
    let on_action = input.on_action.clone();
    let on_copy = input.on_copy.clone();
    let expanded_apps = expanded_apps.clone();
    let groups = groups.into_iter().map(move |group| {
        let app_name = group.app_name.clone();
        let is_expanded = expanded_apps.contains(&app_name);
        render_group(
            group,
            is_expanded,
            style.clone(),
            dismiss.clone(),
            cancel_timeout.clone(),
            on_action.clone(),
            on_copy.clone(),
            entity.clone(),
        )
    });
    popup.children(groups).into_any_element()
}

/// Match the popup list's grouped and collapsed card model. The region tracks
/// expanded groups and is clipped to the actual scroll viewport; text wrapping
/// remains an estimate because GPUI has no post-layout layer-mask callback.
fn popup_list_input_region(
    input: &NotificationPopupInput,
    expanded_apps: &HashSet<String>,
    surface_width: Pixels,
    surface_height: Pixels,
) -> Vec<Bounds<Pixels>> {
    if input.snapshot.state != NotificationSnapshotState::ReadySessionOnly
        || input.snapshot.live.is_empty()
    {
        return Vec::new();
    }
    let margin = valid_dimension(input.edge_margin_px, 0.0);
    let content_width = valid_dimension(input.style.width, 380.0)
        .min((input.output_width_px - margin * 2.0).max(0.0))
        .min((f32::from(surface_width) - margin * 2.0).max(0.0));
    let max_content_height = (input.output_height_px * 0.8 - margin * 2.0 - WINDOW_HEIGHT_BUFFER)
        .max(0.0)
        .min((f32::from(surface_height) - margin * 2.0 - WINDOW_HEIGHT_BUFFER).max(0.0));
    let content_height =
        popup_visible_content_height(input, expanded_apps, content_width).min(max_content_height);
    let y = if input.position.is_top() {
        margin
    } else {
        (f32::from(surface_height) - margin - content_height).max(margin)
    };
    vec![Bounds {
        origin: point(px(margin), px(y)),
        size: Size::new(px(content_width), px(content_height)),
    }]
}

fn popup_visible_content_height(
    input: &NotificationPopupInput,
    expanded_apps: &HashSet<String>,
    content_width: f32,
) -> f32 {
    let groups = group_live_notifications(&input.snapshot.live);
    let mut total = 0.0;
    let chars_per_line = ((content_width - input.style.padding * 2.0 - 20.0) / 7.5).max(1.0);
    for (group_index, group) in groups.iter().enumerate() {
        if group_index > 0 {
            total += input.style.gap;
        }
        // Match render_group's fixed 38px icon row, four-pixel card gap, and
        // symmetric group padding.
        total += input.style.padding * 2.0 + 38.0 + 4.0;
        let expanded = expanded_apps.contains(&group.app_name);
        let visible_notifications = if expanded {
            group.notifications.iter().rev().collect::<Vec<_>>()
        } else {
            group.notifications.iter().rev().take(2).collect::<Vec<_>>()
        };
        for (index, notification) in visible_notifications.iter().enumerate() {
            if index > 0 {
                total += 4.0;
            }
            let title = if notification.summary.trim().is_empty() {
                "Notification"
            } else {
                &notification.summary
            };
            let title_lines = (title.chars().count() as f32 / chars_per_line)
                .ceil()
                .clamp(1.0, 3.0);
            let body_lines = (notification.body.trim().chars().count() as f32 / chars_per_line)
                .ceil()
                .max(1.0);
            let body_height = if expanded {
                body_lines * 18.0
            } else {
                (body_lines * 18.0).min(38.0)
            };
            // Card padding, border, internal gaps, title and body match the
            // same cards that render_notification_item emits.
            total += 22.0 + 5.0 + title_lines * 18.0 + body_height;
            if expanded {
                let button_width = (content_width - 20.0).max(1.0);
                let buttons_per_row = (button_width / 92.0).max(1.0);
                let rows = ((notification.actions.len() + 2) as f32 / buttons_per_row)
                    .ceil()
                    .max(1.0);
                total += 4.0 + rows * 34.0;
            }
        }
    }
    total
}

fn render_group(
    group: NotificationGroup,
    expanded: bool,
    style: NotificationPopupStyle,
    dismiss: DismissNotification,
    cancel_timeout: CancelNotificationTimeout,
    on_action: InvokeNotificationAction,
    on_copy: CopyNotificationBody,
    entity: gpui::Entity<NotificationPopup>,
) -> impl IntoElement {
    let app_name = group.app_name.clone();
    let toggle_app = app_name.clone();
    let toggle_entity = entity.clone();
    let dismiss_group = group
        .notifications
        .iter()
        .map(|notification| notification.id)
        .collect::<Vec<_>>();
    let dismiss_callback = dismiss.clone();
    let timeout_notification_ids = group
        .notifications
        .iter()
        .map(|notification| notification.id)
        .collect::<Vec<_>>();
    let cancel_group_timeouts = cancel_timeout;
    let notifications = if expanded {
        group
            .notifications
            .iter()
            .rev()
            .cloned()
            .collect::<Vec<_>>()
    } else {
        group
            .notifications
            .iter()
            .rev()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
    };
    let cards = notifications.into_iter().map(|notification| {
        render_notification_item(
            notification,
            expanded,
            style.clone(),
            dismiss.clone(),
            on_action.clone(),
            on_copy.clone(),
        )
    });
    let critical = group.has_critical;
    let group_label = if critical {
        format!(
            "{} notifications from {}, critical",
            group.notifications.len(),
            app_name
        )
    } else {
        format!(
            "{} notifications from {}",
            group.notifications.len(),
            app_name
        )
    };
    let icon_label = app_icon_fallback(&app_name);
    let notification_art = if group.notifications.len() == 1 {
        group
            .notifications
            .first()
            .and_then(|notification| notification.image.as_ref())
    } else {
        None
    };
    let notification_art = notification_art.and_then(|image| image_element(image, 38.0));
    let app_icon_image = resolve_notification_icon(&group.app_icon).map(|path| {
        img(path)
            .w(px(38.0))
            .h(px(38.0))
            .object_fit(ObjectFit::Contain)
            .into_any_element()
    });
    let icon_content = match (notification_art, app_icon_image) {
        (Some(art), Some(app_icon)) => div()
            .relative()
            .w(px(38.0))
            .h(px(38.0))
            .child(art)
            .child(
                div()
                    .absolute()
                    .bottom(px(0.0))
                    .right(px(0.0))
                    .w(px(15.0))
                    .h(px(15.0))
                    .rounded_full()
                    .bg(style.card)
                    .child(app_icon),
            )
            .into_any_element(),
        (Some(art), None) | (None, Some(art)) => art,
        (None, None) => div().child(icon_label).into_any_element(),
    };

    div()
        .id(SharedString::from(format!(
            "ii.notifications.group.{app_name}"
        )))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .p(px(style.padding))
        .bg(if critical {
            style.critical_surface
        } else {
            style.surface
        })
        .rounded(px(style.radius))
        .border_1()
        .border_color(style.border)
        .role(gpui::Role::Group)
        .aria_label(SharedString::from(group_label))
        .on_hover(move |entered, _, _| {
            if *entered {
                for id in &timeout_notification_ids {
                    cancel_group_timeouts(*id);
                }
            }
        })
        .on_click(move |_, _, cx| {
            toggle_entity.update(cx, |view, cx| {
                if !view.expanded_apps.insert(toggle_app.clone()) {
                    view.expanded_apps.remove(&toggle_app);
                }
                cx.notify();
            });
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, {
            let toggle_app = app_name.clone();
            let toggle_entity = entity.clone();
            move |_, _, cx| {
                toggle_entity.update(cx, |view, cx| {
                    if !view.expanded_apps.insert(toggle_app.clone()) {
                        view.expanded_apps.remove(&toggle_app);
                    }
                    cx.notify();
                });
                cx.stop_propagation();
            }
        })
        .on_mouse_down(MouseButton::Middle, move |_, _, cx| {
            for id in &dismiss_group {
                dismiss_callback(*id, USER_DISMISSED_REASON);
            }
            cx.stop_propagation();
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .w(px(38.0))
                        .h(px(38.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(if critical { style.accent } else { style.card })
                        .text_color(style.foreground)
                        .text_sm()
                        .child(icon_content),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(if critical {
                            style.critical_foreground
                        } else {
                            style.foreground
                        })
                        .child(app_name),
                )
                .child(
                    div()
                        .px(px(9.0))
                        .py(px(4.0))
                        .rounded_full()
                        .bg(if critical { style.accent } else { style.card })
                        .text_color(if critical {
                            style.critical_foreground
                        } else {
                            style.muted
                        })
                        .text_xs()
                        .child(group.notifications.len().to_string()),
                ),
        )
        .children(cards)
}

fn render_notification_item(
    notification: LiveNotification,
    expanded: bool,
    style: NotificationPopupStyle,
    dismiss: DismissNotification,
    on_action: InvokeNotificationAction,
    on_copy: CopyNotificationBody,
) -> impl IntoElement {
    let notification_id = notification.id;
    let is_critical = notification.urgency == NotificationUrgency::Critical;
    let critical_surface = style.critical_surface;
    let critical_foreground = style.critical_foreground;
    let foreground = style.foreground;
    let muted = style.muted;
    let accent = style.accent;
    let title = if notification.summary.trim().is_empty() {
        "Notification".to_owned()
    } else {
        notification.summary.clone()
    };
    let body = notification.body.trim().to_owned();
    let body_text = if body.is_empty() {
        " ".to_owned()
    } else {
        body.clone()
    };
    let action_title = title.clone();
    let action_style = style.clone();
    let action_buttons = notification
        .actions
        .clone()
        .into_iter()
        .map(move |(action_id, label)| {
            let callback = on_action.clone();
            let accessible_label = SharedString::from(format!("{label}: {action_title}"));
            div()
                .id(SharedString::from(format!(
                    "ii.notifications.item.{notification_id}.action.{action_id}"
                )))
                .min_h(px(34.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .rounded(px(12.0))
                .bg(if is_critical {
                    critical_surface
                } else {
                    action_style.card
                })
                .text_color(if is_critical {
                    critical_foreground
                } else {
                    foreground
                })
                .text_sm()
                .role(Role::Button)
                .aria_label(accessible_label)
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let _ = callback(notification_id, action_id.clone());
                    cx.stop_propagation();
                })
                .child(label)
        });
    let dismiss_callback = dismiss.clone();
    let copy_body = body.clone();
    let copy_callback = on_copy;

    div()
        .id(SharedString::from(format!(
            "ii.notifications.item.{notification_id}"
        )))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .p(px(10.0))
        .bg(if is_critical { style.card } else { style.card })
        .rounded(px(style.card_radius))
        .border_l_2()
        .border_color(if is_critical { accent } else { style.border })
        .role(gpui::Role::Group)
        .aria_label(SharedString::from(format!(
            "Notification from {}: {title}",
            notification.app_name
        )))
        .on_mouse_down(MouseButton::Middle, move |_, _, cx| {
            dismiss_callback(notification_id, USER_DISMISSED_REASON);
            cx.stop_propagation();
        })
        .child(
            div()
                .w_full()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(if is_critical {
                    critical_foreground
                } else {
                    foreground
                })
                .child(title),
        )
        .child(
            div()
                .w_full()
                .text_sm()
                .text_color(muted)
                .when(!expanded, |this| this.max_h(px(38.0)))
                .child(body_text),
        )
        .when(expanded, |this| {
            let copy_dismiss = dismiss.clone();
            this.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.0))
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "ii.notifications.item.{notification_id}.close"
                            )))
                            .min_h(px(34.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .rounded(px(12.0))
                            .bg(if is_critical {
                                critical_surface
                            } else {
                                style.card
                            })
                            .text_color(if is_critical {
                                critical_foreground
                            } else {
                                foreground
                            })
                            .text_sm()
                            .role(Role::Button)
                            .aria_label(SharedString::from("Dismiss notification"))
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                copy_dismiss(notification_id, USER_DISMISSED_REASON);
                                cx.stop_propagation();
                            })
                            .child("Close"),
                    )
                    .children(action_buttons)
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "ii.notifications.item.{notification_id}.copy"
                            )))
                            .min_h(px(34.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .rounded(px(12.0))
                            .bg(if is_critical {
                                critical_surface
                            } else {
                                style.card
                            })
                            .text_color(if is_critical {
                                critical_foreground
                            } else {
                                foreground
                            })
                            .text_sm()
                            .role(Role::Button)
                            .aria_label(SharedString::from("Copy notification body"))
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                copy_callback(copy_body.clone());
                                cx.stop_propagation();
                            })
                            .child("Copy"),
                    ),
            )
        })
}

fn app_icon_fallback(app_name: &str) -> String {
    app_name
        .chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().collect())
        .unwrap_or_else(|| "•".to_owned())
}

fn image_element(image: &NotificationImage, size: f32) -> Option<AnyElement> {
    match image {
        NotificationImage::Png(bytes) => Some(
            img(Arc::new(Image::from_bytes(
                ImageFormat::Png,
                bytes.to_vec(),
            )))
            .w(px(size))
            .h(px(size))
            .rounded_full()
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
        ),
        NotificationImage::Resource(resource) => Some(
            img(resolve_notification_icon(resource)?)
                .w(px(size))
                .h(px(size))
                .rounded_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
        ),
    }
}

/// Resolve sender-provided paths and icon theme names only to files that
/// actually exist. This gives app_icon an explicit host-side theme contract
/// without constructing an unchecked URI or substituting generated art.
fn resolve_notification_icon(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    static ICON_PATH_CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
    let cache = ICON_PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(path) = cache.lock().ok()?.get(value).cloned() {
        return path;
    }
    let resolved = resolve_notification_icon_uncached(value);
    cache
        .lock()
        .ok()?
        .insert(value.to_owned(), resolved.clone());
    resolved
}

fn resolve_notification_icon_uncached(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    if let Some(path) = value.strip_prefix("file://") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let direct = Path::new(value);
    if direct.is_absolute() {
        return direct.is_file().then(|| direct.to_path_buf());
    }
    if value.is_empty() || value.contains('/') || value.contains('\\') {
        return None;
    }

    let mut data_dirs = Vec::new();
    if let Some(home_data) = std::env::var_os("XDG_DATA_HOME") {
        data_dirs.push(PathBuf::from(home_data));
    } else if let Some(home) = std::env::var_os("HOME") {
        data_dirs.push(PathBuf::from(home).join(".local/share"));
    }
    if let Some(dirs) = std::env::var_os("XDG_DATA_DIRS") {
        data_dirs.extend(std::env::split_paths(&dirs));
    } else {
        data_dirs.extend([
            PathBuf::from("/usr/local/share"),
            PathBuf::from("/usr/share"),
        ]);
    }
    let extensions = ["png", "svg", "xpm", "jpg", "jpeg", "webp"];
    let themes = ["hicolor", "Adwaita", "breeze", "Papirus"];
    for data_dir in data_dirs {
        for theme in themes {
            for category in ["apps", "status", "devices", "places", "mimetypes"] {
                for size in [
                    "scalable", "512x512", "256x256", "128x128", "96x96", "64x64", "48x48",
                    "32x32", "24x24", "16x16",
                ] {
                    for extension in extensions {
                        let candidate = data_dir
                            .join("icons")
                            .join(theme)
                            .join(size)
                            .join(category)
                            .join(format!("{value}.{extension}"));
                        if candidate.is_file() {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
        for extension in extensions {
            let candidate = data_dir
                .join("pixmaps")
                .join(format!("{value}.{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        NotificationPopupPosition, WINDOW_HEIGHT_BUFFER, app_icon_fallback,
        group_live_notifications, notification_popup_geometry, popup_list_input_region,
    };
    use crate::notification_service::{
        LiveNotification, NotificationServiceStatus, NotificationSnapshot,
        NotificationSnapshotState, NotificationUrgency,
    };
    use gpui::px;
    use std::collections::HashSet;
    use std::sync::Arc;

    fn popup_input(live: Vec<LiveNotification>) -> super::NotificationPopupInput {
        super::NotificationPopupInput {
            snapshot: NotificationSnapshot {
                service: NotificationServiceStatus::OwnerReady,
                state: NotificationSnapshotState::ReadySessionOnly,
                live,
            },
            position: NotificationPopupPosition::TopRight,
            output_width_px: 1920.0,
            output_height_px: 1080.0,
            edge_margin_px: 12.0,
            style: super::NotificationPopupStyle::default(),
            on_dismiss: Arc::new(|_, _| {}),
            cancel_timeout: Arc::new(|_| {}),
            on_action: Arc::new(|_, _| true),
            on_copy: Arc::new(|_| {}),
        }
    }

    fn notification(id: u32, app: &str) -> LiveNotification {
        LiveNotification {
            id,
            sender: ":1.42".to_owned(),
            app_name: app.to_owned(),
            app_icon: String::new(),
            image: None,
            summary: format!("Message {id}"),
            body: "Test body".to_owned(),
            actions: vec![("open".to_owned(), "Open".to_owned())],
            urgency: NotificationUrgency::Normal,
            expire_timeout_ms: -1,
        }
    }

    #[test]
    fn groups_by_application_and_sorts_by_latest_live_id() {
        let grouped = group_live_notifications(&[
            notification(1, "Terminal"),
            notification(2, "Browser"),
            notification(3, "Terminal"),
        ]);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].app_name, "Terminal");
        assert_eq!(grouped[0].notifications.len(), 2);
        assert_eq!(grouped[1].app_name, "Browser");
    }

    #[test]
    fn popup_geometry_covers_all_corners_and_caps_to_output() {
        let top_left = notification_popup_geometry(
            1920.0,
            1080.0,
            900.0,
            380.0,
            12.0,
            NotificationPopupPosition::TopLeft,
        );
        let bottom_right = notification_popup_geometry(
            1920.0,
            1080.0,
            900.0,
            380.0,
            12.0,
            NotificationPopupPosition::BottomRight,
        );
        assert_eq!((top_left.x, top_left.y), (12.0, 12.0));
        assert_eq!(top_left.height, 864.0);
        let top_right = notification_popup_geometry(
            1920.0,
            1080.0,
            900.0,
            380.0,
            12.0,
            NotificationPopupPosition::TopRight,
        );
        let bottom_left = notification_popup_geometry(
            1920.0,
            1080.0,
            900.0,
            380.0,
            12.0,
            NotificationPopupPosition::BottomLeft,
        );
        assert_eq!((top_right.x, top_right.y), (1528.0, 12.0));
        assert_eq!((bottom_left.x, bottom_left.y), (12.0, 204.0));
        assert_eq!(bottom_right.x, 1528.0);
        assert_eq!(bottom_right.y, 204.0);
        assert_eq!(bottom_right.max_height, 864.0);
    }

    #[test]
    fn popup_geometry_handles_small_outputs_and_invalid_dimensions() {
        let geometry = notification_popup_geometry(
            160.0,
            120.0,
            1_000.0,
            380.0,
            8.0,
            NotificationPopupPosition::BottomLeft,
        );
        assert_eq!(geometry.width, 144.0);
        assert_eq!(geometry.height, 96.0);
        assert_eq!(geometry.x, 8.0);
        assert_eq!(geometry.y, 16.0);
        let unknown_output = notification_popup_geometry(
            1920.0,
            f32::NAN,
            900.0,
            380.0,
            12.0,
            NotificationPopupPosition::TopRight,
        );
        assert_eq!(unknown_output.max_height, 600.0);
    }

    #[test]
    fn list_input_region_matches_collapsed_and_expanded_group_visibility() {
        let input = popup_input(vec![
            notification(1, "Editor"),
            notification(2, "Editor"),
            notification(3, "Editor"),
        ]);
        let collapsed = popup_list_input_region(&input, &HashSet::new(), px(404.0), px(864.0));
        let expanded = popup_list_input_region(
            &input,
            &HashSet::from(["Editor".to_owned()]),
            px(404.0),
            px(864.0),
        );
        assert_eq!(collapsed.len(), 1);
        assert_eq!(expanded.len(), 1);
        assert!(f32::from(expanded[0].size.height) > f32::from(collapsed[0].size.height));
        assert!(f32::from(expanded[0].size.height) <= 864.0 - 24.0 - WINDOW_HEIGHT_BUFFER);
    }

    #[test]
    fn list_input_region_is_corner_aligned_and_never_exceeds_surface() {
        let positions = [
            NotificationPopupPosition::TopLeft,
            NotificationPopupPosition::TopRight,
            NotificationPopupPosition::BottomLeft,
            NotificationPopupPosition::BottomRight,
        ];
        for position in positions {
            let mut input = popup_input(vec![notification(1, "Editor")]);
            input.position = position;
            let region = popup_list_input_region(&input, &HashSet::new(), px(404.0), px(864.0));
            assert_eq!(region.len(), 1);
            let bounds = region[0];
            let x = f32::from(bounds.origin.x);
            let y = f32::from(bounds.origin.y);
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            assert!((x - input.edge_margin_px).abs() < f32::EPSILON);
            assert!(x + width <= 404.0 - input.edge_margin_px + f32::EPSILON);
            assert!(y >= input.edge_margin_px);
            assert!(y + height <= 864.0 - input.edge_margin_px + f32::EPSILON);
            if position.is_top() {
                assert!((y - input.edge_margin_px).abs() < f32::EPSILON);
            } else {
                assert!((y + height - (864.0 - input.edge_margin_px)).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn empty_notification_snapshot_has_no_pointer_input_region() {
        let input = popup_input(Vec::new());
        assert!(popup_list_input_region(&input, &HashSet::new(), px(404.0), px(864.0)).is_empty());
    }

    #[test]
    fn expanded_group_input_region_stops_at_the_scroll_viewport() {
        let notifications = (1..=30).map(|id| notification(id, "Editor")).collect();
        let input = popup_input(notifications);
        let region = popup_list_input_region(
            &input,
            &HashSet::from(["Editor".to_owned()]),
            px(404.0),
            px(240.0),
        );
        assert_eq!(region.len(), 1);
        assert_eq!(
            f32::from(region[0].size.height),
            240.0 - 24.0 - WINDOW_HEIGHT_BUFFER
        );
    }

    #[test]
    fn app_icon_fallback_uses_text_not_a_fake_image_path() {
        assert_eq!(app_icon_fallback("Firefox"), "F");
        assert_eq!(app_icon_fallback(""), "•");
    }
}
