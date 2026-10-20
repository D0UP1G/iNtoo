//! Horizontal ii bar battery indicator.
//!
//! This is a presentation-only component. The caller supplies an explicit
//! battery snapshot and Material/semantic colors; no unavailable readings are
//! converted into a numeric value.

use crate::components::vertical_bar::{BatteryChargeState, BatteryState, BatteryStatus};
use gpui::{AnyElement, Context, Hsla, IntoElement, Render, Role, Window, div, prelude::*, px};

const BATTERY_WIDTH: f32 = 30.0;
const BATTERY_HEIGHT: f32 = 18.0;

/// Material colors for the clipped horizontal meter.
#[derive(Clone, Copy, Debug)]
pub struct HorizontalBatteryColors {
    /// Surface behind the meter, used to calculate text contrast on its
    /// translucent track.
    pub backdrop: Hsla,
    /// Normal fill, preferably Material `on_background` / `on_surface`.
    pub fill: Hsla,
    /// Low, non-charging fill, preferably Material semantic `error`.
    pub low_fill: Hsla,
    /// Text color over the track.
    pub text: Hsla,
    /// Text color over the normal fill.
    pub fill_text: Hsla,
    /// Text color over the semantic low fill, preferably `on_error`.
    pub low_fill_text: Hsla,
}

/// Render a horizontal QML `ClippedProgressBar` equivalent.
///
/// Missing devices and unavailable providers are omitted, matching QML's
/// `Battery.available` visibility gate. A connected battery with an unknown
/// percentage renders an em dash and exposes no numeric current value to
/// assistive technology.
pub fn render_horizontal_battery(
    battery: &BatteryState,
    colors: HorizontalBatteryColors,
) -> Option<AnyElement> {
    if battery.status != BatteryStatus::Available {
        return None;
    }

    let percent = battery.percentage.map(|value| value.min(100));
    let charging = battery.charge == BatteryChargeState::Charging;
    let low_threshold = battery
        .low_threshold
        .is_finite()
        .then(|| battery.low_threshold.clamp(0.0, 100.0));
    let low = !charging
        && percent.is_some_and(|value| {
            low_threshold.is_some_and(|threshold| f64::from(value) <= threshold)
        });
    let fill_color = if low { colors.low_fill } else { colors.fill };
    // QML derives the track from the current highlight, so low battery changes
    // both the filled segment and the translucent remainder.
    let track_color = fill_color.opacity(0.5);
    let preferred_fill_text = if low {
        colors.low_fill_text
    } else {
        colors.fill_text
    };
    let track_text = contrast_text(colors.text, track_color, colors.backdrop);
    let fill_text = contrast_text(preferred_fill_text, fill_color, colors.backdrop);
    let state_label = battery_state_label(battery.charge);
    let accessible_percent = percent
        .map(|value| format!("{value} percent"))
        .unwrap_or_else(|| "unknown".to_owned());
    let visible_percent = percent
        .map(|value| value.to_string())
        .unwrap_or_else(|| "—".to_owned());
    let show_bolt = charging && percent.is_some_and(|value| value < 100);
    let fill_width = percent.map(|value| BATTERY_WIDTH * f32::from(value) / 100.0);

    let time_seconds = match battery.charge {
        BatteryChargeState::Charging => battery.time_to_full_seconds,
        BatteryChargeState::Discharging | BatteryChargeState::Critical => {
            battery.time_to_empty_seconds
        }
        BatteryChargeState::Full | BatteryChargeState::Unknown => None,
    };
    let popup = BatteryPopup {
        charge: battery.charge,
        time_seconds,
        rate_watts: battery.energy_rate_watts,
        colors,
    };

    let mut meter = div()
        .id("ii-horizontal-battery-level")
        .relative()
        .w(px(BATTERY_WIDTH))
        .h(px(BATTERY_HEIGHT))
        .role(Role::ProgressIndicator)
        .aria_orientation(gpui::Orientation::Horizontal)
        .aria_label(format!("Battery level: {accessible_percent}"))
        .aria_min_numeric_value(0.0)
        .aria_max_numeric_value(100.0)
        .when_some(percent, |this, value| {
            this.aria_numeric_value(f64::from(value))
        })
        .rounded_full()
        .overflow_hidden()
        .bg(track_color);

    if let Some(fill_width) = fill_width {
        meter = meter.child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(fill_width))
                .bg(fill_color),
        );
    }

    meter = meter.child(
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .child(battery_text(&visible_percent, show_bolt, track_text)),
    );

    if let Some(fill_width) = fill_width.filter(|width| *width > 0.0) {
        meter = meter.child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(fill_width))
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(BATTERY_WIDTH))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(battery_text(&visible_percent, show_bolt, fill_text)),
                ),
        );
    }

    let indicator = div()
        .id("ii-horizontal-battery")
        .w(px(BATTERY_WIDTH))
        .h(px(BATTERY_HEIGHT))
        .role(Role::Group)
        .tab_index(0)
        .focus_visible(move |style| style.border_1().border_color(colors.fill))
        .aria_label(format!("Battery, {state_label}"))
        .aria_description(popup_accessible_description(battery))
        .child(meter);
    let indicator = indicator.tooltip(move |_, cx| cx.new(|_| popup).into());
    Some(indicator.into_any_element())
}

