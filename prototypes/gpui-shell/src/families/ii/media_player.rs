//! Compact, data-driven ii media card.
//!
//! The caller supplies the player snapshot and action callback. This component
//! does not discover players, select a player, or create artwork.

use std::sync::Arc;

use gpui::{
    AnyElement, FontWeight, IntoElement, KeyDownEvent, Role, SharedString, div, prelude::*, px,
    relative,
};

use crate::components::vertical_bar::{MediaAction, MediaState, MediaStatus};

use super::sidebars::SidebarStyle;

pub type MediaActionHandler = Arc<dyn Fn(MediaAction) + Send + Sync>;

#[derive(Clone)]
pub struct MediaPlayerInput {
    /// `None` and `Some(NoPlayer)` hide the section, matching the QML sidebar.
    pub media: Option<MediaState>,
    pub style: SidebarStyle,
    /// Optional callback. Controls are omitted when no action adapter exists.
    pub on_action: Option<MediaActionHandler>,
}

impl std::fmt::Debug for MediaPlayerInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MediaPlayerInput")
            .field("media", &self.media)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

/// Render a compact player card from an already-resolved snapshot.
pub fn render_media_player(input: MediaPlayerInput) -> AnyElement {
    let Some(media) = input.media else {
        return div().id("ii.media-player.hidden").into_any_element();
    };
    if media.status == MediaStatus::NoPlayer {
        return div().id("ii.media-player.hidden").into_any_element();
    }
    let style = input.style;
    let (heading, active) = match media.status {
        MediaStatus::Unavailable => ("Media unavailable", false),
        MediaStatus::NoPlayer => unreachable!("NoPlayer is collapsed above"),
        MediaStatus::Playing => ("Playing", true),
        MediaStatus::Paused => ("Paused", true),
        MediaStatus::Stopped => ("Stopped", true),
    };
    let mut card = div()
        .id("ii.media-player.card")
        .w_full()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(style.padding))
        .rounded(px(style.card_radius))
        .bg(style.card)
        .text_color(style.foreground)
        .role(Role::Group)
        .aria_label(SharedString::from(format!("Media player: {heading}")))
        .child(
            div()
                .w_full()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(style.muted)
                .child(heading),
        );

    if active {
        if let Some(title) = media.title.as_deref().filter(|s| !s.trim().is_empty()) {
            card = card.child(
                div()
                    .w_full()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(title.to_owned()),
            );
        }
        if let Some(artist) = media.artist.as_deref().filter(|s| !s.trim().is_empty()) {
            card = card.child(
                div()
                    .w_full()
                    .text_xs()
                    .text_color(style.muted)
                    .child(artist.to_owned()),
            );
        }
        if let Some(percent) = media.position_percent {
            let percent = percent.min(100);
            card = card.child(
                div()
                    .id("ii.media-player.progress")
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(style.gap))
                    .role(Role::ProgressIndicator)
                    .aria_label(SharedString::from(format!("Playback position: {percent}%")))
                    .aria_min_numeric_value(0.0)
                    .aria_max_numeric_value(100.0)
                    .aria_numeric_value(f64::from(percent))
                    .child(
                        div()
                            .h(px(4.))
                            .flex_1()
                            .rounded(px(2.))
                            .bg(style.muted.opacity(0.35))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(f32::from(percent) / 100.))
                                    .rounded(px(2.))
                                    .bg(style.accent),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(style.muted)
                            .child(format!("{percent}%")),
                    ),
            );
        }
        if let Some(action) = input.on_action {
            let mut controls = div()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(style.gap));
            if media.can_previous {
                controls = controls.child(control(
                    "previous",
                    "Previous track",
                    "⏮",
                    style.clone(),
                    action.clone(),
                    MediaAction::Previous,
                ));
            }
            if media.can_toggle_playback {
                let (label, glyph) = if media.status == MediaStatus::Playing {
                    ("Pause", "⏸")
                } else {
                    ("Play", "▶")
                };
                controls = controls.child(control(
                    "play-pause",
                    label,
                    glyph,
                    style.clone(),
                    action.clone(),
                    MediaAction::TogglePlayback,
                ));
            }
            if media.can_next {
                controls = controls.child(control(
                    "next",
                    "Next track",
                    "⏭",
                    style,
                    action,
                    MediaAction::Next,
                ));
            }
            card = card.child(controls);
        }
    }
    card.into_any_element()
}

fn control(
    id: &'static str,
    label: &'static str,
    glyph: &'static str,
    style: SidebarStyle,
    action: MediaActionHandler,
    command: MediaAction,
) -> impl IntoElement {
    let key_action = action.clone();
    let click_action = action;
    div()
        .id(SharedString::from(format!("ii.media-player.{id}")))
        .min_w(px(36.))
        .h(px(32.))
        .px(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(style.card_radius))
        .bg(style.surface)
        .text_color(style.foreground)
        .role(Role::Button)
        .aria_label(SharedString::from(label))
        .focusable()
        .tab_stop(true)
        .focus_visible({
            let border = style.border;
            move |s| s.border_1().border_color(border)
        })
        .child(glyph)
        .on_click(move |_, _, _| click_action(command))
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if activate(event) {
                key_action(command);
                cx.stop_propagation();
            }
        })
}

fn activate(event: &KeyDownEvent) -> bool {
    !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space")
}
