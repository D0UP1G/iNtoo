//! Presentational Material Cheatsheet keybind browser.
//!
//! The Niri/Hyprland provider is deliberately outside this component. A host
//! passes a snapshot and owns refresh/lifecycle; this module only filters and
//! renders the records it receives.

use std::sync::Arc;

use gpui::{
    AnyElement, FocusHandle, FontWeight, KeyDownEvent, Role, SharedString,
    StatefulInteractiveElement, div, prelude::*, px, rgba,
};

use crate::theme::MaterialThemeContext;

pub type QueryChange = Arc<dyn Fn(String) + Send + Sync>;
/// The host editor and its focus handle are an inseparable pair. The handle
/// must belong to this editor so the page-level find shortcut can target it.
pub struct CheatsheetSearchInput {
    pub editor: AnyElement,
    pub focus: FocusHandle,
}

/// Hosts must allocate at least this much horizontal space for the keybind
/// page. At this width the title, search control, clear action and rows fit.
pub const CHEATSHEET_MIN_WIDTH: f32 = 640.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeybindProviderState {
    Loading,
    Ready,
    /// Provider successfully loaded and reported no keybinds.
    Empty,
    /// Provider is not supported or could not be contacted.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindRecord {
    pub key: String,
    pub modifiers: Vec<String>,
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindCategory {
    pub name: String,
    pub bindings: Vec<KeybindRecord>,
}

#[derive(Debug, Clone)]
pub struct KeybindSnapshot {
    pub state: KeybindProviderState,
    pub categories: Vec<KeybindCategory>,
    pub detail: Option<String>,
}

pub struct CheatsheetKeybindsInput {
    pub snapshot: KeybindSnapshot,
    /// Controlled search text. The host updates this from the editor callback.
    pub query: String,
    /// Optional host-created editor/focus pair. When omitted, the view shows a
    /// non-interactive placeholder; query changes still come from the host.
    pub search: Option<CheatsheetSearchInput>,
    pub theme: MaterialThemeContext,
    pub labels: CheatsheetLabels,
    /// Called with the replacement query; Escape and the clear button send an
    /// empty string. This is not a provider/search acknowledgement.
    pub on_query_change: QueryChange,
}

impl std::fmt::Debug for CheatsheetKeybindsInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CheatsheetKeybindsInput")
            .field("snapshot", &self.snapshot)
            .field("query", &self.query)
            .field("has_search", &self.search.is_some())
            .field("theme", &self.theme)
            .field("labels", &self.labels)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct CheatsheetLabels {
    pub title: String,
    pub search_placeholder: String,
    pub loading: String,
    pub unavailable: String,
    pub empty: String,
    pub no_results: String,
    pub no_results_detail: String,
    pub clear_search: String,
}

impl Default for CheatsheetLabels {
    fn default() -> Self {
        Self {
            title: "Keybinds".into(),
            search_placeholder: "Search keybinds…".into(),
            loading: "Loading keybinds…".into(),
            unavailable: "Keybinds are unavailable".into(),
            empty: "No keybinds configured".into(),
            no_results: "No matches found".into(),
            no_results_detail: "Try a different search term".into(),
            clear_search: "Clear search".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredKeybind {
    pub category: String,
    pub binding: KeybindRecord,
}

/// Apply QML's case-insensitive key/modifier/comment/category matching.
/// Whitespace around the query is ignored; source order is retained.
pub fn filter_keybinds(categories: &[KeybindCategory], query: &str) -> Vec<FilteredKeybind> {
    let needle = query.trim().to_lowercase();
    categories
        .iter()
        .flat_map(|category| {
            category
                .bindings
                .iter()
                .filter(|binding| {
                    needle.is_empty()
                        || binding.key.to_lowercase().contains(&needle)
                        || binding
                            .modifiers
                            .iter()
                            .any(|modifier| modifier.to_lowercase().contains(&needle))
                        || binding.comment.to_lowercase().contains(&needle)
                        || category.name.to_lowercase().contains(&needle)
                })
                .map(|binding| FilteredKeybind {
                    category: category.name.clone(),
                    binding: binding.clone(),
                })
        })
        .collect()
}

fn total_keybind_count(categories: &[KeybindCategory]) -> usize {
    categories
        .iter()
        .map(|category| category.bindings.len())
        .sum()
}

fn should_show_clear(query: &str) -> bool {
    !query.is_empty()
}

fn effective_provider_state(snapshot: &KeybindSnapshot) -> KeybindProviderState {
    match snapshot.state {
        KeybindProviderState::Ready
            if snapshot
                .categories
                .iter()
                .all(|category| category.bindings.is_empty()) =>
        {
            KeybindProviderState::Empty
        }
        state => state,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProviderStatusMessage {
    Blocking(String),
    Diagnostic(String),
}

fn provider_status_message(
    snapshot: &KeybindSnapshot,
    labels: &CheatsheetLabels,
) -> Option<ProviderStatusMessage> {
    match effective_provider_state(snapshot) {
        KeybindProviderState::Loading => Some(ProviderStatusMessage::Blocking(
            snapshot
                .detail
                .clone()
                .unwrap_or_else(|| labels.loading.clone()),
        )),
        KeybindProviderState::Empty => Some(ProviderStatusMessage::Blocking(
            snapshot
                .detail
                .clone()
                .unwrap_or_else(|| labels.empty.clone()),
        )),
        KeybindProviderState::Unavailable => Some(ProviderStatusMessage::Blocking(
            snapshot
                .detail
                .clone()
                .unwrap_or_else(|| labels.unavailable.clone()),
        )),
        KeybindProviderState::Ready => snapshot
            .detail
            .as_ref()
            .filter(|detail| !detail.trim().is_empty())
            .cloned()
            .map(ProviderStatusMessage::Diagnostic),
    }
}

/// Minimal host API: mount this element in a scroll-capable parent of at least
/// [`CHEATSHEET_MIN_WIDTH`] and provide the provider snapshot, controlled
/// query/editor pair, resolved Material tokens, localized labels, and callback.
/// The host must give the actual editor an accessible name, bind the editor's
/// changes to `query`/`on_query_change`, register window-level Ctrl+F to focus
/// it while this page is active, and persist/restore the Cheatsheet page index.
/// The parent also owns periodic-table page navigation and tab persistence.
pub fn render_cheatsheet_keybinds(input: CheatsheetKeybindsInput) -> AnyElement {
    let colors = &input.theme.colors;
    let surface = colors
        .surface_container_low
        .or(colors.surface)
        .unwrap_or_else(|| rgba(0xf4f1f8ff).into());
    let card = colors
        .surface_container
        .or(colors.surface_variant)
        .unwrap_or_else(|| rgba(0xece6f0ff).into());
    let foreground = colors
        .on_surface
        .or(input.theme.overrides.foreground)
        .unwrap_or_else(|| rgba(0x1b1b1fff).into());
    let muted = colors
        .on_surface_variant
        .or(input.theme.overrides.foreground_muted)
        .unwrap_or_else(|| rgba(0x49454fff).into());
    let accent = colors
        .primary
        .or(input.theme.overrides.accent)
        .unwrap_or_else(|| rgba(0x6750a4ff).into());
    let border = colors
        .outline_variant
        .or(input.theme.overrides.border)
        .unwrap_or_else(|| rgba(0x79747e66).into());
    let query = input.query.clone();
    let is_searching = !query.trim().is_empty();
    let show_clear = should_show_clear(&input.query);
    let filtered = if is_searching {
        filter_keybinds(&input.snapshot.categories, &query)
    } else {
        Vec::new()
    };
    let displayed_count = if is_searching {
        filtered.len()
    } else {
        total_keybind_count(&input.snapshot.categories)
    };
    let labels = input.labels;
    let status = provider_status_message(&input.snapshot, &labels);
    let query_for_key = input.query.clone();
    let search_focus = input.search.as_ref().map(|search| search.focus.clone());
    let search_editor = input.search.map(|search| search.editor);
    let clear_on_key = input.on_query_change.clone();
    let clear_on_click = input.on_query_change.clone();
    let clear_button_keyboard = input.on_query_change.clone();
    let clear_for_no_results = input.on_query_change.clone();
    let mut root = div()
        .id("ii.cheatsheet.keybinds")
        .w_full()
        .h_full()
        .min_w(px(CHEATSHEET_MIN_WIDTH))
        .min_h_0()
        .flex()
        .flex_col()
        .gap(px(12.))
        .p(px(16.))
        .bg(surface)
        .role(Role::Group)
        .aria_label("Cheatsheet keybind browser")
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if !event.is_held
                && event.keystroke.key == "f"
                && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
            {
                if let Some(focus) = &search_focus {
                    focus.focus(window, cx);
                    cx.stop_propagation();
                }
            } else if !query_for_key.is_empty() && event.keystroke.key == "escape" {
                clear_on_key(String::new());
                cx.stop_propagation();
            }
        })
        .child(
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .font_family("Material Symbols Rounded")
                        .text_lg()
                        .text_color(accent)
                        .child("keyboard"),
                )
                .child(
                    div()
                        .id("ii.cheatsheet.keybinds.heading")
                        .text_lg()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(foreground)
                        .role(Role::Heading)
                        .aria_level(1)
                        .child(format!("{} ({})", labels.title, displayed_count)),
                )
                .child(div().flex_1())
                .child(render_search_slot(
                    search_editor,
                    &labels.search_placeholder,
                    &muted,
                    &border,
                ))
                .when(show_clear, |row| {
                    row.child(
                        div()
                            .id("ii.cheatsheet.keybinds.clear")
                            .px(px(10.))
                            .h(px(32.))
                            .flex()
                            .items_center()
                            .rounded(px(16.))
                            .bg(card)
                            .text_sm()
                            .text_color(foreground)
                            .role(Role::Button)
                            .aria_label(SharedString::from(labels.clear_search.clone()))
                            .focusable()
                            .tab_stop(true)
                            .cursor_pointer()
                            .on_click(move |_, _, _| clear_on_click(String::new()))
                            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                                if !event.is_held
                                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                                {
                                    clear_button_keyboard(String::new());
                                    cx.stop_propagation();
                                }
                            })
                            .child(labels.clear_search.clone()),
                    )
                }),
        );

    match status {
        Some(ProviderStatusMessage::Blocking(message)) => {
            return root
                .child(status_card(message, &card, &muted, &border))
                .into_any_element();
        }
        Some(ProviderStatusMessage::Diagnostic(message)) => {
            root = root.child(diagnostic_banner(message, &card, &muted, &border));
        }
        None => {}
    }
    if is_searching && filtered.is_empty() {
        return root
            .child(no_results_card(
                &labels,
                &card,
                &foreground,
                &muted,
                &border,
                clear_for_no_results,
            ))
            .into_any_element();
    }

    if is_searching {
        root = root.child(
            div()
                .id("ii.cheatsheet.keybinds.filtered")
                .w_full()
                .min_h_0()
                .flex_1()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .p(px(8.))
                .gap(px(0.))
                .bg(card)
                .rounded(px(input.theme.shape.medium))
                .border_1()
                .border_color(border)
                .role(Role::List)
                .aria_label(SharedString::from(format!(
                    "{} search results, {displayed_count} keybinds",
                    labels.title
                )))
                .children(filtered.into_iter().enumerate().map(|(index, item)| {
                    render_keybind_row(
                        &item.category,
                        &item.binding,
                        format!("ii.cheatsheet.keybinds.search-row.{index}"),
                        index + 1 < displayed_count,
                        &foreground,
                        &muted,
                        &border,
                        &input.theme.typography.monospace,
                    )
                })),
        );
    } else {
        root = root.child(
            div()
                .id("ii.cheatsheet.keybinds.categories")
                .w_full()
                .min_h_0()
                .flex_1()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(12.))
                .role(Role::Group)
                .aria_label(SharedString::from(format!(
                    "{} categories, {displayed_count} keybinds",
                    labels.title
                )))
                .children(input.snapshot.categories.iter().map(|category| {
                    render_category(
                        category,
                        &card,
                        &foreground,
                        &muted,
                        &accent,
                        &border,
                        &input.theme,
                    )
                })),
        );
    }
    root.into_any_element()
}

