//! Data-driven quick slider presentation for the II system sidebar.
//!
//! Values and mutations are supplied by the adapter. This component never
//! discovers devices or substitutes values for unavailable controls.

use std::sync::{Arc, Mutex};

use gpui::{
    AnyElement, FontWeight, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    Pixels, Role, ScrollDelta, ScrollWheelEvent, SharedString, StatefulInteractiveElement, canvas,
    div, prelude::*, px, relative,
};

use super::sidebars::SidebarStyle;

pub type SliderValueAction = Arc<dyn Fn(String, f32) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickSlidersState {
    Loading,
    Unavailable,
    Ready,
}

#[derive(Debug, Clone)]
pub struct QuickSliderItem {
    pub id: String,
    pub label: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub enabled: bool,
}

#[derive(Clone)]
pub struct QuickSlidersInput {
    pub state: QuickSlidersState,
    pub message: Option<String>,
    pub sliders: Vec<QuickSliderItem>,
    pub style: SidebarStyle,
    /// Receives `(slider_id, requested_value)` for enabled sliders.
    pub on_value_change: SliderValueAction,
}

impl std::fmt::Debug for QuickSlidersInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QuickSlidersInput")
            .field("state", &self.state)
            .field("message", &self.message)
            .field("sliders", &self.sliders)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

/// Render only supplied values. Empty Ready payloads become an explicit empty
/// state, and the list can scroll without claiming the whole sidebar height.
pub fn render_quick_sliders(input: QuickSlidersInput) -> AnyElement {
    let style = input.style;
    let sliders = input.sliders;
    let state = if input.state == QuickSlidersState::Ready && sliders.is_empty() {
        QuickSlidersState::Ready
    } else {
        input.state
    };
    let empty_ready = state == QuickSlidersState::Ready && sliders.is_empty();
    let fallback = match state {
        QuickSlidersState::Loading => "Loading quick sliders…",
        QuickSlidersState::Unavailable => "Quick sliders are unavailable",
        QuickSlidersState::Ready if empty_ready => "No quick sliders available",
        QuickSlidersState::Ready => "",
    };
    let message = input.message.unwrap_or_else(|| fallback.into());
    let rows = sliders
        .into_iter()
        .map(|slider| render_slider(slider, &style, input.on_value_change.clone()));

    div()
        .id("ii.sidebar.quick-sliders")
        .w_full()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .p(px(style.padding))
        .bg(style.surface)
        .when(state == QuickSlidersState::Ready && !empty_ready, |this| {
            this.overflow_y_scroll().children(rows)
        })
        .when(state != QuickSlidersState::Ready || empty_ready, |this| {
            this.child(
                div()
                    .id("ii.sidebar.quick-sliders.status")
                    .w_full()
                    .text_sm()
                    .text_color(style.muted)
                    .focusable()
                    .tab_stop(false)
                    .role(Role::Status)
                    .aria_label(SharedString::from(message.clone()))
                    .child(message),
            )
        })
        .into_any_element()
}

