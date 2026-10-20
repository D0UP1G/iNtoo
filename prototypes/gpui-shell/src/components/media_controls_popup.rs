//! Reusable Material media-controls card for the ii horizontal media popup.
//!
//! This view receives an ordered MPRIS snapshot list. It does not query MPRIS
//! or own popup visibility; all card actions are pinned to their service name.

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use crate::components::vertical_bar::{
    MediaAction, MediaPlayerSnapshot, MediaState, MediaStatus, PlayerMediaCommand,
};
use gpui::{
    AnyElement, FocusHandle, Hsla, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, ObjectFit, Pixels, Role, ScrollDelta, ScrollWheelEvent, SharedString, Window,
    canvas, div, img, prelude::*, px, relative,
};

/// Material roles supplied by the owning bar/theme adapter.
#[derive(Clone, Copy, Debug)]
pub struct MediaControlsPopupColors {
    pub surface: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub track: Hsla,
    pub outline: Hsla,
    pub control_surface: Hsla,
    pub hover_surface: Hsla,
    pub focus: Hsla,
}

/// Explicit presentation and callback inputs for [`render_media_controls_popup`].
#[derive(Clone)]
pub struct MediaControlsPopupProps {
    /// `None` means the player provider has not supplied a snapshot.
    pub media: Option<MediaState>,
    pub colors: MediaControlsPopupColors,
    /// Focus scope for initial keyboard focus and dialog key handling.
    pub focus_handle: FocusHandle,
    pub width_px: f32,
    pub height_px: f32,
    pub radius_px: f32,
    /// Optional MPRIS intent sink; unsupported buttons are omitted.
    pub on_action: Option<Rc<dyn Fn(MediaAction)>>,
    /// Optional close request; the owner remains responsible for hiding the popup.
    pub on_close: Option<Rc<dyn Fn()>>,
}

/// Height for the stacked player cards, keeping one-card and empty layouts at
/// the caller's preferred size.
pub fn card_stack_height(width_px: f32, preferred_height_px: f32, player_count: usize) -> f32 {
    if player_count <= 1 {
        return finite_dimension(preferred_height_px, 150.0);
    }
    let width = finite_dimension(width_px, 380.0);
    let card_height = if width > 0.0 {
        width * (150.0 / 380.0)
    } else {
        finite_dimension(preferred_height_px, 150.0)
    };
    card_height * player_count as f32 + 8.0 * (player_count.saturating_sub(1) as f32)
}

/// Clamp a popup viewport to the available output height while keeping its
/// scroll content free to use the full stacked height.
pub fn bounded_popup_height(content_height_px: f32, available_height_px: f32) -> f32 {
    let content_height = finite_dimension(content_height_px, 150.0);
    finite_dimension(available_height_px, content_height).min(content_height)
}

/// Return the bounded viewport height and full scroll-content height for a
/// player stack. The caller's height is the available viewport, while the
/// stack may grow independently with the number of players.
fn card_stack_geometry(width_px: f32, available_height_px: f32, player_count: usize) -> (f32, f32) {
    let stack_height = card_stack_height(width_px, available_height_px, player_count);
    (
        bounded_popup_height(stack_height, available_height_px),
        stack_height,
    )
}

