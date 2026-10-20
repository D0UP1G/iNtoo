//! Material ii on-screen display, rendered from caller-owned snapshots.
//!
//! QML reference: `modules/onScreenDisplay/OnScreenDisplay.qml` and the
//! indicator components in `modules/onScreenDisplay/indicators/`. Provider
//! discovery, monitor selection, visibility timers, and state ownership belong
//! to the host. Missing data is represented as unavailable/unknown, never 0.

use std::{path::PathBuf, sync::Arc};

use gpui::{
    AnyElement, FontWeight, KeyDownEvent, ObjectFit, Role, SharedString, div, img, prelude::*, px,
};

use crate::components::vertical_bar::MediaAction;

use super::sidebars::SidebarStyle;

#[derive(Clone, Debug, PartialEq)]
pub enum OsdAvailability {
    Ready,
    Loading,
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum OsdIndicator {
    Volume {
        value: Option<f32>,
        muted: Option<bool>,
        protection_message: Option<String>,
    },
    Brightness {
        value: Option<f32>,
        night_filter_active: Option<bool>,
    },
    Microphone {
        value: Option<f32>,
        muted: Option<bool>,
    },
    Media {
        title: Option<String>,
        artist: Option<String>,
        art_url: Option<String>,
        playing: Option<bool>,
        can_previous: bool,
        can_toggle: bool,
        can_next: bool,
    },
    VoiceSearch {
        recording: bool,
        transcribing: bool,
    },
    KeyboardLayout {
        text: Option<String>,
        icon: Option<String>,
        active: Option<bool>,
    },
    Connection {
        title: Option<String>,
        detail: Option<String>,
        icon: Option<String>,
        value_percent: Option<u8>,
        tone: ConnectionTone,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionTone {
    #[default]
    On,
    Off,
    Warn,
}

#[derive(Clone)]
pub struct OsdInput {
    pub indicator: OsdIndicator,
    pub availability: OsdAvailability,
    pub style: SidebarStyle,
    pub output_width: f32,
    pub output_height: f32,
    pub bar_height: f32,
    pub bar_bottom: bool,
    /// QML pins keyboard-layout and connection notices below the top bar.
    pub top_notice: bool,
    pub edge_margin: f32,
    pub on_dismiss: Arc<dyn Fn() + Send + Sync>,
    /// Called for hover enter/leave. The host should hide ordinary OSDs on
    /// enter, and pause/restart the timeout for media (matching QML behavior).
    pub on_hover_changed: Arc<dyn Fn(bool, bool) + Send + Sync>,
    /// QML exposes previous/play-pause/next on its media indicator only.
    pub on_media_action: Arc<dyn Fn(MediaAction) + Send + Sync>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OsdGeometry {
    pub width: f32,
    pub height: f32,
    pub left: f32,
    pub top: f32,
    pub top_notice: bool,
}

impl OsdGeometry {
    pub fn from_output(
        width: f32,
        height: f32,
        bar_height: f32,
        bar_bottom: bool,
        top_notice: bool,
        edge_margin: f32,
    ) -> Self {
        Self::from_output_with_media(
            width,
            height,
            bar_height,
            bar_bottom,
            top_notice,
            edge_margin,
            false,
        )
    }

    pub fn from_output_with_media(
        width: f32,
        height: f32,
        bar_height: f32,
        bar_bottom: bool,
        top_notice: bool,
        edge_margin: f32,
        media: bool,
    ) -> Self {
        let output_width = finite_nonnegative(width);
        let output_height = finite_nonnegative(height);
        let margin = finite_nonnegative(edge_margin).min(output_width * 0.5);
        let compact = output_width <= 1440.0 || output_height <= 800.0;
        let narrow = output_width <= 960.0 || output_height <= 600.0;
        let (max_width, preferred_height): (f32, f32) = if media {
            match (narrow, compact) {
                (true, _) => (260.0, 64.0),
                (false, true) => (284.0, 70.0),
                (false, false) => (300.0, 76.0),
            }
        } else {
            (360.0, 98.0)
        };
        let card_width = (output_width - margin * 2.0).clamp(0.0, max_width);
        let card_height = preferred_height.min(output_height);
        let x = ((output_width - card_width) * 0.5).clamp(0.0, output_width);
        let safe_bar = finite_nonnegative(bar_height).min(output_height);
        let top_aligned = top_notice || !bar_bottom;
        let preferred_y = if top_aligned {
            safe_bar
        } else {
            output_height - safe_bar - card_height
        };
        let y = preferred_y.clamp(0.0, (output_height - card_height).max(0.0));

        Self {
            width: card_width,
            height: card_height,
            left: x,
            top: y,
            top_notice,
        }
    }
}

pub fn render(input: OsdInput) -> AnyElement {
    let top_notice = input.top_notice
        || matches!(
            &input.indicator,
            OsdIndicator::KeyboardLayout { .. } | OsdIndicator::Connection { .. }
        );
    let media = matches!(&input.indicator, OsdIndicator::Media { .. });
    let geometry = OsdGeometry::from_output_with_media(
        input.output_width,
        input.output_height,
        input.bar_height,
        input.bar_bottom,
        top_notice,
        input.edge_margin,
        media,
    );
    if geometry.width <= 0.0 || geometry.height <= 0.0 {
        return div().into_any_element();
    }

    let style = input.style;
    let (icon, title, detail, value, tone) = presentation(&input.indicator, &input.availability);
    let is_media = media;
    let media_compact_margin = if input.output_width <= 960.0 || input.output_height <= 600.0 {
        6.0
    } else {
        8.0
    };
    let media_artwork = match &input.indicator {
        OsdIndicator::Media { art_url, .. } => Some(render_media_artwork(
            art_url.as_deref(),
            (geometry.height - media_compact_margin * 2.0).max(0.0),
            &style,
        )),
        _ => None,
    };
    let progress = progress_value(&input.indicator, &input.availability);
    let dismiss = input.on_dismiss.clone();
    let hover = input.on_hover_changed.clone();
    let accessible = accessible_summary(&title, &detail, value.as_deref());
    let card_radius = style.radius.max(0.0);

    let media_controls = match &input.indicator {
        OsdIndicator::Media {
            can_previous,
            can_toggle,
            can_next,
            playing,
            ..
        } if matches!(&input.availability, OsdAvailability::Ready) => {
            let action = input.on_media_action.clone();
            let previous = (*can_previous).then(|| {
                media_button(
                    "previous",
                    "skip_previous",
                    "Previous track",
                    action.clone(),
                    MediaAction::Previous,
                    &style,
                )
            });
            let toggle = (*can_toggle).then(|| {
                let glyph = if *playing == Some(true) {
                    "pause"
                } else {
                    "play_arrow"
                };
                media_button(
                    "play-pause",
                    glyph,
                    "Play or pause",
                    action.clone(),
                    MediaAction::TogglePlayback,
                    &style,
                )
            });
            let next = (*can_next).then(|| {
                media_button(
                    "next",
                    "skip_next",
                    "Next track",
                    action,
                    MediaAction::Next,
                    &style,
                )
            });
            Some((previous, toggle, next))
        }
        _ => None,
    };

    div()
        .id("ii.osd.surface")
        .absolute()
        .left(px(geometry.left))
        .top(px(geometry.top))
        .w(px(geometry.width))
        .max_w(px(geometry.width))
        .max_h(px(geometry.height))
        .overflow_hidden()
        .on_hover(move |entered, _, _| {
            if *entered && !is_media {
                dismiss();
            } else {
                hover(*entered, is_media);
            }
        })
        .child(
            div()
                .id("ii.osd.card")
                .w_full()
                .h(px(geometry.height))
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(7.0))
                .px(px(14.0))
                .py(px(10.0))
                .when(is_media, |card| {
                    card.gap(px(0.0))
                        .px(px(media_compact_margin))
                        .py(px(media_compact_margin))
                })
                .bg(style.card)
                .border_1()
                .border_color(style.border)
                .rounded(px(card_radius))
                .shadow_lg()
                .role(Role::Status)
                .aria_label(SharedString::from(accessible))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(if let Some(artwork) = media_artwork {
                            artwork
                        } else {
                            div()
                                .size(px(34.0))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(17.0))
                                .bg(style.selected)
                                .text_color(tone.unwrap_or(style.accent))
                                .font_family("Material Symbols Rounded")
                                .text_size(px(22.0))
                                .child(icon)
                                .into_any_element()
                        })
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .w_full()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(style.foreground)
                                        .truncate()
                                        .child(title),
                                )
                                .when(!detail.is_empty(), |column| {
                                    column.child(
                                        div()
                                            .w_full()
                                            .text_xs()
                                            .text_color(style.muted)
                                            .truncate()
                                            .child(detail),
                                    )
                                }),
                        )
                        .when_some(value, |row, value| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(style.foreground)
                                    .child(value),
                            )
                        })
                        .when_some(media_controls, |row, controls| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(2.0))
                                    .children(controls.0)
                                    .children(controls.1)
                                    .children(controls.2),
                            )
                        }),
                )
                .when_some(progress, |card, progress| {
                    card.child(
                        div()
                            .id("ii.osd.progress")
                            .w_full()
                            .h(px(5.0))
                            .overflow_hidden()
                            .rounded(px(3.0))
                            .bg(style.hover)
                            .child(
                                div()
                                    .h_full()
                                    .w(px((geometry.width - 28.0).max(0.0) * progress))
                                    .rounded(px(3.0))
                                    .bg(tone.unwrap_or(style.accent)),
                            ),
                    )
                }),
        )
        .into_any_element()
}

