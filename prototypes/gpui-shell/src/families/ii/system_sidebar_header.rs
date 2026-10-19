//! Data-driven system sidebar identity header.
//!
//! Mirrors the QML profile/classic header choice while keeping host facts out
//! of the renderer. Wallpaper banners, avatars, and account actions require
//! QML services that the GPUI host does not expose yet.

use gpui::{AnyElement, IntoElement, div, prelude::*, px};

use super::sidebars::{SidebarHeaderStyle, SidebarStyle};

#[derive(Debug, Clone)]
pub struct SystemSidebarHeaderInput {
    pub style: SidebarHeaderStyle,
    pub username: Option<String>,
    pub distro: Option<String>,
    pub uptime: Option<String>,
}

/// Builds a header from real host values. Missing values are presented as
/// unavailable instead of borrowing display data from another source.
pub fn render(input: SystemSidebarHeaderInput, style: &SidebarStyle) -> AnyElement {
    let username = input.username.unwrap_or_else(|| "User unavailable".into());
    let distro = input
        .distro
        .filter(|distro| !distro.eq_ignore_ascii_case("unknown"));
    let identity = distro
        .map(|distro| format!("{username}@{distro}"))
        .unwrap_or_else(|| username.clone());
    let uptime = input.uptime.unwrap_or_else(|| "Uptime unavailable".into());

    match input.style {
        SidebarHeaderStyle::Profile => div()
            .id("ii.sidebar.system.profile_header")
            .w_full()
            .flex()
            .items_center()
            .gap(px(style.gap))
            .p(px(style.padding))
            .bg(style.card)
            .rounded(px(style.card_radius))
            .text_color(style.foreground)
            .child(
                div()
                    .size(px(44.))
                    .rounded(px(22.))
                    .bg(style.selected)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(style.accent)
                    .child(initials(&username)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(username),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(style.muted)
                            .child(format!("{identity} · {uptime}")),
                    ),
            )
            .into_any_element(),
        SidebarHeaderStyle::Classic => div()
            .id("ii.sidebar.system.classic_header")
            .w_full()
            .flex()
            .items_center()
            .gap(px(style.gap))
            .p(px(style.padding))
            .bg(style.card)
            .rounded(px(style.card_radius))
            .text_color(style.foreground)
            .child(div().text_sm().text_color(style.muted).child(uptime))
            .into_any_element(),
    }
}

fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// Runtime adapter for OS facts available without the QML SystemInfo service.
pub fn host_input(style: SidebarHeaderStyle) -> SystemSidebarHeaderInput {
    let username = std::env::var("USER")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let distro = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("ID=")
                    .map(|value| value.trim_matches('"').to_owned())
            })
        })
        .filter(|value| !value.trim().is_empty());
    let uptime = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|contents| contents.split_whitespace().next()?.parse::<u64>().ok())
        .map(format_uptime);
    SystemSidebarHeaderInput {
        style,
        username,
        distro,
        uptime,
    }
}

fn format_uptime(total_seconds: u64) -> String {
    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    if days > 0 {
        format!("Up {days}d {hours}h")
    } else if hours > 0 {
        format!("Up {hours}h {minutes}m")
    } else {
        format!("Up {minutes}m")
    }
}
