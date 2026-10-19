//! Data-driven window preview cards for the ii Dock.
//!
//! Window discovery, thumbnail capture, popup anchoring and dismissal remain
//! host responsibilities. This component only presents supplied records.

use std::{path::PathBuf, rc::Rc};

use gpui::{
    AnyElement, App, Hsla, IntoElement, KeyDownEvent, ObjectFit, Role, SharedString, div, img,
    prelude::*, px,
};

pub type WindowAction = Rc<dyn Fn(u64, &mut App)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DockPreviewEdge {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

impl DockPreviewEdge {
    fn horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailState {
    Ready,
    /// Show the cached image while clearly marking it as stale.
    Stale,
    Loading,
    Unavailable,
    Error,
}

#[derive(Debug, Clone)]
pub struct DockPreviewStyle {
    pub surface: Hsla,
    pub card: Hsla,
    pub hover: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub border: Hsla,
    pub radius: f32,
    pub gap: f32,
}

#[derive(Debug, Clone)]
pub struct DockPreviewWindow {
    pub id: u64,
    pub app_name: String,
    pub title: String,
    pub icon_path: Option<PathBuf>,
    pub thumbnail_path: Option<PathBuf>,
    pub thumbnail_state: ThumbnailState,
}

#[derive(Clone)]
pub struct DockPreviewInput {
    pub edge: DockPreviewEdge,
    pub style: DockPreviewStyle,
    pub windows: Vec<DockPreviewWindow>,
    pub on_focus: Option<WindowAction>,
    pub on_close: Option<WindowAction>,
}

/// Render preview cards for the host-provided app windows.
pub fn render(input: DockPreviewInput) -> AnyElement {
    let horizontal = input.edge.horizontal();
    let style = input.style;
    let cards = input.windows.into_iter().map(|window| {
        let id = window.id;
        let focus = input
            .on_focus
            .clone()
            .map(|action| Rc::new(move |cx: &mut App| action(id, cx)) as Rc<dyn Fn(&mut App)>);
        let close = input
            .on_close
            .clone()
            .map(|action| Rc::new(move |cx: &mut App| action(id, cx)) as Rc<dyn Fn(&mut App)>);
        let identity = if window.title.trim().is_empty() {
            window.app_name.clone()
        } else {
            format!("{} — {}", window.app_name, window.title)
        };
        let thumbnail = thumbnail(&window, &style);
        let mut card = div()
            .id(SharedString::from(format!("ii.dock.preview.{}", id)))
            .role(Role::Group)
            .aria_label(SharedString::from(format!("Window preview: {identity}")))
            .w(px(228.0))
            .overflow_hidden()
            .rounded(px(style.radius.max(0.0)))
            .border_1()
            .border_color(style.border)
            .bg(style.card)
            .flex()
            .flex_col();
        let content = div()
            .id(SharedString::from(format!("ii.dock.preview.activate.{id}")))
            .flex()
            .flex_col()
            .when_some(focus, |this, focus| {
                this.role(Role::Button)
                    .aria_label(SharedString::from(format!("Focus window: {identity}")))
                    .focusable()
                    .tab_stop(true)
                    .focus_visible({
                        let accent = style.accent;
                        move |this| this.border_1().border_color(accent)
                    })
                    .hover({
                        let hover = style.hover;
                        move |this| this.bg(hover)
                    })
                    .on_click({
                        let focus = focus.clone();
                        move |_, _, cx| focus(cx)
                    })
                    .on_key_down(move |event: &KeyDownEvent, _, cx| {
                        if is_activate_key(event) {
                            focus(cx);
                        }
                    })
            })
            .child(thumbnail)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .when_some(window.icon_path.clone(), |this, path| {
                        this.child(div().w(px(20.0)).h(px(20.0)).child(img(path).size_full()))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(style.foreground)
                                    .child(window.app_name.clone()),
                            )
                            .child(div().text_size(px(11.0)).text_color(style.muted).child(
                                if window.title.is_empty() {
                                    "Untitled window".into()
                                } else {
                                    window.title.clone()
                                },
                            )),
                    ),
            );
        card = card.child(content);
        if let Some(close) = close {
            card = card.child(
                div()
                    .id(SharedString::from(format!("ii.dock.preview.close.{id}")))
                    .role(Role::Button)
                    .aria_label(SharedString::from(format!("Close window: {identity}")))
                    .focusable()
                    .tab_stop(true)
                    .focus_visible({
                        let accent = style.accent;
                        move |this| this.opacity(1.0).border_1().border_color(accent)
                    })
                    .px(px(7.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .text_color(style.foreground)
                    .opacity(0.55)
                    .hover({
                        let hover = style.hover;
                        move |this| this.bg(hover).opacity(1.0)
                    })
                    .child("×")
                    .on_click({
                        let close = close.clone();
                        move |_, _, cx| close(cx)
                    })
                    .on_key_down(move |event: &KeyDownEvent, _, cx| {
                        if is_activate_key(event) {
                            close(cx);
                        }
                    }),
            );
        }
        card.into_any_element()
    });

    div()
        .id("ii.dock.preview")
        .role(Role::Group)
        .aria_label("Open windows")
        .flex()
        .when(horizontal, |this| this.flex_row())
        .when(!horizontal, |this| this.flex_col())
        .items_start()
        .gap(px(style.gap.max(0.0)))
        .p(px(8.0))
        .rounded(px(style.radius.max(0.0)))
        .border_1()
        .border_color(style.border)
        .bg(style.surface)
        .children(cards)
        .into_any_element()
}

fn thumbnail(window: &DockPreviewWindow, style: &DockPreviewStyle) -> AnyElement {
    let frame = div()
        .w_full()
        .h(px(132.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(style.surface);
    if matches!(
        window.thumbnail_state,
        ThumbnailState::Ready | ThumbnailState::Stale
    ) {
        if let Some(path) = &window.thumbnail_path {
            let muted = style.muted;
            let surface = style.surface;
            let mut frame = frame.child(
                img(path.clone())
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .with_loading(move || preview_status("Loading preview…", surface, muted))
                    .with_fallback(move || preview_status("Preview unavailable", surface, muted)),
            );
            if window.thumbnail_state == ThumbnailState::Stale {
                frame = frame.child(
                    div()
                        .absolute()
                        .right(px(6.0))
                        .bottom(px(6.0))
                        .px(px(6.0))
                        .py(px(3.0))
                        .rounded(px(4.0))
                        .bg(style.surface)
                        .text_size(px(10.0))
                        .text_color(style.muted)
                        .child("Cached · refreshing"),
                );
            }
            return frame.into_any_element();
        }
    }
    let message = match window.thumbnail_state {
        ThumbnailState::Ready | ThumbnailState::Unavailable => "Preview unavailable",
        ThumbnailState::Stale => "Cached preview · refreshing",
        ThumbnailState::Loading => "Loading preview…",
        ThumbnailState::Error => "Preview failed",
    };
    frame
        .text_size(px(12.0))
        .text_color(style.muted)
        .child(message)
        .into_any_element()
}

fn preview_status(message: &'static str, surface: Hsla, muted: Hsla) -> AnyElement {
    div()
        .w_full()
        .h(px(132.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(surface)
        .text_size(px(12.0))
        .text_color(muted)
        .child(message)
        .into_any_element()
}

fn is_activate_key(event: &KeyDownEvent) -> bool {
    !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space")
}