fn render_search_slot(
    editor: Option<AnyElement>,
    placeholder: &str,
    muted: &gpui::Hsla,
    border: &gpui::Hsla,
) -> AnyElement {
    match editor {
        Some(editor) => div()
            .id("ii.cheatsheet.keybinds.search")
            .w(px(250.))
            .h(px(36.))
            .flex()
            .items_center()
            .px(px(8.))
            .rounded(px(8.))
            .border_1()
            .border_color(*border)
            .role(Role::Search)
            .aria_label(SharedString::from(placeholder.to_owned()))
            .child(editor)
            .into_any_element(),
        None => div()
            .id("ii.cheatsheet.keybinds.search.unwired")
            .w(px(250.))
            .h(px(36.))
            .flex()
            .items_center()
            .px(px(10.))
            .rounded(px(8.))
            .border_1()
            .border_color(*border)
            .role(Role::Search)
            .aria_label(SharedString::from(placeholder.to_owned()))
            .text_sm()
            .text_color(*muted)
            .child(placeholder.to_owned())
            .into_any_element(),
    }
}

fn status_card(
    message: String,
    card: &gpui::Hsla,
    muted: &gpui::Hsla,
    border: &gpui::Hsla,
) -> AnyElement {
    div()
        .id("ii.cheatsheet.keybinds.provider-status")
        .w_full()
        .h(px(200.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.))
        .border_1()
        .border_color(*border)
        .bg(*card)
        .text_color(*muted)
        .role(Role::Status)
        .aria_label(SharedString::from(message.clone()))
        .child(message)
        .into_any_element()
}

