//! Horizontal ii bar media indicator.
//!
//! The caller supplies a media snapshot, semantic colors and action callbacks.
//! `OpenControls` is emitted through the supplied host callback. The parent
//! owns routing to the media surface; this component has no global state.

use std::rc::Rc;

use crate::components::vertical_bar::{MediaAction, MediaState, MediaStatus};
use gpui::{
    AnyElement, App, FocusHandle, Hsla, IntoElement, MouseButton, NavigationDirection, PathBuilder,
    Role, ScrollDelta, Window, canvas, div, point, prelude::*, px,
};

const GLYPH_SIZE: f32 = 22.0;
const RING_SIZE: f32 = 20.0;
const ROW_GAP: f32 = 4.0;

/// Theme colors are supplied by the bar family so this visual stays independent
/// of global theme state.
#[derive(Clone, Copy, Debug)]
pub struct HorizontalMediaColors {
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub track: Hsla,
    pub focus: Hsla,
}

/// Render the QML horizontal media summary.
///
/// `max_width_px` bounds the metadata label. `minimum_glyph_width_px` is the
/// minimum slot width requested by the caller. If `on_action` is present,
/// primary click and Enter/Space emit `MediaAction::OpenControls` for the host
/// to route to its media controls surface.
pub fn render_horizontal_media(
    media: Option<&MediaState>,
    max_width_px: f32,
    minimum_glyph_width_px: f32,
    show_verbose_label: bool,
    size_scale: f32,
    colors: HorizontalMediaColors,
    focus_handle: FocusHandle,
    on_action: Option<Rc<dyn Fn(MediaAction)>>,
    on_open_controls: Option<Rc<dyn Fn(&mut Window, &mut App, bool)>>,
) -> AnyElement {
    let size_scale = if size_scale.is_finite() {
        size_scale.clamp(0.75, 1.5)
    } else {
        1.0
    };
    // QML scales metadata width and type, while its media glyph and row gap
    // retain fixed dimensions so text scaling cannot shrink the hit target.
    let glyph_size = GLYPH_SIZE;
    let row_gap = ROW_GAP;
    let (status, title, artist) = match media {
        Some(media)
            if matches!(
                media.status,
                MediaStatus::Playing | MediaStatus::Paused | MediaStatus::Stopped
            ) =>
        {
            (
                media.status,
                media.title.as_deref().unwrap_or_default(),
                media.artist.as_deref().unwrap_or_default(),
            )
        }
        Some(media) => (media.status, "", ""),
        None => (MediaStatus::Unavailable, "", ""),
    };
    let title = clean_music_title(title);
    let title = if title.is_empty() {
        "No media".to_owned()
    } else {
        title
    };
    let artist = artist.trim();
    let has_track_metadata = media.is_some_and(|media| {
        media
            .title
            .as_deref()
            .is_some_and(|title| !title.is_empty())
            || media
                .artist
                .as_deref()
                .is_some_and(|artist| !artist.is_empty())
    });
    let display_text = if artist.is_empty() {
        title.clone()
    } else {
        format!("{title} • {artist}")
    };
    let (glyph, state_label) = media_status_presentation(status);
    let active_player = matches!(
        status,
        MediaStatus::Playing | MediaStatus::Paused | MediaStatus::Stopped
    );
    let progress = media
        .and_then(|media| media.position_percent)
        .map(|percent| percent.min(100));
    let max_width = if show_verbose_label {
        normalized_max_width(max_width_px, minimum_glyph_width_px)
    } else {
        normalized_minimum_width(minimum_glyph_width_px)
    };
    let min_width = normalized_minimum_width(minimum_glyph_width_px);
    let accessible_name = format!("Media {state_label}: {display_text}");
    let ring = progress_ring(progress, colors.track, colors.accent);

    let glyph_visual = div()
        .id("ii-horizontal-media-controls")
        .relative()
        .flex_none()
        .w(px(glyph_size))
        .h(px(glyph_size))
        .child(ring)
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .font_family("Material Symbols Rounded")
                .text_size(px(14.0 * size_scale))
                .text_color(colors.foreground)
                .child(glyph),
        );

    let glyph_button = if let Some(open_controls) = on_open_controls.clone() {
        let mut button = div()
            .id("ii-horizontal-media-controls-button")
            .relative()
            .flex_none()
            .w(px(glyph_size))
            .h(px(glyph_size))
            .role(Role::Button)
            .aria_label(format!("Open media controls for {display_text}"))
            .aria_description(media_actions_description(media, progress))
            .track_focus(&focus_handle)
            .focusable()
            .tab_stop(true)
            .focus_visible(move |style| style.border_1().border_color(colors.focus))
            .child(glyph_visual);
        let mouse_action = open_controls.clone();
        button = button.on_mouse_down(MouseButton::Left, move |_, window, cx| {
            mouse_action(window, cx, true);
            cx.stop_propagation();
        });
        let key_action = open_controls;
        button = button.on_key_down(move |event, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                key_action(window, cx, true);
                cx.stop_propagation();
            }
        });

        if let (Some(media), Some(action)) = (media, on_action.clone()) {
            if active_player && media.can_toggle_playback {
                let middle_action = action.clone();
                button = button.on_mouse_down(MouseButton::Middle, move |_, _, _| {
                    middle_action(MediaAction::TogglePlayback);
                });
            }
            if active_player && media.can_previous {
                let previous_action = action.clone();
                button = button.on_mouse_down(
                    MouseButton::Navigate(NavigationDirection::Back),
                    move |_, _, _| previous_action(MediaAction::Previous),
                );
            }
            if active_player && media.can_next {
                let next_action = action.clone();
                button = button.on_mouse_down(
                    MouseButton::Navigate(NavigationDirection::Forward),
                    move |_, _, _| next_action(MediaAction::Next),
                );
                button = button.on_mouse_down(MouseButton::Right, move |_, _, _| {
                    action(MediaAction::Next);
                });
            }
        }
        button.into_any_element()
    } else {
        glyph_visual.into_any_element()
    };

    let mut row = div()
        .id("ii-horizontal-media")
        .flex()
        .items_center()
        .gap(px(row_gap))
        .min_w(px(min_width))
        .max_w(px(max_width))
        .h_full()
        .overflow_hidden()
        .role(Role::Group)
        .aria_label(accessible_name)
        .child(glyph_button);

    if show_verbose_label && has_track_metadata {
        row = row.w(px(max_width));
    }

    if show_verbose_label {
        row = row.child(
            div()
                .id("ii-horizontal-media-label")
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .truncate()
                .text_size(px(12.0 * size_scale))
                .text_color(if active_player {
                    colors.foreground
                } else {
                    colors.muted
                })
                .child(display_text),
        );
    }

    if let Some(action) = on_action.filter(|_| {
        media.is_some_and(|media| {
            active_player
                && media.can_change_volume
                && media_action_allowed(media, MediaAction::VolumeDelta(0.05))
        })
    }) {
        row = row.on_scroll_wheel(move |event, _, _| {
            let y = match event.delta {
                ScrollDelta::Pixels(delta) => f32::from(delta.y),
                ScrollDelta::Lines(delta) => delta.y,
            };
            if y > 0.0 {
                action(MediaAction::VolumeDelta(0.05));
            } else if y < 0.0 {
                action(MediaAction::VolumeDelta(-0.05));
            }
        });
    }
    row.into_any_element()
}

