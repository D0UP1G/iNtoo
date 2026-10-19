//! Reusable Material foundation for environment plugins.
//!
//! These primitives provide token-backed defaults only. Callers remain free to
//! add layout, interaction, and family-specific styling after applying them.

use gpui::{
    BoxShadow, Div, Hsla, InteractiveElement, ParentElement, Role, StatefulInteractiveElement,
    Styled, div, hsla, px,
};

use crate::theme::{MaterialThemeContext, ThemeTokens};

/// Content state for a non-interactive Material status panel.
///
/// Copy is supplied by the owning feature so the shared component stays free
/// of service and application state. An error message should be an actual
/// error supplied by that feature, never an inferred cause.
pub enum StatusPanelState<'a> {
    Loading {
        title: &'a str,
        description: &'a str,
    },
    Empty {
        title: &'a str,
        description: &'a str,
    },
    Unavailable {
        title: &'a str,
        description: &'a str,
    },
    Error {
        title: &'a str,
        description: &'a str,
    },
}

/// Render status content using the resolved Material roles and iNToo theme
/// scale. This is presentational only: it adds no retry action or lifecycle.
pub fn status_panel(theme: &ThemeTokens, state: StatusPanelState<'_>) -> Div {
    let (label, title, description) = match state {
        StatusPanelState::Loading { title, description } => ("LOADING", title, description),
        StatusPanelState::Empty { title, description } => ("EMPTY", title, description),
        StatusPanelState::Unavailable { title, description } => ("UNAVAILABLE", title, description),
        StatusPanelState::Error { title, description } => ("ERROR", title, description),
    };

    let surface = theme
        .material
        .colors
        .surface_container
        .or(theme.material.colors.surface)
        .or(theme.colors.surface)
        .unwrap_or_else(|| hsla(0.67, 0.025, 0.15, 1.));
    let foreground = theme
        .material
        .colors
        .on_surface
        .or(theme.colors.foreground)
        .unwrap_or_else(|| hsla(0.67, 0.04, 0.94, 1.));
    let muted = theme
        .material
        .colors
        .on_surface_variant
        .or(theme.colors.foreground_muted)
        .unwrap_or_else(|| hsla(0.67, 0.025, 0.72, 1.));
    let outline = theme
        .material
        .colors
        .outline_variant
        .or(theme.material.colors.outline)
        .or(theme.colors.border)
        .unwrap_or_else(|| foreground.opacity(0.14));
    let scale = theme.material.typography.size_scale.max(1.0);
    let padding = theme.material.spacing.lg;
    let radius = theme.material.shape.large;

    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p(px(padding))
        .child(
            div()
                .w_full()
                .max_w(px(440.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(theme.material.spacing.md))
                .rounded(px(radius))
                .border_1()
                .border_color(outline)
                .bg(surface)
                .p(px(padding))
                .id("material.status-panel")
                .role(Role::Status)
                .aria_label(format!("{label}: {title}. {description}"))
                .child(
                    div()
                        .text_size(px(11. * scale))
                        .text_color(foreground)
                        .child(label),
                )
                .child(
                    div()
                        .w_full()
                        .font_family(theme.material.typography.title.clone())
                        .text_size(px(20. * scale))
                        .text_color(foreground)
                        .text_center()
                        .whitespace_normal()
                        .child(title.to_owned()),
                )
                .child(
                    div()
                        .w_full()
                        .font_family(theme.material.typography.main.clone())
                        .text_size(px(14. * scale))
                        .text_color(muted)
                        .text_center()
                        .whitespace_normal()
                        .child(description.to_owned()),
                ),
        )
}

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

/// Apply an outlined pill surface to an iNToo-owned element.
///
/// The caller supplies family colors and geometry; this helper only provides
/// the shared one-pixel outline and fill treatment.
pub fn outlined_pill_surface(element: Div, fill: Hsla, outline: Hsla, radius: f32) -> Div {
    element
        .rounded(px(radius))
        .border_1()
        .border_color(outline)
        .bg(fill)
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
