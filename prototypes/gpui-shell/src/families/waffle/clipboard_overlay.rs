//! Per-output overlay host for Waffle clipboard history.
//!
//! Provider process calls run through GPUI background executor; render and input
//! callbacks only enqueue typed actions.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use async_channel::Sender;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, Render, ScrollHandle, Window, div,
    prelude::*, px,
};

use crate::{families::waffle::controller::ClipboardEvent, theme::ThemeTokens};

use super::{
    clipboard::{self, ClipboardInput, ClipboardState, ClipboardStyle},
    clipboard_provider::{CliphistProvider, Snapshot},
    controller::ClipboardController,
    runtime::{start_menu_style, taskbar_style},
    taskbar::start_menu_search::StartMenuSearch,
};

pub struct WaffleClipboardOverlay {
    theme: ThemeTokens,
    events: Sender<ClipboardEvent>,
    controller: Rc<RefCell<ClipboardController>>,
    display_id: gpui::DisplayId,
    bar_bottom: bool,
    available_height: f32,
    focus_handle: FocusHandle,
    search_editor: Entity<StartMenuSearch>,
    scroll_handle: ScrollHandle,
    focus_on_first_render: bool,
    provider: CliphistProvider,
    pins: Vec<String>,
    state: ClipboardState,
    snapshot: Option<Snapshot>,
    query: String,
    selected: usize,
    clear_confirmation: bool,
    error: Option<String>,
    actions: Sender<ClipboardAction>,
}

#[derive(Clone, Debug)]
enum ClipboardAction {
    Refresh,
    Copy { id: String, pinned: bool },
    Delete(String),
    RequestClear,
    ConfirmClear,
    TogglePin(String, bool),
    Select(usize),
    Query(String),
    CancelClear,
}

