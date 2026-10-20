//! Material II control-panel surface.
//!
//! The QML service adapters for media, wallpaper, weather and system controls
//! are not available in GPUI yet. This view deliberately renders explicit
//! unavailable states instead of substituting sample values.

use std::{sync::Arc, time::Duration};

use gpui::{Animation, AnimationExt, AnyElement, FocusHandle, KeyDownEvent, div, prelude::*, px};

use crate::config::{ControlPanelConfig, ControlPanelSection, ControlPanelStyle, ShellConfig};

use super::{
    media_player::{MediaPlayerInput, render_media_player},
    quick_sliders::{QuickSlidersInput, QuickSlidersState, render_quick_sliders},
    quick_toggles::{QuickTogglesInput, QuickTogglesState, render_quick_toggles},
    sidebars::SidebarStyle,
};

#[derive(Clone)]
pub struct ControlPanelInput {
    pub config: ControlPanelConfig,
    pub style: SidebarStyle,
    pub output_width: f32,
    pub output_height: f32,
    pub bar_bottom: bool,
    pub bar_height: f32,
    pub dock: Option<DockInset>,
    pub media: crate::components::vertical_bar::MediaState,
    pub on_media_action: Arc<dyn Fn(crate::components::vertical_bar::MediaAction) + Send + Sync>,
    pub focus: FocusHandle,
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    pub interactive: bool,
    pub transition_generation: u64,
}