/// Render an ordered card for each player, retaining the QML empty state.
pub fn render_media_controls_popup(props: MediaControlsPopupProps) -> AnyElement {
    let players = props
        .media
        .as_ref()
        .map(|state| state.players.clone())
        .unwrap_or_default();
    if players.is_empty() {
        return render_single_media_controls_popup(props, None, None);
    }
    let count = players.len();
    let card_height = if count > 1 {
        props.width_px * (150.0 / 380.0)
    } else {
        props.height_px
    };
    let cards = players.into_iter().map(|player| {
        let bus_name = player.service_bus_name.clone();
        let action_bus_name = bus_name.clone();
        let callback = props.on_action.clone();
        let mut player_props = props.clone();
        player_props.height_px = card_height;
        player_props.media = Some(player.as_media_state());
        player_props.on_action = callback.map(|callback| {
            Rc::new(move |action| {
                let command = match action {
                    MediaAction::TogglePlayback => {
                        Some(crate::components::vertical_bar::PlayerMediaCommand::TogglePlayback)
                    }
                    MediaAction::Previous => {
                        Some(crate::components::vertical_bar::PlayerMediaCommand::Previous)
                    }
                    MediaAction::Next => {
                        Some(crate::components::vertical_bar::PlayerMediaCommand::Next)
                    }
                    MediaAction::SeekTo { seconds, track_id } => {
                        Some(PlayerMediaCommand::Seek { seconds, track_id })
                    }
                    _ => None,
                };
                if let Some(command) = command {
                    callback(MediaAction::ForPlayer {
                        service_bus_name: action_bus_name.clone(),
                        command,
                    });
                }
            }) as Rc<dyn Fn(MediaAction)>
        });
        div()
            .flex_shrink_0()
            .child(render_single_media_controls_popup(
                player_props,
                Some(bus_name),
                Some(player),
            ))
            .into_any_element()
    });
    let viewport_height = card_stack_geometry(props.width_px, props.height_px, count).0;
    div()
        .id("ii.media-controls.players")
        .w(px(finite_dimension(props.width_px, 480.0)))
        .h(px(viewport_height))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .overflow_y_scroll()
        .children(cards)
        .into_any_element()
}

/// Render one media-controls card using Material colors passed by the host.
fn render_single_media_controls_popup(
    props: MediaControlsPopupProps,
    player_id: Option<String>,
    player: Option<MediaPlayerSnapshot>,
) -> AnyElement {
    let width = finite_dimension(props.width_px, 480.0);
    let height = finite_dimension(props.height_px, 212.0);
    let radius = finite_dimension(props.radius_px, 20.0);
    let colors = props.colors;
    let media = props.media;
    let state_label = media
        .as_ref()
        .map(|state| media_status_label(state.status))
        .unwrap_or("unavailable");
    let active = media.as_ref().is_some_and(is_active_player);
    let title = media
        .as_ref()
        .filter(|_| active)
        .and_then(|state| state.title.as_deref())
        .map(clean_music_title)
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            if active {
                "—".to_owned()
            } else {
                String::new()
            }
        });
    let artist = media
        .as_ref()
        .filter(|_| active)
        .and_then(|state| state.artist.as_deref())
        .map(str::trim)
        .filter(|artist| !artist.is_empty())
        .unwrap_or("");
    let accessible_name = if active {
        if artist.is_empty() {
            format!("Media controls: {title}, {state_label}")
        } else {
            format!("Media controls: {title} by {artist}, {state_label}")
        }
    } else {
        match media.as_ref().map(|state| state.status) {
            Some(MediaStatus::NoPlayer) => "Media controls: no active player".to_owned(),
            _ => "Media controls: unavailable".to_owned(),
        }
    };

    let escape_close = props.on_close.clone();
    let space_action = props.on_action.clone();
    let space_media = media.clone();
    let mut card = div()
        .id(scoped_id("ii.media-controls-popup", player_id.as_deref()))
        .w(px(width))
        .h(px(height))
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .p(px(12.0))
        .rounded(px(radius))
        .border_1()
        .border_color(colors.outline)
        .bg(colors.surface)
        .text_color(colors.foreground)
        .track_focus(&props.focus_handle)
        .focusable()
        .tab_stop(false)
        .role(Role::Dialog)
        .aria_label(SharedString::from(accessible_name))
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if !event.is_held && event.keystroke.key.eq_ignore_ascii_case("escape") {
                if let Some(close) = &escape_close {
                    close();
                }
                window.remove_window();
                cx.stop_propagation();
            } else if !event.is_held && event.keystroke.key.eq_ignore_ascii_case("space") {
                if let (Some(media), Some(action)) = (&space_media, &space_action) {
                    if is_active_player(media) && media.can_toggle_playback {
                        action(MediaAction::TogglePlayback);
                    }
                }
                cx.stop_propagation();
            }
        });

    let header = popup_header(
        &title,
        artist,
        state_label,
        active,
        player.as_ref().and_then(|player| player.art_url.as_deref()),
        colors,
        props.on_close,
        player_id.as_deref(),
    );
    card = card.child(header);

    if active {
        if let Some(media) = media.as_ref() {
            let action = props.on_action.clone();
            card = card.child(render_progress(
                media.position_percent,
                player.as_ref().and_then(|player| player.position_seconds),
                player.as_ref().and_then(|player| player.duration_seconds),
                player.as_ref().is_some_and(|player| player.can_seek),
                player.as_ref().and_then(|player| player.track_id.clone()),
                colors,
                action,
                player_id.as_deref(),
            ));
            card = card.child(render_controls(
                media,
                colors,
                props.on_action,
                player_id.as_deref(),
            ));
        }
    } else {
        let empty_title = if media
            .as_ref()
            .is_some_and(|state| state.status == MediaStatus::NoPlayer)
        {
            "No active player"
        } else {
            "Media unavailable"
        };
        let description = if empty_title == "No active player" {
            "No player is currently providing media controls."
        } else {
            "The media player state is unavailable."
        };
        card = card.child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .child(
                    div()
                        .font_family("Material Symbols Rounded")
                        .text_size(px(26.0))
                        .text_color(colors.muted)
                        .child(if empty_title == "No active player" {
                            "music_note"
                        } else {
                            "music_off"
                        }),
                )
                .child(
                    div()
                        .text_size(px(16.0))
                        .text_color(colors.foreground)
                        .child(empty_title),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(colors.muted)
                        .child(description),
                ),
        );
    }
    card.into_any_element()
}

