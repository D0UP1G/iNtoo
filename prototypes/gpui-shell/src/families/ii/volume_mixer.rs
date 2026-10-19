//! II audio volume dialog content, rendered from an adapter-owned snapshot.
//!
//! The host supplies PipeWire device/app data, appearance colors and callbacks.
//! This component owns only the visible device picker, app rows and empty state.

use std::sync::Arc;

use gpui::{
    AnyElement, FontWeight, Hsla, InteractiveElement, IntoElement, KeyDownEvent, Role,
    SharedString, StatefulInteractiveElement, div, prelude::*, px, relative,
};

pub type Action = Arc<dyn Fn() + Send + Sync>;
pub type IdAction = Arc<dyn Fn(String) + Send + Sync>;
pub type VolumeAction = Arc<dyn Fn(String, f32) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerDataState {
    Ready,
    Loading,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VolumeMixerTab {
    #[default]
    Output,
    Input,
    Music,
}

#[derive(Debug, Clone)]
pub struct VolumeMixerStyle {
    pub surface: Hsla,
    pub elevated: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub danger: Hsla,
    pub border: Hsla,
    pub radius: f32,
    pub small_radius: f32,
    pub spacing: f32,
}

#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    /// `None` means the provider has not identified the default route.
    pub selected: Option<bool>,
    pub available: bool,
}

#[derive(Debug, Clone)]
pub struct AppStream {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub volume: Option<f32>,
    pub max_volume: f32,
    pub muted: Option<bool>,
    pub available: bool,
}

#[derive(Clone)]
pub struct VolumeMixerInput {
    pub selected_tab: VolumeMixerTab,
    pub dialog_shown: bool,
    pub current_device_name: Option<String>,
    pub devices: Vec<AudioDevice>,
    pub device_picker_open: bool,
    pub streams: Vec<AppStream>,
    pub streams_state: MixerDataState,
    pub streams_status_message: Option<String>,
    /// When true, the host has no PipeWire writer. Read-only snapshots remain
    /// visible, while device routing, volume steps, and mute controls are disabled.
    pub read_only: bool,
    pub style: VolumeMixerStyle,
    pub on_close: Action,
    pub on_select_tab: Arc<dyn Fn(VolumeMixerTab) + Send + Sync>,
    pub on_toggle_device_picker: Action,
    pub on_select_device: Option<IdAction>,
    pub on_set_volume: Option<VolumeAction>,
    pub on_toggle_mute: Option<IdAction>,
}