fn presentation(
    indicator: &OsdIndicator,
    availability: &OsdAvailability,
) -> (String, String, String, Option<String>, Option<gpui::Hsla>) {
    if let OsdAvailability::Unavailable(reason) = availability {
        let (icon, title) = indicator_heading(indicator);
        return (
            icon.to_owned(),
            title,
            format!("Unavailable: {reason}"),
            None,
            None,
        );
    }
    if matches!(availability, OsdAvailability::Loading) {
        let (icon, title) = indicator_heading(indicator);
        return (icon.to_owned(), title, "Loading…".to_owned(), None, None);
    }

    match indicator {
        OsdIndicator::Volume {
            value,
            muted,
            protection_message,
        } => {
            let icon = if *muted == Some(true) {
                "volume_off"
            } else {
                "volume_up"
            };
            (
                icon.into(),
                "Volume".into(),
                protection_message.clone().unwrap_or_default(),
                Some(value_label(*value)),
                None,
            )
        }
        OsdIndicator::Brightness {
            value,
            night_filter_active,
        } => {
            let icon = if *night_filter_active == Some(true) {
                "routine"
            } else {
                "light_mode"
            };
            (
                icon.into(),
                "Brightness".into(),
                String::new(),
                Some(value_label(*value)),
                None,
            )
        }
        OsdIndicator::Microphone { value, muted } => {
            let icon = if *muted == Some(true) {
                "mic_off"
            } else {
                "mic"
            };
            (
                icon.into(),
                "Microphone".into(),
                String::new(),
                Some(value_label(*value)),
                None,
            )
        }
        OsdIndicator::Media {
            title,
            artist,
            playing,
            ..
        } => {
            let icon = match playing {
                Some(true) => "play_arrow",
                Some(false) => "pause",
                None => "music_note",
            };
            let title = title
                .as_deref()
                .filter(|text| !text.trim().is_empty())
                .unwrap_or("No media playing")
                .to_owned();
            let detail = artist.clone().unwrap_or_default();
            (icon.into(), title, detail, None, None)
        }
        OsdIndicator::VoiceSearch {
            recording,
            transcribing,
        } => {
            let (icon, detail) = if *transcribing {
                ("cloud_upload", "Transcribing…")
            } else {
                ("mic", "Listening…")
            };
            let detail = if *recording && !*transcribing {
                "Listening… Recording"
            } else {
                detail
            };
            (
                icon.into(),
                "Voice search".into(),
                detail.into(),
                None,
                None,
            )
        }
        OsdIndicator::KeyboardLayout { text, icon, active } => {
            let tone = active.map(|active| {
                if active {
                    gpui::rgb(0x607DFF).into()
                } else {
                    gpui::rgb(0x8A8A8A).into()
                }
            });
            (
                icon.clone().unwrap_or_else(|| "keyboard".into()),
                text.clone().unwrap_or_else(|| "Value unavailable".into()),
                String::new(),
                None,
                tone,
            )
        }
        OsdIndicator::Connection {
            title,
            detail,
            icon,
            value_percent,
            tone,
        } => {
            let accent = match tone {
                ConnectionTone::On => Some(gpui::rgb(0x607DFF).into()),
                ConnectionTone::Off => Some(gpui::rgb(0x8A8A8A).into()),
                ConnectionTone::Warn => Some(gpui::rgb(0xD94B4B).into()),
            };
            let detail = detail.clone().unwrap_or_default();
            let value = value_percent.map(|percent| format!("{percent}%"));
            (
                icon.clone().unwrap_or_else(|| "link".into()),
                title.clone().unwrap_or_else(|| "Connection".into()),
                detail,
                value,
                accent,
            )
        }
    }
}

