//! Compact resource rings for the Widgets sidebar.
//!
//! Values are supplied by the existing resource and UPower adapters. Missing
//! readings remain visibly unknown; this component does not sample services.

use gpui::{AnyElement, FontWeight, IntoElement, PathBuilder, Role, div, point, prelude::*, px};

use crate::components::vertical_bar::{BatteryChargeState, BatteryState, BatteryStatus};

use super::sidebars::SidebarStyle;

#[derive(Clone, Debug)]
pub struct StatusRingsInput {
    pub cpu_percent: Option<f64>,
    pub ram_percent: Option<f64>,
    pub battery: Option<BatteryState>,
    /// True when the provider has not published a fresh sample recently.
    pub stale: bool,
    pub show_cpu: bool,
    pub show_ram: bool,
    pub show_battery: bool,
    pub style: SidebarStyle,
}

pub fn render_status_rings(input: StatusRingsInput) -> AnyElement {
    let style = input.style;
    let stale = input.stale;
    let mut rings = Vec::new();
    if input.show_cpu {
        rings.push(render_ring("CPU", "▥", input.cpu_percent, stale, &style));
    }
    if input.show_ram {
        rings.push(render_ring("RAM", "▤", input.ram_percent, stale, &style));
    }
    if input.show_battery {
        if stale {
            rings.push(render_ring("Battery", "▰", None, true, &style));
        } else if let Some(battery) = input.battery {
            let percent = (battery.status == BatteryStatus::Available)
                .then_some(battery.percentage)
                .flatten()
                .map(f64::from);
            let label = match (battery.status, battery.charge) {
                (BatteryStatus::Unavailable, _) => "Battery unavailable".to_owned(),
                (_, BatteryChargeState::Charging) => "Battery charging".to_owned(),
                (_, BatteryChargeState::Critical) => "Battery critical".to_owned(),
                (_, BatteryChargeState::Full) => "Battery full".to_owned(),
                (_, BatteryChargeState::Discharging) => "Battery".to_owned(),
                (_, BatteryChargeState::Unknown) => "Battery".to_owned(),
            };
            rings.push(render_ring(&label, "▰", percent, false, &style));
        }
    }

    div()
        .id("ii.status-rings")
        .w_full()
        .flex()
        .flex_wrap()
        .gap(px(12.))
        .children(rings)
        .into_any_element()
}

fn render_ring(
    label: &str,
    glyph: &str,
    percent: Option<f64>,
    stale: bool,
    style: &SidebarStyle,
) -> AnyElement {
    let normalized = percent.map(|value| value.clamp(0., 100.));
    let value_color = normalized.map_or(style.muted, |value| {
        if value >= 90. {
            gpui::rgba(0xf85149ff).into()
        } else if value >= 70. {
            gpui::rgba(0xd29922ff).into()
        } else {
            style.accent
        }
    });
    let ring_color = value_color;
    let track_color = style.muted.opacity(0.32);
    let ring = gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = point(bounds.origin.x + px(26.), bounds.origin.y + px(26.));
            let radius = px(22.5);
            let circle_path = |sweep: f64| {
                let angle = sweep.to_radians();
                let end = point(
                    center.x + px(angle.sin() as f32 * 45.),
                    center.y - px(angle.cos() as f32 * 45.),
                );
                let mut path = PathBuilder::stroke(px(3.));
                path.move_to(point(center.x, center.y - radius));
                path.arc_to(point(radius, radius), px(0.), sweep > 180., true, end);
                path.build().ok()
            };
            if let Some(path) = circle_path(359.9) {
                window.paint_path(path, track_color);
            }
            if let Some(value) = normalized.filter(|value| *value > 0.) {
                if let Some(path) = circle_path(value * 3.6) {
                    window.paint_path(path, ring_color);
                }
            }
        },
    )
    .w(px(52.))
    .h(px(52.));
    let aria = if stale {
        format!("{label}: stale")
    } else {
        normalized.map_or_else(
            || format!("{label}: unknown"),
            |value| format!("{label}: {value:.0}%"),
        )
    };
    div()
        .id(format!(
            "ii.status-ring.{}",
            label.to_lowercase().replace(' ', "-")
        ))
        .w(px(64.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .role(Role::Group)
        .aria_label(aria)
        .child(
            div().relative().w(px(52.)).h(px(52.)).child(ring).child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_col()
                    .text_color(value_color)
                    .child(div().text_sm().child(glyph.to_owned()))
                    .child(
                        div().text_xs().font_weight(FontWeight::SEMIBOLD).child(
                            normalized.map_or_else(|| "—".to_owned(), |v| format!("{v:.0}%")),
                        ),
                    ),
            ),
        )
        .child(
            div()
                .text_xs()
                .text_color(style.muted)
                .child(label.to_owned()),
        )
        .when(stale, |ring| {
            ring.child(
                div()
                    .text_xs()
                    .text_color(style.muted)
                    .child("Stale sample"),
            )
        })
        .into_any_element()
}
