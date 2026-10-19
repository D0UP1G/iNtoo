//! Data-driven GPUI presentation for the ii Dock.
//!
//! The QML dock combines pinned desktop entries, compositor windows, hover
//! previews and context actions. This component accepts resolved app/task
//! records and callbacks; source ordering, pin persistence, launch and window
//! activation stay owned by the host adapters.

use std::{cell::RefCell, path::PathBuf, sync::Arc};

use gpui::img;
use gpui::{
    AnyElement, App, ClickEvent, Hsla, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Role, SharedString, Window, div, prelude::*, px,
};
use std::rc::Rc;

pub type IdAction = Rc<dyn Fn(String, &mut App)>;
pub type Action = Arc<dyn Fn() + Send + Sync>;
pub type HoverAction = Rc<dyn Fn(Option<String>, bool, &mut Window, &mut App)>;
pub type DockReorderAction = Rc<dyn Fn(DockDrop, &mut App)>;
pub type DockDragStatusAction = Rc<dyn Fn(&mut App)>;

#[derive(Debug, Clone)]
pub struct DockDragItem {
    pub id: String,
    pub name: String,
    pub pinned: bool,
    pub running: bool,
    pub display_index: usize,
}

#[derive(Debug, Clone)]
pub struct DockDrop {
    pub source: DockDragItem,
    pub target: DockDragItem,
}

struct DockDragSession {
    generation: u64,
    source: DockDragItem,
    target: DockDragItem,
    start: Point<Pixels>,
    primed: bool,
    active: bool,
}

/// Shared between dock cells so a re-render during pointer movement does not
/// lose the in-progress drag. It deliberately stores no persisted order.
#[derive(Default)]
pub struct DockDragState {
    next_generation: u64,
    session: Option<DockDragSession>,
    suppress_release_click_at: Option<Point<Pixels>>,
    cancelled_pointer_release: bool,
    status_generation: u64,
    completion_status: Option<String>,
}

