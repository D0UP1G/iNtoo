//! Read-only wallpaper previews for the ii Widgets sidebar.
//!
//! The gallery presents the supplied directory snapshot. Applying a wallpaper
//! is intentionally unavailable until a supported action adapter exists.

use std::path::PathBuf;

use gpui::{
    AnyElement, App, FontWeight, KeyDownEvent, ObjectFit, Role, ScrollDelta, ScrollHandle,
    SharedString, div, img, prelude::*, px,
};
use std::{fmt, rc::Rc};

use super::sidebars::SidebarStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperGalleryState {
    Loading,
    Ready,
    Error,
}

#[derive(Debug, Clone)]
pub struct WallpaperGalleryEntry {
    pub path: PathBuf,
    pub current: bool,
    pub preview_supported: bool,
    pub preview_unsupported_reason: Option<String>,
}

#[derive(Clone)]
pub struct WallpaperGalleryInput {
    pub state: WallpaperGalleryState,
    pub entries: Vec<WallpaperGalleryEntry>,
    pub error: Option<String>,
    pub show_header: bool,
    pub style: SidebarStyle,
    pub cursor_index: usize,
    pub scroll_handle: ScrollHandle,
    pub on_cursor_change: Rc<dyn Fn(usize, &mut App)>,
}

impl fmt::Debug for WallpaperGalleryInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WallpaperGalleryInput")
            .field("state", &self.state)
            .field("entries", &self.entries)
            .field("error", &self.error)
            .field("show_header", &self.show_header)
            .field("style", &self.style)
            .field("cursor_index", &self.cursor_index)
            .field("scroll_handle", &self.scroll_handle)
            .finish_non_exhaustive()
    }
}

pub fn render_wallpaper_gallery(input: WallpaperGalleryInput) -> AnyElement {
    let style = input.style;
    let entries = input.entries;
    let state = input.state;
    let error = input.error;
    div()
        .id("ii.wallpaper-gallery.card")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.))
        .p(px(12.))
        .bg(style.card)
        .rounded(px(style.card_radius))
        .when(input.show_header, |this| {
            this.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_color(style.muted)
                            .font_family("Material Symbols Rounded")
                            .text_size(px(16.))
                            .child("wallpaper"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(style.muted)
                            .child("Wallpapers"),
                    ),
            )
        })
        .when(state == WallpaperGalleryState::Loading, |this| {
            this.child(status_line(
                "ii.wallpaper-gallery.loading",
                "Loading wallpapers…",
                &style,
            ))
        })
        .when(state == WallpaperGalleryState::Error, |this| {
            this.child(status_line(
                "ii.wallpaper-gallery.error",
                error
                    .as_deref()
                    .unwrap_or("Wallpaper directory unavailable"),
                &style,
            ))
        })
        .when(
            state == WallpaperGalleryState::Ready && entries.is_empty(),
            |this| {
                this.child(status_line(
                    "ii.wallpaper-gallery.empty",
                    "No wallpapers found",
                    &style,
                ))
            },
        )
        .when(
            state == WallpaperGalleryState::Ready && !entries.is_empty(),
            |this| {
                let count = entries.len();
                let current_index = input.cursor_index.min(count.saturating_sub(1));
                let scroll_handle = input.scroll_handle.clone();
                let key_scroll = scroll_handle.clone();
                let wheel_scroll = scroll_handle.clone();
                let key_change = input.on_cursor_change.clone();
                let wheel_change = input.on_cursor_change.clone();
                let accent = style.accent;
                this.child(
                    div()
                        .id("ii.wallpaper-gallery.scroller")
                        .w_full()
                        .h(px(78.))
                        .overflow_x_scroll()
                        .overflow_y_hidden()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .track_scroll(&scroll_handle)
                        .role(Role::Group)
                        .aria_label(format!(
                            "Wallpaper previews, {count} items. Use Left and Right arrows, Home, End, or the mouse wheel to scroll. Applying wallpapers is unavailable."
                        ))
                        .focusable()
                        .tab_stop(true)
                        .focus_visible(move |style| style.border_1().border_color(accent))
                        .on_key_down(move |event: &KeyDownEvent, _, cx| {
                            let target = match event.keystroke.key.as_str() {
                                "left" => current_index.checked_sub(1),
                                "right" => Some((current_index + 1).min(count - 1)),
                                "home" => Some(0),
                                "end" => Some(count - 1),
                                _ => None,
                            };
                            if let Some(target) = target {
                                if !event.is_held {
                                    key_scroll.scroll_to_item(target);
                                    key_change(target, cx);
                                    cx.stop_propagation();
                                }
                            }
                        })
                        .on_scroll_wheel(move |event, window, cx| {
                            let delta = match event.delta {
                                ScrollDelta::Pixels(delta) => {
                                    let x = f32::from(delta.x);
                                    let y = f32::from(delta.y);
                                    if x != 0. { x } else { y }
                                }
                                ScrollDelta::Lines(delta) => {
                                    if delta.x != 0. { delta.x } else { delta.y }
                                }
                            };
                            if delta != 0. {
                                let target = if delta > 0. {
                                    current_index.saturating_sub(1)
                                } else {
                                    (current_index + 1).min(count - 1)
                                };
                                wheel_scroll.scroll_to_item(target);
                                wheel_change(target, cx);
                                window.prevent_default();
                                cx.stop_propagation();
                            }
                        })
                        .children(
                            entries
                                .into_iter()
                                .enumerate()
                                .map(|(index, entry)| render_tile(index, entry, &style)),
                        ),
                )
                .when(error.is_some(), |this| {
                    this.child(status_line(
                        "ii.wallpaper-gallery.scan-note",
                        error.as_deref().unwrap_or_default(),
                        &style,
                    ))
                })
            },
        )
        .when(state == WallpaperGalleryState::Ready, |this| {
            this.child(status_line(
                "ii.wallpaper-gallery.read-only",
                "Preview only. Applying wallpapers is unavailable.",
                &style,
            ))
            .child(status_line(
                "ii.wallpaper-gallery.current-scope",
                "Current marker follows the configured global wallpaper path.",
                &style,
            ))
        })
        .into_any_element()
}