fn indicator_heading(indicator: &OsdIndicator) -> (&'static str, String) {
    match indicator {
        OsdIndicator::Volume { .. } => ("volume_up", "Volume".into()),
        OsdIndicator::Brightness { .. } => ("light_mode", "Brightness".into()),
        OsdIndicator::Microphone { .. } => ("mic", "Microphone".into()),
        OsdIndicator::Media { .. } => ("music_note", "Media".into()),
        OsdIndicator::VoiceSearch { .. } => ("mic", "Voice search".into()),
        OsdIndicator::KeyboardLayout { .. } => ("keyboard", "Keyboard layout".into()),
        OsdIndicator::Connection { .. } => ("link", "Connection".into()),
    }
}

fn value_label(value: Option<f32>) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{:.0}%", value * 100.0))
        .unwrap_or_else(|| "Value unavailable".into())
}

fn progress_value(indicator: &OsdIndicator, availability: &OsdAvailability) -> Option<f32> {
    if !matches!(availability, OsdAvailability::Ready) {
        return None;
    }
    let value = match indicator {
        OsdIndicator::Volume { value, .. }
        | OsdIndicator::Brightness { value, .. }
        | OsdIndicator::Microphone { value, .. } => *value,
        _ => None,
    }?;
    Some(clamp_unit(value))
}