impl DockDragState {
    fn start(&mut self, item: DockDragItem, position: Point<Pixels>) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let generation = self.next_generation;
        self.session = Some(DockDragSession {
            generation,
            source: item.clone(),
            target: item,
            start: position,
            primed: false,
            active: false,
        });
        self.suppress_release_click_at = None;
        self.cancelled_pointer_release = false;
        self.status_generation = self.status_generation.wrapping_add(1).max(1);
        self.completion_status = None;
        generation
    }

    fn prime(&mut self, generation: u64) {
        if let Some(session) = self.session.as_mut()
            && session.generation == generation
        {
            session.primed = true;
        }
    }

    fn observe_move(&mut self, position: Point<Pixels>, target: Option<DockDragItem>) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let dx = f32::from(position.x - session.start.x);
        let dy = f32::from(position.y - session.start.y);
        let crossed_threshold = dx * dx + dy * dy > 18.0 * 18.0;
        if crossed_threshold && !session.primed {
            // QML stops priming when the pointer slips more than 18px before
            // 180ms, preserving the eventual release as an ordinary click.
            self.session = None;
            return false;
        }
        let just_activated = crossed_threshold && session.primed && !session.active;
        if crossed_threshold && session.primed {
            session.active = true;
        }
        if !session.active {
            return false;
        }
        let mut changed = just_activated;
        if let Some(target) = target
            && (session.target.id != target.id
                || session.target.display_index != target.display_index)
        {
            session.target = target;
            changed = true;
        }
        changed
    }

    fn observe_separator(
        &mut self,
        position: Point<Pixels>,
        vertical: bool,
        before: Option<DockDragItem>,
        after: Option<DockDragItem>,
    ) -> bool {
        let activated = self.observe_move(position, None);
        if !self.is_active() {
            return activated;
        }
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        let forward = if vertical {
            position.y > session.start.y
        } else {
            position.x > session.start.x
        };
        let target = if forward {
            after.or(before)
        } else {
            before.or(after)
        };
        let Some(target) = target else { return false };
        self.set_target(target) || activated
    }

    fn set_target(&mut self, target: DockDragItem) -> bool {
        let Some(session) = self.session.as_mut().filter(|session| session.active) else {
            return false;
        };
        if session.target.id == target.id && session.target.display_index == target.display_index {
            return false;
        }
        session.target = target;
        true
    }

    pub fn is_active(&self) -> bool {
        self.session.as_ref().is_some_and(|session| session.active)
    }

    pub fn announce_completion(&mut self, message: String) -> u64 {
        self.status_generation = self.status_generation.wrapping_add(1).max(1);
        self.completion_status = Some(message);
        self.status_generation
    }

    pub fn clear_completion(&mut self, generation: u64) -> bool {
        if generation != self.status_generation || self.completion_status.is_none() {
            return false;
        }
        self.completion_status = None;
        true
    }

    pub fn replace_completion_if_current(
        &mut self,
        generation: u64,
        message: String,
    ) -> Option<u64> {
        if generation != self.status_generation {
            return None;
        }
        Some(self.announce_completion(message))
    }

    fn completion_status(&self) -> Option<String> {
        self.completion_status.clone()
    }

    fn active_source(&self) -> Option<DockDragItem> {
        self.session
            .as_ref()
            .filter(|session| session.active)
            .map(|session| session.source.clone())
    }

    fn finish(&mut self, release: &MouseUpEvent) -> Option<DockDrop> {
        let session = self.session.take()?;
        if !session.active {
            return None;
        }
        self.suppress_release_click_at = Some(release.position);
        Some(DockDrop {
            source: session.source,
            target: session.target,
        })
    }

    fn cancel(&mut self) -> bool {
        self.suppress_release_click_at = None;
        let Some(session) = self.session.take() else {
            self.cancelled_pointer_release = false;
            return false;
        };
        self.cancelled_pointer_release = true;
        session.active
    }

    fn expire(&mut self, generation: u64) -> bool {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.generation == generation)
        {
            return self.cancel();
        }
        false
    }

    fn consume_cancelled_pointer_release(&mut self) -> bool {
        std::mem::take(&mut self.cancelled_pointer_release)
    }

    fn clear_cancelled_pointer_release(&mut self) {
        self.cancelled_pointer_release = false;
    }

    fn suppress_click_for_release(&mut self, release: &MouseUpEvent) {
        self.suppress_release_click_at = Some(release.position);
    }

    fn consume_suppressed_click(&mut self, event: &ClickEvent) -> bool {
        let ClickEvent::Mouse(event) = event else {
            return false;
        };
        if event.down.button != MouseButton::Left
            || self.suppress_release_click_at != Some(event.up.position)
        {
            return false;
        }
        self.suppress_release_click_at = None;
        true
    }

    fn clear_release_click_suppression(&mut self) {
        self.suppress_release_click_at = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DockEdge {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

impl DockEdge {
    fn vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Debug, Clone)]
pub struct DockStyle {
    pub surface: Hsla,
    pub item: Hsla,
    pub item_hover: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub border: Hsla,
    pub radius: f32,
    pub spacing: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DockGeometry {
    /// Output-local bounds provided by the surface host.
    pub width: f32,
    pub height: f32,
    pub edge: DockEdge,
    pub icon_size: f32,
}

#[derive(Debug, Clone)]
pub struct DockTask {
    pub id: u64,
    pub title: String,
    pub focused: bool,
    pub urgent: bool,
}

#[derive(Debug, Clone)]
pub struct DockApp {
    pub id: String,
    pub name: String,
    /// Resolved icon glyph; leave absent when the icon theme adapter cannot
    /// provide one, in which case a neutral marker is used.
    pub glyph: Option<String>,
    /// Existing themed XDG icon image resolved by the host adapter.
    pub icon_path: Option<PathBuf>,
    pub pinned: bool,
    pub tasks: Vec<DockTask>,
}

#[derive(Clone)]
pub struct DockInput {
    pub geometry: DockGeometry,
    pub style: DockStyle,
    pub apps: Vec<DockApp>,
    /// QML's `separatePinnedFromRunning` mode puts pin-only apps before all
    /// running apps while preserving first-open order for that running group.
    pub separate_pinned_from_running: bool,
    pub show_all_window_dots: bool,
    pub smart_indicator: bool,
    pub max_indicator_dots: usize,
    pub drag_reorder_enabled: bool,
    pub drag_state: Rc<RefCell<DockDragState>>,
    pub on_reorder: Option<DockReorderAction>,
    pub on_drag_status_changed: Option<DockDragStatusAction>,
    pub on_activate_app: Option<IdAction>,
    pub on_open_launcher: Option<Action>,
    /// Hover notifications for the app cell (`Some(app_id)`) and dock body (`None`).
    pub on_hover: Option<HoverAction>,
}

/// Render an output-local ii Dock body. The host controls visibility, screen
/// filtering, hover reveal, blur, and layer-shell anchoring.
pub fn render(input: DockInput) -> AnyElement {
    let edge = input.geometry.edge;
    let vertical = edge.vertical();
    let icon_size = input.geometry.icon_size.clamp(24.0, 72.0);
    let slot = icon_size + 12.0;
    let style = input.style.clone();

    let mut entries = Vec::<AnyElement>::new();
    if let Some(activate) = input.on_open_launcher.clone() {
        entries.push(
            action_cell(
                "ii.dock.launcher",
                "Open applications",
                "▦",
                None,
                slot,
                icon_size,
                &style,
                false,
                0,
                None,
                false,
                false,
                Some(Rc::new(move |_: &mut App| activate()) as Rc<dyn Fn(&mut App)>),
                true,
                true,
                None,
                false,
                input.drag_state.clone(),
                None,
                None,
            )
            .into_any_element(),
        );
    }

    let mut saw_pinned_only = false;
    let mut saw_pinned_entry = false;
    let mut inserted_separator = false;
    let mut display_index = 0usize;
    let mut previous_item: Option<DockDragItem> = None;
    for app in &input.apps {
        let separator_before_app = if input.separate_pinned_from_running {
            !inserted_separator && saw_pinned_only && !app.tasks.is_empty()
        } else {
            !inserted_separator && saw_pinned_entry && !app.pinned && !app.tasks.is_empty()
        };
        if separator_before_app {
            let before = previous_item.clone();
            let after = DockDragItem {
                id: app.id.clone(),
                name: app.name.clone(),
                pinned: app.pinned,
                running: !app.tasks.is_empty(),
                display_index: display_index.saturating_add(1),
            };
            let separator_drag_state = input.drag_state.clone();
            let separator_status_changed = input.on_drag_status_changed.clone();
            entries.push(
                div()
                    .id("ii.dock.pinned-running-separator")
                    .when(vertical, |this| this.w(px(1.0)).h(px(18.0)))
                    .when(!vertical, |this| this.w(px(18.0)).h(px(1.0)))
                    .rounded_full()
                    .bg(style.border)
                    .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                        if event.pressed_button == Some(MouseButton::Left)
                            && separator_drag_state.borrow_mut().observe_separator(
                                event.position,
                                vertical,
                                before.clone(),
                                Some(after.clone()),
                            )
                        {
                            if let Some(status_changed) = separator_status_changed.as_ref() {
                                status_changed(cx);
                            } else {
                                cx.notify(window.current_view());
                            }
                        }
                    })
                    .into_any_element(),
            );
            inserted_separator = true;
            display_index = display_index.saturating_add(1);
        }
        saw_pinned_only |= app.pinned && app.tasks.is_empty();
        saw_pinned_entry |= app.pinned;
        let app_action = input.on_activate_app.clone().map(|action| {
            let app_id = app.id.clone();
            Rc::new(move |cx: &mut App| action(app_id.clone(), cx)) as Rc<dyn Fn(&mut App)>
        });
        let label = app_accessible_label(app);
        let glyph = app.glyph.clone().unwrap_or_else(|| "•".into());
        let drag_item = DockDragItem {
            id: app.id.clone(),
            name: app.name.clone(),
            pinned: app.pinned,
            running: !app.tasks.is_empty(),
            display_index,
        };
        let app_cell = action_cell(
            SharedString::from(format!("ii.dock.app.{}", app.id)),
            label,
            glyph,
            app.icon_path.clone(),
            slot,
            icon_size,
            &style,
            !app.tasks.is_empty(),
            app.tasks.len().min(input.max_indicator_dots),
            app.tasks.iter().position(|task| task.focused),
            app.tasks.iter().any(|task| task.urgent),
            app.pinned,
            app_action,
            input.show_all_window_dots,
            input.smart_indicator,
            input.on_hover.clone().map(|hover| {
                let id = app.id.clone();
                Rc::new(move |entered, window: &mut Window, cx: &mut App| {
                    hover(Some(id.clone()), entered, window, cx)
                }) as Rc<dyn Fn(bool, &mut Window, &mut App)>
            }),
            input.drag_reorder_enabled,
            input.drag_state.clone(),
            input.on_drag_status_changed.clone(),
            Some(drag_item.clone()),
        );
        entries.push(
            div()
                .id(SharedString::from(format!("ii.dock.group.{}", app.id)))
                .flex()
                .when(vertical, |this| this.flex_col())
                .when(!vertical, |this| this.flex_row())
                .items_center()
                .child(app_cell)
                .into_any_element(),
        );
        previous_item = Some(drag_item);
        display_index = display_index.saturating_add(1);
    }

    let status = {
        let drag_state = input.drag_state.borrow();
        if let Some(dragging) = drag_state.active_source() {
            let target = drag_state
                .session
                .as_ref()
                .map(|session| session.target.name.clone())
                .unwrap_or_else(|| "target".to_owned());
            Some(format!(
                "Moving {}. Drop on {} to reorder or pin. Press Escape to cancel.",
                dragging.name, target
            ))
        } else {
            drag_state.completion_status()
        }
    };
    if let Some(status) = status {
        entries.push(
            div()
                .id("ii.dock.drag.status")
                .role(Role::Status)
                .aria_label(status.clone())
                .absolute()
                .when(vertical, |this| this.left(px(48.0)).top(px(0.0)))
                .when(!vertical, |this| this.left(px(0.0)).top(px(-26.0)))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(style.radius.min(8.0)))
                .bg(style.surface)
                .border_1()
                .border_color(style.accent)
                .text_size(px(11.0))
                .text_color(style.foreground)
                .child(SharedString::from(status))
                .into_any_element(),
        );
    }

    let move_state = input.drag_state.clone();
    let release_state = input.drag_state.clone();
    let outside_state = input.drag_state.clone();
    let escape_state = input.drag_state.clone();
    let reorder = input.on_reorder.clone();
    let status_changed = input.on_drag_status_changed.clone();
    let move_status_changed = status_changed.clone();

    let dock = div()
        .id("ii.dock")
        .role(Role::Group)
        .aria_label("ii Dock")
        .flex()
        .when(vertical, |this| this.flex_col())
        .when(!vertical, |this| this.flex_row())
        .items_center()
        .justify_center()
        .gap(px(style.spacing.max(0.0)))
        .px(px(8.0))
        .py(px(8.0))
        .rounded(px(style.radius))
        .bg(style.surface)
        .border_1()
        .border_color(style.border)
        .when_some(input.on_hover.clone(), |this, hover| {
            this.on_hover(move |entered, window, cx| hover(None, *entered, window, cx))
        })
        .when(input.drag_reorder_enabled, |this| {
            this.on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                if event.pressed_button == Some(MouseButton::Left)
                    && move_state.borrow_mut().observe_move(event.position, None)
                {
                    if let Some(status_changed) = move_status_changed.as_ref() {
                        status_changed(cx);
                    } else {
                        cx.notify(window.current_view());
                    }
                }
            })
            .capture_any_mouse_up({
                let release_state = release_state.clone();
                let reorder = reorder.clone();
                let status_changed = status_changed.clone();
                move |event: &MouseUpEvent, window, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    let drop = {
                        let mut state = release_state.borrow_mut();
                        if state.consume_cancelled_pointer_release() {
                            state.suppress_click_for_release(event);
                            None
                        } else {
                            state.finish(event)
                        }
                    };
                    if let (Some(drop), Some(reorder)) = (drop, reorder.as_ref()) {
                        reorder(drop, cx);
                    }
                    if let Some(status_changed) = status_changed.as_ref() {
                        status_changed(cx);
                    } else {
                        cx.notify(window.current_view());
                    }
                    let release_state = release_state.clone();
                    cx.defer(move |_| release_state.borrow_mut().clear_release_click_suppression());
                }
            })
            .on_mouse_up_out(MouseButton::Left, {
                let status_changed = status_changed.clone();
                move |_, window, cx| {
                    let changed = outside_state.borrow_mut().cancel();
                    outside_state.borrow_mut().clear_cancelled_pointer_release();
                    if changed {
                        if let Some(status_changed) = status_changed.as_ref() {
                            status_changed(cx);
                        } else {
                            cx.notify(window.current_view());
                        }
                    }
                }
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key.eq("escape") && escape_state.borrow_mut().cancel() {
                    cx.stop_propagation();
                    if let Some(status_changed) = status_changed.as_ref() {
                        status_changed(cx);
                    } else {
                        cx.notify(window.current_view());
                    }
                }
            })
        })
        .relative()
        .children(entries);

    div()
        .id("ii.dock.output")
        .w(px(input.geometry.width.max(1.0)))
        .h(px(input.geometry.height.max(1.0)))
        .flex()
        .when(vertical, |this| {
            this.flex_row()
                .items_center()
                .when(edge == DockEdge::Left, |this| this.justify_start())
                .when(edge == DockEdge::Right, |this| this.justify_end())
        })
        .when(!vertical, |this| {
            this.flex_col()
                .items_center()
                .when(edge == DockEdge::Top, |this| this.justify_start())
                .when(edge == DockEdge::Bottom, |this| this.justify_end())
        })
        .child(dock)
        .into_any_element()
}