/// Renders the content from `VolumeDialogContent.qml` and `VolumeMixerEntry.qml`.
/// Device selection and live PipeWire updates are driven by host callbacks.
pub fn render(input: VolumeMixerInput) -> AnyElement {
    if !input.dialog_shown {
        return div().size_full().into_any_element();
    }

    let style = input.style.clone();
    let is_output = input.selected_tab == VolumeMixerTab::Output;
    let title = match input.selected_tab {
        VolumeMixerTab::Output => "Audio output",
        VolumeMixerTab::Input => "Audio input",
        VolumeMixerTab::Music => "Music volume",
    };
    let close_click = input.on_close.clone();
    let close_key = input.on_close.clone();
    let device_title = input.current_device_name.clone().unwrap_or_else(|| {
        if is_output {
            "Default output unknown".into()
        } else {
            "Default input unknown".into()
        }
    });
    let toggle = input.on_toggle_device_picker.clone();
    let picker = if input.device_picker_open && input.selected_tab != VolumeMixerTab::Music {
        Some(
            div()
                .id("ii.volume_mixer.devices")
                .w_full()
                .max_h(px(250.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(4.))
                .p(px(style.spacing / 2.))
                .bg(style.elevated)
                .border_1()
                .border_color(style.border)
                .rounded(px(style.radius))
                .when(input.devices.is_empty(), |picker| {
                    picker.child(picker_state(
                        if is_output {
                            "No output devices"
                        } else {
                            "No input devices"
                        },
                        "No devices were supplied by the host",
                        &style,
                    ))
                })
                .children(input.devices.iter().map(|device| {
                    let id = device.id.clone();
                    let key_id = id.clone();
                    let click = input.on_select_device.clone();
                    let key = input.on_select_device.clone();
                    let label = device.name.clone();
                    let enabled = device.available && !input.read_only && click.is_some();
                    let selection_label = match device.selected {
                        Some(true) => "selected",
                        Some(false) => "not selected",
                        None => "default route unknown",
                    };
                    div()
                        .id(SharedString::from(format!("ii.volume_mixer.device.{id}")))
                        .role(Role::Button)
                        .w_full()
                        .min_h(px(44.))
                        .px(px(style.spacing))
                        .flex()
                        .items_center()
                        .gap(px(style.spacing))
                        .rounded(px(style.small_radius))
                        .bg(if device.selected == Some(true) {
                            style.selected
                        } else {
                            style.elevated
                        })
                        .text_color(if enabled {
                            style.foreground
                        } else {
                            style.muted
                        })
                        .aria_label(SharedString::from(if enabled {
                            format!("Select audio device {label}, {selection_label}")
                        } else {
                            format!(
                                "Audio device {label}, {selection_label}, selection unavailable"
                            )
                        }))
                        .focusable()
                        .tab_stop(enabled)
                        .focus_visible({
                            let border = style.border;
                            move |s| s.border_1().border_color(border)
                        })
                        .hover({
                            let hover = style.hover;
                            move |s| s.bg(hover)
                        })
                        .when(enabled, |this| {
                            this.on_click(move |_, _, _| {
                                if let Some(action) = &click {
                                    action(id.clone());
                                }
                            })
                        })
                        .on_key_down(move |event: &KeyDownEvent, _, _| {
                            if enabled && activate(event) {
                                if let Some(action) = &key {
                                    action(key_id.clone());
                                }
                            }
                        })
                        .child(div().w(px(20.)).text_color(style.accent).child(
                            if device.selected == Some(true) {
                                "✓"
                            } else if is_output {
                                "◖"
                            } else {
                                "●"
                            },
                        ))
                        .child(div().flex_1().child(label))
                })),
        )
    } else {
        None
    };

    let stream_content: AnyElement = if input.selected_tab == VolumeMixerTab::Music {
        mixer_state_card(
            "Music volume unavailable",
            "MPRIS volume control is not connected in this prototype",
            &style,
        )
    } else {
        match input.streams_state {
            MixerDataState::Loading => mixer_state_card(
                "Loading audio streams",
                input
                    .streams_status_message
                    .as_deref()
                    .unwrap_or("Waiting for the audio provider"),
                &style,
            ),
            MixerDataState::Unavailable => mixer_state_card(
                "Audio streams unavailable",
                input
                    .streams_status_message
                    .as_deref()
                    .unwrap_or("No audio stream provider is connected"),
                &style,
            ),
            MixerDataState::Error => mixer_state_card(
                "Unable to load audio streams",
                input
                    .streams_status_message
                    .as_deref()
                    .unwrap_or("The audio provider returned an error"),
                &style,
            ),
            MixerDataState::Ready if input.streams.is_empty() => mixer_state_card(
                if is_output {
                    "No apps playing audio"
                } else {
                    "No apps using microphone"
                },
                "The provider returned no active streams",
                &style,
            ),
            MixerDataState::Ready => div()
                .id("ii.volume_mixer.streams")
                .w_full()
                .flex_1()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(style.spacing / 2.))
                .children(input.streams.iter().map(|stream| {
                    render_stream(
                        stream,
                        &style,
                        input.read_only,
                        input.on_set_volume.clone(),
                        input.on_toggle_mute.clone(),
                    )
                }))
                .into_any_element(),
        }
    };

    div()
        .id("ii.volume_mixer.content")
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .gap(px(style.spacing))
        .text_color(style.foreground)
        .child(
            div()
                .id("ii.volume_mixer.titlebar")
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .id("ii.volume_mixer.close")
                        .role(Role::Button)
                        .size(px(32.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(style.small_radius))
                        .text_color(style.muted)
                        .aria_label("Done")
                        .focusable()
                        .tab_stop(true)
                        .cursor_pointer()
                        .hover({
                            let hover = style.hover;
                            move |s| s.bg(hover)
                        })
                        .on_click(move |_, _, _| close_click())
                        .on_key_down(move |event: &KeyDownEvent, _, cx| {
                            if activate(event) {
                                close_key();
                                cx.stop_propagation();
                            }
                        })
                        .child("×"),
                ),
        )
        .child(
            div()
                .id("ii.volume_mixer.separator")
                .w_full()
                .h(px(1.))
                .bg(style.border),
        )
        .when(
            input.read_only && input.selected_tab != VolumeMixerTab::Music,
            |root| {
                root.child(
                    div()
                        .id("ii.volume_mixer.read_only")
                        .w_full()
                        .px(px(style.spacing))
                        .py(px(6.))
                        .rounded(px(style.small_radius))
                        .bg(style.elevated)
                        .text_xs()
                        .text_color(style.muted)
                        .child("PipeWire snapshot · controls are read-only"),
                )
            },
        )
        .child(render_tabs(
            input.selected_tab,
            &style,
            input.on_select_tab.clone(),
        ))
        .when(input.selected_tab != VolumeMixerTab::Music, |root| {
            root.child(
                div()
                    .id("ii.volume_mixer.device_picker")
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(button(
                        is_output,
                        input.device_picker_open,
                        "ii.volume_mixer.device_button",
                        if is_output {
                            "Show output devices; default output unknown"
                        } else {
                            "Show input devices; default input unknown"
                        },
                        device_title,
                        &style,
                        move || toggle(),
                    ))
                    .children(picker),
            )
        })
        .child(stream_content)
        .into_any_element()
}

