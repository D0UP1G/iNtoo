//! Presentational Material ii session/power menu.
//!
//! The QML `Session` singleton remains the authority for lock, sleep, logout,
//! power-off and reboot behavior. This view receives only already-resolved
//! availability, presentation strings and theme tokens, and emits typed
//! requests to a future host adapter. It does not run commands or decide
//! whether a session action is safe.

use std::{path::PathBuf, sync::Arc};

use gpui::{
    AnyElement, FocusHandle, FontWeight, InteractiveElement, KeyDownEvent, ObjectFit, Role,
    SharedString, div, prelude::*, px,
};

use crate::theme::MaterialThemeContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionAction {
    Lock,
    Sleep,
    Logout,
    TaskManager,
    Hibernate,
    Shutdown,
    Reboot,
    FirmwareReboot,
}

impl SessionAction {
    pub const ORDER: [Self; 8] = [
        Self::Lock,
        Self::Sleep,
        Self::Logout,
        Self::TaskManager,
        Self::Hibernate,
        Self::Shutdown,
        Self::Reboot,
        Self::FirmwareReboot,
    ];

    pub const fn icon(self) -> &'static str {
        match self {
            Self::Lock => "lock",
            Self::Sleep => "dark_mode",
            Self::Logout => "logout",
            Self::TaskManager => "browse_activity",
            Self::Hibernate => "downloading",
            Self::Shutdown => "power_settings_new",
            Self::Reboot => "restart_alt",
            Self::FirmwareReboot => "settings_applications",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SessionScreenLabels {
    pub title: String,
    pub instructions: String,
    pub session_action_description: String,
    pub selected_session_action_description: String,
    pub lock: String,
    pub sleep: String,
    pub logout: String,
    pub task_manager: String,
    pub hibernate: String,
    pub shutdown: String,
    pub reboot: String,
    pub firmware_reboot: String,
    pub package_manager_warning: String,
    pub download_warning: String,
}

impl SessionScreenLabels {
    fn action(&self, action: SessionAction) -> &str {
        match action {
            SessionAction::Lock => &self.lock,
            SessionAction::Sleep => &self.sleep,
            SessionAction::Logout => &self.logout,
            SessionAction::TaskManager => &self.task_manager,
            SessionAction::Hibernate => &self.hibernate,
            SessionAction::Shutdown => &self.shutdown,
            SessionAction::Reboot => &self.reboot,
            SessionAction::FirmwareReboot => &self.firmware_reboot,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionScreenEvent {
    /// Ask the host to handle this action. The host closes only after an
    /// injected handler acknowledges it; this view never executes operations.
    Activate(SessionAction),
    /// Selection changed by pointer or keyboard navigation.
    Select(SessionAction),
    Dismiss,
}

#[derive(Clone)]
pub struct SessionScreenInput {
    pub labels: SessionScreenLabels,
    pub selected: SessionAction,
    /// QML shows hibernate while capability detection is unknown, and hides it
    /// only after an explicit negative capability result.
    pub show_hibernate: bool,
    pub package_manager_running: bool,
    pub download_running: bool,
    pub closing: bool,
    pub editorial_style: bool,
    pub zzz_style: bool,
    pub wallpaper_preview: Option<PathBuf>,
    pub theme: MaterialThemeContext,
    /// Host focuses this root when opening the surface. The view routes the
    /// QML arrow-key graph here and reports selection changes so the host can
    /// refresh the subtitle; labelled actions retain pointer activation.
    pub focus: FocusHandle,
    pub on_event: Arc<dyn Fn(SessionScreenEvent) + Send + Sync>,
}

/// Render the Material ii session menu. The caller owns visibility, action
/// execution, translation and service warnings.
pub fn render(input: SessionScreenInput) -> AnyElement {
    let colors = &input.theme.colors;
    let primary = colors.primary.unwrap_or_else(|| gpui::rgb(0x6750A4).into());
    let on_primary = colors
        .on_primary
        .unwrap_or_else(|| gpui::rgb(0xFFFFFF).into());
    let on_surface = colors
        .on_surface
        .unwrap_or_else(|| gpui::rgb(0xF5F1FA).into());
    let muted = colors
        .on_surface_variant
        .unwrap_or_else(|| gpui::rgb(0xCAC4D0).into());
    let surface = colors
        .surface_container
        .or(colors.surface)
        .unwrap_or_else(|| gpui::rgb(0x211F26).into());
    let surface_variant = colors
        .secondary_container
        .or(colors.surface_container_high)
        .unwrap_or_else(|| gpui::rgb(0x49454F).into());
    let danger = colors
        .error_container
        .unwrap_or_else(|| gpui::rgb(0x5D1916).into());
    let warning_text = colors
        .on_error_container
        .unwrap_or_else(|| gpui::rgb(0xF9DEDC).into());
    let border = colors
        .outline_variant
        .unwrap_or_else(|| gpui::rgb(0x49454F).into());
    let radius = if input.editorial_style {
        input.theme.shape.small
    } else {
        input.theme.shape.large.max(18.0)
    };
    let tile_radius = if input.zzz_style || input.editorial_style {
        input.theme.shape.medium.max(8.0)
    } else {
        120.0
    };
    let tile_size = if input.editorial_style { 104.0 } else { 120.0 };
    let spacing = if input.editorial_style { 10.0 } else { 15.0 };
    let visible = visible_actions(input.show_hibernate);
    let selected = if visible.contains(&input.selected) {
        input.selected
    } else {
        SessionAction::Lock
    };
    let events = input.on_event.clone();
    let selected_for_keys = selected;
    let show_hibernate = input.show_hibernate;
    let focus = input.focus;
    let root_focus = focus.clone();
    let is_closing = input.closing;
    let editorial = input.editorial_style;
    let zzz = input.zzz_style;
    let labels = input.labels;
    let instructions_color = if zzz { muted } else { on_surface };

    let mut root = div()
        .id("ii.session-screen")
        .size_full()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgb(0x17151B))
        .track_focus(&focus)
        .focusable()
        .tab_stop(false)
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if event.is_held {
                return;
            }
            let key = event.keystroke.key.as_str();
            if key == "escape" {
                (events)(SessionScreenEvent::Dismiss);
                cx.stop_propagation();
                return;
            }
            if matches!(key, "enter" | "return" | "space") {
                // A focused action button receives GPUI's native keyboard
                // activation and dispatches through its click callback. Only
                // activate here when the host-owned root itself has focus.
                if root_focus.is_focused(window) {
                    (events)(SessionScreenEvent::Activate(selected_for_keys));
                }
                cx.stop_propagation();
                return;
            }
            if let Some(direction) = direction_for_key(key) {
                (events)(SessionScreenEvent::Select(move_selection(
                    selected_for_keys,
                    direction,
                    show_hibernate,
                )));
                cx.stop_propagation();
            }
        })
        .on_click({
            let on_event = input.on_event.clone();
            move |_, _, _| on_event(SessionScreenEvent::Dismiss)
        });

    if let Some(wallpaper) = input.wallpaper_preview {
        root = root.child(
            gpui::img(wallpaper)
                .absolute()
                .inset_0()
                .object_fit(ObjectFit::Cover),
        );
    }
    root = root.child(
        div()
            .absolute()
            .inset_0()
            .bg(gpui::hsla(0.75, 0.12, 0.08, 0.44)),
    );

    let mut panel = div()
        .id("ii.session-screen.content")
        .relative()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(if editorial { 12.0 } else { 15.0 }))
        .p(px(if editorial { 32.0 } else { 0.0 }))
        .on_click(|_, _, cx| cx.stop_propagation())
        .when(editorial, |this| {
            this.rounded(px(radius))
                .border_1()
                .border_color(border.opacity(0.5))
                .bg(surface.opacity(0.94))
        })
        .text_color(on_surface);

    let title = labels.title.clone();
    let title_size = if editorial { 34.0 } else { 30.0 };
    panel = panel.child(
        div()
            .text_size(px(title_size))
            .font_weight(if zzz {
                FontWeight::BLACK
            } else if editorial {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            })
            .when(zzz, |this| this.italic())
            .font_family(input.theme.typography.title.clone())
            .child(title),
    );
    panel = panel.child(
        div()
            .text_size(px(if editorial { 13.0 } else { 14.0 }))
            .text_color(instructions_color.opacity(if editorial { 0.72 } else { 1.0 }))
            .text_center()
            .child(labels.instructions.clone()),
    );

    let actions = grid_positions(input.show_hibernate)
        .into_iter()
        .map(|(action, column, row)| {
            let is_selected = action == selected;
            let label = labels.action(action).to_owned();
            let icon = action.icon();
            let on_click = input.on_event.clone();
            let icon_color = if is_selected { on_primary } else { on_surface };
            let background = if is_selected {
                primary
            } else {
                surface_variant
            };
            let hover_background = if editorial || zzz {
                surface_variant
            } else {
                primary
            };
            div()
                .id(SharedString::from(format!(
                    "ii.session-action.{row}.{column}"
                )))
                .w(px(tile_size))
                .h(px(tile_size))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(tile_radius))
                .bg(background)
                .text_color(icon_color)
                .cursor_pointer()
                .focusable()
                .tab_stop(false)
                .role(Role::Button)
                .aria_label(SharedString::from(label.clone()))
                .aria_description(SharedString::from(action_accessible_description(
                    is_selected,
                    &labels.session_action_description,
                    &labels.selected_session_action_description,
                )))
                .hover(move |this| this.bg(hover_background))
                .on_click(move |_, _, cx| {
                    on_click(SessionScreenEvent::Activate(action));
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .font_family("Material Symbols Rounded")
                        .text_size(px(if editorial { 34.0 } else { 45.0 }))
                        .child(icon),
                )
        });
    panel = panel.child(
        div()
            .flex()
            .flex_wrap()
            // Keep a partial second row aligned to column one. QML GridLayout
            // auto-placement omits hidden children and centers each child in
            // its fixed column; centering the row itself would shift it.
            .justify_start()
            .gap(px(spacing))
            .max_w(px(tile_size * 4.0 + spacing * 3.0))
            .children(actions),
    );
    panel = panel.child(
        div()
            .min_w(px(96.0))
            .max_w(px(320.0))
            .px(px(15.0))
            .py(px(10.0))
            .rounded(px(if zzz || editorial { radius } else { 14.0 }))
            .border_1()
            .border_color(border.opacity(0.4))
            .bg(surface.opacity(0.92))
            .text_size(px(14.0))
            .text_color(on_surface)
            .text_center()
            .child(labels.action(selected).to_owned()),
    );

    let mut warnings = div().flex().flex_row().gap(px(10.0));
    if input.package_manager_running {
        warnings = warnings.child(warning(
            labels.package_manager_warning.clone(),
            danger,
            warning_text,
            radius,
        ));
    }
    if input.download_running {
        warnings = warnings.child(warning(
            labels.download_warning.clone(),
            danger,
            warning_text,
            radius,
        ));
    }
    if input.package_manager_running || input.download_running {
        panel = panel.child(warnings);
    }

    root.child(panel.opacity(if is_closing { 0.0 } else { 1.0 }))
        .into_any_element()
}

fn warning(
    text: String,
    background: gpui::Hsla,
    foreground: gpui::Hsla,
    radius: f32,
) -> AnyElement {
    div()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(radius))
        .bg(background)
        .text_color(foreground)
        .text_size(px(13.0))
        .child(text)
        .into_any_element()
}