pub fn render(input: ControlPanelInput) -> AnyElement {
    let geometry = ControlPanelGeometry::from_layout(
        input.output_width,
        input.output_height,
        input.bar_bottom,
        input.bar_height,
        input.dock,
    );
    let sections = input.config.visible_sections();
    let compact = input.config.compact_mode;
    let mut card_style = input.style.clone();
    if input.config.style == ControlPanelStyle::Island {
        card_style.radius = card_style.radius.max(24.0);
    }
    let media = input.media.clone();
    let on_media_action = input.on_media_action.clone();
    let show_wallpaper_scheme_chips = input.config.show_wallpaper_scheme_chips;
    let section_style = card_style.clone();
    let content_sections = sections.into_iter().map(move |section| {
        render_section(
            section,
            &section_style,
            compact,
            media.clone(),
            on_media_action.clone(),
            show_wallpaper_scheme_chips,
        )
    });
    let close = input.on_close.clone();
    let interactive = input.interactive;
    let transition_generation = input.transition_generation;
    let focus = input.focus;
    let panel_radius = card_style.radius;
    let panel_background = card_style.surface;
    let panel_border = card_style.border;
    let panel_width = geometry.width;
    let max_panel_height = geometry.max_height;
    let panel_x = geometry.left;
    let panel_y = geometry.top;
    let pad = if compact { 10.0 } else { 12.0 };
    let gap = if compact { 4.0 } else { 6.0 };

    div()
        .id("ii.control-panel.overlay")
        .absolute()
        .inset_0()
        .focusable()
        .tab_stop(false)
        .track_focus(&focus)
        .on_key_down({
            let close = close.clone();
            move |event: &KeyDownEvent, _, cx| {
                if interactive
                    && !event.is_held
                    && event.keystroke.key.eq_ignore_ascii_case("escape")
                {
                    close();
                    cx.stop_propagation();
                }
            }
        })
        .when(interactive, |this| this.on_click(move |_, _, _| close()))
        .child(
            div()
                .id("ii.control-panel.card")
                .absolute()
                .left(px(panel_x))
                .top(px(panel_y))
                .w(px(panel_width))
                .min_h(px(geometry.minimum_height))
                .max_h(px(max_panel_height))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(gap))
                .p(px(pad))
                .bg(panel_background)
                .border_1()
                .border_color(panel_border)
                .rounded(px(panel_radius))
                .shadow_lg()
                .role(gpui::Role::Dialog)
                .aria_label("Control panel")
                .on_click(|_, _, cx| cx.stop_propagation())
                .children(content_sections)
                .with_animation(
                    format!("ii.control-panel.transition.{transition_generation}"),
                    Animation::new(Duration::from_millis(250)).with_easing(gpui::ease_in_out),
                    move |this, delta| {
                        let progress = if interactive { delta } else { 1.0 - delta };
                        let scale = 0.94 + 0.06 * progress;
                        let width = panel_width * scale;
                        let height = max_panel_height * scale;
                        this.opacity(progress)
                            .w(px(width))
                            .min_h(px(geometry.minimum_height * scale))
                            .max_h(px(height))
                            .left(px(panel_x + (panel_width - width) * 0.5))
                            .top(px(panel_y + (max_panel_height - height) * 0.5
                                - 24.0 * (1.0 - progress)))
                    },
                ),
        )
        .into_any_element()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlPanelGeometry {
    pub width: f32,
    pub minimum_height: f32,
    pub max_height: f32,
    pub left: f32,
    pub top: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DockInset {
    pub position: DockPosition,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPosition {
    Top,
    Bottom,
    Left,
    Right,
}

impl ControlPanelGeometry {
    pub fn from_output(width: f32, height: f32, bar_bottom: bool, bar_height: f32) -> Self {
        Self::from_layout(width, height, bar_bottom, bar_height, None)
    }

    pub fn from_layout(
        width: f32,
        height: f32,
        bar_bottom: bool,
        bar_height: f32,
        dock: Option<DockInset>,
    ) -> Self {
        let min_dimension = width.min(height).max(0.0);
        let safe_padding = (min_dimension * 0.02)
            .round()
            .max(8.0)
            .min(min_dimension * 0.25);
        let bar_reserved = bar_height.max(0.0) + 16.0;
        let dock_top = dock
            .filter(|dock| dock.position == DockPosition::Top)
            .map(|dock| dock.height.max(0.0) + safe_padding)
            .unwrap_or(0.0);
        let dock_bottom = dock
            .filter(|dock| dock.position == DockPosition::Bottom)
            .map(|dock| dock.height.max(0.0) + safe_padding)
            .unwrap_or(0.0);
        let dock_left = dock
            .filter(|dock| dock.position == DockPosition::Left)
            .map(|dock| dock.width.max(0.0) + safe_padding)
            .unwrap_or(0.0);
        let dock_right = dock
            .filter(|dock| dock.position == DockPosition::Right)
            .map(|dock| dock.width.max(0.0) + safe_padding)
            .unwrap_or(0.0);
        let top_reserved = (safe_padding + dock_top + if bar_bottom { 0.0 } else { bar_reserved })
            .min(height.max(0.0) * 0.5);
        let bottom_reserved =
            (safe_padding + dock_bottom + if bar_bottom { bar_reserved } else { 0.0 })
                .min((height - top_reserved).max(0.0));
        let left_reserved = (safe_padding + dock_left).min(width.max(0.0) * 0.5);
        let right_reserved = (safe_padding + dock_right).min((width - left_reserved).max(0.0));
        let available_width = (width - left_reserved - right_reserved).max(1.0);
        let available_height = (height - top_reserved - bottom_reserved).max(1.0);
        let panel_width = available_width.min(380.0).round();
        let panel_height = (available_height - (safe_padding * 0.5).round()).max(1.0);
        let vertical_offset = (bar_height.max(0.0) * 0.5)
            .min((available_height * 0.08).max(0.0))
            .round();
        let safe_top = top_reserved;
        let safe_bottom = height - bottom_reserved;
        let max_top = (safe_bottom - panel_height).max(safe_top);
        let center_y = (safe_top + safe_bottom) * 0.5
            + if bar_bottom {
                -vertical_offset
            } else {
                vertical_offset
            };
        Self {
            width: panel_width,
            minimum_height: panel_height.min(260.0),
            max_height: panel_height,
            left: ((left_reserved + width - right_reserved - panel_width) * 0.5).max(left_reserved),
            top: (center_y - panel_height * 0.5).clamp(safe_top, max_top),
        }
    }
}

impl DockInset {
    pub fn from_shell_config(config: &ShellConfig) -> Option<Self> {
        let dock = config.appearance.source_sections.get("ii.dock")?;
        if dock.get("enable").and_then(serde_json::Value::as_bool) == Some(false) {
            return None;
        }
        let position = match dock
            .get("position")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("bottom")
        {
            "top" => DockPosition::Top,
            "left" => DockPosition::Left,
            "right" => DockPosition::Right,
            _ => DockPosition::Bottom,
        };
        let height = dock
            .get("height")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(60.0)
            .clamp(0.0, 512.0) as f32;
        let width = dock
            .get("width")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(height as f64)
            .clamp(0.0, 512.0) as f32;
        Some(Self {
            position,
            width,
            height,
        })
    }
}

fn render_section(
    section: ControlPanelSection,
    style: &SidebarStyle,
    compact: bool,
    media: crate::components::vertical_bar::MediaState,
    on_media_action: Arc<dyn Fn(crate::components::vertical_bar::MediaAction) + Send + Sync>,
    show_wallpaper_scheme_chips: bool,
) -> AnyElement {
    let (id, title, message) = match section {
        ControlPanelSection::Profile => ("profile", "Profile", "Profile details are unavailable"),
        ControlPanelSection::DateTime => ("date-time", "Date & time", "Clock data is unavailable"),
        ControlPanelSection::Media => {
            if media.status == crate::components::vertical_bar::MediaStatus::NoPlayer {
                return unavailable_card(
                    "media",
                    "Media",
                    "No active player",
                    style,
                    compact,
                    None,
                );
            }
            return render_media_player(MediaPlayerInput {
                media: Some(media),
                style: style.clone(),
                on_action: Some(on_media_action),
            });
        }
        ControlPanelSection::Wallpaper => {
            let extra = (!wallpaper_scheme_chip_labels(show_wallpaper_scheme_chips).is_empty())
                .then(|| render_unavailable_wallpaper_scheme_chips(style));
            return unavailable_card(
                "wallpaper",
                "Wallpaper",
                if show_wallpaper_scheme_chips {
                    "Wallpaper service is unavailable; scheme switching is disabled"
                } else {
                    "Wallpaper service is unavailable"
                },
                style,
                compact,
                extra,
            );
        }
        ControlPanelSection::Weather => ("weather", "Weather", "Weather service is unavailable"),
        ControlPanelSection::System => ("system", "System", "System metrics are unavailable"),
        ControlPanelSection::Sliders => return render_slider_section(style.clone()),
        ControlPanelSection::QuickActions => return render_actions_section(style.clone()),
    };
    let padding = if compact { 10.0 } else { 12.0 };
    div()
        .id(format!("ii.control-panel.section.{id}"))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(padding))
        .rounded(px(style.card_radius))
        .bg(style.card)
        .border_1()
        .border_color(style.border.opacity(0.55))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(style.foreground)
                .child(title),
        )
        .child(
            div()
                .id(format!("ii.control-panel.section.{id}.unavailable"))
                .w_full()
                .py(px(4.0))
                .text_sm()
                .text_color(style.muted)
                .role(gpui::Role::Status)
                .aria_label(message)
                .child(message),
        )
        .into_any_element()
}