fn render_tabs(
    selected: VolumeMixerTab,
    style: &VolumeMixerStyle,
    on_select: Arc<dyn Fn(VolumeMixerTab) + Send + Sync>,
) -> impl IntoElement {
    let tabs = [
        (VolumeMixerTab::Output, "Output"),
        (VolumeMixerTab::Input, "Input"),
        (VolumeMixerTab::Music, "Music"),
    ];
    div()
        .id("ii.volume_mixer.tabs")
        .w_full()
        .flex()
        .gap(px(4.))
        .children(tabs.into_iter().map(|(tab, label)| {
            let active = selected == tab;
            let click = on_select.clone();
            let key = on_select.clone();
            div()
                .id(SharedString::from(format!("ii.volume_mixer.tab.{label}")))
                .role(Role::Tab)
                .flex_1()
                .min_h(px(38.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(style.small_radius))
                .bg(if active {
                    style.selected
                } else {
                    style.surface
                })
                .text_color(if active {
                    style.foreground
                } else {
                    style.muted
                })
                .aria_label(format!("{label} tab"))
                .aria_selected(active)
                .focusable()
                .tab_stop(true)
                .cursor_pointer()
                .on_click(move |_, _, _| click(tab))
                .on_key_down(move |event: &KeyDownEvent, _, cx| {
                    if activate(event) {
                        key(tab);
                        cx.stop_propagation();
                    }
                })
                .child(label)
        }))
}

fn mixer_state_card(title: &str, message: &str, style: &VolumeMixerStyle) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "ii.volume_mixer.streams.state.{title}"
        )))
        .role(Role::Status)
        .aria_label(format!("{title}: {message}"))
        .w_full()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .px(px(style.spacing * 2.))
        .text_sm()
        .text_color(style.muted)
        .child(title.to_owned())
        .child(div().text_xs().child(message.to_owned()))
        .into_any_element()
}

