//! Data-driven Waffle clipboard history panel.
//!
//! Clipboard history and the system clipboard are privacy-sensitive external
//! services. The host supplies their state and mutation callbacks; this view
//! never creates sample entries or assumes that a clipboard daemon is present.
use std::sync::Arc;

use super::taskbar::start_menu_search::StartMenuSearch;

use gpui::{
    ClickEvent, Entity, FocusHandle, Hsla, IntoElement, KeyDownEvent, Pixels, Role, ScrollHandle,
    StatefulInteractiveElement, div, prelude::*, px,
};

pub type Action = Arc<dyn Fn() + Send + Sync>;
pub type EntryAction = Arc<dyn Fn(String) + Send + Sync>;
pub type PinAction = Arc<dyn Fn(String, bool) + Send + Sync>;

#[derive(Clone, Debug)]
pub struct ClipboardStyle {
    pub panel: Hsla,
    pub footer: Hsla,
    pub surface: Hsla,
    pub input: Hsla,
    pub selected: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub danger: Hsla,
    pub border: Hsla,
    pub radius: f32,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipboardState {
    #[default]
    Loading,
    Unavailable,
    Ready,
}

#[derive(Clone, Debug)]
pub struct ClipboardEntry {
    /// Stable provider identity used for deletion and pinning.
    pub id: String,
    /// Display-safe preview supplied by the provider (markup already sanitized).
    pub preview: String,
    pub kind_label: String,
    pub is_image: bool,
    pub pinnable: bool,
    pub pinned: bool,
}

#[derive(Clone, Debug)]
pub struct PinnedEntry {
    pub id: String,
    pub text: String,
    pub preview: String,
}

#[derive(Clone)]
pub struct ClipboardInput {
    pub style: ClipboardStyle,
    pub focus_handle: FocusHandle,
    pub scroll_handle: ScrollHandle,
    /// Height available to this panel after the host accounts for output bounds.
    pub available_height: Pixels,
    pub state: ClipboardState,
    pub status_detail: Option<String>,
    pub entries: Vec<ClipboardEntry>,
    pub pinned: Vec<PinnedEntry>,
    pub query: String,
    pub selected: usize,
    pub clear_confirmation: bool,
    pub search_editor: Option<Entity<StartMenuSearch>>,
    pub on_query: Arc<dyn Fn(String) + Send + Sync>,
    pub on_select: Arc<dyn Fn(usize) + Send + Sync>,
    pub on_copy_entry: EntryAction,
    pub on_copy_pinned: EntryAction,
    pub on_delete: EntryAction,
    pub on_toggle_pin: PinAction,
    pub on_request_clear: Action,
    pub on_clear: Action,
    pub on_cancel_clear: Action,
    pub on_close: Action,
    pub on_refresh: Action,
}

/// Clipboard content view. The host owns its overlay, exclusive focus, and
/// editor integration. Keep provider IO and persistence outside this module.
pub fn render(input: ClipboardInput) -> impl IntoElement {
    let s = input.style.clone();
    let query = input.query.trim().to_lowercase();
    let mut rows: Vec<(String, String, String, bool, bool, bool)> = Vec::new();
    if input.state == ClipboardState::Ready {
        for pin in &input.pinned {
            if query.is_empty() || pin.preview.to_lowercase().contains(&query) {
                rows.push((
                    pin.id.clone(),
                    pin.preview.clone(),
                    "Pinned".into(),
                    true,
                    true,
                    true,
                ));
            }
        }
        for entry in &input.entries {
            if query.is_empty() || entry.preview.to_lowercase().contains(&query) {
                rows.push((
                    entry.id.clone(),
                    entry.preview.clone(),
                    entry.kind_label.clone(),
                    false,
                    entry.pinnable,
                    entry.pinned,
                ));
            }
        }
    }
    let selected_index = input.selected.min(rows.len().saturating_sub(1));
    if !rows.is_empty() {
        input.scroll_handle.scroll_to_item(selected_index);
    }
    let close = input.on_close.clone();
    let select_key = input.on_select.clone();
    let copy_selected = if let Some((id, _, _, pinned, _, _)) = rows.get(selected_index) {
        Some((
            id.clone(),
            if *pinned {
                input.on_copy_pinned.clone()
            } else {
                input.on_copy_entry.clone()
            },
        ))
    } else {
        None
    };
    let refresh_key = input.on_refresh.clone();
    let mut list = div()
        .id("waffle.clipboard.list")
        .flex()
        .flex_col()
        .gap(px(4. * s.scale))
        .overflow_y_scroll();
    if input.state == ClipboardState::Ready {
        if let Some(detail) = input.status_detail.as_deref() {
            list = list.child(status("Clipboard action failed", detail, &s));
        }
    }
    match input.state {
        ClipboardState::Loading => {
            list = list.child(status(
                "Clipboard history",
                "Loading clipboard history…",
                &s,
            ))
        }
        ClipboardState::Unavailable => {
            list = list.child(status(
                "Clipboard history",
                input
                    .status_detail
                    .as_deref()
                    .unwrap_or("Clipboard history service unavailable"),
                &s,
            ))
        }
        ClipboardState::Ready if rows.is_empty() => {
            list = list.child(status(
                "Clipboard is empty",
                if query.is_empty() {
                    "Copy something to see it here"
                } else {
                    "No results found"
                },
                &s,
            ))
        }
        ClipboardState::Ready => {
            for (index, (id, preview, kind, pinned_row, pinnable, pinned)) in
                rows.iter().enumerate()
            {
                let selected = index == selected_index;
                let id_copy = id.clone();
                let id_key = id.clone();
                let id_delete = id.clone();
                let id_pin = id.clone();
                let id_delete_key = id.clone();
                let id_pin_key = id.clone();
                let is_pinned = *pinned;
                let is_pinned_row = *pinned_row;
                let copy = if *pinned_row {
                    input.on_copy_pinned.clone()
                } else {
                    input.on_copy_entry.clone()
                };
                let copy_key = copy.clone();
                let select = input.on_select.clone();
                let select_key = select.clone();
                let delete = input.on_delete.clone();
                let delete_key = delete.clone();
                let toggle = input.on_toggle_pin.clone();
                let toggle_key = toggle.clone();
                let mut row = div()
                    .id(format!("waffle.clipboard.entry.{id}"))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(10. * s.scale))
                    .rounded(px(s.radius * s.scale))
                    .px(px(12. * s.scale))
                    .py(px(8. * s.scale))
                    .bg(if selected { s.selected } else { s.surface })
                    .role(Role::ListBoxOption)
                    .aria_selected(selected)
                    .aria_label(format!("{kind}: {preview}"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .gap(px(3. * s.scale))
                            .child(label(kind.clone(), 11. * s.scale, s.muted, false))
                            .child(label(preview.clone(), 13. * s.scale, s.foreground, false)),
                    );
                if *pinnable || *pinned {
                    row = row.child(button(
                        if is_pinned { "Unpin" } else { "Pin" },
                        &s,
                        move || toggle(id_pin.clone(), !is_pinned),
                    ));
                }
                if !*pinned_row {
                    row = row.child(button("Delete", &s, move || delete(id_delete.clone())));
                }
                row = row
                    .on_click(move |_: &ClickEvent, _, _| {
                        select(index);
                        copy(id_copy.clone());
                    })
                    .on_key_down(move |event: &KeyDownEvent, _, cx| {
                        match event.keystroke.key.as_str() {
                            "enter" | "space" if !event.is_held => {
                                select_key(index);
                                copy_key(id_key.clone());
                                cx.stop_propagation();
                            }
                            "delete" if !is_pinned_row && !event.is_held => {
                                delete_key(id_delete_key.clone());
                                cx.stop_propagation();
                            }
                            "p" if event.keystroke.modifiers.control && !event.is_held => {
                                toggle_key(id_pin_key.clone(), !is_pinned);
                                cx.stop_propagation();
                            }
                            _ => {}
                        }
                    });
                list = list.child(row);
            }
        }
    }
    let header = div()
        .w_full()
        .h(px(56. * s.scale))
        .flex()
        .items_center()
        .gap(px(10. * s.scale))
        .bg(s.footer)
        .px(px(16. * s.scale))
        .child(label("▤", 20. * s.scale, s.accent, true))
        .child(label(
            "Clipboard history",
            18. * s.scale,
            s.foreground,
            true,
        ))
        .child(div().flex_1())
        .child(label(
            if input.state == ClipboardState::Unavailable {
                "—".to_owned()
            } else {
                rows.len().to_string()
            },
            13. * s.scale,
            s.muted,
            false,
        ));
    let confirm = input.clear_confirmation;
    let clear = input.on_clear.clone();
    let request_clear = input.on_request_clear.clone();
    let cancel = input.on_cancel_clear.clone();
    let close_button = input.on_close.clone();
    let clear_area = if input.state != ClipboardState::Ready || rows.is_empty() {
        div()
    } else if confirm {
        div()
            .flex()
            .items_center()
            .gap(px(6. * s.scale))
            .child(label("Clear all?", 13. * s.scale, s.danger, true))
            .child(button("Confirm", &s, move || clear()))
            .child(button("Cancel", &s, move || cancel()))
    } else {
        div()
            .flex()
            .items_center()
            .gap(px(6. * s.scale))
            .child(button("Clear all", &s, move || request_clear()))
    };
    let footer = div()
        .w_full()
        .h(px(36. * s.scale))
        .flex()
        .items_center()
        .gap(px(14. * s.scale))
        .px(px(16. * s.scale))
        .bg(s.footer)
        .child(div().flex_1())
        .children(
            (input.state == ClipboardState::Ready && !rows.is_empty()).then(|| {
                label(
                    "↑↓ Navigate · Enter Copy · Del Delete",
                    11. * s.scale,
                    s.muted,
                    false,
                )
            }),
        )
        .children((input.state != ClipboardState::Loading).then(|| {
            button("Refresh", &s, {
                let refresh = input.on_refresh.clone();
                move || refresh()
            })
        }));
    let viewport_height = input.available_height;
    let focus_handle = input.focus_handle.clone();
    let scroll_handle = input.scroll_handle.clone();
    div()
        .id("waffle.clipboard.panel")
        .w(px(474. * s.scale))
        .max_h(viewport_height)
        .flex()
        .flex_col()
        .gap(px(8. * s.scale))
        .rounded(px(s.radius * s.scale))
        .bg(s.panel)
        .p(px(12. * s.scale))
        .role(Role::Group)
        .aria_label("Clipboard history panel; use Up and Down to select an item and Enter to copy")
        .child(header)
        .children(input.search_editor.map(|editor| editor.into_any_element()))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6. * s.scale))
                .child(clear_area)
                .child(div().flex_1())
                .child(button("Close", &s, move || close_button())),
        )
        .child(
            div()
                .id("waffle.clipboard.list-viewport")
                .min_h(px(0.))
                .flex()
                .flex_1()
                .overflow_y_scroll()
                .role(Role::ListBox)
                .aria_label("Clipboard history items")
                .track_scroll(&scroll_handle)
                .child(list),
        )
        .child(footer)
        .track_focus(&focus_handle)
        .on_key_down(
            move |event: &KeyDownEvent, _, cx| match event.keystroke.key.as_str() {
                "escape" => {
                    close();
                    cx.stop_propagation();
                }
                "up" if input.state == ClipboardState::Ready && !rows.is_empty() => {
                    select_key(selected_index.saturating_sub(1));
                    cx.stop_propagation();
                }
                "down" if input.state == ClipboardState::Ready && !rows.is_empty() => {
                    select_key((selected_index + 1).min(rows.len() - 1));
                    cx.stop_propagation();
                }
                "enter" if !event.is_held => {
                    if let Some((id, copy)) = copy_selected.as_ref() {
                        copy(id.clone());
                    }
                    cx.stop_propagation();
                }
                "r" if event.keystroke.modifiers.control
                    && input.state == ClipboardState::Ready =>
                {
                    refresh_key();
                    cx.stop_propagation();
                }
                _ => {}
            },
        )
}