fn clamp_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn render_media_artwork(url: Option<&str>, size: f32, style: &SidebarStyle) -> AnyElement {
    let selected = style.selected;
    let muted = style.muted;
    let placeholder = move || artwork_placeholder(size, selected, muted);
    let Some(path) = local_artwork_path(url) else {
        return placeholder();
    };
    let loading = placeholder;
    let fallback = move || artwork_placeholder(size, selected, muted);
    img(path)
        .size(px(size))
        .flex_none()
        .rounded(px(8.0))
        .object_fit(ObjectFit::Cover)
        .with_loading(loading)
        .with_fallback(fallback)
        .into_any_element()
}

fn artwork_placeholder(size: f32, background: gpui::Hsla, foreground: gpui::Hsla) -> AnyElement {
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .bg(background)
        .text_color(foreground)
        .font_family("Material Symbols Rounded")
        .text_size(px(size * 0.42))
        .child("music_note")
        .into_any_element()
}

fn local_artwork_path(value: Option<&str>) -> Option<PathBuf> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let path = if let Some(file_url) = value.strip_prefix("file://") {
        let encoded_path = if file_url.starts_with('/') {
            file_url
        } else if let Some(localhost_path) = file_url.strip_prefix("localhost/") {
            return decoded_absolute_path(&format!("/{localhost_path}"));
        } else {
            return None;
        };
        return decoded_absolute_path(encoded_path);
    } else if value.starts_with('/') {
        PathBuf::from(value)
    } else {
        // Never hand remote or unknown schemes to GPUI's image loader.
        return None;
    };
    (path.is_absolute() && path.is_file()).then_some(path)
}