/// A regular action button must not expose toggle/selection state through
/// `aria-selected`. Its accessible description communicates the visual
/// keyboard selection while focus remains on the host-owned root.
fn action_accessible_description<'a>(
    is_selected: bool,
    ordinary: &'a str,
    selected: &'a str,
) -> &'a str {
    if is_selected { selected } else { ordinary }
}

pub fn visible_actions(show_hibernate: bool) -> Vec<SessionAction> {
    if show_hibernate {
        SessionAction::ORDER.to_vec()
    } else {
        vec![
            SessionAction::Lock,
            SessionAction::Sleep,
            SessionAction::Logout,
            SessionAction::TaskManager,
            SessionAction::Shutdown,
            SessionAction::Reboot,
            SessionAction::FirmwareReboot,
        ]
    }
}

/// Assign visible actions to QML GridLayout's four columns in visible-child
/// order. Hidden Hibernate is removed from layout and later buttons flow up.
pub fn grid_positions(show_hibernate: bool) -> Vec<(SessionAction, usize, usize)> {
    visible_actions(show_hibernate)
        .into_iter()
        .enumerate()
        .map(|(index, action)| (action, index % 4, index / 4))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

fn direction_for_key(key: &str) -> Option<Direction> {
    match key {
        "left" | "arrowleft" => Some(Direction::Left),
        "right" | "arrowright" => Some(Direction::Right),
        "up" | "arrowup" => Some(Direction::Up),
        "down" | "arrowdown" => Some(Direction::Down),
        _ => None,
    }
}

/// Mirrors the explicit QML KeyNavigation graph, including the missing-
/// hibernate fallbacks. At a graph edge the current selection is retained.
pub const fn move_selection(
    selected: SessionAction,
    direction: Direction,
    show_hibernate: bool,
) -> SessionAction {
    use Direction::{Down, Left, Right, Up};
    use SessionAction::*;
    match (selected, direction) {
        (Lock, Right) => Sleep,
        (Lock, Down) => {
            if show_hibernate {
                Hibernate
            } else {
                Shutdown
            }
        }
        (Sleep, Left) => Lock,
        (Sleep, Right) => Logout,
        (Sleep, Down) => {
            if show_hibernate {
                Shutdown
            } else {
                Reboot
            }
        }
        (Logout, Left) => Sleep,
        (Logout, Right) => TaskManager,
        (Logout, Down) => {
            if show_hibernate {
                Reboot
            } else {
                FirmwareReboot
            }
        }
        (TaskManager, Left) => Logout,
        (TaskManager, Down) => FirmwareReboot,
        (Hibernate, Up) => Lock,
        (Hibernate, Right) => Shutdown,
        (Shutdown, Left) if show_hibernate => Hibernate,
        (Shutdown, Right) => Reboot,
        (Shutdown, Up) => {
            if show_hibernate {
                Sleep
            } else {
                Lock
            }
        }
        (Reboot, Left) => Shutdown,
        (Reboot, Right) => FirmwareReboot,
        (Reboot, Up) => {
            if show_hibernate {
                Logout
            } else {
                Sleep
            }
        }
        (FirmwareReboot, Left) => Reboot,
        (FirmwareReboot, Up) => {
            if show_hibernate {
                TaskManager
            } else {
                Logout
            }
        }
        (action, _) => action,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hibernate_is_omitted_only_when_capability_is_explicitly_unavailable() {
        assert_eq!(visible_actions(true).len(), 8);
        assert!(!visible_actions(false).contains(&SessionAction::Hibernate));
        assert_eq!(visible_actions(false).len(), 7);
    }

    #[test]
    fn selected_action_description_uses_the_host_localized_string() {
        assert_eq!(
            action_accessible_description(true, "Acción de sesión", "Acción seleccionada"),
            "Acción seleccionada"
        );
    }

    #[test]
    fn unselected_action_description_uses_the_host_localized_string() {
        assert_eq!(
            action_accessible_description(false, "Acción de sesión", "Acción seleccionada"),
            "Acción de sesión"
        );
    }

    #[test]
    fn hidden_hibernate_compacts_the_grid_like_qt_grid_layout() {
        let positions = grid_positions(false);
        assert_eq!(positions.len(), 7);
        assert_eq!(
            positions[4],
            (SessionAction::Shutdown, 0, 1),
            "Shutdown flows into the first column when Hibernate is hidden"
        );
        assert_eq!(positions[5], (SessionAction::Reboot, 1, 1));
        assert_eq!(positions[6], (SessionAction::FirmwareReboot, 2, 1));
    }

    #[test]
    fn visible_hibernate_keeps_the_qml_grid_order() {
        let positions = grid_positions(true);
        assert_eq!(positions[4], (SessionAction::Hibernate, 0, 1));
        assert_eq!(positions[5], (SessionAction::Shutdown, 1, 1));
        assert_eq!(positions[7], (SessionAction::FirmwareReboot, 3, 1));
    }

    #[test]
    fn navigation_matches_qml_edges_with_hibernate_available() {
        assert_eq!(
            move_selection(SessionAction::Lock, Direction::Down, true),
            SessionAction::Hibernate
        );
        assert_eq!(
            move_selection(SessionAction::Sleep, Direction::Down, true),
            SessionAction::Shutdown
        );
        assert_eq!(
            move_selection(SessionAction::TaskManager, Direction::Down, true),
            SessionAction::FirmwareReboot
        );
        assert_eq!(
            move_selection(SessionAction::Shutdown, Direction::Left, true),
            SessionAction::Hibernate
        );
    }

    #[test]
    fn navigation_uses_qml_fallbacks_when_hibernate_is_unavailable() {
        assert_eq!(
            move_selection(SessionAction::Lock, Direction::Down, false),
            SessionAction::Shutdown
        );
        assert_eq!(
            move_selection(SessionAction::Sleep, Direction::Down, false),
            SessionAction::Reboot
        );
        assert_eq!(
            move_selection(SessionAction::Logout, Direction::Down, false),
            SessionAction::FirmwareReboot
        );
        assert_eq!(
            move_selection(SessionAction::Shutdown, Direction::Up, false),
            SessionAction::Lock
        );
    }
}