fn button(
    label_text: &str,
    s: &ClipboardStyle,
    action: impl Fn() + Send + Sync + 'static,
) -> impl IntoElement {
    let name = label_text.to_owned();
    let key_action = Arc::new(action);
    let click_action = key_action.clone();
    div()
        .id(format!(
            "waffle.clipboard.button.{}",
            label_text.to_lowercase().replace(' ', "-")
        ))
        .px(px(8. * s.scale))
        .py(px(5. * s.scale))
        .rounded(px(s.radius * s.scale))
        .bg(s.surface)
        .child(label(
            label_text.to_owned(),
            12. * s.scale,
            s.foreground,
            false,
        ))
        .role(Role::Button)
        .aria_label(name)
        .focusable()
        .tab_stop(true)
        .on_click(move |_: &ClickEvent, _, cx| {
            click_action();
            cx.stop_propagation();
        })
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                key_action();
                cx.stop_propagation();
            }
        })
}

fn status(title: &str, detail: &str, s: &ClipboardStyle) -> impl IntoElement {
    div()
        .id(format!(
            "waffle.clipboard.status.{}",
            title.to_lowercase().replace(' ', "-")
        ))
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8. * s.scale))
        .py(px(28. * s.scale))
        .role(Role::Status)
        .aria_label(format!("{title}. {detail}"))
        .child(label(title, 15. * s.scale, s.foreground, true))
        .child(label(detail, 13. * s.scale, s.muted, false))
}

fn label(text: impl Into<String>, size: f32, color: Hsla, strong: bool) -> impl IntoElement {
    let mut element = div()
        .text_size(px(size))
        .text_color(color)
        .child(text.into());
    if strong {
        element = element.font_weight(gpui::FontWeight::SEMIBOLD);
    }
    element
}
