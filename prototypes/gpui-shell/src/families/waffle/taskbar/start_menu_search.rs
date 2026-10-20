//! Single-line GPUI text input used by the Waffle Start Menu search field.
//!
//! Text editing is owned here so GPUI's platform input bridge can provide
//! clipboard insertion, selection/caret ranges, and IME marked text. The full
//! query is emitted through the caller's controlled callback after edits.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, IntoElement, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, Render,
    Role, ShapedLine, Style, TextRun, UTF16Selection, UnderlineStyle, Window, div, fill, hsla,
    point, prelude::*, px, relative, rgba, size,
};

use super::start_menu::QueryAction;

pub struct StartMenuSearch {
    focus_handle: FocusHandle,
    external_focus_owner: bool,
    content: String,
    placeholder: String,
    accessible_label: String,
    selection: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    on_query_changed: QueryAction,
    pending_controlled_query: Option<String>,
    task_view_key_passthrough: bool,
}

impl StartMenuSearch {
    pub fn new(query: String, on_query_changed: QueryAction, cx: &mut Context<Self>) -> Self {
        Self::new_with_labels(
            query,
            "Search for apps".to_owned(),
            "Search apps, settings, and files".to_owned(),
            on_query_changed,
            cx,
        )
    }

    /// Construct a search editor with caller-owned placeholder and accessible
    /// name while preserving the shared IME and clipboard editing behavior.
    pub fn new_with_labels(
        query: String,
        placeholder: String,
        accessible_label: String,
        on_query_changed: QueryAction,
        cx: &mut Context<Self>,
    ) -> Self {
        let end = query.len();
        Self {
            focus_handle: cx.focus_handle(),
            external_focus_owner: false,
            content: query,
            placeholder,
            accessible_label,
            selection: end..end,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            selecting: false,
            on_query_changed,
            pending_controlled_query: None,
            task_view_key_passthrough: false,
        }
    }

    /// Construct an editor whose parent composite tracks this same focus
    /// handle. This allows sibling result widgets to be exposed as its active
    /// descendant while retaining the platform text input handler.
    pub fn new_with_shared_focus(
        query: String,
        placeholder: String,
        accessible_label: String,
        on_query_changed: QueryAction,
        focus_handle: FocusHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut editor =
            Self::new_with_labels(query, placeholder, accessible_label, on_query_changed, cx);
        editor.focus_handle = focus_handle;
        editor.external_focus_owner = true;
        editor
    }