fn render_slider(
    slider: QuickSliderItem,
    style: &SidebarStyle,
    on_value_change: SliderValueAction,
) -> impl IntoElement {
    let valid_range = slider.min.is_finite()
        && slider.max.is_finite()
        && slider.max > slider.min
        && slider.value.is_finite();
    let enabled = slider.enabled && valid_range;
    let label = slider.label.clone();
    if !enabled {
        return div()
            .id(SharedString::from(format!(
                "ii.sidebar.quick-slider.{}",
                slider.id
            )))
            .w_full()
            .flex()
            .items_center()
            .gap(px(style.gap))
            .p(px(style.padding))
            .rounded(px(style.card_radius))
            .bg(style.card)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(style.muted)
                    .child(label),
            )
            .child(div().text_sm().text_color(style.muted).child("Unavailable"));
    }

    let current = slider.value.clamp(slider.min, slider.max);
    let fill = ((current - slider.min) / (slider.max - slider.min)).clamp(0.0, 1.0);
    let display_value = format_value(current);
    let low = slider.min;
    let high = slider.max;
    let step = ((high - low) / 20.0).max(f32::EPSILON);
    let id = slider.id.clone();
    let track_fill = style.accent;
    let track_background = style.border;
    let gap = style.gap;
    let label_width = 76.0;
    let value_width = 44.0;
    let mut row = div()
        .id(SharedString::from(format!(
            "ii.sidebar.quick-slider.{}",
            slider.id
        )))
        .w_full()
        .flex()
        .items_center()
        .gap(px(gap))
        .p(px(style.padding))
        .rounded(px(style.card_radius))
        .bg(style.card)
        .child(
            div()
                .w(px(label_width))
                .min_w_0()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(style.foreground)
                .child(label.clone()),
        );

    let pointer_bounds: Arc<Mutex<Option<(Pixels, Pixels)>>> = Arc::default();
    let keyboard_callback = on_value_change.clone();
    let mouse_down_callback = on_value_change.clone();
    let mouse_move_callback = on_value_change.clone();
    let wheel_callback = on_value_change;
    let keyboard_id = id.clone();
    let mouse_down_id = id.clone();
    let mouse_move_id = id.clone();
    let wheel_id = id.clone();
    let mouse_down_bounds = pointer_bounds.clone();
    let mouse_move_bounds = pointer_bounds.clone();
    let paint_bounds = pointer_bounds;
    let track = div()
        .id(SharedString::from(format!(
            "ii.sidebar.quick-slider.{}.control",
            slider.id
        )))
        .flex_1()
        .min_w_0()
        .h(px(24.0))
        .relative()
        .flex()
        .items_center()
        .role(Role::Slider)
        .aria_label(label)
        .aria_value(display_value)
        .aria_numeric_value(current as f64)
        .aria_min_numeric_value(low as f64)
        .aria_max_numeric_value(high as f64)
        .aria_numeric_value_step(step as f64)
        .focusable()
        .tab_stop(true)
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            let next = match event.keystroke.key.as_str() {
                "left" | "down" => Some((current - step).max(low)),
                "right" | "up" => Some((current + step).min(high)),
                "pageup" => Some((current + step * 5.0).min(high)),
                "pagedown" => Some((current - step * 5.0).max(low)),
                "home" => Some(low),
                "end" => Some(high),
                _ => None,
            };
            if let Some(value) = next {
                keyboard_callback(keyboard_id.clone(), value);
                cx.stop_propagation();
            }
        })
        .on_mouse_down(MouseButton::Left, {
            move |event: &MouseDownEvent, _, _| {
                if let Some((left, width)) =
                    mouse_down_bounds.lock().ok().and_then(|bounds| *bounds)
                {
                    mouse_down_callback(
                        mouse_down_id.clone(),
                        pointer_value(event.position.x, left, width, low, high),
                    );
                }
            }
        })
        .on_mouse_move({
            move |event: &MouseMoveEvent, _, _| {
                if event.pressed_button == Some(MouseButton::Left)
                    && let Some((left, width)) =
                        mouse_move_bounds.lock().ok().and_then(|bounds| *bounds)
                {
                    mouse_move_callback(
                        mouse_move_id.clone(),
                        pointer_value(event.position.x, left, width, low, high),
                    );
                }
            }
        })
        .on_scroll_wheel(move |event: &ScrollWheelEvent, _, _| {
            let delta = match event.delta {
                ScrollDelta::Pixels(delta) => f32::from(delta.y),
                ScrollDelta::Lines(delta) => delta.y,
            };
            if delta != 0.0 {
                let next = if delta > 0.0 {
                    (current + step).min(high)
                } else {
                    (current - step).max(low)
                };
                wheel_callback(wheel_id.clone(), next);
            }
        })
        .child(
            div()
                .id(SharedString::from(format!(
                    "ii.sidebar.quick-slider.{}.track",
                    slider.id
                )))
                .w_full()
                .h(px(6.0))
                .rounded_full()
                .bg(track_background)
                .child(
                    div()
                        .h(px(6.0))
                        .w(relative(fill))
                        .rounded_full()
                        .bg(track_fill),
                ),
        )
        .child(
            canvas(
                |bounds, _, _| bounds,
                move |bounds, _, _, _| {
                    if let Ok(mut stored) = paint_bounds.lock() {
                        *stored = Some((bounds.origin.x, bounds.size.width));
                    }
                },
            )
            .absolute()
            .inset_0(),
        );
    row = row.child(track).child(
        div()
            .w(px(value_width))
            .text_sm()
            .text_right()
            .text_color(style.muted)
            .child(format_value(current)),
    );
    row
}

fn pointer_value(
    x: gpui::Pixels,
    track_left: Pixels,
    track_width: Pixels,
    min: f32,
    max: f32,
) -> f32 {
    let ratio = ((x - track_left) / track_width.max(px(1.0))).clamp(0.0, 1.0);
    min + ratio * (max - min)
}

fn format_value(value: f32) -> String {
    if value.fract().abs() < 0.001 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}
