//! Output-sized GPUI host for Waffle quick settings.

use std::sync::Arc;

use async_channel::Sender;
use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, Render, ScrollHandle, Window, div,
    prelude::*, px,
};

use crate::{families::waffle::controller::ActionCenterEvent, theme::ThemeTokens};

use super::{
    action_center::{self, ActionCenterInput, ActionCenterStyle, LoadState},
    controller::ActionCenterController,
    runtime::{start_menu_style, taskbar_style},
};

pub struct WaffleActionCenterOverlay {
    theme: ThemeTokens,
    events: Sender<ActionCenterEvent>,
    controller: std::rc::Rc<std::cell::RefCell<ActionCenterController>>,
    display_id: gpui::DisplayId,
    bar_bottom: bool,
    available_height: f32,
    scroll_handle: ScrollHandle,
    focus_handle: FocusHandle,
    focus_on_first_render: bool,
}

impl WaffleActionCenterOverlay {
    pub fn new(
        theme: ThemeTokens,
        events: Sender<ActionCenterEvent>,
        controller: std::rc::Rc<std::cell::RefCell<ActionCenterController>>,
        display_id: gpui::DisplayId,
        bar_bottom: bool,
        available_height: f32,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            theme,
            events,
            controller,
            display_id,
            bar_bottom,
            available_height,
            scroll_handle: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            focus_on_first_render: true,
        }
    }

    fn input(&self) -> ActionCenterInput {
        let taskbar = taskbar_style(&self.theme);
        let menu = start_menu_style(&self.theme, &taskbar);
        let noop = Arc::new(|| {}) as Arc<dyn Fn() + Send + Sync>;
        let page = self.controller.borrow().page(self.display_id);
        let events = self.events.clone();
        let display = self.display_id;
        ActionCenterInput {
            style: ActionCenterStyle {
                background: menu.background,
                panel: menu.surface,
                surface: menu.surface,
                hover: menu.surface_hover,
                foreground: menu.foreground,
                muted: menu.muted,
                accent: menu.accent,
                separator: menu.border,
                radius: menu.radius,
                scale: 1.,
            },
            page,
            page_count: 0,
            toggles: Vec::new(),
            toggle_state: LoadState::Unavailable,
            sliders: Vec::new(),
            media: None,
            media_state: LoadState::Unavailable,
            audio_choices: Vec::new(),
            audio_state: LoadState::Unavailable,
            on_close: Arc::new({
                let events = events.clone();
                move || {
                    let _ = events.try_send(ActionCenterEvent::Close { display });
                }
            }),
            on_page: Arc::new(move |page| {
                let _ = events.try_send(ActionCenterEvent::SelectPage { display, page });
            }),
            on_toggle: Arc::new(|_| {}),
            on_slider: Arc::new(|_, _| {}),
            on_previous: Arc::clone(&noop),
            on_play_pause: Arc::clone(&noop),
            on_next: Arc::clone(&noop),
            on_seek: Arc::new(|_| {}),
            on_choose_audio: Arc::new(|_| {}),
        }
    }
}

impl Focusable for WaffleActionCenterOverlay {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WaffleActionCenterOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_on_first_render {
            window.focus(&self.focus_handle, cx);
            self.focus_on_first_render = false;
        }
        let display = self.display_id;
        let mut panel = div()
            .id("waffle.action-center.panel")
            .absolute()
            .right(px(8.))
            .child(
                div()
                    .id("waffle.action-center.scroll")
                    .max_h(px(self.available_height))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .child(action_center::render(self.input(), window, cx)),
            );
        panel = if self.bar_bottom {
            panel.top(px(8.))
        } else {
            panel.bottom(px(56.))
        };
        let backdrop_events = self.events.clone();
        let key_events = self.events.clone();
        let backdrop = div()
            .id("waffle.action-center.backdrop")
            .absolute()
            .inset_0()
            .on_click(move |_, _, _| {
                let _ = backdrop_events.try_send(ActionCenterEvent::Close { display });
            });
        div()
            .id("waffle.action-center.overlay")
            .size_full()
            .focusable()
            .tab_stop(true)
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "escape" {
                    let _ = key_events.try_send(ActionCenterEvent::Close { display });
                    cx.stop_propagation();
                }
            })
            .child(backdrop)
            .child(panel)
    }
}