fn unavailable_card(
    id: &'static str,
    title: &'static str,
    message: &'static str,
    style: &SidebarStyle,
    compact: bool,
    extra: Option<AnyElement>,
) -> AnyElement {
    let padding = if compact { 10.0 } else { 12.0 };
    div()
        .id(format!("ii.control-panel.section.{id}"))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(padding))
        .rounded(px(style.card_radius))
        .bg(style.card)
        .border_1()
        .border_color(style.border.opacity(0.55))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(style.foreground)
                .child(title),
        )
        .child(
            div()
                .id(format!("ii.control-panel.section.{id}.unavailable"))
                .w_full()
                .py(px(4.0))
                .text_sm()
                .text_color(style.muted)
                .role(gpui::Role::Status)
                .aria_label(message)
                .child(message),
        )
        .children(extra)
        .into_any_element()
}

const WALLPAPER_SCHEME_CHIPS: &[&str] = &[
    "Auto",
    "Content",
    "Expressive",
    "Fidelity",
    "Fruit Salad",
    "Monochrome",
    "Neutral",
    "Rainbow",
    "Tonal Spot",
];

fn wallpaper_scheme_chip_labels(show: bool) -> &'static [&'static str] {
    if show { WALLPAPER_SCHEME_CHIPS } else { &[] }
}