fn diagnostic_banner(
    message: String,
    card: &gpui::Hsla,
    muted: &gpui::Hsla,
    border: &gpui::Hsla,
) -> AnyElement {
    div()
        .id("ii.cheatsheet.keybinds.provider-diagnostic")
        .w_full()
        .flex_shrink_0()
        .px(px(10.))
        .py(px(6.))
        .rounded(px(8.))
        .border_1()
        .border_color(*border)
        .bg(*card)
        .text_xs()
        .text_color(*muted)
        .role(Role::Status)
        .aria_label(SharedString::from(message.clone()))
        .child(message)
        .into_any_element()
}

fn no_results_card(
    labels: &CheatsheetLabels,
    card: &gpui::Hsla,
    foreground: &gpui::Hsla,
    muted: &gpui::Hsla,
    border: &gpui::Hsla,
    on_clear: QueryChange,
) -> AnyElement {
    let on_click = on_clear.clone();
    div()
        .id("ii.cheatsheet.keybinds.no-results")
        .w_full()
        .h(px(200.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .rounded(px(12.))
        .border_1()
        .border_color(*border)
        .bg(*card)
        .child(
            div()
                .text_lg()
                .text_color(*foreground)
                .child(labels.no_results.clone()),
        )
        .child(
            div()
                .text_sm()
                .text_color(*muted)
                .child(labels.no_results_detail.clone()),
        )
        .child(
            div()
                .id("ii.cheatsheet.keybinds.no-results.clear")
                .mt(px(4.))
                .px(px(12.))
                .py(px(6.))
                .rounded(px(16.))
                .bg(*card)
                .text_sm()
                .text_color(*foreground)
                .role(Role::Button)
                .aria_label(SharedString::from(labels.clear_search.clone()))
                .focusable()
                .tab_stop(true)
                .cursor_pointer()
                .on_click(move |_, _, _| on_click(String::new()))
                .on_key_down(move |event: &KeyDownEvent, _, cx| {
                    if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        on_clear(String::new());
                        cx.stop_propagation();
                    }
                })
                .child(labels.clear_search.clone()),
        )
        .into_any_element()
}

fn render_category(
    category: &KeybindCategory,
    card: &gpui::Hsla,
    foreground: &gpui::Hsla,
    muted: &gpui::Hsla,
    accent: &gpui::Hsla,
    border: &gpui::Hsla,
    theme: &MaterialThemeContext,
) -> AnyElement {
    let name = category.name.as_str();
    let count_label = category.bindings.len().to_string();
    let count_bg = theme
        .colors
        .surface_container_high
        .unwrap_or_else(|| rgba(0xe6e0e9ff).into());
    let count = category.bindings.len();
    div()
        .id(SharedString::from(format!(
            "ii.cheatsheet.keybinds.category.{name}"
        )))
        .w_full()
        .flex()
        .flex_col()
        .p(px(4.))
        .rounded(px(theme.shape.medium))
        .border_1()
        .border_color(*border)
        .bg(*card)
        .role(Role::Group)
        .aria_label(SharedString::from(format!("{name}, {count} keybinds")))
        .child(
            div()
                .w_full()
                .h(px(36.))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(8.))
                .child(
                    div()
                        .font_family("Material Symbols Rounded")
                        .text_color(*accent)
                        .child(category_icon(&name)),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "ii.cheatsheet.keybinds.category.heading.{name}"
                        )))
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(*accent)
                        .role(Role::Heading)
                        .aria_level(2)
                        .child(name.to_owned()),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(px(theme.shape.pill))
                        .bg(count_bg)
                        .text_xs()
                        .text_color(*muted)
                        .child(count_label),
                ),
        )
        .child(div().mx(px(8.)).h(px(1.)).bg(*border))
        .child(
            div()
                .id(SharedString::from(format!(
                    "ii.cheatsheet.keybinds.category.list.{name}"
                )))
                .w_full()
                .flex()
                .flex_col()
                .role(Role::List)
                .aria_label(SharedString::from(format!("{name} keybinds")))
                .children(
                    category
                        .bindings
                        .iter()
                        .enumerate()
                        .map(|(index, binding)| {
                            render_keybind_row(
                                name,
                                binding,
                                format!("ii.cheatsheet.keybinds.category-row.{name}.{index}"),
                                index + 1 < count,
                                foreground,
                                muted,
                                border,
                                &theme.typography.monospace,
                            )
                        }),
                ),
        )
        .into_any_element()
}