fn popup_header(
    title: &str,
    artist: &str,
    state: &str,
    active: bool,
    art_url: Option<&str>,
    colors: MediaControlsPopupColors,
    on_close: Option<Rc<dyn Fn()>>,
    player_id: Option<&str>,
) -> impl IntoElement {
    let title = title.to_owned();
    let artist = artist.to_owned();
    let state = state.to_owned();
    let mut header = div()
        .w_full()
        .flex()
        .items_start()
        .gap(px(10.0))
        .child(render_artwork(art_url.filter(|_| active), colors))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .when(active, |this| {
                    this.child(
                        div()
                            .w_full()
                            .truncate()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_size(px(17.0))
                            .text_color(colors.foreground)
                            .child(title.clone()),
                    )
                })
                .when(active && !artist.is_empty(), |this| {
                    this.child(
                        div()
                            .w_full()
                            .truncate()
                            .text_size(px(13.0))
                            .text_color(colors.muted)
                            .child(artist.clone()),
                    )
                })
                .when(active, |this| {
                    this.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(colors.accent)
                            .child(state.clone()),
                    )
                }),
        );

    if let Some(on_close) = on_close {
        header = header.child(popup_close_button(
            scoped_id("ii.media-controls-popup.close", player_id),
            "Close media controls",
            "close",
            colors,
            on_close,
            32.0,
        ));
    }
    header
}

enum ArtworkSource {
    Local(PathBuf),
    Remote(String),
}

fn artwork_placeholder(colors: MediaControlsPopupColors) -> AnyElement {
    div()
        .w(px(56.0))
        .h(px(56.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.0))
        .bg(colors.control_surface)
        .text_color(colors.muted)
        .font_family("Material Symbols Rounded")
        .text_size(px(24.0))
        .child("music_note")
        .into_any_element()
}

fn render_artwork(url: Option<&str>, colors: MediaControlsPopupColors) -> AnyElement {
    let Some(source) = url.and_then(artwork_source) else {
        return artwork_placeholder(colors);
    };
    let fallback = move || artwork_placeholder(colors);
    let loading = move || artwork_placeholder(colors);
    let image = match source {
        ArtworkSource::Local(path) => img(path),
        ArtworkSource::Remote(uri) => img(uri),
    };
    image
        .w(px(56.0))
        .h(px(56.0))
        .flex_none()
        .rounded(px(10.0))
        .object_fit(ObjectFit::Cover)
        .with_loading(loading)
        .with_fallback(fallback)
        .into_any_element()
}