    /// Forward key events from a composite that owns this editor's focus.
    pub fn handle_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_key(event, window, cx);
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<String>) {
        self.placeholder = placeholder.into();
    }

    /// Let the Task View parent handle compositor navigation and close keys
    /// while the search field owns keyboard focus.
    pub fn enable_task_view_key_passthrough(&mut self) {
        self.task_view_key_passthrough = true;
    }

    /// Keeps the editor controlled while avoiding rolling back a keystroke
    /// during the async interval before the adapter publishes its new snapshot.
    pub fn sync_controlled_query(
        &mut self,
        query: String,
        on_query_changed: QueryAction,
        cx: &mut Context<Self>,
    ) {
        self.on_query_changed = on_query_changed;
        if self.pending_controlled_query.as_deref() == Some(query.as_str()) {
            self.pending_controlled_query = None;
        } else if self.pending_controlled_query.is_some() {
            return;
        }

        if self.content != query {
            self.content = query;
            let end = self.content.len();
            self.selection = end..end;
            self.selection_reversed = false;
            self.marked_range = None;
            self.last_layout = None;
            cx.notify();
        }
    }

    /// True only when the editor and adapter snapshot agree. The launcher
    /// uses this to avoid navigating or launching results from an older query.
    pub fn confirms_query(&self, query: &str) -> bool {
        self.pending_controlled_query.is_none() && self.content == query
    }

    /// Current raw editor text, which may be newer than the adapter snapshot.
    pub fn current_query(&self) -> &str {
        &self.content
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content[..offset]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content[offset..]
            .char_indices()
            .nth(1)
            .map_or(self.content.len(), |(index, _)| offset + index)
    }

    fn move_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.selection = offset..offset;
        self.selection_reversed = false;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        if self.selection_reversed {
            self.selection.start = offset;
        } else {
            self.selection.end = offset;
        }
        if self.selection.end < self.selection.start {
            self.selection_reversed = !self.selection_reversed;
            self.selection = self.selection.end..self.selection.start;
        }
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (&self.last_bounds, &self.last_layout) else {
            return 0;
        };
        if position.x <= bounds.left() {
            return 0;
        }
        if position.x >= bounds.right() {
            return self.content.len();
        }
        self.clamp_byte_boundary(
            line.closest_index_for_x(position.x - bounds.left())
                .min(self.content.len()),
            false,
        )
    }

    fn delete_selection(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        self.replace_bytes(self.selection.clone(), "", window, cx);
        true
    }

    fn replace_bytes(
        &mut self,
        range: Range<usize>,
        replacement: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let start = self.clamp_byte_boundary(range.start.min(self.content.len()), false);
        let end = self
            .clamp_byte_boundary(range.end.min(self.content.len()), true)
            .max(start);
        self.content.replace_range(start..end, replacement);
        let caret = start + replacement.len();
        self.selection = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        self.pending_controlled_query = Some(self.content.clone());
        self.last_layout = None;
        window.notify_text_input_changed(&self.focus_handle);
        (self.on_query_changed)(self.content.clone());
        cx.notify();
    }

    fn edit_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if self.task_view_key_passthrough
            && is_task_view_parent_key(
                key,
                modifiers.shift,
                modifiers.control,
                modifiers.alt,
                modifiers.platform,
            )
        {
            return;
        }
        let shift = event.keystroke.modifiers.shift;
        let command = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        let handled = match key {
            "backspace" => {
                if !self.delete_selection(window, cx) {
                    let cursor = self.cursor_offset();
                    if cursor > 0 {
                        self.replace_bytes(self.previous_boundary(cursor)..cursor, "", window, cx);
                    }
                }
                true
            }
            "delete" => {
                if !self.delete_selection(window, cx) {
                    let cursor = self.cursor_offset();
                    if cursor < self.content.len() {
                        self.replace_bytes(cursor..self.next_boundary(cursor), "", window, cx);
                    }
                }
                true
            }
            "left" => {
                let cursor = self.cursor_offset();
                let target = if shift || self.selection.is_empty() {
                    self.previous_boundary(cursor)
                } else if self.selection_reversed {
                    self.selection.start
                } else {
                    self.selection.start
                };
                if shift {
                    self.select_to(target, window, cx)
                } else {
                    self.move_to(target, window, cx)
                }
                true
            }
            "right" => {
                let cursor = self.cursor_offset();
                let target = if shift || self.selection.is_empty() {
                    self.next_boundary(cursor)
                } else {
                    self.selection.end
                };
                if shift {
                    self.select_to(target, window, cx)
                } else {
                    self.move_to(target, window, cx)
                }
                true
            }
            "home" => {
                if shift {
                    self.select_to(0, window, cx)
                } else {
                    self.move_to(0, window, cx)
                }
                true
            }
            "end" => {
                let end = self.content.len();
                if shift {
                    self.select_to(end, window, cx)
                } else {
                    self.move_to(end, window, cx)
                }
                true
            }
            "a" if command => {
                self.selection = 0..self.content.len();
                self.selection_reversed = false;
                window.notify_text_selection_changed(&self.focus_handle);
                cx.notify();
                true
            }
            "v" if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace_text_in_range(None, &text.replace(['\n', '\r'], " "), window, cx);
                }
                true
            }
            "c" if command => {
                if !self.selection.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.content[self.selection.clone()].to_owned(),
                    ));
                }
                true
            }
            "x" if command => {
                if !self.selection.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.content[self.selection.clone()].to_owned(),
                    ));
                    self.delete_selection(window, cx);
                }
                true
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selecting = true;
        window.focus(&self.focus_handle, cx);
        window.request_virtual_keyboard();
        let offset = self.index_for_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, window, cx);
        } else {
            self.move_to(offset, window, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selecting {
            let offset = self.index_for_position(event.position);
            self.select_to(offset, window, cx);
        }
    }

    fn utf16_to_byte(&self, offset: usize, round_up: bool) -> usize {
        utf16_offset_to_byte(&self.content, offset, round_up)
    }

    fn byte_to_utf16(&self, offset: usize) -> usize {
        let offset = self.clamp_byte_boundary(offset.min(self.content.len()), false);
        self.content[..offset].encode_utf16().count()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        let start = self.utf16_to_byte(range.start, false);
        let end = self.utf16_to_byte(range.end, true).max(start);
        start..end
    }

    fn clamp_byte_boundary(&self, offset: usize, round_up: bool) -> usize {
        if self.content.is_char_boundary(offset) {
            return offset;
        }
        let mut previous = 0;
        for (boundary, _) in self.content.char_indices() {
            if boundary > offset {
                return if round_up { boundary } else { previous };
            }
            previous = boundary;
        }
        if round_up {
            self.content.len()
        } else {
            previous
        }
    }
}