impl WaffleClipboardOverlay {
    pub fn new(
        theme: ThemeTokens,
        events: Sender<ClipboardEvent>,
        controller: Rc<RefCell<ClipboardController>>,
        display_id: gpui::DisplayId,
        bar_bottom: bool,
        available_height: f32,
        pins: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (actions, receiver) = async_channel::unbounded();
        let focus_handle = cx.focus_handle();
        let query_action = {
            let actions = actions.clone();
            Arc::new(move |query| {
                let _ = actions.try_send(ClipboardAction::Query(query));
            }) as Arc<dyn Fn(String) + Send + Sync>
        };
        let search_editor = cx.new(|cx| {
            StartMenuSearch::new_with_shared_focus(
                String::new(),
                "Search clipboard history".to_owned(),
                "Search clipboard history".to_owned(),
                query_action,
                focus_handle.clone(),
                cx,
            )
        });
        let overlay = Self {
            theme,
            events,
            controller,
            display_id,
            bar_bottom,
            available_height,
            focus_handle,
            search_editor,
            scroll_handle: ScrollHandle::new(),
            focus_on_first_render: true,
            provider: CliphistProvider::default(),
            pins,
            state: ClipboardState::Loading,
            snapshot: None,
            query: String::new(),
            selected: 0,
            clear_confirmation: false,
            error: None,
            actions,
        };
        let provider = overlay.provider.clone();
        let pins_for_load = overlay.pins.clone();
        let close_sender = overlay.events.clone();
        let pins_sender = overlay.events.clone();
        let display = overlay.display_id;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { provider.list(&pins_for_load) })
                .await;
            apply_result(&this, cx, result);
            while let Ok(action) = receiver.recv().await {
                match action {
                    ClipboardAction::Select(index) => {
                        let _ = this.update(cx, |view, cx| {
                            view.selected = index;
                            cx.notify();
                        });
                    }
                    ClipboardAction::Query(query) => {
                        let _ = this.update(cx, |view, cx| {
                            view.query = query;
                            view.selected = 0;
                            cx.notify();
                        });
                    }
                    ClipboardAction::CancelClear => {
                        let _ = this.update(cx, |view, cx| {
                            view.clear_confirmation = false;
                            cx.notify();
                        });
                    }
                    ClipboardAction::Refresh => {
                        let (provider, pins) = match this
                            .update(cx, |view, _| (view.provider.clone(), view.pins.clone()))
                        {
                            Ok(data) => data,
                            Err(_) => break,
                        };
                        let _ = this.update(cx, |view, cx| {
                            view.state = ClipboardState::Loading;
                            view.error = None;
                            cx.notify();
                        });
                        let result = cx
                            .background_executor()
                            .spawn(async move { provider.list(&pins) })
                            .await;
                        apply_result(&this, cx, result);
                    }
                    ClipboardAction::Copy { id, pinned } => {
                        let (provider, pin_text) = match this.update(cx, |view, _| {
                            let text = view
                                .snapshot
                                .as_ref()
                                .and_then(|s| s.pins.iter().find(|p| p.id == id))
                                .map(|p| p.text.clone());
                            (view.provider.clone(), text)
                        }) {
                            Ok(data) => data,
                            Err(_) => break,
                        };
                        let result = cx
                            .background_executor()
                            .spawn(async move {
                                if pinned {
                                    match pin_text.as_deref() {
                                        Some(text) => provider.copy_pinned(text),
                                        None => {
                                            Err(super::clipboard_provider::ProviderError::Parse {
                                                line: 0,
                                                detail: "configured pin is unavailable".into(),
                                            })
                                        }
                                    }
                                } else {
                                    provider.copy(&id)
                                }
                            })
                            .await;
                        match result {
                            Ok(()) => {
                                let _ = close_sender.try_send(ClipboardEvent::Close { display });
                            }
                            Err(error) => set_error(&this, cx, error),
                        }
                    }
                    ClipboardAction::Delete(id) => {
                        let (provider, pins, entry) = match this.update(cx, |view, _| {
                            let entry = view
                                .snapshot
                                .as_ref()
                                .and_then(|s| s.entries.iter().find(|e| e.id == id))
                                .cloned();
                            (view.provider.clone(), view.pins.clone(), entry)
                        }) {
                            Ok(data) => data,
                            Err(_) => break,
                        };
                        if let Some(entry) = entry {
                            let mutation = cx
                                .background_executor()
                                .spawn(async move { provider.delete(&entry) })
                                .await;
                            match mutation {
                                Err(error) => set_error(&this, cx, error),
                                Ok(()) => {
                                    let provider =
                                        match this.update(cx, |view, _| view.provider.clone()) {
                                            Ok(provider) => provider,
                                            Err(_) => break,
                                        };
                                    let result = cx
                                        .background_executor()
                                        .spawn(async move { provider.list(&pins) })
                                        .await;
                                    apply_mutation_result(&this, cx, result);
                                }
                            }
                        }
                    }
                    ClipboardAction::RequestClear => {
                        let _ = this.update(cx, |view, cx| {
                            view.clear_confirmation = true;
                            cx.notify();
                        });
                    }
                    ClipboardAction::ConfirmClear => {
                        let (confirm, provider, pins) = match this.update(cx, |view, _| {
                            (
                                view.clear_confirmation,
                                view.provider.clone(),
                                view.pins.clone(),
                            )
                        }) {
                            Ok(data) => data,
                            Err(_) => break,
                        };
                        if confirm {
                            let _ = this.update(cx, |view, cx| {
                                view.clear_confirmation = false;
                                cx.notify();
                            });
                            let mutation = cx
                                .background_executor()
                                .spawn(async move { provider.wipe() })
                                .await;
                            match mutation {
                                Err(error) => set_error(&this, cx, error),
                                Ok(()) => {
                                    let provider =
                                        match this.update(cx, |view, _| view.provider.clone()) {
                                            Ok(provider) => provider,
                                            Err(_) => break,
                                        };
                                    let result = cx
                                        .background_executor()
                                        .spawn(async move { provider.list(&pins) })
                                        .await;
                                    apply_mutation_result(&this, cx, result);
                                }
                            }
                        }
                    }
                    ClipboardAction::TogglePin(id, pinned) => {
                        let (provider, row_preview, mut pins) = match this.update(cx, |view, _| {
                            let entry = view
                                .snapshot
                                .as_ref()
                                .and_then(|s| s.entries.iter().find(|e| e.id == id));
                            let pin_index = id
                                .strip_prefix("configured-pin-")
                                .and_then(|value| value.parse::<usize>().ok());
                            (
                                view.provider.clone(),
                                entry.map(|e| e.preview.clone()).or_else(|| {
                                    pin_index.and_then(|index| {
                                        view.snapshot
                                            .as_ref()?
                                            .pins
                                            .get(index)
                                            .map(|p| p.preview.clone())
                                    })
                                }),
                                view.pins.clone(),
                            )
                        }) {
                            Ok(data) => data,
                            Err(_) => break,
                        };
                        if pinned {
                            match cx
                                .background_executor()
                                .spawn(async move { provider.decode_for_pin(&id) })
                                .await
                            {
                                Ok(text) if !text.is_empty() && !pins.contains(&text) => {
                                    pins.push(text)
                                }
                                Ok(_) => {}
                                Err(error) => {
                                    set_error(&this, cx, error);
                                    continue;
                                }
                            }
                        } else if let Some(index) = id
                            .strip_prefix("configured-pin-")
                            .and_then(|value| value.parse::<usize>().ok())
                        {
                            if index < pins.len() {
                                pins.remove(index);
                            }
                        } else if let Some(preview) = row_preview {
                            pins.retain(|pin| {
                                pin.lines()
                                    .find(|line| !line.trim().is_empty())
                                    .unwrap_or(pin)
                                    .trim()
                                    != preview.trim()
                            });
                        }
                        let provider = match this.update(cx, |view, _| view.provider.clone()) {
                            Ok(p) => p,
                            Err(_) => break,
                        };
                        let refresh_pins = pins.clone();
                        let result = cx
                            .background_executor()
                            .spawn(async move { provider.list(&refresh_pins) })
                            .await;
                        let _ = this.update(cx, |view, cx| {
                            view.pins = pins.clone();
                            cx.notify();
                        });
                        let _ = pins_sender.try_send(ClipboardEvent::PinsChanged { pins });
                        apply_mutation_result(&this, cx, result);
                    }
                }
            }
        })
        .detach();
        overlay
    }

    fn input(&self) -> ClipboardInput {
        let taskbar = taskbar_style(&self.theme);
        let menu = start_menu_style(&self.theme, &taskbar);
        let close_events = self.events.clone();
        let display = self.display_id;
        let scale = (self.available_height / 680.).clamp(0.01, 1.);
        let actions = self.actions.clone();
        let selected = self.selected;
        let editor = self.search_editor.clone();

        let snapshot = self.snapshot.clone().unwrap_or(Snapshot {
            entries: Vec::new(),
            pins: Vec::new(),
        });
        let entries: Vec<_> = snapshot
            .entries
            .iter()
            .map(|e| super::clipboard::ClipboardEntry {
                id: e.id.clone(),
                preview: e.preview.clone(),
                kind_label: if e.is_image {
                    "Image".into()
                } else {
                    "Text".into()
                },
                is_image: e.is_image,
                pinnable: e.pinnable,
                pinned: e.pinned,
            })
            .collect();
        let pinned = snapshot
            .pins
            .iter()
            .map(|p| super::clipboard::PinnedEntry {
                id: p.id.clone(),
                text: p.text.clone(),
                preview: p.preview.clone(),
            })
            .collect();
        ClipboardInput {
            style: ClipboardStyle {
                panel: menu.background,
                footer: menu.surface,
                surface: menu.surface,
                input: menu.surface_hover,
                selected: menu.surface_hover,
                foreground: menu.foreground,
                muted: menu.muted,
                accent: menu.accent,
                danger: menu.accent,
                border: menu.border,
                radius: menu.radius,
                scale,
            },
            focus_handle: self.focus_handle.clone(),
            scroll_handle: self.scroll_handle.clone(),
            state: self.state,
            status_detail: self.error.clone(),
            entries,
            pinned,
            query: self.query.clone(),
            selected,
            clear_confirmation: self.clear_confirmation,
            available_height: px(self.available_height),
            search_editor: Some(editor),
            on_query: {
                let tx = actions.clone();
                Arc::new(move |query| {
                    let _ = tx.try_send(ClipboardAction::Query(query));
                })
            },
            on_select: {
                let tx = actions.clone();
                Arc::new(move |index| {
                    let _ = tx.try_send(ClipboardAction::Select(index));
                })
            },
            on_copy_entry: {
                let tx = actions.clone();
                Arc::new(move |id| {
                    let _ = tx.try_send(ClipboardAction::Copy { id, pinned: false });
                })
            },
            on_copy_pinned: {
                let tx = actions.clone();
                Arc::new(move |id| {
                    let _ = tx.try_send(ClipboardAction::Copy { id, pinned: true });
                })
            },
            on_delete: {
                let tx = actions.clone();
                Arc::new(move |id| {
                    let _ = tx.try_send(ClipboardAction::Delete(id));
                })
            },
            on_toggle_pin: {
                let tx = actions.clone();
                Arc::new(move |id, pin| {
                    let _ = tx.try_send(ClipboardAction::TogglePin(id, pin));
                })
            },
            on_request_clear: {
                let tx = actions.clone();
                Arc::new(move || {
                    let _ = tx.try_send(ClipboardAction::RequestClear);
                })
            },
            on_clear: {
                let tx = actions.clone();
                Arc::new(move || {
                    let _ = tx.try_send(ClipboardAction::ConfirmClear);
                })
            },
            on_cancel_clear: {
                let tx = actions.clone();
                Arc::new(move || {
                    let _ = tx.try_send(ClipboardAction::CancelClear);
                })
            },
            on_close: Arc::new(move || {
                let _ = close_events.try_send(ClipboardEvent::Close { display });
            }),
            on_refresh: Arc::new(move || {
                let _ = actions.try_send(ClipboardAction::Refresh);
            }),
        }
    }
}