fn render_tile(index: usize, entry: WallpaperGalleryEntry, style: &SidebarStyle) -> AnyElement {
    let path = entry.path;
    let current = entry.current;
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Wallpaper")
        .to_owned();
    let supported = entry.preview_supported;
    let unsupported_reason = entry
        .preview_unsupported_reason
        .unwrap_or_else(|| "This image format is not supported for previews".into());
    let description = if current {
        format!("{label}, configured wallpaper preview")
    } else {
        format!("{label}, not the configured wallpaper")
    };
    let tile_id = SharedString::from(format!("ii.wallpaper-gallery.tile.{index}"));
    let mut preview = div()
        .id(tile_id)
        .w(px(130.))
        .h(px(78.))
        .relative()
        .flex_shrink_0()
        .overflow_hidden()
        .rounded(px(style.card_radius.min(8.)))
        .bg(style.surface)
        .border_1()
        .border_color(if current { style.accent } else { style.border })
        .role(Role::Group)
        .aria_label(description)
        .when(current, |this| this.border_2().border_color(style.accent));

    if supported {
        preview = preview.child(
            img(path.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .with_loading({
                    let color = style.muted;
                    move || {
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(color)
                            .text_xs()
                            .child("Loading preview…")
                            .into_any_element()
                    }
                })
                .with_fallback({
                    let color = style.muted;
                    move || {
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(color)
                            .text_xs()
                            .child("Preview unavailable")
                            .into_any_element()
                    }
                }),
        );
    } else {
        preview = preview.child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .px(px(6.))
                .text_xs()
                .text_color(style.muted)
                .child(format!("Preview unavailable\n{unsupported_reason}")),
        );
    }
    if current {
        preview = preview.child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .size(px(28.))
                        .rounded(px(8.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(style.accent)
                        .text_color(style.surface)
                        .font_family("Material Symbols Rounded")
                        .text_size(px(18.))
                        .child("check"),
                ),
        );
    }
    preview.into_any_element()
}

fn status_line(id: &'static str, text: &str, style: &SidebarStyle) -> AnyElement {
    div()
        .id(id)
        .role(Role::Status)
        .w_full()
        .text_sm()
        .text_color(style.muted)
        .child(text.to_owned())
        .into_any_element()
}