fn is_task_view_parent_key(
    key: &str,
    shift: bool,
    control: bool,
    alt: bool,
    platform: bool,
) -> bool {
    !shift && !control && !alt && !platform && matches!(key, "left" | "right" | "delete")
}

fn utf16_offset_to_byte(text: &str, offset: usize, round_up: bool) -> usize {
    let mut utf16 = 0;
    let mut byte = 0;
    for ch in text.chars() {
        let next_utf16 = utf16 + ch.len_utf16();
        if offset < next_utf16 {
            return if offset == utf16 || !round_up {
                byte
            } else {
                byte + ch.len_utf8()
            };
        }
        utf16 = next_utf16;
        byte += ch.len_utf8();
    }
    byte
}

impl EntityInputHandler for StartMenuSearch {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let bytes = self.range_from_utf16(&range);
        adjusted.replace(self.byte_to_utf16(bytes.start)..self.byte_to_utf16(bytes.end));
        Some(self.content[bytes].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.byte_to_utf16(self.selection.start)..self.byte_to_utf16(self.selection.end),
            reversed: self.selection_reversed,
        })
    }

    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let reversed = self.selection_reversed && range.start != range.end;
        self.selection = self.range_from_utf16(&range);
        self.selection_reversed = reversed;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.content.encode_utf16().count())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.byte_to_utf16(range.start)..self.byte_to_utf16(range.end))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bytes = range
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selection.clone());
        self.replace_bytes(bytes, text, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bytes = range
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selection.clone());
        let start = bytes.start;
        self.content.replace_range(bytes.clone(), text);
        let inserted_utf16 = text.encode_utf16().count();
        self.selection = selected
            .map(|range| {
                let local_start =
                    utf16_offset_to_byte(text, range.start.min(inserted_utf16), false);
                let local_end = utf16_offset_to_byte(text, range.end.min(inserted_utf16), true);
                (start + local_start)..(start + local_end).min(start + text.len())
            })
            .unwrap_or_else(|| start + text.len()..start + text.len());
        self.selection_reversed = false;
        self.marked_range = (!text.is_empty()).then_some(start..start + text.len());
        self.pending_controlled_query = Some(self.content.clone());
        self.last_layout = None;
        window.notify_text_input_changed(&self.focus_handle);
        (self.on_query_changed)(self.content.clone());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range);
        let bounds = self.last_bounds?;
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn selection_bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        self.bounds_for_range(range, bounds, window, cx)
            .into_iter()
            .collect()
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        Some(self.byte_to_utf16(line.closest_index_for_x(point.x - bounds.left())))
    }
}

