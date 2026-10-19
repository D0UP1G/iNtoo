//! Read-only current-weather card for the ii Widgets sidebar.
//!
//! Weather values and provider state come from `context_weather_state`; this
//! renderer never invents fallback readings or starts service actions.

use gpui::{AnyElement, FontWeight, Role, SharedString, div, prelude::*, px};

use super::{
    context_weather_state::{ContextWeatherState, ContextWeatherStatus, WeatherSnapshot},
    sidebars::SidebarStyle,
};

#[derive(Clone, Debug)]
pub struct ContextWeatherInput {
    pub state: ContextWeatherState,
    pub style: SidebarStyle,
}

pub fn render_context_weather(input: ContextWeatherInput) -> AnyElement {
    let style = input.style;
    let state = input.state;
    let snapshot = state.snapshot.clone();
    let status = if state.stale && snapshot.is_some() {
        let age = state
            .updated_at_ms
            .map(weather_age_label)
            .unwrap_or_else(|| "last successful update time unavailable".to_owned());
        let message = match &state.status {
            ContextWeatherStatus::Loading => {
                format!("Refreshing weather · showing cached data, {age}")
            }
            ContextWeatherStatus::Unavailable(reason) => {
                format!("Showing stale weather, {age} · refresh failed: {reason}")
            }
            ContextWeatherStatus::Ready => format!("Showing cached weather, {age}"),
        };
        Some((
            matches!(&state.status, ContextWeatherStatus::Loading),
            message,
        ))
    } else {
        match state.status {
            ContextWeatherStatus::Ready if snapshot.is_some() => None,
            ContextWeatherStatus::Ready => Some((false, "Weather data is unavailable".to_owned())),
            ContextWeatherStatus::Loading => {
                (snapshot.is_none()).then(|| (true, "Loading weather…".to_owned()))
            }
            ContextWeatherStatus::Unavailable(message) => Some((false, message)),
        }
    };
    let body = snapshot.map(|snapshot| weather_body(snapshot, &style));

    div()
        .id("ii.context-weather.card")
        .role(Role::Group)
        .aria_label("Current weather")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(8.))
        .text_color(style.foreground)
        .when_some(body, |this, body| this.child(body))
        .when_some(status, |this, (loading, message)| {
            this.child(
                div()
                    .id(if loading {
                        "ii.context-weather.loading"
                    } else {
                        "ii.context-weather.unavailable"
                    })
                    .role(Role::Status)
                    .aria_label(SharedString::from(message.clone()))
                    .text_sm()
                    .text_color(style.muted)
                    .child(message),
            )
        })
        .into_any_element()
}

fn weather_age_label(updated_at_ms: u64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(updated_at_ms);
    let age_seconds = now_ms.saturating_sub(updated_at_ms) / 1000;
    if age_seconds < 60 {
        "updated less than a minute ago".to_owned()
    } else if age_seconds < 3600 {
        format!("updated {} min ago", age_seconds / 60)
    } else if age_seconds < 86400 {
        format!("updated {} hr ago", age_seconds / 3600)
    } else {
        format!("updated {} days ago", age_seconds / 86400)
    }
}

fn weather_body(snapshot: WeatherSnapshot, style: &SidebarStyle) -> impl IntoElement {
    let icon = weather_icon(snapshot.weather_code, snapshot.is_night);
    let temperature = snapshot.temperature.clone();
    let condition = snapshot.condition.clone();
    let city = snapshot.city.clone();
    let humidity = snapshot.humidity.clone();
    let wind = snapshot.wind.clone();
    let feels_like = snapshot.feels_like.clone();
    let feels_summary = if feels_like.is_empty() {
        String::new()
    } else {
        format!(", feels like {feels_like}")
    };
    let accessible_summary = format!(
        "Current weather{}: {}, {}, humidity {}, wind {}{}",
        city.as_deref()
            .map(|city| format!(" in {city}"))
            .unwrap_or_default(),
        condition,
        temperature,
        humidity,
        wind,
        feels_summary,
    );
    div()
        .id("ii.context-weather.content")
        .role(Role::Group)
        .aria_label(SharedString::from(accessible_summary))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .font_family("Material Symbols Rounded")
                        .text_size(px(36.))
                        .text_color(style.accent)
                        .child(icon),
                )
                .child(
                    div()
                        .text_color(style.foreground)
                        .text_size(px(29.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(temperature),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .child(
                            div()
                                .min_w_0()
                                .text_sm()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(style.foreground)
                                .child(condition),
                        )
                        .when_some(city, |this, city| {
                            this.child(
                                div()
                                    .min_w_0()
                                    .text_xs()
                                    .text_color(style.muted)
                                    .child(city),
                            )
                        }),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(16.))
                .child(weather_stat("humidity_percentage", humidity, style))
                .child(weather_stat("air", wind, style))
                .when(!feels_like.is_empty(), |this| {
                    this.child(weather_stat("thermostat", feels_like, style))
                }),
        )
}

fn weather_stat(icon: &'static str, value: String, style: &SidebarStyle) -> impl IntoElement {
    let label = match icon {
        "humidity_percentage" => "Humidity",
        "air" => "Wind",
        "thermostat" => "Feels like",
        _ => "Weather detail",
    };
    div()
        .id(SharedString::from(format!(
            "ii.context-weather.stat.{icon}"
        )))
        .role(Role::Group)
        .aria_label(SharedString::from(format!("{label}: {value}")))
        .flex()
        .items_center()
        .gap(px(4.))
        .child(
            div()
                .font_family("Material Symbols Rounded")
                .text_size(px(14.))
                .text_color(style.muted)
                .child(icon),
        )
        .child(div().text_sm().text_color(style.foreground).child(value))
}

fn weather_icon(code: u8, is_night: bool) -> &'static str {
    let icon = match code {
        0 => "clear_day",
        1 | 2 => "partly_cloudy_day",
        3 => "cloud",
        45 | 48 => "foggy",
        51 | 53 | 55 | 56 | 57 | 61 | 63 | 66 | 67 | 80 | 81 => "rainy",
        65 | 82 => "weather_hail",
        71 | 77 | 85 => "cloudy_snowing",
        73 | 75 | 86 => "weather_snowy",
        95 | 96 | 99 => "thunderstorm",
        // QML maps unknown WMO values to wttr code 113, which is clear_day.
        _ => "clear_day",
    };
    if is_night {
        match icon {
            "clear_day" => "bedtime",
            "partly_cloudy_day" => "partly_cloudy_night",
            _ => icon,
        }
    } else {
        icon
    }
}
