//! Data-driven renderer for the Iris Palette / Spotlight surface.
//!
//! Search, clipboard and launcher services stay outside this view. The family
//! adapter supplies grouped results, resolved Iris palette tokens and actions.

use std::{rc::Rc, sync::Arc};

use gpui::{AnyElement, IntoElement, Role, div, prelude::*, px};
use gpui::{App, Entity, FocusHandle, KeyDownEvent, ScrollHandle, Window};

use crate::{
    components::material,
    theme::{MaterialThemeContext, ThemeTokens},
};

#[path = "style_tokens.rs"]
mod style_tokens;
use style_tokens::IrisPalette;

pub type ResultAction = Arc<dyn Fn(String) + Send + Sync>;
pub type ResultDismissAction = Rc<dyn Fn(&mut Window, &mut App)>;
pub type PaletteKeyAction = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;
pub type QueryAction = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct PaletteStyle {
    pub tokens: IrisPalette,
    /// Shared Material surface helpers supply the structural radius/frame.
    /// Visible fills come from IrisStyle tokens applied after that base.
    pub material: MaterialThemeContext,
    pub radius: f32,
    pub row_radius: f32,
    pub padding: f32,
    pub row_height: f32,
    pub width: f32,
    pub max_height: f32,
}

impl PaletteStyle {
    pub fn from_theme(theme: &ThemeTokens, width: f32, max_height: f32) -> Option<Self> {
        Some(Self {
            tokens: IrisPalette::from_theme(theme)?,
            material: theme.material.clone(),
            radius: 28.0,
            row_radius: 14.0,
            padding: 16.0,
            row_height: 48.0,
            width,
            max_height,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteResult {
    /// Stable source identity: app id, action id, clipboard entry id, etc.
    pub id: String,
    pub title: String,
    pub detail: Option<String>,
    pub leading: Option<String>,
    pub trailing: Option<String>,
    pub accessible_label: String,
}

#[derive(Debug, Clone)]
pub struct PaletteGroup {
    pub id: String,
    pub title: String,
    pub results: Vec<PaletteResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaletteStatus {
    #[default]
    Ready,
    Loading,
    Empty,
    Error,
}

#[derive(Clone)]
pub struct PaletteInput {
    pub style: PaletteStyle,
    pub query: String,
    pub placeholder: String,
    pub groups: Vec<PaletteGroup>,
    /// Index in flattened group order, matching IrisPalette's selected index.
    pub active_index: Option<usize>,
    pub hovered_index: Option<usize>,
    pub status: PaletteStatus,
    pub status_message: Option<String>,
    pub on_result: ResultAction,
}

/// Render the Iris Palette panel. This is a controlled presentation component:
/// query edits are sent to the adapter, which returns the updated view model.
pub fn render(input: PaletteInput) -> impl IntoElement {
    render_with_editor(
        input,
        None,
        None,
        ScrollHandle::new(),
        Rc::new(|_, _| {}),
        Rc::new(|_, _, _| {}),
    )
}

/// Render the palette with the shared platform text editor for clipboard and
/// IME input. The Iris host owns the entity and its query callback.
pub fn render_with_editor(
    input: PaletteInput,
    editor: Option<Entity<crate::families::waffle::taskbar::start_menu_search::StartMenuSearch>>,
    focus_handle: Option<FocusHandle>,
    scroll_handle: ScrollHandle,
    on_result_dismiss: ResultDismissAction,
    on_palette_key: PaletteKeyAction,
) -> impl IntoElement {
    let style = input.style;
    let tokens = style.tokens;
    let query = if input.query.is_empty() {
        input.placeholder.clone()
    } else {
        input.query.clone()
    };
    let query_color = if input.query.is_empty() {
        tokens.preset.muted
    } else {
        tokens.text
    };
    let mut flattened = 0usize;
    let mut children = Vec::<AnyElement>::new();
    let mut active_child = None;
    for group in &input.groups {
        let heading = div()
            .id(format!("iris.palette.group.{}.heading", group.id))
            .w_full()
            .px(px(style.padding))
            .pt(px(12.0))
            .pb(px(6.0))
            .text_size(px(12.0))
            .text_color(tokens.preset.muted)
            .focusable()
            .tab_stop(false)
            .role(Role::Heading)
            .aria_level(3)
            .child(group.title.clone())
            .into_any_element();
        children.push(heading);
        for result in &group.results {
            let index = flattened;
            flattened += 1;
            let is_active = input.active_index == Some(index);
            let is_hovered = input.hovered_index == Some(index);
            let background = if is_active || is_hovered {
                tokens.fill_active
            } else {
                tokens.material_high
            };
            let action = input.on_result.clone();
            let dismiss = Rc::clone(&on_result_dismiss);
            let result_id = result.id.clone();
            if input.active_index == Some(index) {
                active_child = Some(children.len());
            }
            let mut row =
                material::surface(&style.material, tokens.material_high, style.row_radius)
                    .id(format!("iris.palette.result.{}", result.id))
                    .w_full()
                    .min_h(px(style.row_height))
                    .px(px(style.padding))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .bg(background)
                    .text_color(tokens.text)
                    .aria_label(result.accessible_label.clone())
                    .focusable()
                    .tab_stop(false)
                    .role(Role::ListBoxOption)
                    .aria_selected(is_active)
                    .on_click(move |_, window, cx| {
                        action(result_id.clone());
                        dismiss(window, cx);
                    });

            if let Some(leading) = &result.leading {
                row = row.child(
                    div()
                        .w(px(28.0))
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(10.0))
                        .bg(tokens.fill)
                        .text_color(tokens.accent)
                        .child(leading.clone()),
                );
            }

            let title = div()
                .w_full()
                .text_color(tokens.text)
                .child(result.title.clone());
            let mut text_stack = div().flex_1().flex().flex_col().gap(px(2.0)).child(title);
            if let Some(detail) = &result.detail {
                text_stack = text_stack.child(
                    div()
                        .w_full()
                        .text_size(px(12.0))
                        .text_color(tokens.preset.subtext)
                        .child(detail.clone()),
                );
            }
            row = row.child(text_stack);
            if let Some(trailing) = &result.trailing {
                row = row.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(tokens.preset.muted)
                        .child(trailing.clone()),
                );
            }
            if is_active {
                row = row.aria_active_descendant();
            }
            children.push(row.into_any_element());
        }
    }
    if let Some(index) = active_child {
        scroll_handle.scroll_to_item(index);
    }

    let results = div()
        .id("iris.palette.results")
        .w_full()
        .max_h(px(style.max_height - 65.0))
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .track_scroll(&scroll_handle)
        .focusable()
        .tab_stop(false)
        .role(Role::ListBox)
        .aria_label("Search results")
        .children(children)
        .into_any_element();
    let mut content = div().w_full().flex().flex_col();
    if input.status != PaletteStatus::Loading {
        content = content.child(results);
    }
    if input.status != PaletteStatus::Ready {
        let message = input.status_message.unwrap_or_else(|| match input.status {
            PaletteStatus::Loading => "Searching…".to_owned(),
            PaletteStatus::Empty => "No results".to_owned(),
            PaletteStatus::Error => "Search failed".to_owned(),
            PaletteStatus::Ready => String::new(),
        });
        let color = if input.status == PaletteStatus::Error {
            tokens.accent
        } else {
            tokens.preset.muted
        };
        content = content.child(
            div()
                .id("iris.palette.status.message")
                .w_full()
                .py(px(24.0))
                .px(px(style.padding))
                .text_color(color)
                .focusable()
                .tab_stop(false)
                .role(Role::Status)
                .child(message),
        );
    }

    let mut palette = material::surface(&style.material, tokens.material_high, style.radius)
        .id("iris.palette.combobox")
        .w(px(style.width))
        .max_h(px(style.max_height))
        .flex()
        .flex_col()
        .overflow_hidden()
        .border_1()
        .border_color(tokens.hairline)
        .bg(tokens.material_high)
        .text_color(tokens.text)
        .on_key_down(move |event, window, cx| on_palette_key(event, window, cx))
        .child(
            div()
                .id("iris.palette.search")
                .w_full()
                .min_h(px(64.0))
                .px(px(style.padding))
                .flex()
                .items_center()
                .gap(px(12.0))
                .text_size(px(20.0))
                .text_color(tokens.accent)
                .child("⌕")
                .child(if let Some(editor) = editor {
                    div().flex_1().child(editor)
                } else {
                    div().flex_1().text_color(query_color).child(query)
                }),
        )
        .child(div().w_full().h(px(1.0)).bg(tokens.hairline))
        .child(div().w_full().px(px(8.0)).pb(px(8.0)).child(content));
    if let Some(focus_handle) = focus_handle {
        palette = palette
            .track_focus(&focus_handle)
            .focusable()
            .role(Role::ComboBox)
            .aria_label("Search applications");
    }
    palette
}
