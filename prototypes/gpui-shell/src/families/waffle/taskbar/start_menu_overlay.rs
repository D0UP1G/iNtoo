//! Full-output GPUI overlay composition for the Waffle Start Menu.
//!
//! The host owns this view's layer-shell window and visibility. The supplied
//! StartMenuInput callbacks keep ownership of search data and app actions;
//! `on_close` is installed as the menu's close action and is also used for
//! clicks on the transparent backdrop.

use std::sync::Arc;

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, Pixels, Render, Size, Window, div,
    prelude::*, px,
};

use super::{
    start_menu::{self, MenuAction, StartMenuInput},
    start_menu_search::StartMenuSearch,
};

const MENU_WIDTH: f32 = 600.;
const HORIZONTAL_MARGIN: f32 = 16.;
const BAR_GAP: f32 = 8.;

/// Placement values needed to position the menu inside a full-output surface.
#[derive(Clone, Copy, Debug)]
pub struct StartMenuOverlayAnchor {
    pub display_size: Size<Pixels>,
    pub bar_bottom: bool,
    pub left_align_apps: bool,
    pub bar_height: Pixels,
}

/// A controlled Start Menu view intended to be the root of an output-sized
/// `Layer::Overlay` surface. It does not create or configure its own window.
pub struct WaffleStartMenuOverlay {
    input: StartMenuInput,
    search_input: Entity<StartMenuSearch>,
    anchor: StartMenuOverlayAnchor,
    on_close: MenuAction,
    needs_initial_focus: bool,
}

impl WaffleStartMenuOverlay {
    /// Build the overlay contents. The caller retains ownership of the source
    /// callbacks and window lifecycle; this view only invokes them.
    pub fn new(
        mut input: StartMenuInput,
        anchor: StartMenuOverlayAnchor,
        on_close: MenuAction,
        cx: &mut Context<Self>,
    ) -> Self {
        input.on_close = Arc::clone(&on_close);
        let search_input = cx.new(|cx| {
            StartMenuSearch::new(input.query.clone(), input.on_query_changed.clone(), cx)
        });
        Self {
            input,
            search_input,
            anchor,
            on_close,
            needs_initial_focus: true,
        }
    }
}

impl Focusable for WaffleStartMenuOverlay {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search_input.focus_handle(cx)
    }
}

impl Render for WaffleStartMenuOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.needs_initial_focus {
            window.focus(&self.search_input.focus_handle(cx), cx);
            self.needs_initial_focus = false;
        }
        let query = self.input.query.clone();
        let on_query_changed = self.input.on_query_changed.clone();
        self.search_input.update(cx, |search, cx| {
            search.sync_controlled_query(query, on_query_changed, cx);
        });

        let close = Arc::clone(&self.on_close);
        let search = self.search_input.read(cx);
        let current_query = search.current_query().to_owned();
        let results_current = search.confirms_query(&self.input.query);
        let display_width = f32::from(self.anchor.display_size.width);
        let display_height = f32::from(self.anchor.display_size.height);
        let menu_width = MENU_WIDTH.min((display_width - HORIZONTAL_MARGIN * 2.).max(1.));
        let max_left = (display_width - menu_width - HORIZONTAL_MARGIN).max(HORIZONTAL_MARGIN);
        let centered_left = ((display_width - menu_width) / 2.).clamp(HORIZONTAL_MARGIN, max_left);
        let left = if self.anchor.left_align_apps {
            HORIZONTAL_MARGIN.min(max_left)
        } else {
            centered_left
        };
        let menu_height =
            (display_height - f32::from(self.anchor.bar_height) - BAR_GAP - HORIZONTAL_MARGIN * 2.)
                .max(1.);
        let edge_gap = self.anchor.bar_height + px(BAR_GAP);
        let mut menu = div().absolute().left(px(left));
        menu = if self.anchor.bar_bottom {
            menu.bottom(edge_gap)
        } else {
            menu.top(edge_gap)
        };

        div()
            .id("waffle.start-menu.overlay")
            .relative()
            .size_full()
            .child(
                div()
                    .id("waffle.start-menu.backdrop")
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .left_0()
                    .on_click(move |_, _, _| close()),
            )
            .child(menu.child(start_menu::render_sized(
                self.input.clone(),
                self.search_input.clone(),
                self.search_input.focus_handle(cx),
                current_query,
                results_current,
                px(menu_width),
                px(menu_height),
            )))
    }
}
