//! Output-sized transparent host for dock-positioned media controls.
//!
//! The surface owns only outside-click dismissal and placement. The wrapped
//! popup continues to own its card presentation and keyboard behavior.

use std::rc::Rc;

use gpui::{Context, IntoElement, Render, Window, div, prelude::*, px, rgba};

use crate::components::media_controls_popup::{
    MediaControlsPopupProps, render_media_controls_popup,
};
use crate::components::vertical_bar::MediaState;

pub type MediaSurfaceDismissAction = Rc<dyn Fn()>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaControlsDockEdge {
    Top,
    Right,
    Bottom,
    Left,
}

/// Margins from the corresponding output edges, in GPUI logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MediaControlsMargins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Clone)]
pub struct MediaControlsSurfaceProps {
    pub output_name: String,
    pub dock_edge: MediaControlsDockEdge,
    pub margins: MediaControlsMargins,
    pub popup: MediaControlsPopupProps,
    pub on_dismiss: MediaSurfaceDismissAction,
}

pub struct MediaControlsSurface {
    pub props: MediaControlsSurfaceProps,
    did_focus: bool,
}

impl MediaControlsSurface {
    pub fn new(props: MediaControlsSurfaceProps) -> Self {
        Self {
            props,
            did_focus: false,
        }
    }

    pub fn set_media_state(&mut self, media: MediaState, cx: &mut Context<Self>) {
        self.props.popup.height_px = crate::components::media_controls_popup::card_stack_height(
            self.props.popup.width_px,
            self.props.popup.height_px,
            media.players.len(),
        );
        self.props.popup.media = Some(media);
        cx.notify();
    }
}

impl Render for MediaControlsSurface {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.did_focus {
            self.props.popup.focus_handle.focus(window, cx);
            self.did_focus = true;
        }
        let output_size = window.bounds().size;
        let output_width = output_size.width.as_f32();
        let output_height = output_size.height.as_f32();
        let (card_width, card_height) = fit_card_size(
            output_width,
            output_height,
            self.props.popup.width_px,
            self.props.popup.height_px,
        );
        let (left, top) = card_position(
            output_width,
            output_height,
            card_width,
            card_height,
            self.props.dock_edge,
            self.props.margins,
        );

        let mut popup = self.props.popup.clone();
        popup.width_px = card_width;
        popup.height_px = card_height;

        let dismiss = self.props.on_dismiss.clone();
        let root_id = format!("ii.media-controls.surface.{}", self.props.output_name);
        div()
            .id(root_id)
            .size_full()
            .bg(rgba(0x00000000))
            .on_any_mouse_down(move |_, _, _| dismiss())
            .child(
                div()
                    .id("ii.media-controls.surface.card-hit-area")
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                    .child(render_media_controls_popup(popup)),
            )
    }
}

/// Keep the card within the output while preserving its preferred size.
fn fit_card_size(
    output_width: f32,
    output_height: f32,
    card_width: f32,
    card_height: f32,
) -> (f32, f32) {
    let output_width = finite_non_negative(output_width);
    let output_height = finite_non_negative(output_height);
    (
        finite_non_negative(card_width).min(output_width),
        finite_non_negative(card_height).min(output_height),
    )
}

/// Place a card against the selected output edge and center it on the other axis.
fn card_position(
    output_width: f32,
    output_height: f32,
    card_width: f32,
    card_height: f32,
    edge: MediaControlsDockEdge,
    margins: MediaControlsMargins,
) -> (f32, f32) {
    let output_width = finite_non_negative(output_width);
    let output_height = finite_non_negative(output_height);
    let card_width = finite_non_negative(card_width).min(output_width);
    let card_height = finite_non_negative(card_height).min(output_height);
    let left_margin = finite_non_negative(margins.left);
    let right_margin = finite_non_negative(margins.right);
    let top_margin = finite_non_negative(margins.top);
    let bottom_margin = finite_non_negative(margins.bottom);

    match edge {
        MediaControlsDockEdge::Top => (
            ((output_width - card_width) / 2.0).max(0.0),
            top_margin.min(output_height - card_height),
        ),
        MediaControlsDockEdge::Right => (
            (output_width - card_width - right_margin).max(0.0),
            ((output_height - card_height) / 2.0).max(0.0),
        ),
        MediaControlsDockEdge::Bottom => (
            ((output_width - card_width) / 2.0).max(0.0),
            (output_height - card_height - bottom_margin).max(0.0),
        ),
        MediaControlsDockEdge::Left => (
            left_margin.min(output_width - card_width),
            ((output_height - card_height) / 2.0).max(0.0),
        ),
    }
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_size_is_clamped_to_valid_output_dimensions() {
        assert_eq!(fit_card_size(1280.0, 720.0, 480.0, 212.0), (480.0, 212.0));
        assert_eq!(fit_card_size(300.0, 100.0, 480.0, 212.0), (300.0, 100.0));
        assert_eq!(fit_card_size(f32::NAN, 100.0, 480.0, -3.0), (0.0, 0.0));
    }

    #[test]
    fn card_position_respects_dock_edge_and_margins() {
        let margins = MediaControlsMargins {
            top: 12.0,
            right: 18.0,
            bottom: 24.0,
            left: 30.0,
        };
        assert_eq!(
            card_position(
                1280.0,
                720.0,
                480.0,
                212.0,
                MediaControlsDockEdge::Top,
                margins
            ),
            (400.0, 12.0)
        );
        assert_eq!(
            card_position(
                1280.0,
                720.0,
                480.0,
                212.0,
                MediaControlsDockEdge::Right,
                margins
            ),
            (782.0, 254.0)
        );
        assert_eq!(
            card_position(
                1280.0,
                720.0,
                480.0,
                212.0,
                MediaControlsDockEdge::Bottom,
                margins
            ),
            (400.0, 484.0)
        );
        assert_eq!(
            card_position(
                1280.0,
                720.0,
                480.0,
                212.0,
                MediaControlsDockEdge::Left,
                margins
            ),
            (30.0, 254.0)
        );
    }

    #[test]
    fn oversized_margins_and_cards_never_place_outside_output() {
        assert_eq!(
            card_position(
                300.0,
                100.0,
                500.0,
                120.0,
                MediaControlsDockEdge::Top,
                MediaControlsMargins {
                    top: f32::INFINITY,
                    ..Default::default()
                }
            ),
            (0.0, 0.0)
        );
        assert_eq!(
            card_position(
                300.0,
                100.0,
                200.0,
                80.0,
                MediaControlsDockEdge::Left,
                MediaControlsMargins {
                    left: 500.0,
                    ..Default::default()
                }
            ),
            (100.0, 10.0)
        );
    }
}