#[derive(Clone, Copy, Debug)]
struct BatteryPopup {
    charge: BatteryChargeState,
    time_seconds: Option<u64>,
    rate_watts: Option<f64>,
    colors: HorizontalBatteryColors,
}

impl Render for BatteryPopup {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let full = self.charge == BatteryChargeState::Full;
        let rate = valid_rate(self.rate_watts);
        let time_label = if self.charge == BatteryChargeState::Charging {
            "Time to full"
        } else {
            "Time to empty"
        };
        let status_label = match self.charge {
            BatteryChargeState::Full => "Fully charged",
            BatteryChargeState::Charging => "Charging",
            BatteryChargeState::Discharging | BatteryChargeState::Critical => "Discharging",
            BatteryChargeState::Unknown => "Charge state unknown",
        };

        let mut popup = div()
            .id("ii-horizontal-battery-tooltip")
            .w(px(232.0))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(self.colors.text.opacity(0.16))
            .bg(self.colors.backdrop)
            .text_color(self.colors.text)
            .role(Role::Tooltip);

        let header = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .font_family("Material Symbols Rounded")
                    .text_size(px(20.0))
                    .text_color(self.colors.fill)
                    .child("battery_android_full"),
            )
            .child(div().text_size(px(14.0)).child("Battery"));
        popup = popup.child(header);

        if let Some(seconds) = self
            .time_seconds
            .filter(|seconds| show_time_row(self.charge, Some(*seconds), rate))
        {
            popup = popup.child(popup_row(
                "schedule",
                time_label,
                Some(format_time(seconds)),
                self.colors,
            ));
        }

        let show_status = show_status_row(self.charge, rate);
        if show_status {
            let watts = (!full)
                .then(|| rate.map(|rate| format!("{rate:.2} W")))
                .flatten();
            popup = popup.child(popup_row("bolt", status_label, watts, self.colors));
        }

        popup
    }
}

fn popup_row(
    icon: &'static str,
    label: &'static str,
    value: Option<String>,
    colors: HorizontalBatteryColors,
) -> impl IntoElement {
    let mut row = div()
        .flex()
        .items_center()
        .gap(px(7.0))
        .child(
            div()
                .font_family("Material Symbols Rounded")
                .text_size(px(18.0))
                .text_color(colors.text)
                .child(icon),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(colors.text)
                .child(label),
        );
    if let Some(value) = value {
        row = row.child(
            div()
                .ml_auto()
                .text_size(px(12.0))
                .text_color(colors.text)
                .child(value),
        );
    }
    row
}