fn render_unavailable_wallpaper_scheme_chips(style: &SidebarStyle) -> AnyElement {
    div()
        .id("ii.control-panel.wallpaper.scheme-chips")
        .w_full()
        .flex()
        .flex_wrap()
        .gap(px(6.0))
        .children(wallpaper_scheme_chip_labels(true).iter().map(|label| {
            div()
                .id(format!(
                    "ii.control-panel.wallpaper.scheme.{}",
                    label.to_lowercase().replace(' ', "-")
                ))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(style.card_radius))
                .border_1()
                .border_color(style.border.opacity(0.5))
                .bg(style.surface)
                .opacity(0.45)
                .text_xs()
                .text_color(style.muted)
                .aria_label(format!("{label}, unavailable"))
                .child(*label)
        }))
        .into_any_element()
}

fn render_slider_section(style: SidebarStyle) -> AnyElement {
    div()
        .id("ii.control-panel.section.sliders")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(style.padding))
        .rounded(px(style.card_radius))
        .bg(style.card)
        .border_1()
        .border_color(style.border.opacity(0.55))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(style.foreground)
                .child("Volume & brightness"),
        )
        .child(render_quick_sliders(QuickSlidersInput {
            state: QuickSlidersState::Unavailable,
            message: Some("Audio and brightness controls are unavailable".into()),
            sliders: Vec::new(),
            style,
            on_value_change: Arc::new(|_, _| {}),
        }))
        .into_any_element()
}

fn render_actions_section(style: SidebarStyle) -> AnyElement {
    div()
        .id("ii.control-panel.section.quick-actions")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(style.padding))
        .rounded(px(style.card_radius))
        .bg(style.card)
        .border_1()
        .border_color(style.border.opacity(0.55))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(style.foreground)
                .child("Quick actions"),
        )
        .child(render_quick_toggles(QuickTogglesInput {
            state: QuickTogglesState::Unavailable,
            message: Some("System actions are unavailable".into()),
            toggles: Vec::new(),
            style,
            on_toggle: Arc::new(|_, _| {}),
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{ControlPanelGeometry, DockInset, DockPosition, wallpaper_scheme_chip_labels};

    #[test]
    fn wallpaper_scheme_chips_are_gated_and_remain_unavailable_placeholders() {
        assert!(wallpaper_scheme_chip_labels(false).is_empty());
        assert_eq!(
            wallpaper_scheme_chip_labels(true),
            &[
                "Auto",
                "Content",
                "Expressive",
                "Fidelity",
                "Fruit Salad",
                "Monochrome",
                "Neutral",
                "Rainbow",
                "Tonal Spot",
            ]
        );
    }

    #[test]
    fn geometry_caps_width_and_reserves_bar_safe_area() {
        let geometry = ControlPanelGeometry::from_output(1920.0, 1080.0, false, 48.0);
        assert_eq!(geometry.width, 380.0);
        assert!(geometry.left >= 38.0);
        assert!(geometry.top >= 70.0);
        assert!(geometry.max_height <= 1080.0);
    }

    #[test]
    fn geometry_handles_small_and_bottom_bar_outputs() {
        let geometry = ControlPanelGeometry::from_output(640.0, 480.0, true, 40.0);
        assert!(geometry.width <= 380.0);
        assert!(geometry.top >= 8.0);
        assert!(geometry.max_height >= 260.0);
    }

    #[test]
    fn geometry_stays_inside_very_small_output_bounds() {
        let geometry = ControlPanelGeometry::from_output(180.0, 120.0, false, 80.0);
        assert!(geometry.left >= 0.0);
        assert!(geometry.top >= 0.0);
        assert!(geometry.left + geometry.width <= 180.0);
        assert!(geometry.top + geometry.max_height <= 120.0);
    }

    #[test]
    fn dock_reservation_is_disabled_and_normalized_from_source_config() {
        let config = crate::config::ConfigLoad::from_bytes(
            br#"{"dock":{"enable":true,"position":"left","height":72}}"#,
        )
        .config;
        let dock = DockInset::from_shell_config(&config).expect("default dock enabled");
        assert_eq!(dock.position, DockPosition::Left);
        assert_eq!(dock.width, 72.0);
        assert_eq!(dock.height, 72.0);

        let disabled = crate::config::ConfigLoad::from_bytes(
            br#"{"dock":{"enable":false,"position":"bottom"}}"#,
        )
        .config;
        assert!(DockInset::from_shell_config(&disabled).is_none());
    }
}
