//! Native popup view for ii Dock window cards. Ownership and timers stay in
//! `IiDockSurface`; this view only reports pointer and keyboard state.

use gpui::{App, IntoElement, KeyDownEvent, Render, Window, div, prelude::*};
use std::rc::Rc;

use super::dock_preview::{self, DockPreviewInput};

pub type PointerAction = Rc<dyn Fn(bool, &mut App)>;
pub type EscapeAction = Rc<dyn Fn(&mut App)>;

pub struct DockPreviewHost {
    pub input: DockPreviewInput,
    pub on_hover: PointerAction,
    pub on_escape: EscapeAction,
}

impl Render for DockPreviewHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let hover = self.on_hover.clone();
        let escape = self.on_escape.clone();
        div()
            .id("ii.dock.preview.host")
            .on_hover(move |entered, _, cx| hover(*entered, cx))
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if event.keystroke.key.eq_ignore_ascii_case("escape") {
                    escape(cx);
                }
            })
            .child(dock_preview::render(self.input.clone()))
            .into_any_element()
    }
}