fn artwork_source(url: &str) -> Option<ArtworkSource> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    if let Some(path) = url.strip_prefix("file://") {
        let path = path
            .strip_prefix("localhost/")
            .map(|path| format!("/{path}"))
            .unwrap_or_else(|| path.to_owned());
        return percent_decode_path(&path).map(|path| ArtworkSource::Local(PathBuf::from(path)));
    }
    if url.starts_with("https://") || url.starts_with("http://") {
        return Some(ArtworkSource::Remote(url.to_owned()));
    }
    if url.starts_with('/') {
        return Some(ArtworkSource::Local(PathBuf::from(url)));
    }
    None
}

fn percent_decode_path(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = *bytes.get(index + 1)?;
            let lo = *bytes.get(index + 2)?;
            decoded.push((hex_value(hi)? << 4) | hex_value(lo)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn render_progress(
    position_percent: Option<u8>,
    position_seconds: Option<u64>,
    duration_seconds: Option<u64>,
    can_seek: bool,
    track_id: Option<String>,
    colors: MediaControlsPopupColors,
    on_action: Option<Rc<dyn Fn(MediaAction)>>,
    player_id: Option<&str>,
) -> impl IntoElement {
    let duration = duration_seconds.filter(|duration| *duration > 0);
    let current = position_seconds.unwrap_or_else(|| {
        duration
            .zip(position_percent)
            .map(|(duration, percent)| duration * u64::from(percent.min(100)) / 100)
            .unwrap_or(0)
    });
    let current = duration
        .map(|duration| current.min(duration))
        .unwrap_or(current);
    let ratio = duration
        .map(|duration| current as f32 / duration as f32)
        .or_else(|| position_percent.map(|percent| f32::from(percent.min(100)) / 100.0))
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let current_label = format_media_time(current);
    let duration_label = duration
        .map(format_media_time)
        .unwrap_or_else(|| "--:--".to_owned());
    let label = format!("Playback position {current_label} of {duration_label}");
    let seek_enabled = can_seek && duration.is_some() && track_id.is_some() && on_action.is_some();
    let pointer_bounds: Rc<RefCell<Option<(Pixels, Pixels)>>> = Rc::default();
    let mut track = div()
        .id(scoped_id("ii.media-controls-popup.progress", player_id))
        .relative()
        .h(px(20.0))
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .aria_label(SharedString::from(label))
        .child(
            div()
                .relative()
                .h(px(5.0))
                .w_full()
                .rounded_full()
                .overflow_hidden()
                .bg(colors.track)
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(relative(ratio))
                        .rounded_full()
                        .bg(colors.accent),
                ),
        );
    if seek_enabled {
        let duration = duration.expect("seek requires duration");
        let action = on_action.expect("seek requires callback");
        let track_id = track_id.expect("seek requires a current track ID");
        let down_action = action.clone();
        let move_action = action.clone();
        let key_action = action.clone();
        let wheel_action = action;
        let key_track_id = track_id.clone();
        let down_track_id = track_id.clone();
        let move_track_id = track_id.clone();
        let wheel_track_id = track_id;
        let down_bounds = pointer_bounds.clone();
        let move_bounds = pointer_bounds.clone();
        let paint_bounds = pointer_bounds;
        track = track
            .role(Role::Slider)
            .aria_value(current_label.clone())
            .aria_numeric_value(current as f64)
            .aria_min_numeric_value(0.0)
            .aria_max_numeric_value(duration as f64)
            .aria_numeric_value_step(5.0)
            .focusable()
            .tab_stop(true)
            .focus_visible(move |style| style.border_1().border_color(colors.focus))
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                let next = match event.keystroke.key.as_str() {
                    "left" | "down" => Some(current.saturating_sub(5)),
                    "right" | "up" => Some(current.saturating_add(5).min(duration)),
                    "pageup" => Some(current.saturating_add(30).min(duration)),
                    "pagedown" => Some(current.saturating_sub(30)),
                    "home" => Some(0),
                    "end" => Some(duration),
                    _ => None,
                };
                if !event.is_held
                    && let Some(position) = next
                {
                    key_action(MediaAction::SeekTo {
                        seconds: position,
                        track_id: key_track_id.clone(),
                    });
                    cx.stop_propagation();
                }
            })
            .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, _| {
                if let Some((left, width)) = *down_bounds.borrow() {
                    down_action(MediaAction::SeekTo {
                        seconds: pointer_seek_value(event.position.x, left, width, duration),
                        track_id: down_track_id.clone(),
                    });
                }
            })
            .on_mouse_move(move |event: &MouseMoveEvent, _, _| {
                if event.pressed_button == Some(MouseButton::Left)
                    && let Some((left, width)) = *move_bounds.borrow()
                {
                    move_action(MediaAction::SeekTo {
                        seconds: pointer_seek_value(event.position.x, left, width, duration),
                        track_id: move_track_id.clone(),
                    });
                }
            })
            .on_scroll_wheel(move |event: &ScrollWheelEvent, _, _| {
                let delta = match event.delta {
                    ScrollDelta::Pixels(delta) => f32::from(delta.y),
                    ScrollDelta::Lines(delta) => delta.y,
                };
                if delta != 0.0 {
                    let next = if delta > 0.0 {
                        current.saturating_add(5).min(duration)
                    } else {
                        current.saturating_sub(5)
                    };
                    wheel_action(MediaAction::SeekTo {
                        seconds: next,
                        track_id: wheel_track_id.clone(),
                    });
                }
            })
            .child(
                canvas(
                    |bounds, _, _| bounds,
                    move |bounds, _, _, _| {
                        *paint_bounds.borrow_mut() = Some((bounds.origin.x, bounds.size.width));
                    },
                )
                .absolute()
                .inset_0(),
            );
    } else {
        track = track
            .role(Role::ProgressIndicator)
            .aria_numeric_value((ratio * 100.0) as f64);
    }
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(colors.muted)
                .child(current_label),
        )
        .child(track)
        .child(
            div()
                .text_size(px(10.0))
                .text_color(colors.muted)
                .child(duration_label),
        )
}