fn progress_ring(progress: Option<u8>, track: Hsla, accent: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = point(bounds.origin.x + px(10.0), bounds.origin.y + px(10.0));
            let radius = px(8.0);
            let make_path = |sweep: f64| {
                let angle = sweep.to_radians();
                let end = point(
                    center.x + px(angle.sin() as f32 * 16.0),
                    center.y - px(angle.cos() as f32 * 16.0),
                );
                let mut path = PathBuilder::stroke(px(1.5));
                path.move_to(point(center.x, center.y - radius));
                path.arc_to(point(radius, radius), px(0.0), sweep > 180.0, true, end);
                path.build().ok()
            };
            if let Some(path) = make_path(359.9) {
                window.paint_path(path, track);
            }
            if let Some(progress) = progress.filter(|progress| *progress > 0) {
                if let Some(path) = make_path(f64::from(progress) * 3.6) {
                    window.paint_path(path, accent);
                }
            }
        },
    )
    .w(px(RING_SIZE))
    .h(px(RING_SIZE))
}

fn media_actions_description(media: Option<&MediaState>, progress: Option<u8>) -> String {
    let mut details = vec![format!(
        "Playback progress {}",
        progress
            .map(|value| format!("{value}%"))
            .unwrap_or_else(|| "unavailable".to_owned())
    )];
    if let Some(media) = media {
        if matches!(
            media.status,
            MediaStatus::Playing | MediaStatus::Paused | MediaStatus::Stopped
        ) {
            if media.can_toggle_playback {
                details.push("middle click to toggle playback".to_owned());
            }
            if media.can_previous {
                details.push("back button to play previous track".to_owned());
            }
            if media.can_next {
                details.push("forward or right button to play next track".to_owned());
            }
            if media.can_change_volume {
                details.push("scroll to adjust volume".to_owned());
            }
        }
    }
    details.join("; ")
}

fn media_status_presentation(status: MediaStatus) -> (&'static str, &'static str) {
    match status {
        MediaStatus::Unavailable => ("music_note", "unavailable"),
        MediaStatus::NoPlayer => ("music_note", "no player"),
        MediaStatus::Playing => ("pause", "playing"),
        MediaStatus::Paused => ("music_note", "paused"),
        MediaStatus::Stopped => ("music_note", "stopped"),
    }
}