fn render_stream(
    stream: &AppStream,
    style: &VolumeMixerStyle,
    read_only: bool,
    set_volume: Option<VolumeAction>,
    toggle_mute: Option<IdAction>,
) -> AnyElement {
    if !stream.available {
        return unavailable_stream_card(stream, style);
    }
    let id = stream.id.clone();
    let label = stream.name.clone();
    let volume_text = stream
        .volume
        .map(|volume| format!("{}%", (volume * 100.0).round() as i32))
        .unwrap_or_else(|| "Volume unknown".into());
    let normalized = stream.volume.map(|volume| {
        if stream.max_volume > 0.0 {
            (volume / stream.max_volume).clamp(0.0, 1.0)
        } else {
            0.0
        }
    });
    let minus_id = id.clone();
    let plus_id = id.clone();
    let minus_action = set_volume.clone();
    let plus_action = set_volume.clone();
    let mute_id_click = id.clone();
    let mute_id_key = id.clone();
    let mute_click = toggle_mute.clone();
    let enabled = !read_only && set_volume.is_some() && toggle_mute.is_some();
    let mute_key = toggle_mute;
    let next_down = stream.volume.map(|volume| (volume - 0.05).max(0.0));
    let next_up = stream
        .volume
        .map(|volume| (volume + 0.05).min(stream.max_volume.max(1.0)));
    div()
        .id(SharedString::from(format!("ii.volume_mixer.stream.{id}")))
        .role(Role::Group)
        .aria_label(format!(
            "{}, {}{}",
            stream.name,
            volume_text,
            match stream.muted {
                Some(true) => ", muted",
                Some(false) => ", not muted",
                None => ", mute state unknown",
            }
        ))
        .w_full()
        .min_h(px(76.))
        .p(px(style.spacing))
        .flex()
        .items_center()
        .gap(px(style.spacing))
        .bg(style.surface)
        .rounded(px(style.small_radius))
        .child(
            div()
                .size(px(40.))
                .rounded(px(style.small_radius))
                .bg(style.elevated)
                .flex()
                .items_center()
                .justify_center()
                .text_color(style.accent)
                .child(stream.icon.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(div().text_sm().child(label)),
                        )
                        .child(div().text_xs().text_color(style.muted).child(volume_text)),
                )
                .when_some(normalized, |content, bar_fill| {
                    content.child(
                        div()
                            .w_full()
                            .h(px(5.))
                            .rounded(px(3.))
                            .bg(style.elevated)
                            .when_some(Some(bar_fill), |track, bar_fill| {
                                track.child(
                                    div()
                                        .h_full()
                                        .w(relative(bar_fill.clamp(0.0, 1.0)))
                                        .rounded(px(3.))
                                        .bg(style.accent),
                                )
                            }),
                    )
                })
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(step_button(
                            format!("{id}.down"),
                            "Decrease volume",
                            "−",
                            style,
                            enabled,
                            move || {
                                if let Some(action) = &minus_action {
                                    if let Some(value) = next_down {
                                        action(minus_id.clone(), value);
                                    }
                                }
                            },
                        ))
                        .child(step_button(
                            format!("{id}.up"),
                            "Increase volume",
                            "+",
                            style,
                            enabled,
                            move || {
                                if let Some(action) = &plus_action {
                                    if let Some(value) = next_up {
                                        action(plus_id.clone(), value);
                                    }
                                }
                            },
                        )),
                ),
        )
        .child(mute_button(
            format!("{id}.mute"),
            if stream.muted == Some(true) {
                "Unmute stream"
            } else if stream.muted == Some(false) {
                "Mute stream"
            } else {
                "Mute state unknown"
            },
            if stream.muted == Some(true) {
                "◌"
            } else {
                "◖"
            },
            style,
            enabled,
            move || {
                if let Some(action) = &mute_click {
                    action(mute_id_click.clone());
                }
            },
            move || {
                if let Some(action) = &mute_key {
                    action(mute_id_key.clone());
                }
            },
        ))
        .into_any_element()
}

fn unavailable_stream_card(stream: &AppStream, style: &VolumeMixerStyle) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "ii.volume_mixer.stream.{}.unavailable",
            stream.id
        )))
        .role(Role::Group)
        .aria_label(format!("{}: unavailable", stream.name))
        .w_full()
        .min_h(px(76.))
        .p(px(style.spacing))
        .flex()
        .items_center()
        .gap(px(style.spacing))
        .bg(style.surface)
        .rounded(px(style.small_radius))
        .child(
            div()
                .size(px(40.))
                .rounded(px(style.small_radius))
                .bg(style.elevated)
                .flex()
                .items_center()
                .justify_center()
                .text_color(style.muted)
                .child(stream.icon.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().text_sm().child(stream.name.clone()))
                .child(div().text_xs().text_color(style.muted).child("Unavailable")),
        )
        .into_any_element()
}

