//! Read-only ControlsCard preview for the ii Widgets sidebar.
//!
//! The prototype has no runtime providers or actions for these controls, so
//! their state remains unknown and no visual control is interactive.

use gpui::{AnyElement, FontWeight, Role, SharedString, div, prelude::*, px};

use super::sidebars::SidebarStyle;

#[derive(Debug, Clone)]
pub struct ControlsCardInput {
    pub show_dark_mode: bool,
    pub show_dnd: bool,
    pub show_night_light: bool,
    pub show_game_mode: bool,
    pub show_network: bool,
    pub show_bluetooth: bool,
    pub show_settings: bool,
    pub show_lock: bool,
    pub style: SidebarStyle,
}

pub fn render_controls_card(input: ControlsCardInput) -> AnyElement {
    let style = input.style;
    let toggles = [
        (input.show_dark_mode, "Dark mode", "dark_mode"),
        (input.show_dnd, "Do not disturb", "do_not_disturb_on"),
        (input.show_night_light, "Night light", "nightlight"),
        (input.show_game_mode, "Game mode", "sports_esports"),
    ];
    let actions = [
        (input.show_network, "Network", "wifi"),
        (input.show_bluetooth, "Bluetooth", "bluetooth"),
        (input.show_settings, "Settings", "settings"),
        (input.show_lock, "Lock", "lock"),
    ];
    let visible_toggles = toggles
        .into_iter()
        .filter(|(visible, _, _)| *visible)
        .map(|(_, label, glyph)| render_control(label, glyph, 22., &style));
    let visible_actions = actions
        .into_iter()
        .filter(|(visible, _, _)| *visible)
        .map(|(_, label, glyph)| render_control(label, glyph, 20., &style));
    let has_toggles = toggles.iter().any(|(visible, _, _)| *visible);
    let has_actions = actions.iter().any(|(visible, _, _)| *visible);

    if !has_toggles && !has_actions {
        return div().into_any_element();
    }

    div()
        .id("ii.controls-card")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .px(px(12.))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(0.))
                .children(visible_toggles)
                .when(has_toggles && has_actions, |row| {
                    row.child(
                        div()
                            .mx(px(8.))
                            .w(px(1.))
                            .h(px(24.))
                            .rounded(px(1.))
                            .bg(style.border.opacity(0.5)),
                    )
                })
                .children(visible_actions),
        )
        .child(
            div()
                .id("ii.controls-card.unavailable")
                .role(Role::Status)
                .w_full()
                .text_xs()
                .text_color(style.muted)
                .text_align(gpui::TextAlign::Center)
                .font_weight(FontWeight::NORMAL)
                .child("Controls unavailable; state and actions are not connected."),
        )
        .into_any_element()
}

fn render_control(
    label: &'static str,
    glyph: &'static str,
    icon_size: f32,
    style: &SidebarStyle,
) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "ii.controls-card.{}",
            label.to_ascii_lowercase().replace(' ', "-")
        )))
        .size(px(40.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(style.card_radius.min(20.)))
        .text_color(style.muted)
        .font_family("Material Symbols Rounded")
        .text_size(px(icon_size))
        .role(Role::Group)
        .aria_label(format!("{label}: state unknown; control unavailable"))
        .child(glyph)
        .into_any_element()
}