fn apply_result(
    this: &gpui::WeakEntity<WaffleClipboardOverlay>,
    cx: &mut gpui::AsyncApp,
    result: Result<Snapshot, super::clipboard_provider::ProviderError>,
) {
    let _ = this.update(cx, |view, cx| {
        match result {
            Ok(snapshot) => {
                view.snapshot = Some(snapshot);
                view.state = ClipboardState::Ready;
                view.error = None;
            }
            Err(error) => {
                view.state = if view.snapshot.is_some() {
                    ClipboardState::Ready
                } else {
                    ClipboardState::Unavailable
                };
                view.error = Some(format!("{error:?}"));
            }
        }
        view.clear_confirmation = false;
        cx.notify();
    });
}

/// A mutation succeeded but its follow-up read failed, so cached rows can no
/// longer be shown as current. Keep the provider error and require refresh.
fn apply_mutation_result(
    this: &gpui::WeakEntity<WaffleClipboardOverlay>,
    cx: &mut gpui::AsyncApp,
    result: Result<Snapshot, super::clipboard_provider::ProviderError>,
) {
    match result {
        Ok(snapshot) => apply_result(this, cx, Ok(snapshot)),
        Err(error) => {
            let _ = this.update(cx, |view, cx| {
                view.snapshot = None;
                view.state = ClipboardState::Unavailable;
                view.error = Some(format!("{error:?}"));
                view.clear_confirmation = false;
                cx.notify();
            });
        }
    }
}