impl Focusable for StartMenuSearch {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for StartMenuSearch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .id("waffle.start-menu.search-input")
            .w_full()
            .h(px(54.))
            .flex()
            .items_center()
            .key_context("WaffleStartMenuSearch");
        if !self.external_focus_owner {
            root = root
                .track_focus(&self.focus_handle(cx))
                .focusable()
                .tab_stop(true);
        }
        root.cursor(CursorStyle::IBeam)
            .role(Role::TextInput)
            .aria_label(self.accessible_label.clone())
            .on_key_down(cx.listener(Self::edit_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .child(SearchTextElement { input: cx.entity() }),
            )
    }
}

#[cfg(test)]
mod task_view_key_policy_tests {
    use super::is_task_view_parent_key;

    #[test]
    fn task_view_parent_key_policy_is_limited_to_navigation_and_close() {
        assert!(is_task_view_parent_key("left", false, false, false, false));
        assert!(is_task_view_parent_key("right", false, false, false, false));
        assert!(is_task_view_parent_key(
            "delete", false, false, false, false
        ));
        assert!(!is_task_view_parent_key("left", true, false, false, false));
        assert!(!is_task_view_parent_key("right", false, true, false, false));
        assert!(!is_task_view_parent_key(
            "delete", false, false, true, false
        ));
        assert!(!is_task_view_parent_key(
            "delete", false, false, false, true
        ));
        assert!(!is_task_view_parent_key(
            "backspace",
            false,
            false,
            false,
            false
        ));
        assert!(!is_task_view_parent_key("tab", false, false, false, false));
        assert!(!is_task_view_parent_key("a", false, false, false, false));
    }
}

struct SearchTextElement {
    input: Entity<StartMenuSearch>,
}
struct SearchTextPrepaint {
    line: Option<ShapedLine>,
    selection: Option<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for SearchTextElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for SearchTextElement {
    type RequestLayoutState = ();
    type PrepaintState = SearchTextPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> SearchTextPrepaint {
        let input = self.input.read(cx);
        let empty = input.content.is_empty();
        let shown = if empty {
            input.placeholder.clone()
        } else {
            input.content.clone()
        };
        let text_style = window.text_style();
        let base_run = TextRun {
            len: shown.len(),
            font: text_style.font(),
            color: if empty {
                hsla(0., 0., 0.5, 0.72)
            } else {
                text_style.color
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if !empty {
            if let Some(marked) = input.marked_range.as_ref() {
                vec![
                    TextRun {
                        len: marked.start,
                        ..base_run.clone()
                    },
                    TextRun {
                        len: marked.end - marked.start,
                        underline: Some(UnderlineStyle {
                            color: Some(base_run.color),
                            thickness: px(1.),
                            wavy: false,
                        }),
                        ..base_run.clone()
                    },
                    TextRun {
                        len: shown.len() - marked.end,
                        ..base_run
                    },
                ]
                .into_iter()
                .filter(|run| run.len > 0)
                .collect()
            } else {
                vec![base_run]
            }
        } else {
            vec![base_run]
        };
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(shown.into(), font_size, &runs, None);
        let cursor = if empty { 0 } else { input.cursor_offset() };
        let (selection, cursor_quad) = if input.selection.is_empty() || empty {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + line.x_for_index(cursor), bounds.top()),
                        size(px(1.5), bounds.size.height),
                    ),
                    text_style.color,
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(input.selection.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(input.selection.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0x664a90e2),
                )),
                None,
            )
        };
        SearchTextPrepaint {
            line: Some(line),
            selection,
            cursor: cursor_quad,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut SearchTextPrepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        if let Some(line) = prepaint.line.take() {
            let _ = line.paint(
                bounds.origin,
                window.line_height(),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
            if focus_handle.is_focused(window) {
                if let Some(cursor) = prepaint.cursor.take() {
                    window.paint_quad(cursor);
                }
            }
            self.input.update(cx, |input, _| {
                input.last_layout = Some(line);
                input.last_bounds = Some(bounds);
            });
        }
    }
}