fn pointer_seek_value(x: Pixels, left: Pixels, width: Pixels, duration: u64) -> u64 {
    let width = f32::from(width).max(1.0);
    let ratio = (f32::from(x - left) / width).clamp(0.0, 1.0);
    (ratio * duration as f32).round() as u64
}

fn format_media_time(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn render_controls(
    media: &MediaState,
    colors: MediaControlsPopupColors,
    on_action: Option<Rc<dyn Fn(MediaAction)>>,
    player_id: Option<&str>,
) -> impl IntoElement {
    let available_actions = supported_playback_actions(media);
    let mut controls = div()
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.0));

    if let Some(action) = on_action {
        if available_actions.contains(&MediaAction::Previous) {
            let callback = action.clone();
            controls = controls.child(popup_action_button(
                scoped_id("ii.media-controls-popup.previous", player_id),
                "Previous track",
                "skip_previous",
                colors,
                move || callback(MediaAction::Previous),
                36.0,
            ));
        }
        if available_actions.contains(&MediaAction::TogglePlayback) {
            let (label, glyph) = if media.status == MediaStatus::Playing {
                ("Pause", "pause")
            } else {
                ("Play", "play_arrow")
            };
            let callback = action.clone();
            controls = controls.child(popup_action_button(
                scoped_id("ii.media-controls-popup.play-pause", player_id),
                label,
                glyph,
                colors,
                move || callback(MediaAction::TogglePlayback),
                44.0,
            ));
        }
        if available_actions.contains(&MediaAction::Next) {
            controls = controls.child(popup_action_button(
                scoped_id("ii.media-controls-popup.next", player_id),
                "Next track",
                "skip_next",
                colors,
                move || action(MediaAction::Next),
                36.0,
            ));
        }
    }
    controls
}

