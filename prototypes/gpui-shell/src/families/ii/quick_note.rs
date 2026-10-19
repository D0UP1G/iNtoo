//! Current-tab QuickNote card. The adapter owns loading and persistence;
//! this view keeps only the editable draft and pending save state.

use std::sync::Arc;

use gpui::{
    App, ClickEvent, FontWeight, KeyDownEvent, SharedString, StatefulInteractiveElement, Window,
    div, prelude::*, px,
};

use super::{
    quick_note_store::{QuickNoteSnapshot, QuickNoteStatus},
    sidebars::SidebarStyle,
};

pub struct QuickNoteCard {
    snapshot: QuickNoteSnapshot,
    draft: String,
    baseline: Option<String>,
    pending: Option<String>,
    refreshing: bool,
    cursor: usize,
    editing: bool,
    style: SidebarStyle,
    on_save: Arc<dyn Fn(String) + Send + Sync>,
    on_clear: Arc<dyn Fn() + Send + Sync>,
}

impl QuickNoteCard {
    pub fn new(
        snapshot: QuickNoteSnapshot,
        style: SidebarStyle,
        on_save: Arc<dyn Fn(String) + Send + Sync>,
        on_clear: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let baseline = snapshot.text.clone();
        let draft = baseline.clone().unwrap_or_default();
        let cursor = draft.len();
        Self {
            snapshot,
            draft,
            baseline,
            pending: None,
            refreshing: false,
            cursor,
            editing: false,
            style,
            on_save,
            on_clear,
        }
    }

    pub fn sync(
        &mut self,
        snapshot: QuickNoteSnapshot,
        style: SidebarStyle,
        on_save: Arc<dyn Fn(String) + Send + Sync>,
        on_clear: Arc<dyn Fn() + Send + Sync>,
    ) {
        self.style = style;
        self.on_save = on_save;
        self.on_clear = on_clear;
        if self
            .pending
            .as_deref()
            .is_some_and(|text| snapshot.text.as_deref() == Some(text))
        {
            self.pending = None;
            self.draft = snapshot.text.clone().unwrap_or_default();
            self.baseline = snapshot.text.clone();
            self.cursor = self.draft.len();
            self.editing = false;
        } else if self.pending.is_none()
            && !self.editing
            && snapshot.status == QuickNoteStatus::Ready
        {
            self.draft = snapshot.text.clone().unwrap_or_default();
            self.cursor = self.draft.len();
            self.baseline = snapshot.text.clone();
        } else if snapshot.status == QuickNoteStatus::Unavailable {
            // Keep the draft after a conflict or write failure so it can be copied/retried.
            self.pending = None;
            self.editing = false;
        }
        self.snapshot = snapshot;
    }