fn format_time(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours}h, {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn valid_rate(rate: Option<f64>) -> Option<f64> {
    rate.filter(|rate| rate.is_finite() && *rate >= 0.0)
}

fn show_time_row(
    charge: BatteryChargeState,
    seconds: Option<u64>,
    rate_watts: Option<f64>,
) -> bool {
    charge != BatteryChargeState::Full
        && seconds.is_some_and(|seconds| seconds > 0)
        && valid_rate(rate_watts).is_some_and(|rate| rate > 0.01)
}

fn show_status_row(charge: BatteryChargeState, rate_watts: Option<f64>) -> bool {
    charge == BatteryChargeState::Full || valid_rate(rate_watts).is_some_and(|rate| rate > 0.0)
}

fn popup_accessible_description(battery: &BatteryState) -> String {
    let full = battery.charge == BatteryChargeState::Full;
    let rate = valid_rate(battery.energy_rate_watts);
    let mut parts = Vec::new();
    let time_seconds = match battery.charge {
        BatteryChargeState::Charging => battery.time_to_full_seconds,
        BatteryChargeState::Discharging | BatteryChargeState::Critical => {
            battery.time_to_empty_seconds
        }
        BatteryChargeState::Full | BatteryChargeState::Unknown => None,
    };
    if show_time_row(battery.charge, time_seconds, rate) {
        if let Some(seconds) = time_seconds {
            let label = if battery.charge == BatteryChargeState::Charging {
                "Time to full"
            } else {
                "Time to empty"
            };
            parts.push(format!("{label}: {}", format_time(seconds)));
        }
    }
    if !full && show_status_row(battery.charge, rate) {
        if let Some(rate) = rate {
            parts.push(format!("{rate:.2} watts"));
        }
    }
    parts.join(", ")
}

fn battery_state_label(state: BatteryChargeState) -> &'static str {
    match state {
        BatteryChargeState::Charging => "charging",
        BatteryChargeState::Discharging => "discharging",
        BatteryChargeState::Critical => "critical",
        BatteryChargeState::Full => "full",
        BatteryChargeState::Unknown => "charge state unknown",
    }
}

fn battery_text(percent: &str, show_bolt: bool, color: Hsla) -> AnyElement {
    // Keep these visual text elements unlabelled; the surrounding Group and
    // ProgressIndicator provide the accessible state and value exactly once.
    let mut content = div()
        .flex()
        .items_center()
        .justify_center()
        .gap_0()
        .text_color(color);

    if show_bolt {
        content = content.child(
            div()
                .font_family("Material Symbols Rounded")
                .text_size(px(14.0))
                .child("bolt"),
        );
    }

    content
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(percent.to_owned()),
        )
        .into_any_element()
}

/// Keep the supplied Material foreground when it has sufficient contrast;
/// otherwise use whichever of black or white is clearer on the composited
/// color. The track alpha is composed over the caller's current surface.
fn contrast_text(preferred: Hsla, color: Hsla, backdrop: Hsla) -> Hsla {
    let visible_background = composite_rgb(
        hsla_to_rgb(color),
        color.a,
        hsla_to_rgb(backdrop),
        backdrop.a,
    );
    if foreground_contrast_ratio(preferred, visible_background) >= 4.5 {
        preferred
    } else {
        strongest_contrast_rgb(visible_background)
    }
}

fn foreground_contrast_ratio(foreground: Hsla, background: [f32; 3]) -> f32 {
    let visible_foreground = composite_rgb(hsla_to_rgb(foreground), foreground.a, background, 1.0);
    contrast_ratio_rgb(visible_foreground, background)
}

fn strongest_contrast_rgb(background: [f32; 3]) -> Hsla {
    let black = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0,
        a: 1.0,
    };
    let white = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 1.0,
    };
    if contrast_ratio_rgb(hsla_to_rgb(black), background)
        >= contrast_ratio_rgb(hsla_to_rgb(white), background)
    {
        black
    } else {
        white
    }
}