fn scoped_id(base: &str, player_id: Option<&str>) -> String {
    match player_id {
        Some(player_id) => format!("{base}.{player_id}"),
        None => base.to_owned(),
    }
}

fn popup_action_button(
    id: String,
    label: &'static str,
    glyph: &'static str,
    colors: MediaControlsPopupColors,
    callback: impl Fn() + 'static,
    size: f32,
) -> impl IntoElement {
    popup_icon_button(id, label, glyph, colors, move |_| callback(), size)
}

fn popup_close_button(
    id: String,
    label: &'static str,
    glyph: &'static str,
    colors: MediaControlsPopupColors,
    callback: Rc<dyn Fn()>,
    size: f32,
) -> impl IntoElement {
    popup_icon_button(
        id,
        label,
        glyph,
        colors,
        move |window| {
            callback();
            window.remove_window();
        },
        size,
    )
}

fn popup_icon_button(
    id: String,
    label: &'static str,
    glyph: &'static str,
    colors: MediaControlsPopupColors,
    callback: impl Fn(&mut Window) + 'static,
    size: f32,
) -> impl IntoElement {
    let callback = Rc::new(callback);
    let click_callback = callback.clone();
    let key_callback = callback;
    div()
        .id(id)
        .flex_none()
        .w(px(size))
        .h(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(colors.control_surface)
        .text_color(colors.foreground)
        .role(Role::Button)
        .aria_label(label)
        .focusable()
        .tab_stop(true)
        .focus_visible(move |style| style.border_1().border_color(colors.focus))
        .hover(move |style| style.bg(colors.hover_surface))
        .font_family("Material Symbols Rounded")
        .text_size(px(if size > 40.0 { 24.0 } else { 20.0 }))
        .child(glyph)
        .on_click(move |_, window, _| click_callback(window))
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                key_callback(window);
                cx.stop_propagation();
            }
        })
}

fn clean_music_title(title: &str) -> String {
    let mut cleaned = title.to_owned();
    for (open, close, replace_with_space) in [
        ('(', ')', true),
        ('[', ']', true),
        ('{', '}', true),
        ('【', '】', false),
        ('《', '》', false),
        ('「', '」', false),
        ('『', '』', false),
    ] {
        let trimmed = cleaned.trim_start();
        if let Some(rest) = trimmed.strip_prefix(open) {
            if let Some(end) = rest.find(close) {
                let suffix = &rest[end + close.len_utf8()..];
                cleaned = if replace_with_space {
                    format!(" {suffix}")
                } else {
                    suffix.to_owned()
                };
            }
        }
    }
    cleaned.trim().to_owned()
}

fn media_status_label(status: MediaStatus) -> &'static str {
    match status {
        MediaStatus::Unavailable => "unavailable",
        MediaStatus::NoPlayer => "no active player",
        MediaStatus::Playing => "playing",
        MediaStatus::Paused => "paused",
        MediaStatus::Stopped => "stopped",
    }
}

fn is_active_player(media: &MediaState) -> bool {
    matches!(
        media.status,
        MediaStatus::Playing | MediaStatus::Paused | MediaStatus::Stopped
    )
}

fn supported_playback_actions(media: &MediaState) -> Vec<MediaAction> {
    if !is_active_player(media) {
        return Vec::new();
    }
    let mut actions = Vec::with_capacity(3);
    if media.can_previous {
        actions.push(MediaAction::Previous);
    }
    if media.can_toggle_playback {
        actions.push(MediaAction::TogglePlayback);
    }
    if media.can_next {
        actions.push(MediaAction::Next);
    }
    actions
}