fn button(
    is_output: bool,
    is_open: bool,
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    text: impl Into<SharedString>,
    style: &VolumeMixerStyle,
    action: impl Fn() + 'static,
) -> impl IntoElement {
    let border = style.border;
    let hover = style.hover;
    let id = id.into();
    let label = label.into();
    let text = text.into();
    let click_action = Arc::new(action);
    let key_action = click_action.clone();
    div()
        .id(id)
        .role(Role::Button)
        .w_full()
        .min_h(px(44.))
        .px(px(style.spacing))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(style.spacing))
        .rounded(px(style.small_radius))
        .bg(style.elevated)
        .text_color(style.foreground)
        .aria_label(label)
        .focusable()
        .tab_stop(true)
        .focus_visible(move |s| s.border_1().border_color(border))
        .hover(move |s| s.bg(hover))
        .on_click(move |_, _, _| click_action())
        .on_key_down(move |event: &KeyDownEvent, _, _| {
            if activate(event) {
                key_action();
            }
        })
        .child(
            div()
                .w(px(24.))
                .text_lg()
                .text_color(style.accent)
                .child(if is_output { "◖" } else { "●" }),
        )
        .child(div().flex_1().min_w_0().child(text))
        .child(
            div()
                .text_color(style.muted)
                .child(if is_open { "⌃" } else { "⌄" }),
        )
}

fn picker_state(title: &str, message: &str, style: &VolumeMixerStyle) -> AnyElement {
    div()
        .id("ii.volume_mixer.devices.empty")
        .role(Role::Status)
        .aria_label(format!("{title}: {message}"))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(3.))
        .rounded(px(style.small_radius))
        .bg(style.surface)
        .px(px(style.spacing))
        .py(px(style.spacing / 2.))
        .child(div().text_sm().child(title.to_owned()))
        .child(
            div()
                .text_xs()
                .text_color(style.muted)
                .child(message.to_owned()),
        )
        .into_any_element()
}

fn step_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    text: &'static str,
    style: &VolumeMixerStyle,
    enabled: bool,
    action: impl Fn() + 'static,
) -> impl IntoElement {
    let border = style.border;
    let hover = style.hover;
    let id = id.into();
    let label = label.into();
    let click_action = Arc::new(action);
    let key_action = click_action.clone();
    div()
        .id(id)
        .role(Role::Button)
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(14.))
        .text_color(if enabled {
            style.foreground
        } else {
            style.muted
        })
        .aria_label(label)
        .focusable()
        .tab_stop(enabled)
        .focus_visible(move |s| s.border_1().border_color(border))
        .hover(move |s| s.bg(hover))
        .when(enabled, |this| this.on_click(move |_, _, _| click_action()))
        .on_key_down(move |event: &KeyDownEvent, _, _| {
            if enabled && activate(event) {
                key_action();
            }
        })
        .child(text)
}

fn mute_button(
    id: impl Into<SharedString>,
    label: &'static str,
    text: &'static str,
    style: &VolumeMixerStyle,
    enabled: bool,
    click: impl Fn() + 'static,
    key: impl Fn() + 'static,
) -> impl IntoElement {
    let border = style.border;
    let hover = style.hover;
    let id = id.into();
    div()
        .id(id)
        .role(Role::Button)
        .size(px(36.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(18.))
        .bg(if label == "Mute stream" {
            style.elevated
        } else {
            style.danger
        })
        .text_color(if enabled {
            style.foreground
        } else {
            style.muted
        })
        .aria_label(label)
        .focusable()
        .tab_stop(enabled)
        .focus_visible(move |s| s.border_1().border_color(border))
        .hover(move |s| s.bg(hover))
        .when(enabled, |this| this.on_click(move |_, _, _| click()))
        .on_key_down(move |event: &KeyDownEvent, _, _| {
            if enabled && activate(event) {
                key();
            }
        })
        .child(text)
}

fn activate(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held
}