fn contrast_ratio_rgb(foreground: [f32; 3], background: [f32; 3]) -> f32 {
    let foreground = relative_luminance(foreground);
    let background = relative_luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

fn composite_rgb(
    foreground: [f32; 3],
    foreground_alpha: f32,
    background: [f32; 3],
    background_alpha: f32,
) -> [f32; 3] {
    let foreground_alpha = foreground_alpha.clamp(0.0, 1.0);
    let background_alpha = background_alpha.clamp(0.0, 1.0);
    let alpha = foreground_alpha + background_alpha * (1.0 - foreground_alpha);
    if alpha == 0.0 {
        return [0.0; 3];
    }
    std::array::from_fn(|index| {
        (foreground[index] * foreground_alpha
            + background[index] * background_alpha * (1.0 - foreground_alpha))
            / alpha
    })
}

fn relative_luminance(rgb: [f32; 3]) -> f32 {
    let linear = rgb.map(|channel| {
        let channel = channel.clamp(0.0, 1.0);
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
}

fn hsla_to_rgb(color: Hsla) -> [f32; 3] {
    if color.s == 0.0 {
        return [color.l; 3];
    }
    let q = if color.l < 0.5 {
        color.l * (1.0 + color.s)
    } else {
        color.l + color.s - color.l * color.s
    };
    let p = 2.0 * color.l - q;
    [
        hue_to_rgb(p, q, color.h + 1.0 / 3.0),
        hue_to_rgb(p, q, color.h),
        hue_to_rgb(p, q, color.h - 1.0 / 3.0),
    ]
}

fn hue_to_rgb(p: f32, q: f32, mut hue: f32) -> f32 {
    if hue < 0.0 {
        hue += 1.0;
    }
    if hue > 1.0 {
        hue -= 1.0;
    }
    if hue < 1.0 / 6.0 {
        p + (q - p) * 6.0 * hue
    } else if hue < 0.5 {
        q
    } else if hue < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - hue) * 6.0
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_popup_time_like_qml() {
        assert_eq!(format_time(3 * 3600 + 17 * 60 + 59), "3h, 17m");
        assert_eq!(format_time(59), "0m");
        assert_eq!(format_time(60), "1m");
    }

    #[test]
    fn time_row_requires_non_full_state_valid_time_and_meaningful_rate() {
        assert!(show_time_row(
            BatteryChargeState::Charging,
            Some(3600),
            Some(0.02)
        ));
        assert!(!show_time_row(
            BatteryChargeState::Full,
            Some(3600),
            Some(10.0)
        ));
        assert!(!show_time_row(
            BatteryChargeState::Discharging,
            Some(0),
            Some(10.0)
        ));
        assert!(!show_time_row(
            BatteryChargeState::Charging,
            Some(3600),
            Some(0.01)
        ));
        assert!(!show_time_row(
            BatteryChargeState::Charging,
            Some(3600),
            Some(f64::NAN)
        ));
    }

    #[test]
    fn status_row_shows_full_or_positive_rate_only() {
        assert!(show_status_row(BatteryChargeState::Full, None));
        assert!(show_status_row(BatteryChargeState::Discharging, Some(0.01)));
        assert!(!show_status_row(BatteryChargeState::Charging, Some(0.0)));
        assert!(!show_status_row(BatteryChargeState::Charging, None));
        assert!(!show_status_row(
            BatteryChargeState::Charging,
            Some(f64::INFINITY)
        ));
    }

    #[test]
    fn accessible_description_includes_popup_details() {
        let battery = BatteryState {
            status: BatteryStatus::Available,
            charge: BatteryChargeState::Charging,
            percentage: Some(42),
            low_threshold: 15.0,
            time_to_full_seconds: Some(3660),
            time_to_empty_seconds: None,
            energy_rate_watts: Some(12.5),
        };

        assert_eq!(
            popup_accessible_description(&battery),
            "Time to full: 1h, 1m, 12.50 watts"
        );
    }

    #[test]
    fn unknown_charge_state_is_not_announced_as_discharging() {
        assert_eq!(
            battery_state_label(BatteryChargeState::Unknown),
            "charge state unknown"
        );
        let battery = BatteryState {
            status: BatteryStatus::Available,
            charge: BatteryChargeState::Unknown,
            percentage: Some(42),
            low_threshold: 15.0,
            time_to_full_seconds: None,
            time_to_empty_seconds: None,
            energy_rate_watts: Some(3.5),
        };
        assert_eq!(popup_accessible_description(&battery), "3.50 watts");
    }
}