/// Apply the same leading bracket stripping as `StringUtils.cleanMusicTitle`.
pub fn clean_music_title(title: &str) -> String {
    let mut cleaned = title.to_owned();
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        loop {
            let trimmed = cleaned.trim_start();
            let Some(rest) = trimmed.strip_prefix(open) else {
                break;
            };
            let Some(end) = rest.find(close) else {
                break;
            };
            cleaned = format!(" {}", &rest[end + close.len_utf8()..]);
        }
    }
    for (open, close) in [('【', '】'), ('《', '》'), ('「', '」'), ('『', '』')] {
        let trimmed = cleaned.trim_start();
        if let Some(rest) = trimmed.strip_prefix(open) {
            if let Some(end) = rest.find(close) {
                cleaned = rest[end + close.len_utf8()..].to_owned();
            }
        }
    }
    cleaned.trim().to_owned()
}

/// Normalize widths so the configured maximum never clips the interactive glyph.
pub fn normalized_max_width(max_width_px: f32, minimum_glyph_width_px: f32) -> f32 {
    let minimum = normalized_minimum_width(minimum_glyph_width_px);
    if max_width_px.is_finite() {
        max_width_px.max(minimum).max(0.0)
    } else {
        minimum
    }
}

pub fn normalized_minimum_width(minimum_glyph_width_px: f32) -> f32 {
    if minimum_glyph_width_px.is_finite() {
        minimum_glyph_width_px.max(0.0)
    } else {
        GLYPH_SIZE
    }
}

/// Whether a requested action is valid for this media snapshot.
pub fn media_action_allowed(media: &MediaState, action: MediaAction) -> bool {
    let active = matches!(
        media.status,
        MediaStatus::Playing | MediaStatus::Paused | MediaStatus::Stopped
    );
    match action {
        MediaAction::OpenControls
        | MediaAction::CloseControls
        | MediaAction::ForPlayer { .. }
        | MediaAction::SeekTo { .. } => true,
        MediaAction::TogglePlayback => active && media.can_toggle_playback,
        MediaAction::Previous => active && media.can_previous,
        MediaAction::Next => active && media.can_next,
        MediaAction::VolumeDelta(delta) => {
            active && media.can_change_volume && delta.is_finite() && delta != 0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(status: MediaStatus) -> MediaState {
        MediaState {
            status,
            title: None,
            artist: None,
            position_percent: None,
            can_toggle_playback: true,
            can_previous: true,
            can_next: true,
            can_change_volume: true,
            players: Vec::new(),
        }
    }

    #[test]
    fn cleans_qml_music_title_prefixes() {
        assert_eq!(clean_music_title(" (Official) [HD] Track "), "Track");
        assert_eq!(clean_music_title("【Touhou】Track"), "Track");
        assert_eq!(clean_music_title("『Opening』 Track"), "Track");
        assert_eq!(clean_music_title("  {Live}  Track"), "Track");
        assert_eq!(clean_music_title("(Live) (Official) [HD] Track"), "Track");
        assert_eq!(clean_music_title("  Track  "), "Track");
    }

    #[test]
    fn width_limits_are_finite_and_keep_glyph_visible() {
        assert_eq!(normalized_minimum_width(16.0), 16.0);
        assert_eq!(normalized_minimum_width(f32::NAN), 22.0);
        assert_eq!(normalized_max_width(220.0, 26.0), 220.0);
        assert_eq!(normalized_max_width(12.0, 26.0), 26.0);
        assert_eq!(normalized_max_width(f32::INFINITY, 26.0), 26.0);
    }

    #[test]
    fn status_glyphs_and_labels_match_qml_playback_states() {
        assert_eq!(
            media_status_presentation(MediaStatus::Unavailable),
            ("music_note", "unavailable")
        );
        assert_eq!(
            media_status_presentation(MediaStatus::NoPlayer),
            ("music_note", "no player")
        );
        assert_eq!(
            media_status_presentation(MediaStatus::Playing),
            ("pause", "playing")
        );
        assert_eq!(
            media_status_presentation(MediaStatus::Paused),
            ("music_note", "paused")
        );
        assert_eq!(
            media_status_presentation(MediaStatus::Stopped),
            ("music_note", "stopped")
        );
    }

    #[test]
    fn action_guards_follow_media_capabilities() {
        let no_player = media(MediaStatus::NoPlayer);
        assert!(!media_action_allowed(
            &no_player,
            MediaAction::TogglePlayback
        ));
        assert!(!media_action_allowed(
            &no_player,
            MediaAction::VolumeDelta(0.05)
        ));
        assert!(media_action_allowed(&no_player, MediaAction::OpenControls));

        let mut playing = media(MediaStatus::Playing);
        playing.can_previous = false;
        assert!(media_action_allowed(&playing, MediaAction::TogglePlayback));
        assert!(!media_action_allowed(&playing, MediaAction::Previous));
        assert!(media_action_allowed(&playing, MediaAction::Next));
        assert!(media_action_allowed(
            &playing,
            MediaAction::VolumeDelta(0.05)
        ));
        assert!(media_action_allowed(
            &playing,
            MediaAction::VolumeDelta(-0.05)
        ));
        assert!(!media_action_allowed(
            &playing,
            MediaAction::VolumeDelta(0.0)
        ));

        playing.can_change_volume = false;
        assert!(!media_action_allowed(
            &playing,
            MediaAction::VolumeDelta(0.05)
        ));
    }
}