fn set_error(
    this: &gpui::WeakEntity<WaffleClipboardOverlay>,
    cx: &mut gpui::AsyncApp,
    error: super::clipboard_provider::ProviderError,
) {
    let _ = this.update(cx, |view, cx| {
        view.error = Some(format!("{error:?}"));
        if view.snapshot.is_some() {
            view.state = ClipboardState::Ready;
        } else {
            view.state = ClipboardState::Unavailable;
        }
        cx.notify();
    });
}

impl Focusable for WaffleClipboardOverlay {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search_editor.focus_handle(cx)
    }
}

impl Render for WaffleClipboardOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_on_first_render {
            window.focus(&self.search_editor.focus_handle(cx), cx);
            self.focus_on_first_render = false;
        }
        // Read the controller so this entity stays bound to its output state.
        let _open = self.controller.borrow().state(self.display_id);
        let query = self.query.clone();
        let query_actions = {
            let tx = self.actions.clone();
            Arc::new(move |query| {
                let _ = tx.try_send(ClipboardAction::Query(query));
            }) as Arc<dyn Fn(String) + Send + Sync>
        };
        self.search_editor.update(cx, |editor, cx| {
            editor.sync_controlled_query(query, query_actions, cx)
        });
        let display = self.display_id;
        let backdrop_events = self.events.clone();
        let key_events = self.events.clone();
        let backdrop = div()
            .id("waffle.clipboard.backdrop")
            .absolute()
            .inset_0()
            .on_click(move |_, _, _| {
                let _ = backdrop_events.try_send(ClipboardEvent::Close { display });
            });
        let mut panel = div()
            .id("waffle.clipboard.positioned-panel")
            .absolute()
            .right(px(8.))
            .child(
                div()
                    .max_h(px(self.available_height))
                    .child(clipboard::render(self.input())),
            );
        panel = if self.bar_bottom {
            panel.top(px(8.))
        } else {
            panel.bottom(px(56.))
        };
        div()
            .id("waffle.clipboard.overlay")
            .size_full()
            .focusable()
            .tab_stop(true)
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "escape" {
                    let _ = key_events.try_send(ClipboardEvent::Close { display });
                    cx.stop_propagation();
                }
            })
            .child(backdrop)
            .child(panel)
    }
}