fn render_keybind_row(
    category: &str,
    binding: &KeybindRecord,
    row_id: String,
    show_divider: bool,
    foreground: &gpui::Hsla,
    muted: &gpui::Hsla,
    border: &gpui::Hsla,
    monospace: &str,
) -> AnyElement {
    let modifiers: Vec<_> = binding
        .modifiers
        .iter()
        .map(|modifier| substitute_key(modifier))
        .collect();
    let main_key = substitute_key(&binding.key);
    let show_main = binding.key != "Super_L";
    let keys = modifiers
        .into_iter()
        .enumerate()
        .flat_map(|(index, modifier)| {
            let mut elements = vec![key_cap(&modifier, monospace, foreground, border)];
            if index + 1 < binding.modifiers.len() || show_main {
                elements.push(
                    div()
                        .text_xs()
                        .text_color(*muted)
                        .child("+")
                        .into_any_element(),
                );
            }
            elements
        })
        .chain(show_main.then(|| key_cap(&main_key, monospace, foreground, border)));
    let comment = if binding.comment.trim().is_empty() {
        binding.key.clone()
    } else {
        binding.comment.clone()
    };
    let key_summary = binding
        .modifiers
        .iter()
        .map(|modifier| substitute_key(modifier))
        .chain(show_main.then(|| main_key.clone()))
        .collect::<Vec<_>>()
        .join(" + ");
    let accessible_summary = format!("{category}: {key_summary}. {comment}");
    div()
        .id(SharedString::from(row_id))
        .w_full()
        .min_h(px(40.))
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(8.))
        .role(Role::ListItem)
        .aria_label(SharedString::from(accessible_summary))
        .when(show_divider, |row| row.border_b_1().border_color(*border))
        .child(
            div()
                .w(px(140.))
                .flex_shrink_0()
                .text_xs()
                .text_color(*muted)
                .child(category.to_owned()),
        )
        .child(
            div()
                .w(px(220.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(5.))
                .children(keys),
        )
        .child(
            div()
                .flex_1()
                .text_sm()
                .text_color(*foreground)
                .child(comment),
        )
        .into_any_element()
}