fn decoded_absolute_path(encoded_path: &str) -> Option<PathBuf> {
    let bytes = encoded_path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let high = hex_nibble(*bytes.get(index + 1)?)?;
                let low = hex_nibble(*bytes.get(index + 2)?)?;
                decoded.push((high << 4) | low);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    let decoded = String::from_utf8(decoded).ok()?;
    if decoded.contains('\0') {
        return None;
    }
    let path = PathBuf::from(decoded);
    (path.is_absolute() && path.is_file()).then_some(path)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn accessible_summary(title: &str, detail: &str, value: Option<&str>) -> String {
    [Some(title), (!detail.is_empty()).then_some(detail), value]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
}

fn media_button(
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    action: Arc<dyn Fn(MediaAction) + Send + Sync>,
    command: MediaAction,
    style: &SidebarStyle,
) -> AnyElement {
    let key_command = command.clone();
    div()
        .id(SharedString::from(format!("ii.osd.media.{id}")))
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(15.0))
        .text_color(style.foreground)
        .font_family("Material Symbols Rounded")
        .text_size(px(20.0))
        .role(Role::Button)
        .aria_label(label)
        .focusable()
        .tab_stop(true)
        .on_click({
            let action = action.clone();
            move |_, _, cx| {
                action(command.clone());
                cx.stop_propagation();
            }
        })
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if !event.is_held
                && (event.keystroke.key.eq_ignore_ascii_case("enter")
                    || event.keystroke.key.eq_ignore_ascii_case("space"))
            {
                action(key_command.clone());
                cx.stop_propagation();
            }
        })
        .child(glyph)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_centers_card_and_respects_bar_edge() {
        let top = OsdGeometry::from_output(1920.0, 1080.0, 48.0, false, false, 16.0);
        let bottom = OsdGeometry::from_output(1920.0, 1080.0, 48.0, true, false, 16.0);
        let notice = OsdGeometry::from_output(1920.0, 1080.0, 48.0, true, true, 16.0);
        assert_eq!(top.width, 360.0);
        assert_eq!(top.left, 780.0);
        assert_eq!(top.top, 48.0);
        assert_eq!(bottom.top, 934.0);
        assert_eq!(notice.top, 48.0);
        assert!(notice.top_notice);
    }

    #[test]
    fn geometry_is_bounded_for_tiny_or_invalid_outputs() {
        let tiny = OsdGeometry::from_output(80.0, 40.0, 64.0, false, false, 16.0);
        assert_eq!(tiny.width, 48.0);
        assert_eq!(tiny.height, 40.0);
        assert_eq!(tiny.left, 16.0);
        assert_eq!(tiny.top, 0.0);

        let invalid = OsdGeometry::from_output(f32::NAN, -1.0, f32::INFINITY, false, false, -10.0);
        assert_eq!(invalid.width, 0.0);
        assert_eq!(invalid.height, 0.0);
        assert_eq!(invalid.left, 0.0);
        assert_eq!(invalid.top, 0.0);
    }

    #[test]
    fn media_geometry_matches_qml_compact_card_sizes() {
        let regular =
            OsdGeometry::from_output_with_media(1920.0, 1080.0, 48.0, false, false, 16.0, true);
        let compact =
            OsdGeometry::from_output_with_media(1366.0, 768.0, 48.0, false, false, 16.0, true);
        let narrow =
            OsdGeometry::from_output_with_media(900.0, 540.0, 48.0, false, false, 16.0, true);
        assert_eq!((regular.width, regular.height), (300.0, 76.0));
        assert_eq!((compact.width, compact.height), (284.0, 70.0));
        assert_eq!((narrow.width, narrow.height), (260.0, 64.0));
    }

    #[test]
    fn local_artwork_parser_accepts_existing_local_file_urls_only() {
        use std::{fs, time::SystemTime};
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("intoo osd art {stamp}.png"));
        fs::write(&path, b"placeholder test image").unwrap();
        let encoded = format!("file://{}", path.to_string_lossy().replace(' ', "%20"));
        assert_eq!(local_artwork_path(Some(&encoded)), Some(path.clone()));
        assert_eq!(
            local_artwork_path(Some(&format!("file://localhost{}", path.display()))),
            Some(path.clone())
        );
        assert_eq!(
            local_artwork_path(Some(path.to_str().unwrap())),
            Some(path.clone())
        );
        assert_eq!(
            local_artwork_path(Some("https://example.test/cover.png")),
            None
        );
        assert_eq!(
            local_artwork_path(Some("file://remote-host/tmp/cover.png")),
            None
        );
        assert_eq!(local_artwork_path(Some("file:///tmp/bad%2G.png")), None);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn absent_remote_or_missing_artwork_uses_placeholder_path() {
        assert_eq!(local_artwork_path(None), None);
        assert_eq!(local_artwork_path(Some("")), None);
        assert_eq!(
            local_artwork_path(Some("https://example.test/cover.png")),
            None
        );
        assert_eq!(local_artwork_path(Some("file:///does/not/exist.png")), None);
    }

    #[test]
    fn unknown_or_out_of_range_levels_are_not_fabricated() {
        assert_eq!(value_label(None), "Value unavailable");
        assert_eq!(value_label(Some(1.4)), "140%");
        assert_eq!(
            progress_value(
                &OsdIndicator::Volume {
                    value: Some(1.4),
                    muted: None,
                    protection_message: None
                },
                &OsdAvailability::Ready,
            ),
            Some(1.0)
        );
        assert_eq!(
            progress_value(
                &OsdIndicator::Volume {
                    value: Some(0.4),
                    muted: None,
                    protection_message: None
                },
                &OsdAvailability::Unavailable("PipeWire disconnected".into()),
            ),
            None
        );
    }

    #[test]
    fn unavailable_provider_is_distinct_from_zero_level() {
        let (icon, title, detail, value, _) = presentation(
            &OsdIndicator::Brightness {
                value: None,
                night_filter_active: None,
            },
            &OsdAvailability::Unavailable("brightness provider is not connected".into()),
        );
        assert_eq!(icon, "light_mode");
        assert_eq!(title, "Brightness");
        assert!(detail.contains("Unavailable"));
        assert!(value.is_none());
    }

    #[test]
    fn notice_presentation_keeps_qml_titles_and_media_controls_are_capability_gated() {
        let media = OsdIndicator::Media {
            title: Some("Track".into()),
            artist: Some("Artist".into()),
            art_url: None,
            playing: Some(true),
            can_previous: false,
            can_toggle: true,
            can_next: true,
        };
        let (_, title, detail, _, _) = presentation(&media, &OsdAvailability::Ready);
        assert_eq!(title, "Track");
        assert_eq!(detail, "Artist");
        assert!(matches!(
            media,
            OsdIndicator::Media {
                can_previous: false,
                can_toggle: true,
                can_next: true,
                ..
            }
        ));

        let (_, title, detail, _, _) = presentation(
            &OsdIndicator::KeyboardLayout {
                text: Some("US".into()),
                icon: None,
                active: Some(true),
            },
            &OsdAvailability::Ready,
        );
        assert_eq!(title, "US");
        assert_eq!(detail, "");
    }
}
