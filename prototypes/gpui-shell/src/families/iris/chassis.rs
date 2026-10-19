//! Island family resting chassis and taskbar composition.
//!
//! This is a presentation-only view. Runtime adapters provide the active
//! page, clock, activity labels and configured edge pieces. The QML Iris
//! composition has substantially more state (glass-field masks, satellites,
//! docking, morph origins and page content); this component is a first visual
//! port boundary and deliberately does not claim pixel or behavior parity.

use crate::theme::ThemeTokens;
use gpui::{Hsla, IntoElement, div, prelude::*, px};

#[path = "style_tokens.rs"]
mod style_tokens;
use style_tokens::IrisPalette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IrisEdge {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl IrisEdge {
    fn vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IslandPresentation {
    #[default]
    Rest,
    Expanded,
    /// Closing keeps the expanded page mounted while its content fades and
    /// the chassis contracts. `transition` is 1 at expanded and 0 at rest.
    Closing,
}

#[derive(Debug, Clone)]
pub struct IrisPiece {
    /// Stable id from IrisPieces (for example `media`, `sound`, or `timer`).
    pub id: String,
    pub label: String,
    pub glyph: String,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct IrisChassisStyle {
    pub body: Hsla,
    pub body_expanded: Hsla,
    pub piece: Hsla,
    pub piece_active: Hsla,
    pub foreground: Hsla,
    pub muted_foreground: Hsla,
    pub accent: Hsla,
    pub outline: Hsla,
    pub rest_width: f32,
    pub rest_height: f32,
    pub bar_height: f32,
    pub expanded_width: f32,
    pub expanded_height: f32,
    pub corner_radius: f32,
    pub piece_radius: f32,
    pub gap: f32,
    pub padding: f32,
    pub density: f32,
}

impl Default for IrisChassisStyle {
    fn default() -> Self {
        // Neutral preview values only. The IrisStyle/QML theme resolver must
        // supply production material, geometry, density and family overrides.
        Self {
            body: gpui::rgba(0x202126f2).into(),
            body_expanded: gpui::rgba(0x202126f8).into(),
            piece: gpui::rgba(0x32343bee).into(),
            piece_active: gpui::rgba(0x4b455dee).into(),
            foreground: gpui::rgba(0xf5f3f8ff).into(),
            muted_foreground: gpui::rgba(0xb5b2bcff).into(),
            accent: gpui::rgba(0xc4a7ffff).into(),
            outline: gpui::rgba(0x77717f99).into(),
            rest_width: 204.0,
            rest_height: 44.0,
            bar_height: 48.0,
            expanded_width: 440.0,
            expanded_height: 300.0,
            corner_radius: 24.0,
            piece_radius: 22.0,
            gap: 8.0,
            padding: 14.0,
            density: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IrisChassisGeometry {
    /// Runtime-derived compact target width; QML computes this from visible
    /// modules, so the adapter must provide it instead of guessing.
    pub rest_width: f32,
    /// Runtime-derived expanded width and content height.
    pub expanded_width: f32,
    pub expanded_height: f32,
    pub bar_height: f32,
}

impl IrisChassisStyle {
    /// Build production colors and geometric scale from resolved Iris tokens.
    /// Dynamic geometry is supplied by the family runtime just as QML derives
    /// it from the active page and modules.
    pub fn from_theme(theme: &ThemeTokens, geometry: IrisChassisGeometry) -> Option<Self> {
        let palette = IrisPalette::from_theme(theme)?;
        let density = theme.shape.density.clamp(0.8, 1.35);
        let transparent_clip = gpui::rgba(0x00000001).into();
        Some(Self {
            // IrisIsland paints a 0.004-alpha clipping body; the actual
            // material/backdrop belongs to IrisFrame's field renderer.
            body: transparent_clip,
            body_expanded: transparent_clip,
            piece: palette.fill,
            piece_active: palette.fill_active,
            foreground: palette.text,
            muted_foreground: palette.preset.subtext,
            accent: palette.accent,
            outline: gpui::rgba(0x00000000).into(),
            rest_width: geometry.rest_width,
            rest_height: 42.0,
            bar_height: geometry.bar_height,
            expanded_width: geometry.expanded_width,
            expanded_height: geometry.expanded_height,
            corner_radius: 21.0,
            piece_radius: 21.0,
            gap: 8.0,
            padding: 20.0,
            density,
        })
    }
}

#[derive(Debug, Clone)]
pub struct IrisChassisInput {
    pub style: IrisChassisStyle,
    pub edge: IrisEdge,
    pub presentation: IslandPresentation,
    /// Transition amount from compact (0) to expanded (1). For Closing this
    /// decreases from 1 to 0 as supplied by the host animation clock.
    pub transition: f32,
    pub notch: bool,
    pub full_width: bool,
    pub clock: Option<String>,
    pub date: Option<String>,
    pub primary_activity: Option<String>,
    pub secondary_activity: Option<String>,
    pub page_title: Option<String>,
    pub page_detail: Option<String>,
    pub leading_pieces: Vec<IrisPiece>,
    pub trailing_pieces: Vec<IrisPiece>,
}

/// Compose the Iris edge pieces, Island chassis, and optional full-width bar.
/// Edge/output anchoring stays with the layer-shell host. Pass fully resolved
/// family tokens from the `IrisStyle` adapter; this function owns no globals.
pub fn render(input: IrisChassisInput) -> impl IntoElement {
    let style = input.style.clone();
    let transition = input.transition.clamp(0.0, 1.0);
    let expanded = input.presentation != IslandPresentation::Rest;
    let vertical = input.edge.vertical();
    let bar_height = px(style.bar_height * style.density.max(0.1));
    let body_width = if expanded {
        style.rest_width + (style.expanded_width - style.rest_width) * transition
    } else {
        style.rest_width
    } * style.density.max(0.1);
    let body_height = if expanded {
        style.rest_height + (style.expanded_height - style.rest_height) * transition
    } else {
        style.rest_height
    } * style.density.max(0.1);

    let leading = input
        .leading_pieces
        .iter()
        .map(|piece| render_piece(piece, &style));
    let trailing = input
        .trailing_pieces
        .iter()
        .map(|piece| render_piece(piece, &style));
    let core = render_island_core(&input, &style, transition, body_width, body_height);

    let island = if vertical {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(style.gap * style.density))
            .children(leading)
            .child(core)
            .children(trailing)
    } else {
        div()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(style.gap * style.density))
            .children(leading)
            .child(core)
            .children(trailing)
    };

    if input.full_width && !vertical {
        div()
            .w_full()
            .h(bar_height)
            .flex()
            .items_center()
            .justify_center()
            .px(px(style.padding * style.density))
            .bg(style.body)
            .border_b_1()
            .border_color(style.outline)
            .child(island)
    } else {
        // `notch` removes the outer breathing room: host alignment can then
        // melt the Island into its screen edge. Floating mode keeps the gap.
        let outer_gap = if input.notch {
            0.0
        } else {
            style.gap * style.density
        };
        div()
            .flex()
            .items_center()
            .justify_center()
            .px(px(outer_gap))
            .py(px(outer_gap))
            .child(island)
    }
}

fn render_island_core(
    input: &IrisChassisInput,
    style: &IrisChassisStyle,
    transition: f32,
    body_width: f32,
    body_height: f32,
) -> impl IntoElement {
    let clock = input.clock.clone().unwrap_or_default();
    let date = input.date.clone().unwrap_or_default();
    let primary = input.primary_activity.clone();
    let compact_primary = primary.clone();
    let secondary = input.secondary_activity.clone();
    let compact_secondary = secondary.clone();
    let title = input.page_title.clone();
    let detail = input.page_detail.clone();
    let is_expanded = input.presentation != IslandPresentation::Rest;
    let background = if is_expanded {
        style.body_expanded
    } else {
        style.body
    };
    let open_radius = 30.0 * style.density;
    let radius = (style.corner_radius + (open_radius - style.corner_radius) * transition)
        .min(body_height / 2.0);

    let compact_contents = div()
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(style.gap * style.density))
        .child(
            div()
                .w(px(7.0 * style.density))
                .h(px(7.0 * style.density))
                .rounded_full()
                .bg(style.accent),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .text_color(style.foreground)
                .child(compact_primary.unwrap_or_else(|| clock.clone()))
                .when(!date.is_empty(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(style.muted_foreground)
                            .child(date),
                    )
                }),
        )
        .when(compact_secondary.is_some(), |this| {
            this.child(
                div()
                    .w(px(4.0 * style.density))
                    .h(px(4.0 * style.density))
                    .rounded_full()
                    .bg(style.muted_foreground),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(style.muted_foreground)
                    .child(compact_secondary.unwrap_or_default()),
            )
        });

    let expanded_contents = div()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .gap(px(style.gap * style.density))
        .px(px(style.padding * style.density))
        .py(px(style.padding * style.density))
        .text_color(style.foreground)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .child(title.unwrap_or_else(|| "Island".into())),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(style.muted_foreground)
                        .child(clock),
                ),
        )
        .child(div().h(px(1.0)).w_full().bg(style.outline))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(style.gap * style.density))
                .child(
                    div()
                        .text_color(style.foreground)
                        .child(primary.unwrap_or_else(|| "Ready".into())),
                )
                .when(detail.is_some(), |this| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(style.muted_foreground)
                            .child(detail.unwrap_or_default()),
                    )
                }),
        );

    div()
        .id("iris.chassis")
        .w(px(body_width.max(1.0)))
        .h(px(body_height.max(1.0)))
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded(px(radius))
        .bg(background)
        .border_1()
        .border_color(style.outline)
        .when(transition < 0.999, |this| this.child(compact_contents))
        .when(transition > 0.001, |this| this.child(expanded_contents))
}

fn render_piece(piece: &IrisPiece, style: &IrisChassisStyle) -> impl IntoElement {
    let background = if piece.active {
        style.piece_active
    } else {
        style.piece
    };
    let label: gpui::SharedString = format!("{} {}", piece.glyph, piece.label).into();
    div()
        .id(format!("iris.piece.{}", piece.id))
        .min_w(px(style.rest_height * style.density))
        .h(px(style.rest_height * style.density))
        .px(px(style.gap * style.density))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(style.piece_radius * style.density))
        .bg(background)
        .text_color(style.foreground)
        .child(label.to_string())
}
