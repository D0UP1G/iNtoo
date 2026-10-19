//! Reusable Material foundation for environment plugins.
//!
//! These primitives provide token-backed defaults only. Callers remain free to
//! add layout, interaction, and family-specific styling after applying them.

use gpui::{BoxShadow, Div, Hsla, InteractiveElement, StatefulInteractiveElement, Styled, div, px};

use crate::theme::MaterialThemeContext;

/// The shared surface role used for cards and shell surfaces.
pub fn surface_color(theme: &MaterialThemeContext, fallback: Hsla) -> Hsla {
    theme
        .colors
        .surface_container_low
        .or(theme.colors.surface)
        .unwrap_or(fallback)
}

/// Start a token-backed Material surface. Further GPUI styling can be chained
/// by each environment plugin to create its own composition and silhouette.
pub fn surface(theme: &MaterialThemeContext, fallback: Hsla, radius: f32) -> Div {
    div().rounded(px(radius)).bg(surface_color(theme, fallback))
}

/// Apply the shared Material action-button appearance to an existing control.
///
/// The caller owns its identity, accessibility metadata, focusability, and
/// event handlers. `fallback_*` colors preserve an iNToo-defined appearance
/// when generated Material roles are unavailable.
pub fn action_button<E>(
    element: E,
    theme: &MaterialThemeContext,
    fallback_foreground: Hsla,
    fallback_outline: Hsla,
    fallback_focus: Hsla,
    selected: bool,
) -> E
where
    E: InteractiveElement + StatefulInteractiveElement + Styled,
{
    let foreground = theme
        .colors
        .on_surface
        .or(theme.colors.on_surface_variant)
        .unwrap_or(fallback_foreground);
    let outline = theme
        .colors
        .outline_variant
        .or(theme.colors.outline)
        .unwrap_or(fallback_outline);
    let focus = theme
        .colors
        .primary
        .or(theme.colors.outline)
        .unwrap_or(fallback_focus);
    let hover = theme
        .colors
        .surface_container_high
        .or(theme.colors.surface_container)
        .unwrap_or_else(|| outline.opacity(0.16));
    let pressed = theme
        .colors
        .surface_container_highest
        .or(theme.colors.surface_container_high)
        .unwrap_or_else(|| outline.opacity(0.28));
    let horizontal_padding = theme.spacing.sm;
    let vertical_padding = theme.spacing.xs;
    let radius = theme.shape.medium;

    element
        .px(px(horizontal_padding))
        .py(px(vertical_padding))
        .rounded(px(radius))
        .border_1()
        .border_color(if selected { focus } else { outline })
        .text_color(foreground)
        .hover(move |style| style.bg(hover))
        .active(move |style| style.bg(pressed))
        .focus_visible(move |style| style.border_1().border_color(focus))
}

/// Add the shared keyboard focus treatment to a focusable GPUI control.
///
/// A translucent primary tint and inset edge cue provide a visible keyboard
/// focus state without changing the control's bounds or unfocused appearance.
/// The caller supplies a family fallback when necessary.
pub fn focusable_button<E>(element: E, theme: &MaterialThemeContext, fallback_focus: Hsla) -> E
where
    E: InteractiveElement + Styled,
{
    let focus = theme.colors.primary.unwrap_or(fallback_focus);
    let edge = theme
        .colors
        .outline
        .or(theme.colors.outline_variant)
        .unwrap_or(focus);
    focusable_button_with_roles(element, focus, edge)
}

/// Add keyboard focus styling with caller-selected family roles.
///
/// This variant lets an environment retain its own focus hierarchy while
/// still using the shared Material focus treatment. The caller chooses the
/// fill and inset edge independently; the default helper above continues to
/// prefer Material primary and outline roles.
pub fn focusable_button_with_roles<E>(element: E, focus: Hsla, edge: Hsla) -> E
where
    E: InteractiveElement + Styled,
{
    element.focus_visible(move |style| {
        style.bg(focus.opacity(0.16)).shadow(vec![
            BoxShadow::new(px(0.), px(0.), edge)
                .spread_radius(px(1.))
                .inset(),
        ])
    })
}