    pub fn snapshot(&self) -> &QuickNoteSnapshot {
        &self.snapshot
    }

    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        if self.snapshot.status != QuickNoteStatus::Ready
            || self.snapshot.current_tab.is_none()
            || self.pending.is_some()
        {
            return;
        }
        let text = self.draft.clone();
        self.pending = Some(text.clone());
        (self.on_save)(text);
        cx.notify();
    }

    fn clear(&mut self, cx: &mut gpui::Context<Self>) {
        if self.snapshot.status != QuickNoteStatus::Ready
            || self.snapshot.current_tab.is_none()
            || self.pending.is_some()
        {
            return;
        }
        self.draft.clear();
        self.pending = Some(String::new());
        (self.on_clear)();
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut gpui::Context<Self>) {
        self.draft = self.baseline.clone().unwrap_or_default();
        self.pending = None;
        self.editing = false;
        self.cursor = self.draft.len();
        cx.notify();
    }

    fn reload(&mut self, cx: &mut gpui::Context<Self>) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        let _ = cx
            .spawn(async move |this, cx| {
                let snapshot = cx
                    .background_executor()
                    .spawn(async { super::quick_note_store::load() })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.baseline = snapshot.text.clone();
                    this.snapshot = snapshot;
                    this.pending = None;
                    this.refreshing = false;
                    this.editing = true;
                    cx.notify();
                });
            })
            .detach();
        cx.notify();
    }

    fn on_editor_key(&mut self, event: &KeyDownEvent, cx: &mut gpui::Context<Self>) {
        if self.snapshot.status != QuickNoteStatus::Ready || self.pending.is_some() || event.is_held
        {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter" if event.keystroke.modifiers.control || event.keystroke.modifiers.platform => {
                self.save(cx)
            }
            "enter" => self.insert_text("\n"),
            "backspace" if self.cursor > 0 => {
                let previous = self.draft[..self.cursor]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                self.draft.replace_range(previous..self.cursor, "");
                self.cursor = previous;
            }
            "delete" if self.cursor < self.draft.len() => {
                let next = self.draft[self.cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| self.cursor + i)
                    .unwrap_or(self.draft.len());
                self.draft.replace_range(self.cursor..next, "");
            }
            "left" => {
                self.cursor = self.draft[..self.cursor]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            }
            "right" => {
                self.cursor = self.draft[self.cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| self.cursor + i)
                    .unwrap_or(self.draft.len())
            }
            "home" => {
                self.cursor = self.draft[..self.cursor]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0)
            }
            "end" => {
                self.cursor = self.draft[self.cursor..]
                    .find('\n')
                    .map(|i| self.cursor + i)
                    .unwrap_or(self.draft.len())
            }
            "escape" => self.cancel(cx),
            "space" => self.insert_text(" "),
            key if key.chars().count() == 1
                && !event.keystroke.modifiers.control
                && !event.keystroke.modifiers.platform =>
            {
                self.insert_text(key)
            }
            _ => return,
        }
        cx.notify();
    }

    fn insert_text(&mut self, text: &str) {
        self.draft.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.editing = true;
    }
}

