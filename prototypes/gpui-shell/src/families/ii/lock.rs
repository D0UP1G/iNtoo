//! Presentational Material ii lock surface.
//!
//! This component is deliberately unhosted. A real compositor lock host
//! must own authentication, secret memory, output coverage, and lock lifetime.
//! The view keeps no password text: printable key events are passed one `char`
//! at a time to the host callback, which must append to a short-lived secret
//! buffer and clear it after submit, timeout, failure, or unlock.
//! The host supplies and initially focuses `focus_handle`; the focused root
//! continues routing keys when the view switches from clock to login.

use std::{path::PathBuf, sync::Arc};

use gpui::{
    FocusHandle, FontWeight, InteractiveElement, KeyDownEvent, ObjectFit, Role, SharedString,
    StatefulInteractiveElement, div, img, prelude::*, px,
};

use crate::theme::MaterialThemeContext;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LockView {
    #[default]
    Clock,
    Login,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LockStatus {
    #[default]
    Locked,
    Authenticating,
    Failure,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FingerprintStatus {
    #[default]
    Unavailable,
    Ready,
    Scanning,
}

/// Events routed to a session-lock host. Secret characters are never retained
/// by this component or included in debug/telemetry state.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LockUserAction {
    Character(char),
    Backspace,
    ClearPassword,
    Submit { inhibit_idle: bool },
    ReturnToClock,
    EnterLogin,
}

#[derive(Clone)]
pub struct LockSurfaceInput {
    pub view: LockView,
    pub status: LockStatus,
    pub fingerprint: FingerprintStatus,
    /// Count only; password bytes/chars stay in the authentication host.
    pub password_length: usize,
    pub user_name: String,
    pub user_detail: Option<String>,
    pub time: String,
    pub date: String,
    /// Only local image files are accepted by this presentation input.
    pub wallpaper_preview: Option<PathBuf>,
    pub theme: MaterialThemeContext,
    /// Host-owned focus target; focus it when the secure compositor surface is
    /// mounted. The view keeps keyboard routing at this root across view changes.
    pub focus_handle: FocusHandle,
    pub on_action: Arc<dyn Fn(LockUserAction) + Send + Sync>,
}

/// Material lock surface visual. This does not create a lock window and must
/// only be mounted by a compositor-backed locking integration.
pub fn render(input: LockSurfaceInput) -> gpui::AnyElement {
    let primary = input
        .theme
        .colors
        .primary
        .unwrap_or_else(|| gpui::rgb(0x6750A4).into());
    let on_surface = input
        .theme
        .colors
        .on_surface
        .unwrap_or_else(|| gpui::rgb(0xF5F1FA).into());
    let muted = input
        .theme
        .colors
        .on_surface_variant
        .unwrap_or_else(|| gpui::rgb(0xCAC4D0).into());
    let surface = input
        .theme
        .colors
        .surface_container
        .or(input.theme.colors.surface)
        .unwrap_or_else(|| gpui::rgb(0x211F26).into());
    let outline = input
        .theme
        .colors
        .outline_variant
        .or(input.theme.colors.outline)
        .unwrap_or_else(|| gpui::rgb(0x49454F).into());
    let error = input
        .theme
        .colors
        .error
        .unwrap_or_else(|| gpui::rgb(0xF2B8B5).into());
    let scale = input.theme.typography.size_scale.max(0.75);
    let radius = input.theme.shape.large.max(28.0);
    let status_text = match input.status {
        LockStatus::Locked => "Enter your password",
        LockStatus::Authenticating => "Authenticating…",
        LockStatus::Failure => "Incorrect password",
    };
    let status_color = if input.status == LockStatus::Failure {
        error
    } else {
        muted
    };
    let fingerprint_text = match input.fingerprint {
        FingerprintStatus::Unavailable => None,
        FingerprintStatus::Ready => Some("Touch the fingerprint sensor to unlock"),
        FingerprintStatus::Scanning => Some("Checking fingerprint…"),
    };
    let key_action = input.on_action.clone();
    let submit_action = input.on_action.clone();
    let root_focus = input.focus_handle.clone();
    let key_view = input.view;
    let key_status = input.status;
    let key_password_length = input.password_length;
    let mut root = div()
        .size_full()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgb(0x17151B))
        .id("ii.lock.surface")
        .track_focus(&input.focus_handle)
        .tab_stop(false)
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if root_focus.contains_focused(window, cx) || event.keystroke.key == "escape" {
                if let Some(action) =
                    key_action_for(key_view, key_status, key_password_length, event)
                {
                    if dispatch_root_action(root_focus.is_focused(window), action) {
                        key_action(action);
                    }
                }
            }
        });

    if let Some(wallpaper) = input.wallpaper_preview {
        root = root.child(
            img(wallpaper)
                .size_full()
                .absolute()
                .top_0()
                .left_0()
                .object_fit(ObjectFit::Cover),
        );
    }
    root = root.child(
        div()
            .absolute()
            .inset_0()
            .bg(gpui::hsla(0.75, 0.12, 0.08, 0.56)),
    );

    let mut panel = div()
        .relative()
        .w(px(440.0))
        .max_w(px(440.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(16.0))
        .rounded(px(radius))
        .border_1()
        .border_color(outline.opacity(0.72))
        .bg(surface.opacity(0.90))
        .p(px(32.0))
        .shadow_lg()
        .id("ii.lock.panel")
        .role(Role::Group)
        .aria_label("Screen locked")
        .child(
            div()
                .font_family(input.theme.typography.monospace.clone())
                .text_size(px(64.0 * scale))
                .font_weight(FontWeight::LIGHT)
                .text_color(on_surface)
                .child(input.time),
        )
        .child(
            div()
                .text_size(px(16.0 * scale))
                .text_color(muted)
                .child(input.date),
        )
        .child(
            div()
                .mt(px(8.0))
                .w(px(72.0))
                .h(px(72.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(primary.opacity(0.24))
                .text_size(px(32.0))
                .text_color(on_surface)
                .child(user_initial(&input.user_name)),
        )
        .child(
            div()
                .font_family(input.theme.typography.title.clone())
                .text_size(px(22.0 * scale))
                .font_weight(FontWeight::MEDIUM)
                .text_color(on_surface)
                .child(input.user_name),
        );
    if let Some(detail) = input.user_detail {
        panel = panel.child(
            div()
                .text_size(px(13.0 * scale))
                .text_color(muted)
                .child(detail),
        );
    }

    if input.view == LockView::Login {
        let secret_count = input.password_length;
        let submit_enabled = secret_count > 0 && input.status != LockStatus::Authenticating;
        let submit_color = if submit_enabled { primary } else { outline };
        let password_hint = if secret_count == 0 {
            "Password".to_owned()
        } else {
            "• ".repeat(secret_count.min(32))
        };
        panel = panel.child(
            div()
                .mt(px(8.0))
                .w_full()
                .h(px(56.0))
                .flex()
                .items_center()
                .gap(px(12.0))
                .px(px(18.0))
                .rounded_full()
                .border_1()
                .border_color(if input.status == LockStatus::Failure {
                    error
                } else {
                    outline
                })
                .bg(surface)
                .child(
                    div()
                        .flex_1()
                        .id("ii.lock.password-input")
                        .role(Role::TextInput)
                        .aria_label("Password")
                        .aria_description("Password characters are masked")
                        .aria_value(password_accessible_value(secret_count))
                        .text_size(px(16.0 * scale))
                        .text_color(if secret_count == 0 { muted } else { on_surface })
                        .child(password_hint),
                )
                .child({
                    let submit = submit_action.clone();
                    let mut button = div()
                        .size(px(40.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(submit_color)
                        .text_size(px(20.0))
                        .text_color(on_surface)
                        .id("ii.lock.submit")
                        .role(Role::Button)
                        .aria_label("Submit password")
                        .aria_description(if submit_enabled {
                            "Submits the password to the authentication host"
                        } else if input.status == LockStatus::Authenticating {
                            "Submission is unavailable while authentication is in progress"
                        } else {
                            "Enter a password to enable submission"
                        })
                        .child("→");
                    if submit_enabled {
                        let key_submit = submit.clone();
                        button = button
                            .focusable()
                            .tab_stop(true)
                            .on_click(move |_, _, _| {
                                submit(LockUserAction::Submit {
                                    inhibit_idle: false,
                                })
                            })
                            .on_key_down(move |event: &KeyDownEvent, _, _| {
                                if let Some(action) = button_key_action(event) {
                                    key_submit(action);
                                }
                            });
                    }
                    button
                }),
        );
        panel = panel.child(
            div()
                .text_size(px(14.0 * scale))
                .text_color(status_color)
                .id("ii.lock.status")
                .role(Role::Status)
                .child(status_text),
        );
        if let Some(text) = fingerprint_text {
            panel = panel.child(
                div()
                    .text_size(px(13.0 * scale))
                    .text_color(muted)
                    .child(format!("◉  {text}")),
            );
        }
        panel = panel.child(
            div()
                .mt(px(4.0))
                .text_size(px(13.0 * scale))
                .text_color(muted)
                .child("Esc · clear password / return to clock"),
        );
    } else {
        panel = panel.child(
            div()
                .mt(px(8.0))
                .text_size(px(14.0 * scale))
                .text_color(muted)
                .child("Press any key to unlock"),
        );
    }

    root.child(panel).into_any_element()
}

fn dispatch_root_action(root_focused: bool, action: LockUserAction) -> bool {
    root_focused
        || !matches!(
            action,
            LockUserAction::Submit { .. } | LockUserAction::Character(' ')
        )
}

fn password_accessible_value(length: usize) -> String {
    if length == 0 {
        "empty".to_owned()
    } else {
        format!("{length} characters entered")
    }
}

/// GPUI turns unmodified Enter/Space on a focused `focusable` div into click
/// on key-up. Handle only Ctrl+Enter here because modified keys bypass that
/// native activation and QML uses Ctrl as the inhibit-idle submit modifier.
fn button_key_action(event: &KeyDownEvent) -> Option<LockUserAction> {
    (!event.is_held
        && event.keystroke.modifiers.control
        && matches!(event.keystroke.key.as_str(), "enter" | "return"))
    .then_some(LockUserAction::Submit { inhibit_idle: true })
}

/// Route lock-key behavior without storing a password in view state.
pub fn key_action_for(
    view: LockView,
    status: LockStatus,
    password_length: usize,
    event: &KeyDownEvent,
) -> Option<LockUserAction> {
    let key = event.keystroke.key.as_str();
    let key_char = event.keystroke.key_char.as_deref();
    if key == "escape" {
        return if password_length > 0 {
            Some(LockUserAction::ClearPassword)
        } else if view == LockView::Login {
            Some(LockUserAction::ReturnToClock)
        } else {
            None
        };
    }

    if status == LockStatus::Authenticating {
        return None;
    }

    if view == LockView::Clock {
        if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
            return Some(LockUserAction::EnterLogin);
        }
        return Some(match key_char.and_then(single_printable_char) {
            Some(character) => LockUserAction::Character(character),
            None => LockUserAction::EnterLogin,
        });
    }

    match key {
        "enter" | "return" if password_length > 0 => Some(LockUserAction::Submit {
            inhibit_idle: event.keystroke.modifiers.control,
        }),
        "backspace" if password_length > 0 => Some(LockUserAction::Backspace),
        _ => key_char
            .and_then(single_printable_char)
            .filter(|_| !event.keystroke.modifiers.control && !event.keystroke.modifiers.platform)
            .map(LockUserAction::Character),
    }
}

fn single_printable_char(value: &str) -> Option<char> {
    let mut chars = value.chars();
    let character = chars.next()?;
    (chars.next().is_none() && !character.is_control()).then_some(character)
}

fn user_initial(user_name: &str) -> SharedString {
    user_name
        .chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().collect::<String>())
        .filter(|initial| !initial.is_empty())
        .unwrap_or_else(|| "•".to_owned())
        .into()
}

#[cfg(test)]
mod tests {
    use gpui::{KeyDownEvent, Keystroke, Modifiers};

    use super::{
        LockStatus, LockUserAction, LockView, button_key_action, dispatch_root_action,
        key_action_for, password_accessible_value,
    };

    fn key(key: &str, key_char: Option<&str>, control: bool) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: Keystroke {
                key: key.to_owned(),
                key_char: key_char.map(str::to_owned),
                modifiers: Modifiers {
                    control,
                    ..Default::default()
                },
            },
            is_held: false,
            prefer_character_input: true,
        }
    }

    #[test]
    fn first_printable_key_routes_to_login_and_includes_character() {
        assert!(matches!(
            key_action_for(
                LockView::Clock,
                LockStatus::Locked,
                0,
                &key("a", Some("a"), false)
            ),
            Some(LockUserAction::Character('a'))
        ));
        assert!(matches!(
            key_action_for(
                LockView::Clock,
                LockStatus::Locked,
                0,
                &key("enter", None, false)
            ),
            Some(LockUserAction::EnterLogin)
        ));
        assert!(matches!(
            key_action_for(
                LockView::Clock,
                LockStatus::Locked,
                0,
                &key("a", Some("a"), true)
            ),
            Some(LockUserAction::EnterLogin)
        ));
    }

    #[test]
    fn escape_clears_before_it_returns_to_clock() {
        assert!(matches!(
            key_action_for(
                LockView::Login,
                LockStatus::Locked,
                4,
                &key("escape", None, false)
            ),
            Some(LockUserAction::ClearPassword)
        ));
        assert!(matches!(
            key_action_for(
                LockView::Login,
                LockStatus::Locked,
                0,
                &key("escape", None, false)
            ),
            Some(LockUserAction::ReturnToClock)
        ));
    }

    #[test]
    fn submit_requires_nonempty_secret_and_keeps_control_inhibit_semantics() {
        assert!(
            key_action_for(
                LockView::Login,
                LockStatus::Locked,
                0,
                &key("enter", None, false)
            )
            .is_none()
        );
        assert!(matches!(
            key_action_for(
                LockView::Login,
                LockStatus::Locked,
                2,
                &key("enter", None, true)
            ),
            Some(LockUserAction::Submit { inhibit_idle: true })
        ));
        assert!(
            key_action_for(
                LockView::Login,
                LockStatus::Authenticating,
                2,
                &key("enter", None, false)
            )
            .is_none()
        );
    }

    #[test]
    fn descendant_focus_keeps_character_routing_without_duplicate_submit() {
        assert!(dispatch_root_action(false, LockUserAction::Character('x')));
        assert!(!dispatch_root_action(false, LockUserAction::Character(' ')));
        assert!(dispatch_root_action(true, LockUserAction::Character(' ')));
        assert!(!dispatch_root_action(
            false,
            LockUserAction::Submit {
                inhibit_idle: false
            }
        ));
        assert!(dispatch_root_action(
            true,
            LockUserAction::Submit {
                inhibit_idle: false
            }
        ));
    }

    #[test]
    fn accessible_password_value_contains_only_empty_or_character_count() {
        assert_eq!(password_accessible_value(0), "empty");
        assert_eq!(password_accessible_value(3), "3 characters entered");
    }

    #[test]
    fn button_keydown_only_handles_ctrl_enter_activation_modifier() {
        assert!(button_key_action(&key("enter", None, false)).is_none());
        assert!(button_key_action(&key("space", None, false)).is_none());
        assert!(matches!(
            button_key_action(&key("enter", None, true)),
            Some(LockUserAction::Submit { inhibit_idle: true })
        ));
    }
}
