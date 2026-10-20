//! Presentational Material ii clipboard history panel.
//!
//! `modules/clipboard/ClipboardPanel.qml` owns clipboard-service behavior and
//! localization. This view accepts a sanitized snapshot, renders the Material
//! ii surface, and emits typed requests. It never reads clipboard data or runs
//! commands. In particular, preview text is deliberately omitted from `Debug`
//! implementations and this module does not log entry content.

use std::sync::Arc;

use gpui::{
    AnyElement, ClickEvent, FocusHandle, FontWeight, InteractiveElement, KeyDownEvent, Pixels,
    Role, ScrollHandle, SharedString, StatefulInteractiveElement, div, prelude::*, px,
};

use crate::theme::MaterialThemeContext;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClipboardViewState {
    Loading,
    Unavailable,
    Empty,
    Populated,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipboardSearchMode {
    /// Show only entries and pins whose preview contains the query.
    #[default]
    Filter,
    /// Preserve all rows and mark matches; Tab and Shift+Tab cycle matches.
    Navigate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardRowSource {
    History,
    Pinned,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardEntry {
    /// Stable provider identity. The host maps it to its opaque source record.
    pub id: String,
    /// Sanitized display preview only; never use this value for mutations.
    pub preview: String,
    pub kind_label: String,
    pub is_image: bool,
    pub pinnable: bool,
    pub pinned: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardPinnedEntry {
    /// Stable host-owned pin identity. The underlying pin text stays outside
    /// this view and is never included in emitted actions.
    pub id: String,
    pub preview: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardRow {
    pub id: String,
    pub preview: String,
    pub kind_label: String,
    pub source: ClipboardRowSource,
    pub is_image: bool,
    pub is_match: bool,
    pub pinnable: bool,
    pub pinned: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardActionSource {
    History,
    Pinned,
}

/// Requests emitted to the surface host. Provider success/failure and panel
/// lifecycle remain host-owned; emitting an action is not an acknowledgement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardAction {
    SetQuery(String),
    SetSearchMode(ClipboardSearchMode),
    Select(usize),
    Activate {
        id: String,
        source: ClipboardActionSource,
    },
    DeleteHistory(String),
    SetPinned {
        id: String,
        pinned: bool,
    },
    RequestClear,
    ConfirmClear,
    CancelClear,
    Refresh,
    Dismiss,
    ToggleKeyboardHints,
}

#[derive(Clone)]
pub struct ClipboardLabels {
    pub title: String,
    pub search_placeholder: String,
    pub loading: String,
    pub unavailable: String,
    pub empty: String,
    pub no_results: String,
    pub pinned: String,
    pub image: String,
    pub filter_mode: String,
    pub navigate_mode: String,
    pub matches: String,
    pub keyboard_hints: String,
    pub clear: String,
    pub clear_question: String,
    pub confirm: String,
    pub cancel: String,
    pub refresh: String,
    pub close: String,
    pub pin: String,
    pub unpin: String,
    pub delete: String,
    pub unavailable_detail: String,
    pub empty_detail: String,
    pub no_results_detail: String,
    pub hints: String,
}

pub struct ClipboardViewInput {
    pub state: ClipboardViewState,
    pub status_detail: Option<String>,
    pub entries: Vec<ClipboardEntry>,
    pub pinned: Vec<ClipboardPinnedEntry>,
    pub query: String,
    pub search_mode: ClipboardSearchMode,
    /// Index in the current visible row model, matching QML ListView semantics.
    pub selected: usize,
    pub clear_confirmation: bool,
    pub show_keyboard_hints: bool,
    pub labels: ClipboardLabels,
    pub theme: MaterialThemeContext,
    pub focus: FocusHandle,
    /// When present, keys entered in the search field are distinguished from
    /// panel navigation. Arrow keys and Enter intentionally still navigate,
    /// matching QML's searchField key handlers; printable j/k remain text.
    pub search_focus: Option<FocusHandle>,
    pub scroll: ScrollHandle,
    /// The host supplies a GPUI text editor and binds its query callback to
    /// `ClipboardAction::SetQuery`. Passing `None` leaves a labelled query
    /// display in place, useful while the host editor is not yet wired. When
    /// this is `Some`, `search_focus` must be the same editor's focus handle.
    pub search_editor: Option<AnyElement>,
    /// Bound after the panel host applies its output geometry.
    pub available_height: Pixels,
    pub on_action: Arc<dyn Fn(ClipboardAction) + Send + Sync>,
}

/// Build the same ordered model as QML: pins lead, then history. In navigate
/// mode every history row remains visible and is marked by the query; pins
/// remain pinned at the top and are retained whenever navigating.
pub fn build_rows(
    state: ClipboardViewState,
    entries: &[ClipboardEntry],
    pinned: &[ClipboardPinnedEntry],
    query: &str,
    mode: ClipboardSearchMode,
) -> Vec<ClipboardRow> {
    if !matches!(
        state,
        ClipboardViewState::Empty | ClipboardViewState::Populated
    ) {
        return Vec::new();
    }

    let needle = query.trim().to_lowercase();
    let has_query = !needle.is_empty();
    let mut rows = Vec::with_capacity(entries.len().saturating_add(pinned.len()));

    for pin in pinned {
        let is_match = !has_query || pin.preview.to_lowercase().contains(&needle);
        if is_match || mode == ClipboardSearchMode::Navigate {
            rows.push(ClipboardRow {
                id: pin.id.clone(),
                preview: pin.preview.clone(),
                kind_label: "pinned".into(),
                source: ClipboardRowSource::Pinned,
                is_image: false,
                is_match,
                pinnable: false,
                pinned: true,
            });
        }
    }

    for entry in entries {
        let is_match = !has_query || entry.preview.to_lowercase().contains(&needle);
        if is_match || mode == ClipboardSearchMode::Navigate {
            rows.push(ClipboardRow {
                id: entry.id.clone(),
                preview: entry.preview.clone(),
                kind_label: if entry.is_image {
                    "image".into()
                } else {
                    entry.kind_label.clone()
                },
                source: ClipboardRowSource::History,
                is_image: entry.is_image,
                is_match,
                pinnable: entry.pinnable,
                pinned: entry.pinned,
            });
        }
    }

    rows
}

/// Clamp keyboard selection to the visible row range. Empty models use zero
/// as the inert selection value, as in the QML ListView's `-1` empty state.
pub fn clamp_selection(selected: usize, row_count: usize) -> usize {
    selected.min(row_count.saturating_sub(1))
}

/// Cycle to the next or previous matching row, skipping non-matches. Returns
/// `None` when there is no distinct match to navigate to.
pub fn adjacent_match(rows: &[ClipboardRow], selected: usize, forward: bool) -> Option<usize> {
    let count = rows.len();
    if count == 0 {
        return None;
    }
    let start = selected.min(count - 1);
    for offset in 1..=count {
        let index = if forward {
            (start + offset) % count
        } else {
            (start + count - (offset % count)) % count
        };
        if rows[index].is_match {
            return Some(index);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct KeyModifiers {
    shift: bool,
    control: bool,
    alt: bool,
    platform: bool,
}

impl KeyModifiers {
    fn is_none(self) -> bool {
        !self.shift && !self.control && !self.alt && !self.platform
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PanelKeyAction {
    Dismiss,
    MovePrevious,
    MoveNext,
    Activate,
    RequestClear,
    DeleteSelected,
    CopySelected,
    TogglePin,
    NextMatch,
    PreviousMatch,
    Refresh,
    ToggleKeyboardHints,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyDispatch {
    Panel(PanelKeyAction),
    SearchEditor,
    Ignore,
}

/// Pure modifier/focus policy shared by the GPUI event handler and regression
/// tests. Search editing keeps Space/j/k; any Shift+Delete requests a clear
/// confirmation, and unmodified Delete removes the selected row.
fn key_dispatch_policy(
    key: &str,
    modifiers: KeyModifiers,
    search_has_focus: bool,
    mode: ClipboardSearchMode,
    query_active: bool,
    has_data: bool,
) -> KeyDispatch {
    if search_has_focus && matches!(key, "space" | "j" | "k") {
        return KeyDispatch::SearchEditor;
    }

    let panel_action = match key {
        "escape" => PanelKeyAction::Dismiss,
        "up" if !modifiers.control => PanelKeyAction::MovePrevious,
        "down" if !modifiers.control => PanelKeyAction::MoveNext,
        "j" if !modifiers.control && !search_has_focus => PanelKeyAction::MoveNext,
        "k" if !modifiers.control && !search_has_focus => PanelKeyAction::MovePrevious,
        "enter" | "return" | "space" => PanelKeyAction::Activate,
        "delete" if modifiers.shift => PanelKeyAction::RequestClear,
        "delete" if modifiers.is_none() => PanelKeyAction::DeleteSelected,
        "delete" => return KeyDispatch::Ignore,
        "c" if (modifiers.control || modifiers.platform) && !modifiers.shift && !modifiers.alt => {
            PanelKeyAction::CopySelected
        }
        "p" if modifiers.control && has_data => PanelKeyAction::TogglePin,
        "tab" if mode == ClipboardSearchMode::Navigate && query_active => PanelKeyAction::NextMatch,
        "backtab" if mode == ClipboardSearchMode::Navigate && query_active => {
            PanelKeyAction::PreviousMatch
        }
        "r" if modifiers.control => PanelKeyAction::Refresh,
        "f10" => PanelKeyAction::ToggleKeyboardHints,
        _ => return KeyDispatch::Ignore,
    };

    KeyDispatch::Panel(panel_action)
}

/// Render the Material ii clipboard panel. Service state, search editing,
/// persistence, copying, deletion, pinning, and dismissal are injected.
pub fn render(input: ClipboardViewInput) -> AnyElement {
    let theme = input.theme;
    let colors = &theme.colors;
    let primary = colors.primary.unwrap_or_else(|| gpui::rgb(0x6750A4).into());
    let on_surface = colors
        .on_surface
        .unwrap_or_else(|| gpui::rgb(0xF5F1FA).into());
    let muted = colors
        .on_surface_variant
        .unwrap_or_else(|| gpui::rgb(0xCAC4D0).into());
    let surface = colors
        .surface_container
        .or(colors.surface)
        .unwrap_or_else(|| gpui::rgb(0x211F26).into());
    let list_surface = colors
        .surface_container_low
        .or(colors.surface_variant)
        .unwrap_or_else(|| gpui::rgb(0x2B2930).into());
    let selected_color = colors
        .primary_container
        .or(colors.surface_container_high)
        .unwrap_or_else(|| gpui::rgb(0x4F465F).into());
    let selected_ink = colors
        .on_primary_container
        .or(colors.on_surface)
        .unwrap_or_else(|| gpui::rgb(0xF5F1FA).into());
    let border = colors
        .outline_variant
        .or(colors.outline)
        .unwrap_or_else(|| gpui::rgb(0x49454F).into());
    let danger = colors.error.unwrap_or_else(|| gpui::rgb(0xF2B8B5).into());
    let scale = theme.typography.size_scale.max(0.75);
    let spacing = theme.spacing;
    let radius = theme.shape.large.max(12.0);
    let rows = build_rows(
        input.state,
        &input.entries,
        &input.pinned,
        &input.query,
        input.search_mode,
    );
    let selected = clamp_selection(input.selected, rows.len());
    if !rows.is_empty() {
        input.scroll.scroll_to_item(selected);
    }
    let on_action = input.on_action.clone();

    let count_label = if input.state == ClipboardViewState::Unavailable {
        "—".to_owned()
    } else {
        rows.len().to_string()
    };
    let title = input.labels.title.clone();
    let header = div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(spacing.md * scale))
        .child(
            div()
                .font_family("Material Symbols Rounded")
                .text_size(px(24.0 * scale))
                .text_color(primary)
                .child("content_paste"),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(18.0 * scale))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(on_surface)
                .child(title),
        )
        .child(
            div()
                .text_sm()
                .text_color(muted)
                .child(format!("({count_label})")),
        );

    let mut search = div()
        .id("ii.clipboard.search")
        .w_full()
        .min_h(px(48.0 * scale))
        .flex()
        .items_center()
        .gap(px(spacing.sm * scale))
        .px(px(spacing.md * scale))
        .rounded(px(theme.shape.medium.max(8.0)))
        .bg(list_surface)
        .border_1()
        .border_color(border)
        .role(Role::Group)
        .aria_label(SharedString::from(input.labels.search_placeholder.clone()))
        .child(
            div()
                .font_family("Material Symbols Rounded")
                .text_color(muted)
                .child("search"),
        );
    if let Some(editor) = input.search_editor {
        search = search.child(div().flex_1().min_w_0().child(editor));
    } else {
        search = search.child(
            div()
                .id("ii.clipboard.search.fallback")
                .flex_1()
                .min_w_0()
                .text_sm()
                .text_color(muted)
                .role(Role::TextInput)
                .aria_label(SharedString::from(input.labels.search_placeholder.clone()))
                .child(if input.query.is_empty() {
                    input.labels.search_placeholder.clone()
                } else {
                    input.query.clone()
                }),
        );
    }

    let mut toolbar = div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(spacing.xs * scale));
    if !input.query.trim().is_empty() {
        let target_mode = match input.search_mode {
            ClipboardSearchMode::Filter => ClipboardSearchMode::Navigate,
            ClipboardSearchMode::Navigate => ClipboardSearchMode::Filter,
        };
        let mode_label = match input.search_mode {
            ClipboardSearchMode::Filter => input.labels.filter_mode.clone(),
            ClipboardSearchMode::Navigate => input.labels.navigate_mode.clone(),
        };
        let mode_action = on_action.clone();
        toolbar = toolbar.child(button(
            "ii.clipboard.search-mode",
            mode_label,
            None,
            &on_surface,
            &list_surface,
            radius,
            scale,
            move || mode_action(ClipboardAction::SetSearchMode(target_mode)),
        ));
        if input.search_mode == ClipboardSearchMode::Navigate {
            let matches = rows.iter().filter(|row| row.is_match).count();
            toolbar = toolbar.child(
                div()
                    .px(px(spacing.sm * scale))
                    .text_sm()
                    .text_color(muted)
                    .child(format!("{matches} {}", input.labels.matches)),
            );
        }
    }

    let on_action = input.on_action.clone();
    let input_has_data = matches!(
        input.state,
        ClipboardViewState::Empty | ClipboardViewState::Populated
    );
    let mut actions = div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(spacing.xs * scale));
    if matches!(
        input.state,
        ClipboardViewState::Empty | ClipboardViewState::Populated
    ) {
        if input.clear_confirmation {
            let confirm = on_action.clone();
            actions = actions.child(label(&input.labels.clear_question, danger, scale));
            actions = actions.child(button(
                "ii.clipboard.confirm-clear",
                input.labels.confirm.clone(),
                None,
                &selected_ink,
                &danger,
                radius,
                scale,
                move || confirm(ClipboardAction::ConfirmClear),
            ));
            let cancel = on_action.clone();
            actions = actions.child(button(
                "ii.clipboard.cancel-clear",
                input.labels.cancel.clone(),
                None,
                &on_surface,
                &list_surface,
                radius,
                scale,
                move || cancel(ClipboardAction::CancelClear),
            ));
        } else {
            let request = on_action.clone();
            actions = actions.child(button(
                "ii.clipboard.request-clear",
                input.labels.clear.clone(),
                None,
                &danger,
                &list_surface,
                radius,
                scale,
                move || request(ClipboardAction::RequestClear),
            ));
        }
    }
    let refresh = on_action.clone();
    actions = actions.child(button(
        "ii.clipboard.refresh",
        input.labels.refresh.clone(),
        None,
        &on_surface,
        &list_surface,
        radius,
        scale,
        move || refresh(ClipboardAction::Refresh),
    ));
    let hints = on_action.clone();
    actions = actions.child(button(
        "ii.clipboard.keyboard-hints",
        input.labels.keyboard_hints.clone(),
        None,
        &on_surface,
        &list_surface,
        radius,
        scale,
        move || hints(ClipboardAction::ToggleKeyboardHints),
    ));
    if !input.clear_confirmation {
        let dismiss = on_action.clone();
        actions = actions.child(button(
            "ii.clipboard.close",
            input.labels.close.clone(),
            None,
            &on_surface,
            &list_surface,
            radius,
            scale,
            move || dismiss(ClipboardAction::Dismiss),
        ));
    }

    let mut list = div()
        .id("ii.clipboard.list")
        .w_full()
        .min_h(px(160.0 * scale))
        .max_h(px(480.0 * scale))
        .flex()
        .flex_col()
        .gap(px(spacing.xs * scale))
        .p(px(spacing.sm * scale))
        .rounded(px(theme.shape.medium.max(8.0)))
        .bg(list_surface)
        .overflow_y_scroll()
        .track_scroll(&input.scroll)
        .role(Role::ListBox)
        .aria_label(SharedString::from(input.labels.title.clone()));

    if input.state == ClipboardViewState::Populated
        && let Some(detail) = input.status_detail.as_deref()
    {
        list = list.child(status(
            "Clipboard operation failed",
            Some(detail),
            muted,
            scale,
        ));
    }

    match input.state {
        ClipboardViewState::Loading => {
            list = list.child(status(&input.labels.loading, None, muted, scale));
        }
        ClipboardViewState::Unavailable => {
            list = list.child(status(
                &input.labels.unavailable,
                Some(
                    input
                        .status_detail
                        .as_deref()
                        .unwrap_or(&input.labels.unavailable_detail),
                ),
                muted,
                scale,
            ));
        }
        ClipboardViewState::Empty if rows.is_empty() => {
            list = list.child(status(
                if input.query.trim().is_empty() {
                    &input.labels.empty_detail
                } else {
                    &input.labels.no_results_detail
                },
                Some(if input.query.trim().is_empty() {
                    &input.labels.empty
                } else {
                    &input.labels.no_results
                }),
                muted,
                scale,
            ));
        }
        ClipboardViewState::Populated if rows.is_empty() => {
            list = list.child(status(
                &input.labels.no_results_detail,
                Some(&input.labels.no_results),
                muted,
                scale,
            ));
        }
        ClipboardViewState::Empty | ClipboardViewState::Populated => {
            for (index, row) in rows.iter().enumerate() {
                let is_selected = index == selected;
                let row_bg = if is_selected { selected_color } else { surface };
                let row_ink = if is_selected {
                    selected_ink
                } else {
                    on_surface
                };
                let row_muted = if is_selected { selected_ink } else { muted };
                let source = match row.source {
                    ClipboardRowSource::History => ClipboardActionSource::History,
                    ClipboardRowSource::Pinned => ClipboardActionSource::Pinned,
                };
                let row_kind = if row.source == ClipboardRowSource::Pinned {
                    input.labels.pinned.clone()
                } else if row.is_image {
                    input.labels.image.clone()
                } else {
                    row.kind_label.clone()
                };
                let pinned_accessibility_suffix =
                    if row.pinned && row.source == ClipboardRowSource::History {
                        format!(", {}", input.labels.pinned)
                    } else {
                        String::new()
                    };
                let selected_action = on_action.clone();
                let id = row.id.clone();
                let is_match = row.is_match;
                let mut item = div()
                    .id(SharedString::from(format!("ii.clipboard.row.{}", row.id)))
                    .w_full()
                    .min_h(px(58.0 * scale))
                    .flex()
                    .items_center()
                    .gap(px(spacing.sm * scale))
                    .px(px(spacing.md * scale))
                    .py(px(spacing.sm * scale))
                    .rounded(px(theme.shape.small.max(6.0)))
                    .bg(row_bg)
                    .opacity(if is_match { 1.0 } else { 0.45 })
                    .role(Role::ListBoxOption)
                    .aria_selected(is_selected)
                    .aria_label(SharedString::from(format!(
                        "{}: {}{}",
                        row_kind, row.preview, pinned_accessibility_suffix
                    )))
                    .on_click(move |_: &ClickEvent, _, _| {
                        selected_action(ClipboardAction::Activate {
                            id: id.clone(),
                            source,
                        });
                    });

                item = item.child(
                    div()
                        .w(px(48.0 * scale))
                        .flex_shrink_0()
                        .font_family("Material Symbols Rounded")
                        .text_size(px(20.0 * scale))
                        .text_color(if row.pinned { primary } else { row_muted })
                        .child(if row.pinned {
                            "keep"
                        } else if row.is_image {
                            "image"
                        } else {
                            "content_paste"
                        }),
                );
                item = item.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.0 * scale))
                        .child(
                            div()
                                .text_xs()
                                .text_color(row_muted)
                                .child(row_kind.clone()),
                        )
                        .child(
                            div()
                                .w_full()
                                .text_sm()
                                .text_color(row_ink)
                                .child(row.preview.clone()),
                        ),
                );

                if row.pinnable || row.pinned {
                    let pin = on_action.clone();
                    let id = row.id.clone();
                    let pinned = row.pinned;
                    let pin_label = if pinned {
                        input.labels.unpin.clone()
                    } else {
                        input.labels.pin.clone()
                    };
                    let pin_accessible_name =
                        format!("{}: {}: {}", pin_label, row_kind, row.preview);
                    item = item.child(button(
                        format!("ii.clipboard.pin.{}", row.id),
                        pin_label,
                        Some(pin_accessible_name),
                        &row_ink,
                        &surface,
                        theme.shape.small.max(6.0),
                        scale,
                        move || {
                            pin(ClipboardAction::SetPinned {
                                id: id.clone(),
                                pinned: !pinned,
                            })
                        },
                    ));
                }
                if row.source == ClipboardRowSource::History {
                    let delete = on_action.clone();
                    let id = row.id.clone();
                    let delete_accessible_name =
                        format!("{}: {}: {}", input.labels.delete, row_kind, row.preview);
                    item = item.child(button(
                        format!("ii.clipboard.delete.{}", row.id),
                        input.labels.delete.clone(),
                        Some(delete_accessible_name),
                        &row_ink,
                        &surface,
                        theme.shape.small.max(6.0),
                        scale,
                        move || delete(ClipboardAction::DeleteHistory(id.clone())),
                    ));
                }

                list = list.child(item);
            }
        }
    }

    let mut root = div()
        .id("ii.clipboard.panel")
        .w(px(600.0 * scale))
        .max_h(input.available_height)
        .flex()
        .flex_col()
        .gap(px(spacing.sm * scale))
        .p(px(spacing.md * scale))
        .rounded(px(radius))
        .bg(surface)
        .border_1()
        .border_color(border)
        .role(Role::Group)
        .aria_label(SharedString::from(input.labels.title.clone()))
        .focusable()
        .tab_stop(false)
        .child(header)
        .child(search)
        .child(toolbar)
        .child(actions)
        .child(list)
        .track_focus(&input.focus)
        .on_key_down({
            let action = input.on_action.clone();
            let rows = rows.clone();
            let mode = input.search_mode;
            let query_active = !input.query.trim().is_empty();
            let clear_confirmation = input.clear_confirmation;
            let search_focus = input.search_focus;
            move |event: &KeyDownEvent, window, cx| {
                if event.is_held {
                    return;
                }
                let search_has_focus = search_focus
                    .as_ref()
                    .is_some_and(|focus| focus.is_focused(window));
                let policy = key_dispatch_policy(
                    event.keystroke.key.as_str(),
                    KeyModifiers {
                        shift: event.keystroke.modifiers.shift,
                        control: event.keystroke.modifiers.control,
                        alt: event.keystroke.modifiers.alt,
                        platform: event.keystroke.modifiers.platform,
                    },
                    search_has_focus,
                    mode,
                    query_active,
                    input_has_data,
                );
                let KeyDispatch::Panel(panel_action) = policy else {
                    return;
                };
                let current = clamp_selection(selected, rows.len());
                match panel_action {
                    PanelKeyAction::Dismiss => action(ClipboardAction::Dismiss),
                    PanelKeyAction::MovePrevious => {
                        action(ClipboardAction::Select(current.saturating_sub(1)))
                    }
                    PanelKeyAction::MoveNext => {
                        if !rows.is_empty() {
                            action(ClipboardAction::Select((current + 1).min(rows.len() - 1)));
                        }
                    }
                    PanelKeyAction::Activate | PanelKeyAction::CopySelected => {
                        if let Some(row) = rows.get(current) {
                            action(ClipboardAction::Activate {
                                id: row.id.clone(),
                                source: match row.source {
                                    ClipboardRowSource::History => ClipboardActionSource::History,
                                    ClipboardRowSource::Pinned => ClipboardActionSource::Pinned,
                                },
                            });
                        }
                    }
                    PanelKeyAction::RequestClear => action(if clear_confirmation {
                        ClipboardAction::ConfirmClear
                    } else {
                        ClipboardAction::RequestClear
                    }),
                    PanelKeyAction::DeleteSelected => {
                        if let Some(row) = rows.get(current) {
                            match row.source {
                                ClipboardRowSource::Pinned => action(ClipboardAction::SetPinned {
                                    id: row.id.clone(),
                                    pinned: false,
                                }),
                                ClipboardRowSource::History => {
                                    action(ClipboardAction::DeleteHistory(row.id.clone()))
                                }
                            }
                        }
                    }
                    PanelKeyAction::TogglePin => {
                        if let Some(row) = rows.get(current) {
                            if row.pinnable || row.pinned {
                                action(ClipboardAction::SetPinned {
                                    id: row.id.clone(),
                                    pinned: !row.pinned,
                                });
                            }
                        }
                    }
                    PanelKeyAction::NextMatch => {
                        if let Some(index) = adjacent_match(&rows, current, true) {
                            action(ClipboardAction::Select(index));
                        }
                    }
                    PanelKeyAction::PreviousMatch => {
                        if let Some(index) = adjacent_match(&rows, current, false) {
                            action(ClipboardAction::Select(index));
                        }
                    }
                    PanelKeyAction::Refresh => action(ClipboardAction::Refresh),
                    PanelKeyAction::ToggleKeyboardHints => {
                        action(ClipboardAction::ToggleKeyboardHints)
                    }
                }
                cx.stop_propagation();
            }
        });

    if input.show_keyboard_hints {
        root = root.child(
            div()
                .w_full()
                .text_xs()
                .text_color(muted)
                .child(input.labels.hints),
        );
    }

    root.into_any_element()
}

fn button(
    id: impl Into<SharedString>,
    label: String,
    accessible_name: Option<String>,
    foreground: &gpui::Hsla,
    background: &gpui::Hsla,
    radius: f32,
    scale: f32,
    action: impl Fn() + Send + Sync + 'static,
) -> impl IntoElement {
    let label_for_accessibility = accessible_name.unwrap_or_else(|| label.clone());
    let key_action = Arc::new(action);
    let click_action = key_action.clone();
    div()
        .id(id.into())
        .px(px(10.0 * scale))
        .py(px(6.0 * scale))
        .rounded(px(radius))
        .bg(*background)
        .text_sm()
        .text_color(*foreground)
        .role(Role::Button)
        .aria_label(SharedString::from(label_for_accessibility))
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .hover({
            let background = *background;
            move |style| style.bg(background)
        })
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
        .child(label)
}

fn status(title: &str, detail: Option<&str>, muted: gpui::Hsla, scale: f32) -> AnyElement {
    let mut content = div()
        .id(SharedString::from(format!("ii.clipboard.status.{title}")))
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.0 * scale))
        .py(px(28.0 * scale))
        .role(Role::Status)
        .aria_label(SharedString::from(match detail {
            Some(detail) => format!("{title}. {detail}"),
            None => title.to_owned(),
        }))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(muted)
                .child(title.to_owned()),
        );
    if let Some(detail) = detail {
        content = content.child(div().text_sm().text_color(muted).child(detail.to_owned()));
    }
    content.into_any_element()
}

fn label(text: &str, color: gpui::Hsla, scale: f32) -> impl IntoElement {
    div()
        .px(px(8.0 * scale))
        .text_sm()
        .text_color(color)
        .child(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, preview: &str, pinned: bool) -> ClipboardEntry {
        ClipboardEntry {
            id: id.into(),
            preview: preview.into(),
            kind_label: "text".into(),
            is_image: false,
            pinnable: true,
            pinned,
        }
    }

    fn pin(id: &str, preview: &str) -> ClipboardPinnedEntry {
        ClipboardPinnedEntry {
            id: id.into(),
            preview: preview.into(),
        }
    }

    #[test]
    fn loading_and_unavailable_never_expose_stale_rows() {
        let entries = [entry("history-1", "private preview", false)];
        let pins = [pin("pin-1", "private pin")];
        for state in [ClipboardViewState::Loading, ClipboardViewState::Unavailable] {
            assert!(build_rows(state, &entries, &pins, "", ClipboardSearchMode::Filter).is_empty());
        }
    }

    #[test]
    fn filter_mode_places_matching_pins_before_matching_history() {
        let entries = [
            entry("history-hit", "Alpha clipboard", false),
            entry("history-miss", "unrelated", false),
        ];
        let pins = [pin("pin-miss", "other"), pin("pin-hit", "ALPHA pin")];
        let rows = build_rows(
            ClipboardViewState::Populated,
            &entries,
            &pins,
            " alpha ",
            ClipboardSearchMode::Filter,
        );
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["pin-hit", "history-hit"]
        );
        assert!(rows.iter().all(|row| row.is_match));
        assert_eq!(rows[0].source, ClipboardRowSource::Pinned);
        assert_eq!(rows[1].source, ClipboardRowSource::History);
    }

    #[test]
    fn navigate_mode_keeps_nonmatching_rows_marked_and_keeps_pin_order() {
        let entries = [
            entry("history-hit", "Needle", false),
            entry("history-miss", "Else", false),
        ];
        let pins = [pin("pin-miss", "Else pin"), pin("pin-hit", "Needle pin")];
        let rows = build_rows(
            ClipboardViewState::Populated,
            &entries,
            &pins,
            "needle",
            ClipboardSearchMode::Navigate,
        );
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["pin-miss", "pin-hit", "history-hit", "history-miss"]
        );
        assert_eq!(
            rows.iter().map(|row| row.is_match).collect::<Vec<_>>(),
            [false, true, true, false]
        );
    }

    #[test]
    fn empty_query_marks_every_row_and_does_not_drop_any_pin() {
        let rows = build_rows(
            ClipboardViewState::Populated,
            &[entry("history", "text", false)],
            &[pin("pin", "saved")],
            "   ",
            ClipboardSearchMode::Navigate,
        );
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.is_match));
    }

    #[test]
    fn pin_and_image_semantics_survive_row_mapping() {
        let mut image = entry("image", "image payload", true);
        image.is_image = true;
        image.pinnable = false;
        let rows = build_rows(
            ClipboardViewState::Populated,
            &[image],
            &[pin("pin", "saved text")],
            "",
            ClipboardSearchMode::Filter,
        );
        assert_eq!(rows[0].source, ClipboardRowSource::Pinned);
        assert!(rows[0].pinned);
        assert_eq!(rows[1].kind_label, "image");
        assert!(rows[1].pinned);
        assert!(!rows[1].pinnable);
    }

    #[test]
    fn selection_clamps_to_visible_rows_and_handles_empty_list() {
        assert_eq!(clamp_selection(4, 3), 2);
        assert_eq!(clamp_selection(0, 0), 0);
    }

    #[test]
    fn next_and_previous_match_cycle_while_skipping_nonmatches() {
        let rows = build_rows(
            ClipboardViewState::Populated,
            &[
                entry("hit-1", "query one", false),
                entry("miss", "other", false),
                entry("hit-2", "query two", false),
            ],
            &[],
            "query",
            ClipboardSearchMode::Navigate,
        );
        assert_eq!(adjacent_match(&rows, 0, true), Some(2));
        assert_eq!(adjacent_match(&rows, 2, true), Some(0));
        assert_eq!(adjacent_match(&rows, 0, false), Some(2));
        assert_eq!(adjacent_match(&rows, 2, false), Some(0));
    }

    #[test]
    fn next_and_previous_match_return_none_without_matches_or_rows() {
        let rows = build_rows(
            ClipboardViewState::Populated,
            &[entry("miss", "other", false)],
            &[],
            "absent",
            ClipboardSearchMode::Navigate,
        );
        assert_eq!(adjacent_match(&rows, 0, true), None);
        assert_eq!(adjacent_match(&[], 0, false), None);
    }

    #[test]
    fn search_editor_keeps_printable_navigation_keys_and_space_as_text() {
        let no_modifiers = KeyModifiers::default();
        for key in ["j", "k", "space"] {
            assert_eq!(
                key_dispatch_policy(
                    key,
                    no_modifiers,
                    true,
                    ClipboardSearchMode::Filter,
                    false,
                    true,
                ),
                KeyDispatch::SearchEditor,
                "{key} must reach the focused search editor"
            );
        }
        assert_eq!(
            key_dispatch_policy(
                "space",
                no_modifiers,
                false,
                ClipboardSearchMode::Filter,
                false,
                true,
            ),
            KeyDispatch::Panel(PanelKeyAction::Activate)
        );
        for key in ["up", "down", "enter"] {
            assert!(matches!(
                key_dispatch_policy(
                    key,
                    no_modifiers,
                    true,
                    ClipboardSearchMode::Filter,
                    false,
                    true,
                ),
                KeyDispatch::Panel(_)
            ));
        }
    }

    #[test]
    fn delete_dispatch_uses_qml_shift_bit_and_rejects_other_modified_delete() {
        let dispatch = |modifiers| {
            key_dispatch_policy(
                "delete",
                modifiers,
                false,
                ClipboardSearchMode::Filter,
                false,
                true,
            )
        };
        assert_eq!(
            dispatch(KeyModifiers::default()),
            KeyDispatch::Panel(PanelKeyAction::DeleteSelected)
        );
        assert_eq!(
            dispatch(KeyModifiers {
                shift: true,
                ..KeyModifiers::default()
            }),
            KeyDispatch::Panel(PanelKeyAction::RequestClear)
        );
        for modifiers in [
            KeyModifiers {
                shift: true,
                control: true,
                ..KeyModifiers::default()
            },
            KeyModifiers {
                shift: true,
                alt: true,
                ..KeyModifiers::default()
            },
            KeyModifiers {
                shift: true,
                platform: true,
                ..KeyModifiers::default()
            },
        ] {
            assert_eq!(
                dispatch(modifiers),
                KeyDispatch::Panel(PanelKeyAction::RequestClear)
            );
        }
        for modifiers in [
            KeyModifiers {
                control: true,
                ..KeyModifiers::default()
            },
            KeyModifiers {
                alt: true,
                ..KeyModifiers::default()
            },
            KeyModifiers {
                platform: true,
                ..KeyModifiers::default()
            },
            KeyModifiers {
                control: true,
                alt: true,
                ..KeyModifiers::default()
            },
        ] {
            assert_eq!(dispatch(modifiers), KeyDispatch::Ignore);
        }
    }

    #[test]
    fn empty_snapshot_has_no_rows_and_query_can_produce_no_results() {
        assert!(
            build_rows(
                ClipboardViewState::Empty,
                &[],
                &[],
                "",
                ClipboardSearchMode::Filter,
            )
            .is_empty()
        );
        assert!(
            build_rows(
                ClipboardViewState::Populated,
                &[entry("history", "unrelated", false)],
                &[],
                "absent",
                ClipboardSearchMode::Filter,
            )
            .is_empty()
        );
    }

    #[test]
    fn previews_and_pin_text_are_never_embedded_in_action_payloads() {
        let history_action = ClipboardAction::Activate {
            id: "provider-id".into(),
            source: ClipboardActionSource::History,
        };
        let pin_action = ClipboardAction::SetPinned {
            id: "pin-id".into(),
            pinned: false,
        };
        assert_eq!(
            history_action,
            ClipboardAction::Activate {
                id: "provider-id".into(),
                source: ClipboardActionSource::History,
            }
        );
        assert_eq!(
            pin_action,
            ClipboardAction::SetPinned {
                id: "pin-id".into(),
                pinned: false
            }
        );
    }
}
