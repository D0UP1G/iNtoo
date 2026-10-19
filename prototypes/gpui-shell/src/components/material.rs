//! Reusable Material foundation for environment plugins.
//!
//! These primitives provide token-backed defaults only. Callers remain free to
//! add layout, interaction, and family-specific styling after applying them.

use gpui::{Div, Hsla, InteractiveElement, Styled, div, px};

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

/// Add the shared keyboard focus treatment to a focusable GPUI control.
///
/// A translucent primary tint provides a visible keyboard focus state without
/// changing the control's layout or unfocused appearance. The caller supplies
/// a family fallback when necessary.
pub fn focusable_button<E>(element: E, theme: &MaterialThemeContext, fallback_focus: Hsla) -> E
where
    E: InteractiveElement + Styled,
{
    let focus = theme.colors.primary.unwrap_or(fallback_focus);
    element.focus_visible(move |style| style.bg(focus.opacity(0.16)))
}