fn action_cell(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    glyph: impl Into<SharedString>,
    icon_path: Option<PathBuf>,
    slot: f32,
    icon_size: f32,
    style: &DockStyle,
    running: bool,
    window_count: usize,
    focused_index: Option<usize>,
    urgent: bool,
    pinned: bool,
    activate: Option<Rc<dyn Fn(&mut App)>>,
    show_all_window_dots: bool,
    smart_indicator: bool,
    on_hover: Option<Rc<dyn Fn(bool, &mut Window, &mut App)>>,
    drag_reorder_enabled: bool,
    drag_state: Rc<RefCell<DockDragState>>,
    on_drag_status_changed: Option<DockDragStatusAction>,
    drag_item: Option<DockDragItem>,
) -> impl IntoElement {
    let id = id.into();
    let label = label.into();
    let glyph = glyph.into();
    let dragging = drag_item.as_ref().is_some_and(|item| {
        drag_state
            .borrow()
            .active_source()
            .is_some_and(|source| source.id.eq_ignore_ascii_case(&item.id))
    });
    let accessible_label = if dragging {
        format!("{label}, moving; release over an app to reorder or pin; press Escape to cancel")
    } else {
        label.to_string()
    };
    div()
        .id(id)
        .aria_label(accessible_label)
        .hover({
            let color = style.item_hover;
            move |this| this.bg(color)
        })
        .when_some(activate, |this, activate| {
            let click_activate = activate.clone();
            this.role(Role::Button)
                .focusable()
                .tab_stop(true)
                .focus_visible({
                    let accent = style.accent;
                    move |style| style.border_1().border_color(accent)
                })
                .on_click({
                    let drag_state = drag_state.clone();
                    move |event, _, cx| {
                        if !drag_state.borrow_mut().consume_suppressed_click(event) {
                            click_activate(cx)
                        }
                    }
                })
                .on_key_down(move |event: &KeyDownEvent, _, cx| {
                    if is_activate_key(event) {
                        activate(cx);
                    }
                })
        })
        .when(drag_reorder_enabled, |this| {
            let start_state = drag_state.clone();
            let move_state = drag_state.clone();
            let prime_state = drag_state.clone();
            let expiry_state = drag_state.clone();
            let expiry_status_changed = on_drag_status_changed.clone();
            let move_status_changed = on_drag_status_changed.clone();
            let source_item = drag_item.clone();
            let target_item = drag_item.clone();
            this.on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
                let Some(source_item) = source_item.clone() else {
                    return;
                };
                let generation = start_state.borrow_mut().start(source_item, event.position);
                let prime_state = prime_state.clone();
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(180))
                        .await;
                    prime_state.borrow_mut().prime(generation);
                })
                .detach();
                let expiry_state = expiry_state.clone();
                let status_changed = expiry_status_changed.clone();
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(30))
                        .await;
                    if expiry_state.borrow_mut().expire(generation)
                        && let Some(status_changed) = status_changed
                    {
                        cx.update(|cx| status_changed(cx));
                    }
                })
                .detach();
            })
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    let target = target_item.clone();
                    if move_state.borrow_mut().observe_move(event.position, target) {
                        if let Some(status_changed) = move_status_changed.as_ref() {
                            status_changed(cx);
                        } else {
                            cx.notify(window.current_view());
                        }
                    }
                }
            })
            .when(dragging, |this| this.border_1().border_color(style.accent))
        })
        .when_some(on_hover, |this, hover| {
            this.on_hover(move |entered, window, cx| hover(*entered, window, cx))
        })
        .w(px(slot))
        .h(px(slot))
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px((icon_size + 8.0) / 2.0))
        .when(focused_index.is_some(), |this| {
            this.border_1().border_color(style.accent)
        })
        .when(urgent && focused_index.is_none(), |this| {
            this.border_1().border_color(style.foreground)
        })
        .when(focused_index.is_none() && !urgent, |this| {
            this.border_1().border_color(style.border)
        })
        .bg(if focused_index.is_some() {
            style.item_hover
        } else {
            style.item
        })
        .text_color(style.foreground)
        .child(if let Some(path) = icon_path {
            div()
                .w(px(icon_size.min(32.0)))
                .h(px(icon_size.min(32.0)))
                .child(img(path).size_full())
                .into_any_element()
        } else {
            div()
                .text_size(px(icon_size.min(32.0)))
                .child(glyph)
                .into_any_element()
        })
        .when(pinned, |this| {
            this.child(
                div()
                    .absolute()
                    .top(px(3.0))
                    .right(px(3.0))
                    .w(px(4.0))
                    .h(px(4.0))
                    .rounded_full()
                    .bg(style.muted),
            )
        })
        .when(running, |this| {
            let dot_count = if show_all_window_dots || focused_index.is_some() {
                window_count.max(1)
            } else {
                0
            };
            let indicators = (0..dot_count).map(|index| {
                let is_active =
                    focused_index.is_some_and(|focused| !smart_indicator || focused == index);
                div()
                    .w(px(if is_active { 10.0 } else { 4.0 }))
                    .h(px(4.0))
                    .rounded_full()
                    .bg(if is_active { style.accent } else { style.muted })
            });
            this.when(dot_count > 0, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(px(2.0))
                        .flex()
                        .justify_center()
                        .gap(px(3.0))
                        .children(indicators),
                )
            })
            .when(!show_all_window_dots && focused_index.is_none(), |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(px(2.0))
                        .flex()
                        .justify_center()
                        .child(div().w(px(5.0)).h(px(5.0)).rounded_full().bg(style.muted)),
                )
            })
        })
}

fn app_accessible_label(app: &DockApp) -> String {
    let mut parts = vec![app.name.clone()];
    if app.pinned {
        parts.push("pinned".into());
    }
    if !app.tasks.is_empty() {
        parts.push(format!("{} windows", app.tasks.len()));
    }
    if app.tasks.iter().any(|task| task.focused) {
        parts.push("focused".into());
    }
    if app.tasks.iter().any(|task| task.urgent) {
        parts.push("urgent".into());
    }
    parts.join(", ")
}

fn is_activate_key(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held
}
