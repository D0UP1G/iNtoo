//! Isolated, data-driven Waffle Action Center presentation.
//!
//! Service discovery and mutations belong to the caller. Missing service data is
//! rendered as unavailable instead of being replaced with a plausible default.
use std::sync::Arc;

use gpui::{
    ClickEvent, Entity, Hsla, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, Role, Window, div, prelude::*, px,
};

pub type Action = Arc<dyn Fn() + Send + Sync>;
pub type ItemAction = Arc<dyn Fn(String) + Send + Sync>;
pub type ValueAction = Arc<dyn Fn(f32) + Send + Sync>;
pub type SliderAction = Arc<dyn Fn(String, f32) + Send + Sync>;

struct SeekPosition {
    value: f32,
    last_external: f32,
}

#[derive(Clone, Debug)]
pub struct ActionCenterStyle {
    pub background: Hsla,
    pub panel: Hsla,
    pub surface: Hsla,
    pub hover: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub separator: Hsla,
    pub radius: f32,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoadState {
    #[default]
    Loading,
    Ready,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct Toggle {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub enabled: Option<bool>,
    pub available: bool,
}
#[derive(Clone, Debug)]
pub struct Slider {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub value: Option<f32>,
    pub max: f32,
    pub available: bool,
}
#[derive(Clone, Debug)]
pub struct Media {
    pub title: String,
    pub artist: String,
    pub player: Option<String>,
    pub position: Option<f32>,
    pub duration: Option<f32>,
    pub playing: Option<bool>,
    pub can_previous: bool,
    pub can_next: bool,
    pub can_seek: bool,
    pub artwork_label: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub selected: bool,
    pub available: bool,
}

#[derive(Clone)]
pub struct ActionCenterInput {
    pub style: ActionCenterStyle,
    pub page: usize,
    pub page_count: usize,
    pub toggles: Vec<Toggle>,
    pub toggle_state: LoadState,
    pub sliders: Vec<Slider>,
    pub media: Option<Media>,
    pub media_state: LoadState,
    pub audio_choices: Vec<Choice>,
    pub audio_state: LoadState,
    pub on_close: Action,
    pub on_page: Arc<dyn Fn(usize) + Send + Sync>,
    pub on_toggle: ItemAction,
    pub on_slider: SliderAction,
    pub on_previous: Action,
    pub on_play_pause: Action,
    pub on_next: Action,
    pub on_seek: ValueAction,
    pub on_choose_audio: ItemAction,
}

/// Render the right-aligned Fluent-style panel body. The overlay/window and
/// keyboard focus policy are owned by the surface host.
pub fn render<T: 'static>(
    input: ActionCenterInput,
    window: &mut Window,
    cx: &mut Context<T>,
) -> impl IntoElement {
    let s = input.style.clone();
    let close = input.on_close.clone();
    let seek_state = input.media.as_ref().map(|media| {
        let external = media.position.unwrap_or(0.);
        let state =
            window.use_keyed_state("waffle.action-center.seek-position", cx, move |_, _| {
                SeekPosition {
                    value: external,
                    last_external: external,
                }
            });
        state.update(cx, |state, _| {
            if state.last_external != external {
                state.value = external;
                state.last_external = external;
            }
        });
        state
    });
    let mut children = Vec::new();
    match input.media_state {
        LoadState::Unavailable => {
            children.push(status_card("Media", "Media service unavailable", &s).into_any_element())
        }
        LoadState::Loading if input.media.is_none() => children
            .push(status_card("Media", "Loading player information…", &s).into_any_element()),
        _ => {
            if let Some(media) = input.media.as_ref() {
                children.push(
                    media_card(
                        media,
                        &s,
                        input.on_previous.clone(),
                        input.on_play_pause.clone(),
                        input.on_next.clone(),
                        input.on_seek.clone(),
                        seek_state.clone().expect("media seek state initialized"),
                    )
                    .into_any_element(),
                );
            }
        }
    }
    let current = input.page.min(input.page_count.saturating_sub(1));
    let toggle_start = current.saturating_mul(6);
    let toggle_items = if input.toggle_state == LoadState::Ready {
        input
            .toggles
            .iter()
            .skip(toggle_start)
            .take(6)
            .map(|t| toggle_card(t, &s, input.on_toggle.clone()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let toggle_grid = div()
        .w_full()
        .flex()
        .flex_wrap()
        .gap(px(10. * s.scale))
        .children(toggle_items);
    let page_action = input.on_page.clone();
    let accent = s.accent;
    let muted = s.muted;
    let max_page = input.page_count.saturating_sub(1);
    let pages = (0..input.page_count).map(move |index| {
        let action = page_action.clone();
        let selected = index == current;
        div()
            .id(format!("waffle.action-center.page.{index}"))
            .w(px(18.))
            .h(px(18.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(if selected { accent } else { muted })
            .role(Role::Button)
            .aria_label(format!(
                "Page {} of {}, {}",
                index + 1,
                max_page + 1,
                if selected { "current" } else { "not current" }
            ))
            .focusable()
            .tab_stop(true)
            .on_click({
                let action = action.clone();
                move |_: &ClickEvent, _, _| action(index)
            })
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                    action(index);
                    cx.stop_propagation();
                } else if event.keystroke.key == "left" && index > 0 {
                    action(index - 1);
                    cx.stop_propagation();
                } else if event.keystroke.key == "right" && index < max_page {
                    action(index + 1);
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .w(px(if selected { 7. } else { 5. }))
                    .h(px(if selected { 7. } else { 5. }))
                    .rounded_full()
                    .bg(if selected { muted } else { accent }),
            )
    });
    let toggles_panel = div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .p(px(18. * s.scale))
        .child(toggle_grid)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .children(pages),
        );
    children.push(toggles_panel.into_any_element());
    if input.toggle_state == LoadState::Unavailable {
        children.push(
            status_card("Quick settings", "Toggle service unavailable", &s).into_any_element(),
        );
    } else if input.toggle_state == LoadState::Loading && input.toggles.is_empty() {
        children
            .push(status_card("Quick settings", "Loading toggle services…", &s).into_any_element());
    }
    if !input.sliders.is_empty() {
        children.push(
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .p(px(12.))
                .children(
                    input
                        .sliders
                        .iter()
                        .map(|v| slider_row(v, &s, input.on_slider.clone())),
                )
                .into_any_element(),
        );
    }
    if input.sliders.iter().any(|v| !v.available) {
        children.push(
            status_card("Brightness or volume", "Control service unavailable", &s)
                .into_any_element(),
        );
    }
    let choices = input
        .audio_choices
        .iter()
        .map(|choice| choice_card(choice, &s, input.on_choose_audio.clone()));
    if input.audio_state == LoadState::Unavailable {
        children
            .push(status_card("Audio devices", "Audio service unavailable", &s).into_any_element());
    } else if input.audio_state == LoadState::Loading && input.audio_choices.is_empty() {
        children
            .push(status_card("Audio devices", "Loading audio devices…", &s).into_any_element());
    } else if !input.audio_choices.is_empty() {
        children.push(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .p(px(12.))
                .children(choices)
                .into_any_element(),
        );
    }
    div()
        .id("waffle.action-center")
        .w(px(358. * s.scale))
        .flex()
        .flex_col()
        .gap(px(12. * s.scale))
        .p(px(12. * s.scale))
        .rounded(px(s.radius))
        .bg(s.background)
        .text_color(s.foreground)
        .border_1()
        .border_color(s.separator)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(6.))
                .child(div().text_lg().child("Quick settings"))
                .child(button("Close action center", "×", s.clone(), Some(close))),
        )
        .children(children)
}

fn toggle_card(toggle: &Toggle, s: &ActionCenterStyle, action: ItemAction) -> impl IntoElement {
    let id = toggle.id.clone();
    let enabled = toggle.enabled.unwrap_or(false);
    let label = toggle.title.clone();
    let available = toggle.available && toggle.enabled.is_some();
    let icon = if toggle.icon.is_empty() {
        "◇".to_owned()
    } else {
        toggle.icon.clone()
    };
    let description = toggle.description.clone();
    let hover = s.hover;
    let accent = s.accent;
    let state = if !toggle.available {
        "Unavailable"
    } else if toggle.enabled.is_none() {
        "Unknown"
    } else if enabled {
        "On"
    } else {
        "Off"
    };
    div()
        .id(format!("waffle.action-center.toggle.{}", toggle.id))
        .w(px(96. * s.scale))
        .min_h(px(82. * s.scale))
        .p(px(10. * s.scale))
        .rounded(px(s.radius))
        .flex()
        .flex_col()
        .justify_between()
        .bg(if enabled { s.panel } else { s.surface })
        .hover(move |st| st.bg(hover))
        .role(Role::Button)
        .aria_label(format!("{label}, {state}"))
        .focusable()
        .tab_stop(true)
        .on_click({
            let action = action.clone();
            let id = id.clone();
            move |_: &ClickEvent, _, _| {
                if available {
                    action(id.clone())
                }
            }
        })
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                if available {
                    action(id.clone())
                }
                cx.stop_propagation();
            }
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_lg()
                        .text_color(if enabled { accent } else { s.muted })
                        .child(icon),
                )
                .child(div().text_xs().text_color(s.muted).child(state)),
        )
        .child(div().text_sm().child(label.clone()))
        .child(
            div()
                .text_xs()
                .text_color(s.muted)
                .child(if description.is_empty() {
                    " ".to_owned()
                } else {
                    description.clone()
                }),
        )
}
fn media_card(
    m: &Media,
    s: &ActionCenterStyle,
    prev: Action,
    play: Action,
    next: Action,
    seek: ValueAction,
    seek_state: Entity<SeekPosition>,
) -> impl IntoElement {
    let title = if m.player.is_none() {
        "Media unavailable".to_owned()
    } else if m.title.is_empty() {
        "Not playing".to_owned()
    } else {
        m.title.clone()
    };
    let duration = m.duration.unwrap_or(0.).max(0.);
    let position = m.position.unwrap_or(0.).clamp(0., duration);
    let can_seek = m.can_seek && duration > 0. && m.position.is_some();
    let track = match (m.position, m.duration) {
        (Some(p), Some(d)) if d > 0. => format!("{} / {}", time(p), time(d)),
        _ => "Track position unavailable".into(),
    };
    let scale = s.scale;
    let panel_width = 358. * scale;
    let track_left_offset = (12. + 16. + 104. + 14.) * scale;
    let track_width = (358. - 12. - 16. - 104. - 14. - 16. - 12.) * scale;
    let seek_bar = if can_seek {
        let mouse_down_seek = seek.clone();
        let mouse_move_seek = seek.clone();
        let keyboard_seek = seek.clone();
        let mouse_state = seek_state.clone();
        let move_state = seek_state.clone();
        let key_state = seek_state.clone();
        div()
            .id("waffle.action-center.media.seek")
            .w_full()
            .h(px(18.))
            .flex()
            .items_center()
            .role(Role::Slider)
            .aria_label("Media position")
            .aria_value(format!("{} of {} seconds", time(position), time(duration)))
            .aria_numeric_value(position as f64)
            .aria_min_numeric_value(0.)
            .aria_max_numeric_value(duration as f64)
            .aria_numeric_value_step(5.)
            .focusable()
            .tab_stop(true)
            .child(
                div()
                    .w_full()
                    .h(px(4.))
                    .rounded_full()
                    .bg(s.separator)
                    .child(
                        div()
                            .w(px(track_width * position / duration.max(1.)))
                            .h(px(4.))
                            .rounded_full()
                            .bg(s.accent),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                move |e: &MouseDownEvent, w: &mut Window, cx| {
                    let next = range_value(
                        e.position.x,
                        w,
                        panel_width,
                        track_left_offset,
                        track_width,
                        duration,
                    );
                    mouse_state.update(cx, |state, cx| {
                        state.value = next;
                        cx.notify();
                    });
                    mouse_down_seek(next);
                },
            )
            .on_mouse_move(move |e: &MouseMoveEvent, w: &mut Window, cx| {
                if e.pressed_button == Some(MouseButton::Left) {
                    let next = range_value(
                        e.position.x,
                        w,
                        panel_width,
                        track_left_offset,
                        track_width,
                        duration,
                    );
                    move_state.update(cx, |state, cx| {
                        state.value = next;
                        cx.notify();
                    });
                    mouse_move_seek(next);
                }
            })
            .on_key_down(move |e: &KeyDownEvent, _, cx| {
                let current = key_state.read(cx).value;
                let next = match e.keystroke.key.as_str() {
                    "left" | "down" => Some((current - 5.).max(0.)),
                    "right" | "up" => Some((current + 5.).min(duration)),
                    "home" => Some(0.),
                    "end" => Some(duration),
                    _ => None,
                };
                if let Some(next) = next {
                    key_state.update(cx, |state, cx| {
                        state.value = next;
                        cx.notify();
                    });
                    keyboard_seek(next);
                    cx.stop_propagation();
                }
            })
            .into_any_element()
    } else if m.can_seek {
        div()
            .text_xs()
            .text_color(s.muted)
            .child("Track position unavailable")
            .into_any_element()
    } else {
        div().child("").into_any_element()
    };
    let player = m
        .player
        .clone()
        .unwrap_or_else(|| "Player unavailable".into());
    let artist = if m.artist.is_empty() {
        "Artist unavailable".into()
    } else {
        m.artist.clone()
    };
    div()
        .w_full()
        .min_h(px(136. * scale))
        .p(px(16. * scale))
        .rounded(px(s.radius))
        .bg(s.panel)
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(14. * scale))
                .child(
                    div()
                        .w(px(104. * scale))
                        .h(px(104. * scale))
                        .rounded(px(s.radius))
                        .bg(s.surface)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_2xl()
                        .text_color(s.muted)
                        .child(m.artwork_label.clone().unwrap_or_else(|| "♫".into())),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .gap(px(6.))
                        .child(div().text_sm().child(title))
                        .child(div().text_xs().text_color(s.muted).child(artist))
                        .child(div().text_xs().text_color(s.muted).child(track))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(button(
                                    "Previous track",
                                    "|◀",
                                    s.clone(),
                                    if m.player.is_some() && m.can_previous {
                                        Some(prev)
                                    } else {
                                        None
                                    },
                                ))
                                .child(button(
                                    "Play or pause",
                                    if m.playing == Some(true) {
                                        "Ⅱ"
                                    } else {
                                        "▶"
                                    },
                                    s.clone(),
                                    if m.player.is_some() && m.playing.is_some() {
                                        Some(play)
                                    } else {
                                        None
                                    },
                                ))
                                .child(button(
                                    "Next track",
                                    "▶|",
                                    s.clone(),
                                    if m.player.is_some() && m.can_next {
                                        Some(next)
                                    } else {
                                        None
                                    },
                                ))
                                .child(div().text_xs().text_color(s.muted).child(player)),
                        )
                        .child(seek_bar),
                ),
        )
}
fn slider_row(v: &Slider, s: &ActionCenterStyle, action: SliderAction) -> gpui::AnyElement {
    let value = v.value.map(|n| n.clamp(0., v.max.max(0.)));
    let pct = value.map(|n| {
        if v.max > 0. {
            (n / v.max * 100.).round() as u32
        } else {
            0
        }
    });
    let label = v.title.clone();
    let max = v.max.max(0.);
    let fill = pct.unwrap_or(0) as f32 / 100.;
    let id = v.id.clone();
    let available = v.available && value.is_some();
    if !available {
        return div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .w(px(22.))
                    .text_lg()
                    .text_color(s.muted)
                    .child(if v.icon.is_empty() {
                        "◉".into()
                    } else {
                        v.icon.clone()
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_col()
                    .child(div().text_sm().child(v.title.clone()))
                    .child(div().text_xs().text_color(s.muted).child("Unavailable")),
            )
            .into_any_element();
    }
    let scale = s.scale;
    let panel_width = 358. * scale;
    let track_left_offset = (12. + 22. + 10.) * scale;
    let track_width = (358. - 12. - 22. - 10. - 12.) * scale;
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().w(px(22.)).text_lg().child(if v.icon.is_empty() {
            "◉".into()
        } else {
            v.icon.clone()
        }))
        .child(
            div()
                .flex()
                .flex_1()
                .flex_col()
                .gap(px(5.))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .child(div().text_sm().child(label.clone()))
                        .child(
                            div().text_xs().text_color(s.muted).child(
                                pct.map(|p| format!("{p}%"))
                                    .unwrap_or_else(|| "Unavailable".into()),
                            ),
                        ),
                )
                .child(
                    div()
                        .id(format!("waffle.action-center.slider.{id}"))
                        .w_full()
                        .h(px(5.))
                        .rounded_full()
                        .bg(s.separator)
                        .child(
                            div()
                                .w(px(track_width * fill))
                                .h(px(5.))
                                .rounded_full()
                                .bg(s.accent),
                        )
                        .role(Role::Slider)
                        .aria_label(label.clone())
                        .aria_value(
                            pct.map(|v| format!("{v}%"))
                                .unwrap_or_else(|| "Unavailable".into()),
                        )
                        .aria_numeric_value(value.unwrap_or(0.) as f64)
                        .aria_min_numeric_value(0.)
                        .aria_max_numeric_value(max as f64)
                        .aria_numeric_value_step((max * 0.05).max(0.01) as f64)
                        .focusable()
                        .tab_stop(true)
                        .on_mouse_down(MouseButton::Left, {
                            let cb = action.clone();
                            let id = id.clone();
                            move |e: &MouseDownEvent, w: &mut Window, _| {
                                if available {
                                    cb(
                                        id.clone(),
                                        range_value(
                                            e.position.x,
                                            w,
                                            panel_width,
                                            track_left_offset,
                                            track_width,
                                            max,
                                        ),
                                    )
                                }
                            }
                        })
                        .on_mouse_move({
                            let cb = action.clone();
                            let id = id.clone();
                            move |e: &MouseMoveEvent, w: &mut Window, _| {
                                if available && e.pressed_button == Some(MouseButton::Left) {
                                    cb(
                                        id.clone(),
                                        range_value(
                                            e.position.x,
                                            w,
                                            panel_width,
                                            track_left_offset,
                                            track_width,
                                            max,
                                        ),
                                    )
                                }
                            }
                        })
                        .on_key_down(move |e: &KeyDownEvent, _, cx| {
                            if available {
                                if let Some(current) = value {
                                    let step = (max * 0.05).max(0.01);
                                    let next = match e.keystroke.key.as_str() {
                                        "left" | "down" => Some((current - step).max(0.)),
                                        "right" | "up" => Some((current + step).min(max)),
                                        "home" => Some(0.),
                                        "end" => Some(max),
                                        _ => None,
                                    };
                                    if let Some(v) = next {
                                        action(id.clone(), v);
                                        cx.stop_propagation();
                                    }
                                }
                            }
                        }),
                ),
        )
        .into_any_element()
}
fn choice_card(c: &Choice, s: &ActionCenterStyle, action: ItemAction) -> impl IntoElement {
    let id = c.id.clone();
    let name = c.label.clone();
    let detail = c.detail.clone();
    let available = c.available;
    let selected = c.selected;
    let hover = s.hover;
    let accent = s.accent;
    let muted = s.muted;
    let state = if !available {
        "unavailable"
    } else if selected {
        "selected"
    } else {
        "not selected"
    };
    let key_action = action.clone();
    div()
        .id(format!("waffle.action-center.choice.{}", id))
        .min_h(px(38.))
        .px(px(10.))
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_between()
        .bg(if selected { s.panel } else { s.surface })
        .hover(move |style| style.bg(hover))
        .role(Role::Button)
        .aria_label(format!("{name}, {state}"))
        .focusable()
        .tab_stop(available)
        .on_click({
            let id = id.clone();
            move |_: &ClickEvent, _, _| {
                if available {
                    action(id.clone())
                }
            }
        })
        .on_key_down(move |e: &KeyDownEvent, _, cx| {
            if available && is_activate(e) {
                key_action(id.clone());
                cx.stop_propagation();
            }
        })
        .child(
            div()
                .flex()
                .flex_col()
                .child(div().text_sm().child(name))
                .when_some(detail, |el, value| {
                    el.child(div().text_xs().text_color(muted).child(value))
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(if available { accent } else { muted })
                .child(if !available {
                    "Unavailable"
                } else if selected {
                    "✓ Selected"
                } else {
                    ""
                }),
        )
}
fn is_activate(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held
}
fn range_value(
    x: gpui::Pixels,
    window: &Window,
    panel_width: f32,
    track_offset: f32,
    track_width: f32,
    max: f32,
) -> f32 {
    let bounds = window.bounds();
    let panel_left = bounds.origin.x + bounds.size.width - px(panel_width);
    let ratio = ((x - (panel_left + px(track_offset))) / px(track_width.max(1.))).clamp(0., 1.);
    ratio * max
}

fn status_card(
    title: &'static str,
    detail: &'static str,
    s: &ActionCenterStyle,
) -> impl IntoElement {
    div()
        .id(format!("waffle.action-center.status.{title}"))
        .w_full()
        .p(px(12.))
        .rounded(px(s.radius))
        .bg(s.panel)
        .role(Role::Status)
        .aria_label(format!("{title}: {detail}"))
        .child(div().text_sm().child(title))
        .child(div().text_xs().text_color(s.muted).child(detail))
}
fn button(
    label: &'static str,
    text: &'static str,
    s: ActionCenterStyle,
    action: Option<Action>,
) -> impl IntoElement {
    let enabled = action.is_some();
    let accessible_name = if enabled {
        label.to_owned()
    } else {
        format!("{label}, unavailable")
    };
    let click_action = action.clone();
    let key_action = action;
    div()
        .id(format!("waffle.action-center.button.{label}"))
        .min_w(px(30.))
        .min_h(px(30.))
        .px(px(6.))
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_center()
        .bg(if enabled { s.surface } else { s.panel })
        .hover({
            let h = s.hover;
            move |st| if enabled { st.bg(h) } else { st }
        })
        .role(Role::Button)
        .aria_label(accessible_name)
        .focusable()
        .tab_stop(enabled)
        .on_click(move |_: &ClickEvent, _, _| {
            if let Some(action) = &click_action {
                action()
            }
        })
        .on_key_down(move |e: &KeyDownEvent, _, cx| {
            if enabled && is_activate(e) {
                if let Some(action) = &key_action {
                    action()
                }
                cx.stop_propagation()
            }
        })
        .child(if enabled { text } else { "·" })
}

fn time(v: f32) -> String {
    let n = v.max(0.) as u64;
    format!("{}:{:02}", n / 60, n % 60)
}