fn key_cap(key: &str, monospace: &str, foreground: &gpui::Hsla, border: &gpui::Hsla) -> AnyElement {
    div()
        .px(px(6.))
        .py(px(3.))
        .rounded(px(5.))
        .border_1()
        .border_color(*border)
        .font_family(monospace.to_owned())
        .text_xs()
        .text_color(*foreground)
        .child(key.to_owned())
        .into_any_element()
}

fn substitute_key(key: &str) -> String {
    match key {
        "Super" => "󰖳".into(),
        "mouse_up" => "Scroll ↓".into(),
        "mouse_down" => "Scroll ↑".into(),
        "mouse:272" => "LMB".into(),
        "mouse:273" => "RMB".into(),
        "mouse:275" => "MouseBack".into(),
        "Slash" => "/".into(),
        "Hash" => "#".into(),
        "Return" => "Enter".into(),
        _ => key.to_owned(),
    }
}

fn category_icon(name: &str) -> &'static str {
    match name {
        "System" => "settings_power",
        "ii Shell" => "dashboard",
        "Window Switcher" => "swap_horiz",
        "Region Tools" => "screenshot_region",
        "Applications" => "apps",
        "Window Management" => "select_window",
        "Focus" => "filter_center_focus",
        "Move Windows" => "open_with",
        "Workspaces" => "workspaces",
        "Screenshots" => "screenshot_monitor",
        "Media" => "music_note",
        "Brightness" => "brightness_medium",
        "Layout" => "view_quilt",
        "Resize" => "aspect_ratio",
        "Monitors" => "monitor",
        "Other" => "more_horiz",
        _ => "keyboard",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<KeybindCategory> {
        vec![
            KeybindCategory {
                name: "System".into(),
                bindings: vec![KeybindRecord {
                    key: "Return".into(),
                    modifiers: vec!["Super".into()],
                    comment: "Open terminal".into(),
                }],
            },
            KeybindCategory {
                name: "Window Management".into(),
                bindings: vec![KeybindRecord {
                    key: "Q".into(),
                    modifiers: vec!["Super".into(), "Shift".into()],
                    comment: "Close window".into(),
                }],
            },
        ]
    }

    #[test]
    fn search_matches_key_modifier_comment_and_category_case_insensitively() {
        let categories = fixture();
        for query in ["return", "shift", "terminal", "window man"] {
            assert_eq!(filter_keybinds(&categories, query).len(), 1, "{query}");
        }
    }

    #[test]
    fn search_trims_query_and_preserves_provider_order() {
        let categories = fixture();
        let results = filter_keybinds(&categories, "  SUPER  ");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].category, "System");
        assert_eq!(results[1].category, "Window Management");
        assert_eq!(filter_keybinds(&categories, "  ").len(), 2);
    }

    #[test]
    fn one_filter_pass_preserves_category_identity_and_source_category_counts() {
        let mut categories = fixture();
        categories[0].bindings.push(KeybindRecord {
            key: "Escape".into(),
            modifiers: vec![],
            comment: "Close menu".into(),
        });
        let rows = filter_keybinds(&categories, "terminal");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].category, "System");
        assert_eq!(total_keybind_count(&categories), 3);
    }

    #[test]
    fn whitespace_query_keeps_qml_clear_affordance_without_filtering_rows() {
        let categories = fixture();
        let query = "   ";
        assert!(!should_show_clear(""));
        assert!(should_show_clear(query));
        assert!(query.trim().is_empty());
        assert_eq!(filter_keybinds(&categories, query).len(), 2);
    }

    #[test]
    fn provider_states_do_not_fabricate_bindings_or_collapse_empty_and_unavailable() {
        let loading = KeybindSnapshot {
            state: KeybindProviderState::Loading,
            categories: vec![],
            detail: None,
        };
        let empty = KeybindSnapshot {
            state: KeybindProviderState::Empty,
            categories: vec![],
            detail: None,
        };
        let unavailable = KeybindSnapshot {
            state: KeybindProviderState::Unavailable,
            categories: vec![],
            detail: Some("Niri keybind provider is not connected".into()),
        };
        assert_eq!(loading.state, KeybindProviderState::Loading);
        assert_eq!(empty.state, KeybindProviderState::Empty);
        assert_eq!(unavailable.state, KeybindProviderState::Unavailable);
        assert!(loading.categories.is_empty());
        assert!(empty.categories.is_empty());
        assert!(unavailable.categories.is_empty());
        assert_eq!(
            unavailable.detail.as_deref(),
            Some("Niri keybind provider is not connected")
        );
        let labels = CheatsheetLabels::default();
        assert_eq!(
            provider_status_message(&loading, &labels),
            Some(ProviderStatusMessage::Blocking("Loading keybinds…".into()))
        );
        assert_eq!(
            provider_status_message(&empty, &labels),
            Some(ProviderStatusMessage::Blocking(
                "No keybinds configured".into()
            ))
        );
        assert_eq!(
            provider_status_message(&unavailable, &labels),
            Some(ProviderStatusMessage::Blocking(
                "Niri keybind provider is not connected".into()
            ))
        );
        assert_eq!(
            provider_status_message(
                &KeybindSnapshot {
                    state: KeybindProviderState::Ready,
                    categories: fixture(),
                    detail: None,
                },
                &labels,
            ),
            None
        );
        let ready_with_diagnostic = KeybindSnapshot {
            state: KeybindProviderState::Ready,
            categories: fixture(),
            detail: Some("Some Hyprland sources could not be loaded".into()),
        };
        assert_eq!(
            provider_status_message(&ready_with_diagnostic, &labels),
            Some(ProviderStatusMessage::Diagnostic(
                "Some Hyprland sources could not be loaded".into()
            ))
        );
        assert_eq!(
            effective_provider_state(&ready_with_diagnostic),
            KeybindProviderState::Ready
        );
        assert_eq!(total_keybind_count(&ready_with_diagnostic.categories), 2);
        assert_eq!(
            filter_keybinds(&ready_with_diagnostic.categories, "terminal").len(),
            1,
            "a ready diagnostic is non-blocking and the provider rows remain available"
        );
        assert_eq!(
            effective_provider_state(&KeybindSnapshot {
                state: KeybindProviderState::Ready,
                categories: vec![],
                detail: None,
            }),
            KeybindProviderState::Empty
        );
    }

    #[test]
    fn key_display_substitutions_follow_qml_contract() {
        assert_eq!(substitute_key("Return"), "Enter");
        assert_eq!(substitute_key("mouse:272"), "LMB");
        assert_eq!(substitute_key("mouse_down"), "Scroll ↑");
        assert_eq!(substitute_key("XF86AudioMute"), "XF86AudioMute");
    }
}