fn finite_dimension(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(status: MediaStatus) -> MediaState {
        MediaState {
            status,
            title: Some("[Official] Song".to_owned()),
            artist: Some("Artist".to_owned()),
            position_percent: Some(101),
            can_toggle_playback: true,
            can_previous: true,
            can_next: false,
            can_change_volume: false,
            players: Vec::new(),
        }
    }

    #[test]
    fn title_cleanup_matches_qml_leading_bracket_rules() {
        assert_eq!(clean_music_title(" (live) [HD] Song "), "Song");
        assert_eq!(clean_music_title("【Touhou】Song"), "Song");
        assert_eq!(clean_music_title("『Opening』 Song"), "Song");
        assert_eq!(clean_music_title(" Song "), "Song");
    }

    #[test]
    fn empty_and_unavailable_states_have_distinct_labels() {
        assert_eq!(
            media_status_label(MediaStatus::NoPlayer),
            "no active player"
        );
        assert_eq!(media_status_label(MediaStatus::Unavailable), "unavailable");
        assert_eq!(media_status_label(MediaStatus::Playing), "playing");
        assert_eq!(media_status_label(MediaStatus::Paused), "paused");
        assert_eq!(media_status_label(MediaStatus::Stopped), "stopped");
        assert!(!is_active_player(&media(MediaStatus::NoPlayer)));
        assert!(is_active_player(&media(MediaStatus::Playing)));
    }

    #[test]
    fn dimensions_are_finite_and_non_negative() {
        assert_eq!(finite_dimension(480.0, 1.0), 480.0);
        assert_eq!(finite_dimension(-4.0, 1.0), 0.0);
        assert_eq!(finite_dimension(f32::INFINITY, 480.0), 480.0);
        assert_eq!(finite_dimension(f32::NAN, 212.0), 212.0);
    }

    #[test]
    fn progress_is_clamped_to_percent_range() {
        assert_eq!(
            media(MediaStatus::Playing)
                .position_percent
                .map(|v| v.min(100)),
            Some(100)
        );
        assert_eq!(None::<u8>.map(|value| value.min(100)), None);
    }

    #[test]
    fn playback_actions_respect_capabilities_and_active_player_state() {
        let mut state = media(MediaStatus::Paused);
        state.can_toggle_playback = false;
        state.can_next = true;
        assert_eq!(
            supported_playback_actions(&state),
            vec![MediaAction::Previous, MediaAction::Next]
        );
        assert!(supported_playback_actions(&media(MediaStatus::NoPlayer)).is_empty());
    }

    #[test]
    fn artwork_sources_support_local_file_urls_and_http_but_reject_unknown_schemes() {
        assert!(
            matches!(artwork_source("file:///tmp/album%20cover.png"), Some(ArtworkSource::Local(path)) if path == PathBuf::from("/tmp/album cover.png"))
        );
        assert!(
            matches!(artwork_source("https://example.test/cover.jpg"), Some(ArtworkSource::Remote(url)) if url == "https://example.test/cover.jpg")
        );
        assert!(artwork_source("ftp://example.test/cover.jpg").is_none());
        assert!(artwork_source("file:///tmp/bad%Q0.jpg").is_none());
    }

    #[test]
    fn timeline_time_format_and_stack_height_handle_long_and_multiple_tracks() {
        assert_eq!(format_media_time(65), "1:05");
        assert_eq!(format_media_time(3_661), "1:01:01");
        assert_eq!(card_stack_height(380.0, 150.0, 0), 150.0);
        assert_eq!(card_stack_height(380.0, 150.0, 1), 150.0);
        assert_eq!(card_stack_height(380.0, 150.0, 2), 308.0);
    }

    #[test]
    fn multi_player_stack_keeps_content_tall_inside_bounded_viewport() {
        assert_eq!(card_stack_geometry(380.0, 220.0, 5), (220.0, 782.0));
        assert_eq!(card_stack_geometry(380.0, 900.0, 5), (782.0, 782.0));
        assert_eq!(card_stack_geometry(380.0, 150.0, 1), (150.0, 150.0));
        assert_eq!(bounded_popup_height(782.0, 100.0), 100.0);
        assert_eq!(bounded_popup_height(782.0, f32::INFINITY), 782.0);
    }
}