impl Render for QuickNoteCard {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let style = self.style.clone();
        let title = self
            .snapshot
            .title
            .clone()
            .unwrap_or_else(|| "Quick note".into());
        let error = self.snapshot.error.clone();
        let state = self.snapshot.status;
        let ready = state == QuickNoteStatus::Ready && self.snapshot.current_tab.is_some();
        let pending = self.pending.is_some();
        let has_note = self
            .snapshot
            .text
            .as_ref()
            .is_some_and(|text| !text.is_empty());
        let draft = if self.editing {
            let mut visible = self.draft.clone();
            visible.insert(self.cursor, '│');
            visible
        } else {
            self.draft.clone()
        };
        let save = cx.listener(|this, _, _, cx| this.save(cx));
        let save_key = cx.listener(|this, event: &KeyDownEvent, _, cx| {
            if activates(event) {
                this.save(cx);
                cx.stop_propagation();
            }
        });
        let clear = cx.listener(|this, _, _, cx| this.clear(cx));
        let clear_key = cx.listener(|this, event: &KeyDownEvent, _, cx| {
            if activates(event) {
                this.clear(cx);
                cx.stop_propagation();
            }
        });
        let cancel = cx.listener(|this, _, _, cx| this.cancel(cx));
        let cancel_key = cx.listener(|this, event: &KeyDownEvent, _, cx| {
            if activates(event) {
                this.cancel(cx);
                cx.stop_propagation();
            }
        });
        let reload = cx.listener(|this, _, _, cx| this.reload(cx));
        let reload_key = cx.listener(|this, event: &KeyDownEvent, _, cx| {
            if activates(event) {
                this.reload(cx);
                cx.stop_propagation();
            }
        });
        let editor_key =
            cx.listener(|this, event: &KeyDownEvent, _, cx| this.on_editor_key(event, cx));
        let save_button = quick_note_action(
            "ii.quick-note.save",
            "Save note",
            "Save",
            ready && !pending,
            &style,
            save,
            save_key,
        );
        let clear_button = quick_note_action(
            "ii.quick-note.clear",
            "Clear current note",
            "Clear",
            ready && has_note && !pending,
            &style,
            clear,
            clear_key,
        );
        let cancel_button = quick_note_action(
            "ii.quick-note.cancel",
            "Cancel edits",
            "Cancel",
            ready && !pending,
            &style,
            cancel,
            cancel_key,
        );
        let reload_button = quick_note_action(
            "ii.quick-note.reload",
            "Reload saved note while keeping this draft",
            if self.refreshing {
                "Reloading…"
            } else {
                "Reload"
            },
            !self.refreshing,
            &style,
            reload,
            reload_key,
        );
        div()
            .id("ii.quick-note.card")
            .w_full()
            .flex()
            .flex_col()
            .gap(px(style.gap))
            .p(px(style.padding))
            .bg(style.card)
            .rounded(px(style.card_radius))
            .text_color(style.foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                    .when(ready && has_note && !pending, |header| header.child(clear_button)),
            )
            .when(state == QuickNoteStatus::Loading, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(style.muted)
                        .child("Loading current note…"),
                )
            })
            .when(state == QuickNoteStatus::Unavailable, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(style.muted)
                        .child(error.map(|error| {
                            if error.contains("changed while saving") {
                                "The saved note changed elsewhere. Your draft is kept. Reload to read the latest version before retrying.".to_owned()
                            } else {
                                error
                            }
                        }).unwrap_or_else(|| "Current note is unavailable".into())),
                )
                .when(!draft.is_empty(), |this| {
                    this.child(
                        div()
                            .id("ii.quick-note.unsaved-draft")
                            .w_full()
                            .max_h(px(120.))
                            .p(px(8.))
                            .bg(style.surface)
                            .rounded(px(style.card_radius))
                            .role(gpui::Role::TextInput)
                            .aria_label("Unsaved Quick note draft, read only")
                            .aria_value(draft.clone())
                            .focusable()
                            .tab_stop(true)
                            .child(draft.clone()),
                    )
                })
                .child(reload_button)
            })
            .when(
                state == QuickNoteStatus::Ready && self.snapshot.text.is_none(),
                |this| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(style.muted)
                            .child("This tab has no saved note."),
                    )
                },
            )
            .when(ready, |this| {
                this.child(
                    div()
                        .id("ii.quick-note.editor")
                        .w_full()
                        .min_h(px(100.))
                        .max_h(px(160.))
                        .p(px(8.))
                        .bg(style.surface)
                        .rounded(px(style.card_radius))
                        .border_1()
                        .border_color(style.border)
                        .focusable()
                        .tab_stop(true)
                        .role(gpui::Role::TextInput)
                        .aria_label("Quick note text")
                        .focus_visible({
                            let accent = style.accent;
                            move |s| s.border_color(accent)
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.editing = true;
                            cx.notify();
                        }))
                        .on_key_down(move |event, window, cx| editor_key(event, window, cx))
                        .child(if draft.is_empty() {
                            "Write a note…".into()
                        } else {
                            SharedString::from(draft)
                        }),
                )
                .when(pending, |this| {
                    this.child(
                        div()
                            .id("ii.quick-note.saving-status")
                            .role(gpui::Role::Status)
                            .aria_label("Saving Quick note")
                            .text_sm()
                            .text_color(style.muted)
                            .child("Saving…"),
                    )
                })
                .when(!pending, |this| {
                    this.child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(style.gap))
                            .child(cancel_button)
                            .child(save_button),
                    )
                })
            })
    }
}

fn quick_note_action(
    id: &'static str,
    label: &'static str,
    text: &'static str,
    enabled: bool,
    style: &SidebarStyle,
    callback: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    key_callback: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let border = style.border;
    let hover = style.hover;
    div()
        .id(id)
        .px(px(9.))
        .py(px(5.))
        .rounded(px(style.card_radius))
        .bg(style.card)
        .text_color(if enabled {
            style.foreground
        } else {
            style.muted
        })
        .aria_label(label)
        .when(enabled, |this| {
            this.role(gpui::Role::Button)
                .focusable()
                .tab_stop(true)
                .focus_visible(move |s| s.border_1().border_color(border))
                .hover(move |s| s.bg(hover))
                .on_click(callback)
                .on_key_down(move |event, window, cx| key_callback(event, window, cx))
        })
        .child(text)
}

fn activates(event: &KeyDownEvent) -> bool {
    !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space")
}
