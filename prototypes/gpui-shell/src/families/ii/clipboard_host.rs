//! Per-output host for the Material ii clipboard panel.
//!
//! The host owns one provider snapshot, search editor, and action queue per
//! output. It borrows the Waffle cliphist adapter as a data contract only; it
//! does not share Waffle controllers, windows, or visibility maps.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use async_channel::Sender;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, Render, ScrollHandle, Window, div,
    prelude::*,
};

use crate::{
    families::{
        ii::clipboard::{
            self, ClipboardAction, ClipboardActionSource, ClipboardEntry, ClipboardLabels,
            ClipboardPinnedEntry, ClipboardSearchMode, ClipboardViewInput, ClipboardViewState,
        },
        waffle::{
            clipboard_provider::{CliphistProvider, Snapshot},
            taskbar::start_menu_search::StartMenuSearch,
        },
    },
    theme::ThemeTokens,
};

#[derive(Clone, Debug)]
enum HostAction {
    View {
        generation: u64,
        model_generation: u64,
        action: ClipboardAction,
    },
    Query {
        generation: u64,
        query: String,
    },
}

pub struct IiClipboardHost {
    display: gpui::DisplayId,
    theme: ThemeTokens,
    events: Sender<crate::surface_manager::IiClipboardSurfaceEvent>,
    provider: CliphistProvider,
    configured_pins: Vec<String>,
    snapshot: Option<Snapshot>,
    state: ClipboardViewState,
    error: Option<String>,
    query: String,
    search_mode: ClipboardSearchMode,
    selected: usize,
    clear_confirmation: bool,
    show_keyboard_hints: bool,
    focus: FocusHandle,
    search_editor: Entity<StartMenuSearch>,
    scroll: ScrollHandle,
    actions: Sender<HostAction>,
    generation: u64,
    model_generation: u64,
    focus_on_first_render: bool,
    dismissed: bool,
}

/// Advance the output route generation so any previously scheduled close
/// timer becomes stale. Shared with the surface manager's close/reopen path.
pub(crate) fn next_route_generation(generation: &mut u64) -> u64 {
    *generation = ROUTE_GENERATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    *generation
}

pub(crate) const fn close_timer_is_current(current: Option<u64>, timer: u64) -> bool {
    matches!(current, Some(current) if current == timer)
}

const MAX_PIN_CHARS: usize = 8_000;
const MAX_CONFIG_WRITE_ATTEMPTS: usize = 4;
static CONFIG_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static ROUTE_GENERATION_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipboardToggleRouteAction {
    Open,
    Dismiss,
}

pub(crate) const fn clipboard_toggle_route_action(
    surface_exists: bool,
) -> ClipboardToggleRouteAction {
    if surface_exists {
        ClipboardToggleRouteAction::Dismiss
    } else {
        ClipboardToggleRouteAction::Open
    }
}

/// Match `Cliphist.qml`: strip browser wrappers only when their markers are
/// present; ordinary copied text is otherwise retained as-is.
pub fn clean_pinned_text(text: &str) -> String {
    let is_browser_markup = text
        .starts_with("<meta http-equiv=\"content-type\" content=\"text/html")
        || text.contains("<!--StartFragment-->");
    let cleaned = if is_browser_markup {
        strip_html_tags(&remove_html_comments(text))
            .trim()
            .to_owned()
    } else {
        text.to_owned()
    };
    cleaned.chars().take(MAX_PIN_CHARS).collect()
}

fn remove_html_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("<!--") {
        output.push_str(&rest[..start]);
        let comment = &rest[start + 4..];
        let Some(end) = comment.find("-->") else {
            return output;
        };
        rest = &comment[end + 3..];
    }
    output.push_str(rest);
    output
}

fn strip_html_tags(input: &str) -> String {
    let output = strip_html_tags_only(input);
    [
        ("&nbsp;", " "),
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
    ]
    .into_iter()
    .fold(output, |text, (entity, decoded)| {
        replace_ascii_case_insensitive(&text, entity, decoded)
    })
}

fn strip_html_tags_only(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;
    for character in input.chars() {
        match character {
            '<' if !in_tag => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
}

fn replace_ascii_case_insensitive(input: &str, needle: &str, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;
    while let Some(start) = remaining.find('&') {
        output.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        if remaining.len() >= needle.len()
            && remaining
                .get(..needle.len())
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(needle))
        {
            output.push_str(replacement);
            remaining = &remaining[needle.len()..];
        } else {
            output.push('&');
            remaining = &remaining[1..];
        }
    }
    output.push_str(remaining);
    output
}

fn material_pin_id(text: &str) -> String {
    let mut left = 0xcbf29ce484222325_u64;
    let mut right = 0x84222325cbf29ce4_u64;
    for byte in text.bytes() {
        left = (left ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        right = (right ^ u64::from(byte).rotate_left(1)).wrapping_mul(0x100000001b3);
    }
    format!("configured-pin-{left:016x}{right:016x}")
}

fn model_generation_is_current(current: u64, action: u64) -> bool {
    current == action
}

fn material_entry_is_image(source_row: &str) -> bool {
    let Some((id, preview)) = source_row.split_once('\t') else {
        return false;
    };
    if id.is_empty()
        || !id.bytes().all(|byte| byte.is_ascii_digit())
        || !preview.starts_with("[[")
        || !preview.ends_with("]]")
        || !preview.contains("binary data")
    {
        return false;
    }
    let bytes = preview.as_bytes();
    (0..bytes.len().saturating_sub(2)).any(|index| {
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        let mut cursor = index;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'x') {
            return false;
        }
        cursor += 1;
        let second_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        cursor > second_start
    })
}

fn sanitize_display_text(text: &str) -> String {
    text.chars()
        .filter_map(|character| match character {
            '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}' | '\u{00ad}' => None,
            '\u{00a0}' => Some(' '),
            '\u{2028}' | '\u{2029}' => Some('\n'),
            character => Some(character),
        })
        .collect()
}

fn format_material_preview(source_row: &str, is_image: bool) -> String {
    let Some((_, source_preview)) = source_row.split_once('\t') else {
        return String::new();
    };
    let mut cleaned = source_preview.to_owned();
    if is_image {
        let trimmed = cleaned.trim_start();
        if let Some(end) = trimmed.find("]]") {
            cleaned = trimmed[end + 2..].trim_start().to_owned();
        }
    }
    let unwrapped = cliphist_markup_preview(&cleaned);
    if unwrapped != cleaned {
        cleaned = if unwrapped.is_empty() {
            "Rich text".to_owned()
        } else {
            unwrapped
        };
    }
    sanitize_display_text(&cleaned)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn cliphist_markup_preview(text: &str) -> String {
    let Some(tag) = text.trim_start().strip_prefix('<') else {
        return text.to_owned();
    };
    let mut chars = tag.chars();
    let begins_markup = match chars.next() {
        Some('!') => true,
        Some('/') => chars
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic()),
        Some(character) => character.is_ascii_alphabetic(),
        None => false,
    };
    if !begins_markup {
        return text.to_owned();
    }

    let mut stripped = remove_html_comments(text);
    if let Some(open) = stripped.rfind('<')
        && !stripped[open..].contains('>')
    {
        stripped.truncate(open);
    }
    strip_html_tags(&stripped)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn filter_browser_markup_for_copy(payload: &[u8]) -> Vec<u8> {
    const META_PREFIX: &[u8] = b"<meta http-equiv=\"content-type\" content=\"text/html";
    const FRAGMENT: &[u8] = b"<!--StartFragment-->";
    if !payload.starts_with(META_PREFIX) && !payload.windows(FRAGMENT.len()).any(|w| w == FRAGMENT)
    {
        return payload.to_vec();
    }
    let mut html = String::from_utf8_lossy(payload).into_owned();
    if html.starts_with("<meta")
        && let Some(end) = html.find('>')
    {
        html.drain(..=end);
    }
    html = remove_html_element_blocks(&html, "script");
    html = remove_html_element_blocks(&html, "style");
    html = remove_html_comments(&html);
    html = replace_html_line_breaks(&html);
    html = replace_html_images(&html);
    let mut plain = strip_html_tags_only(&html);
    plain = plain
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    while plain.contains("\n\n\n") {
        plain = plain.replace("\n\n\n", "\n\n");
    }
    html_escape::decode_html_entities(plain.trim())
        .as_bytes()
        .to_vec()
}

fn remove_html_element_blocks(input: &str, element: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let open = format!("<{element}");
    let close = format!("</{element}");
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(relative) = lower[cursor..].find(&open) {
        let start = cursor + relative;
        output.push_str(&input[cursor..start]);
        let Some(close_start_relative) = lower[start..].find(&close) else {
            return output;
        };
        let close_start = start + close_start_relative;
        let Some(close_end_relative) = input[close_start..].find('>') else {
            return output;
        };
        cursor = close_start + close_end_relative + 1;
    }
    output.push_str(&input[cursor..]);
    output
}

fn replace_html_line_breaks(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;
    while let Some(start) = remaining.find('<') {
        output.push_str(&remaining[..start]);
        let Some(end_relative) = remaining[start..].find('>') else {
            output.push_str(&remaining[start..]);
            return output;
        };
        let end = start + end_relative;
        let tag = remaining[start + 1..end].trim().to_ascii_lowercase();
        if tag == "br" || tag == "br/" || tag == "br /" {
            output.push('\n');
        } else if let Some(tag) = tag.strip_prefix('/')
            && matches!(
                tag.trim(),
                "p" | "div"
                    | "li"
                    | "tr"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "blockquote"
                    | "pre"
            )
        {
            output.push('\n');
        } else {
            output.push_str(&remaining[start..=end]);
        }
        remaining = &remaining[end + 1..];
    }
    output.push_str(remaining);
    output
}

fn replace_html_images(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(relative) = lower[cursor..].find("<img") {
        let start = cursor + relative;
        output.push_str(&input[cursor..start]);
        let Some(end_relative) = input[start..].find('>') else {
            output.push_str(&input[start..]);
            return output;
        };
        let end = start + end_relative;
        let tag = &input[start..=end];
        if let Some(src) = quoted_attribute(tag, "src") {
            output.push_str(src);
        }
        cursor = end + 1;
    }
    output.push_str(&input[cursor..]);
    output
}

fn quoted_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{name}=");
    let index = lower.find(&needle)? + needle.len();
    let quote = *tag.as_bytes().get(index)?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let value_start = index + 1;
    let value_end = tag[value_start..].find(quote as char)? + value_start;
    Some(&tag[value_start..value_end])
}

fn resolve_material_pin<'a>(
    pins: &'a [crate::families::waffle::clipboard_provider::ConfiguredPin],
    id: &str,
) -> Option<&'a crate::families::waffle::clipboard_provider::ConfiguredPin> {
    pins.iter().find(|pin| material_pin_id(&pin.text) == id)
}

/// Merge only clipboard pin data into the current config while retaining
/// unrelated root and clipboard members.
fn merge_clipboard_pins(root: &mut serde_json::Value, pins: &[String]) -> Result<(), String> {
    let root = root
        .as_object_mut()
        .ok_or_else(|| "runtime config root must be a JSON object".to_owned())?;
    let clipboard = root
        .entry("clipboard")
        .or_insert_with(|| serde_json::Value::Object(Default::default()))
        .as_object_mut()
        .ok_or_else(|| "runtime config clipboard section must be an object".to_owned())?;
    clipboard.insert(
        "pinned".into(),
        serde_json::Value::Array(
            pins.iter()
                .cloned()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );
    Ok(())
}

/// Retry a fresh JSON merge when the runtime config changes before atomic
/// replacement. QML does not share a file lock, leaving a small compare/rename
/// race that cannot be closed from this host alone.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PinMutation {
    Add(String),
    Remove(String),
}

fn apply_pin_mutation(pins: &[String], mutation: &PinMutation) -> Vec<String> {
    match mutation {
        PinMutation::Add(text) => {
            if pins.iter().any(|pin| pin == text) {
                pins.to_vec()
            } else {
                std::iter::once(text.clone())
                    .chain(pins.iter().cloned())
                    .collect()
            }
        }
        PinMutation::Remove(text) => pins.iter().filter(|pin| *pin != text).cloned().collect(),
    }
}

fn config_pins(root: &serde_json::Value) -> Result<Vec<String>, String> {
    let Some(clipboard) = root.get("clipboard") else {
        return Ok(Vec::new());
    };
    let Some(pinned) = clipboard.get("pinned") else {
        return Ok(Vec::new());
    };
    pinned
        .as_array()
        .ok_or_else(|| "runtime config clipboard.pinned must be an array".to_owned())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "runtime config clipboard.pinned must contain strings".to_owned())
        })
        .collect()
}

fn persist_clipboard_pin_mutation(
    path: &Path,
    mutation: &PinMutation,
) -> Result<Vec<String>, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.json");
    for attempt in 0..MAX_CONFIG_WRITE_ATTEMPTS {
        let before = fs::read(path).map_err(|error| format!("read config: {error}"))?;
        let current_permissions = fs::metadata(path)
            .map_err(|error| format!("read config permissions: {error}"))?
            .permissions();
        let mut root: serde_json::Value =
            serde_json::from_slice(&before).map_err(|error| format!("parse config: {error}"))?;
        let pins = apply_pin_mutation(&config_pins(&root)?, mutation);
        merge_clipboard_pins(&mut root, &pins)?;
        let mut serialized = serde_json::to_vec_pretty(&root)
            .map_err(|error| format!("serialize config: {error}"))?;
        serialized.push(b'\n');

        if fs::read(path).map_err(|error| format!("recheck config: {error}"))? != before {
            continue;
        }

        let sequence = CONFIG_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp_path = parent.join(format!(
            ".{filename}.ii-clipboard-{}-{sequence}-{attempt}.tmp",
            std::process::id()
        ));
        let write_result = (|| -> std::io::Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp_path)?;
            file.set_permissions(current_permissions.clone())?;
            file.write_all(&serialized)?;
            file.sync_all()?;
            if fs::read(path)? != before {
                return Err(std::io::Error::other(
                    "runtime config changed during pin write",
                ));
            }
            fs::rename(&temp_path, path)?;
            if let Ok(directory) = OpenOptions::new().read(true).open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temp_path);
            if error.to_string() == "runtime config changed during pin write" {
                continue;
            }
            return Err(format!("write config: {error}"));
        }

        let written = fs::read(path).map_err(|error| format!("verify config: {error}"))?;
        let actual: serde_json::Value =
            serde_json::from_slice(&written).map_err(|error| format!("verify config: {error}"))?;
        if actual == root {
            return Ok(pins);
        }
        return Err("runtime config changed during pin write; retry the pin action".into());
    }
    Err("runtime config changed during pin write; retry the pin action".into())
}

impl IiClipboardHost {
    pub fn new(
        theme: ThemeTokens,
        display: gpui::DisplayId,
        configured_pins: Vec<String>,
        events: Sender<crate::surface_manager::IiClipboardSurfaceEvent>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (actions, receiver) = async_channel::unbounded();
        let focus = cx.focus_handle();
        let query_sender = actions.clone();
        let query_action: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |query| {
            let _ = query_sender.try_send(HostAction::Query {
                generation: 1,
                query,
            });
        });
        let search_editor = cx.new(|cx| {
            StartMenuSearch::new_with_shared_focus(
                String::new(),
                "Search clipboard history".into(),
                "Search clipboard history".into(),
                query_action,
                focus.clone(),
                cx,
            )
        });
        let host = Self {
            display,
            theme,
            events,
            provider: CliphistProvider::default(),
            configured_pins,
            snapshot: None,
            state: ClipboardViewState::Loading,
            error: None,
            query: String::new(),
            search_mode: ClipboardSearchMode::Filter,
            selected: 0,
            clear_confirmation: false,
            show_keyboard_hints: false,
            focus,
            search_editor,
            scroll: ScrollHandle::new(),
            actions,
            generation: 1,
            model_generation: 1,
            focus_on_first_render: true,
            dismissed: false,
        };
        host.start_action_loop(receiver, cx);
        host
    }

    fn start_action_loop(
        &self,
        receiver: async_channel::Receiver<HostAction>,
        cx: &mut Context<Self>,
    ) {
        let initial_provider = self.provider.clone();
        let initial_pins = self.configured_pins.clone();
        let initial_generation = self.generation;
        let initial_model_generation = self.model_generation;
        cx.spawn(async move |this, cx| {
            let provider = initial_provider.clone();
            let pins = initial_pins.clone();
            let result = cx
                .background_executor()
                .spawn(async move { provider.list(&pins) })
                .await;
            apply_snapshot(
                &this,
                cx,
                initial_generation,
                initial_model_generation,
                result,
                false,
            );

            while let Ok(action) = receiver.recv().await {
                match action {
                    HostAction::Query { generation, query } => {
                        if !host_generation_is_current(&this, cx, generation) {
                            continue;
                        }
                        let _ = this.update(cx, |view, cx| {
                            view.query = query;
                            view.selected = 0;
                            cx.notify();
                        });
                    }
                    HostAction::View {
                        generation,
                        model_generation,
                        action,
                    } => {
                        if !host_action_is_current(&this, cx, generation, model_generation) {
                            continue;
                        }
                        if handle_action(&this, cx, generation, model_generation, action).await {
                            break;
                        }
                    }
                }
            }
        })
        .detach();
    }

    /// Called by an output route when this host is reopened during its hide
    /// grace. Invalidates all in-flight work, resets QML's open-time state, and
    /// fetches a fresh history snapshot.
    pub fn reopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.model_generation = self.model_generation.wrapping_add(1);
        self.dismissed = false;
        self.focus_on_first_render = true;
        window.set_input_region(None);
        window.focus(&self.search_editor.focus_handle(cx), cx);
        self.focus_on_first_render = false;
        self.query.clear();
        self.search_mode = ClipboardSearchMode::Filter;
        self.selected = 0;
        self.clear_confirmation = false;
        self.show_keyboard_hints = false;
        self.state = ClipboardViewState::Loading;
        self.error = None;
        let generation = self.generation;
        let model_generation = self.model_generation;
        let provider = self.provider.clone();
        let pins = self.configured_pins.clone();
        let query_actions = self.actions.clone();
        self.search_editor.update(cx, |editor, cx| {
            editor.sync_controlled_query(
                String::new(),
                Arc::new(move |query| {
                    let _ = query_actions.try_send(HostAction::Query { generation, query });
                }),
                cx,
            );
        });
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { provider.list(&pins) })
                .await;
            apply_snapshot(&this, cx, generation, model_generation, result, false);
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn dismiss_for_grace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismissed = true;
        self.generation = self.generation.wrapping_add(1);
        self.model_generation = self.model_generation.wrapping_add(1);
        window.set_input_region(Some(&[]));
        window.blur(cx);
        cx.notify();
    }

    pub fn set_shared_pins(&mut self, pins: Vec<String>, cx: &mut Context<Self>) {
        if self.configured_pins == pins {
            return;
        }
        self.configured_pins = pins;
        self.model_generation = self.model_generation.wrapping_add(1);
        self.snapshot = None;
        self.state = ClipboardViewState::Loading;
        self.error = None;
        let _ = self.actions.try_send(HostAction::View {
            generation: self.generation,
            model_generation: self.model_generation,
            action: ClipboardAction::Refresh,
        });
        cx.notify();
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    fn input(&self, cx: &App) -> ClipboardViewInput {
        let snapshot = self.snapshot.as_ref();
        let entries = snapshot
            .map(|snapshot| {
                snapshot
                    .entries
                    .iter()
                    .map(|entry| {
                        let is_image = material_entry_is_image(&entry.source_row);
                        ClipboardEntry {
                            id: entry.id.clone(),
                            preview: format_material_preview(&entry.source_row, is_image),
                            kind_label: if is_image { "Image" } else { "Text" }.into(),
                            is_image,
                            pinnable: !is_image,
                            pinned: entry.pinned,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let pinned = snapshot
            .map(|snapshot| {
                snapshot
                    .pins
                    .iter()
                    .map(|pin| ClipboardPinnedEntry {
                        id: material_pin_id(&pin.text),
                        preview: pin.preview.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let actions = self.actions.clone();
        let generation = self.generation;
        let model_generation = self.model_generation;
        let focus = self.focus.clone();
        let search_focus = self.search_editor.focus_handle(cx);
        ClipboardViewInput {
            state: self.state,
            status_detail: self.error.clone(),
            entries,
            pinned,
            query: self.query.clone(),
            search_mode: self.search_mode,
            selected: self.selected,
            clear_confirmation: self.clear_confirmation,
            show_keyboard_hints: self.show_keyboard_hints,
            labels: material_labels(),
            theme: self.theme.material.clone(),
            focus: focus.clone(),
            search_focus: Some(search_focus),
            scroll: self.scroll.clone(),
            search_editor: Some(self.search_editor.clone().into_any_element()),
            available_height: gpui::px(680.0),
            on_action: Arc::new(move |action| {
                let _ = actions.try_send(HostAction::View {
                    generation,
                    model_generation,
                    action,
                });
            }),
        }
    }
}

impl Focusable for IiClipboardHost {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for IiClipboardHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.dismissed {
            return div().id("ii.clipboard.dismissed").absolute().inset_0();
        }
        if self.focus_on_first_render {
            window.focus(&self.search_editor.focus_handle(cx), cx);
            self.focus_on_first_render = false;
        }
        let query = self.query.clone();
        let query_actions = self.actions.clone();
        let generation = self.generation;
        self.search_editor.update(cx, |editor, cx| {
            editor.sync_controlled_query(
                query,
                Arc::new(move |query| {
                    let _ = query_actions.try_send(HostAction::Query { generation, query });
                }),
                cx,
            );
        });
        let display = self.display;
        let generation = self.generation;
        let backdrop_events = self.events.clone();
        let view = clipboard::render(self.input(cx));
        div()
            .id("ii.clipboard.host")
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("ii.clipboard.backdrop")
                    .absolute()
                    .inset_0()
                    .on_click(move |_, _, _| {
                        let _ = backdrop_events.try_send(
                            crate::surface_manager::IiClipboardSurfaceEvent::Dismiss {
                                display,
                                generation,
                            },
                        );
                    }),
            )
            .child(
                div()
                    .id("ii.clipboard.layer")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("ii.clipboard.panel-hit-target")
                            .on_click(|_, _, cx| cx.stop_propagation())
                            .child(view),
                    ),
            )
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::{
        ClipboardToggleRouteAction, clipboard_toggle_route_action, close_timer_is_current,
        next_route_generation,
    };

    #[test]
    fn reopening_invalidates_a_pending_close_timer() {
        let mut generation = 7;
        let close_timer = next_route_generation(&mut generation);

        next_route_generation(&mut generation); // route reopened in grace

        assert!(!close_timer_is_current(Some(generation), close_timer));
    }

    #[test]
    fn a_later_dismissal_supersedes_earlier_close_timer() {
        let mut generation = 0;
        let first_close = next_route_generation(&mut generation);
        next_route_generation(&mut generation); // reopen
        let second_close = next_route_generation(&mut generation);

        assert!(!close_timer_is_current(Some(generation), first_close));
        assert!(close_timer_is_current(Some(generation), second_close));
        assert!(!close_timer_is_current(None, second_close));
    }

    #[test]
    fn toggle_during_grace_opens_again_and_invalidates_old_timer() {
        assert_eq!(
            clipboard_toggle_route_action(false),
            ClipboardToggleRouteAction::Open
        );
        assert_eq!(
            clipboard_toggle_route_action(true),
            ClipboardToggleRouteAction::Dismiss
        );

        let mut generation = 0;
        let old_close = next_route_generation(&mut generation);
        // Toggle sees no surface after immediate dismissal and routes Open.
        assert_eq!(
            clipboard_toggle_route_action(false),
            ClipboardToggleRouteAction::Open
        );
        next_route_generation(&mut generation);
        assert!(!close_timer_is_current(Some(generation), old_close));
    }
}

#[cfg(test)]
mod pin_contract_tests {
    use super::{
        CONFIG_TEMP_SEQUENCE, Ordering, PinMutation, apply_pin_mutation, clean_pinned_text,
        config_pins, merge_clipboard_pins, persist_clipboard_pin_mutation, pin_matches_preview,
    };
    use serde_json::json;
    use std::fs;

    #[test]
    fn runtime_pin_merge_keeps_unrelated_config_fields() {
        let mut root = json!({
            "other": {"keep": true},
            "clipboard": {"limit": 24, "pinned": ["old"]}
        });
        merge_clipboard_pins(&mut root, &["new".into()]).unwrap();
        assert_eq!(root["other"]["keep"], true);
        assert_eq!(root["clipboard"]["limit"], 24);
        assert_eq!(config_pins(&root).unwrap(), ["new"]);
    }

    #[test]
    fn pin_persistence_atomically_merges_current_json() {
        let path = std::env::temp_dir().join(format!(
            "intoo-ii-clipboard-test-{}-{}.json",
            std::process::id(),
            CONFIG_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(
            &path,
            br#"{"unrelated":{"keep":true},"clipboard":{"limit":24,"pinned":["existing"]}}"#,
        )
        .unwrap();

        let pins = persist_clipboard_pin_mutation(&path, &PinMutation::Add("new".into()))
            .expect("pin config write succeeds");
        let written: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

        assert_eq!(pins, ["new", "existing"]);
        assert_eq!(written["unrelated"]["keep"], true);
        assert_eq!(written["clipboard"]["limit"], 24);
        assert_eq!(config_pins(&written).unwrap(), pins);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn pin_delta_applies_to_latest_config_snapshot() {
        let latest = vec!["from-other-writer".to_owned()];
        assert_eq!(
            apply_pin_mutation(&latest, &PinMutation::Add("new".into())),
            ["new", "from-other-writer"]
        );
        assert_eq!(
            apply_pin_mutation(
                &["target".into(), "from-other-writer".into()],
                &PinMutation::Remove("target".into())
            ),
            ["from-other-writer"]
        );
    }

    #[test]
    fn duplicate_pin_add_is_idempotent() {
        assert_eq!(
            apply_pin_mutation(&["same".into()], &PinMutation::Add("same".into())),
            ["same"]
        );
    }

    #[test]
    fn browser_markup_cleanup_matches_cliphist_contract() {
        assert_eq!(
            clean_pinned_text(
                "<meta http-equiv=\"content-type\" content=\"text/html\"><!--comment--><b> hello </b>"
            ),
            "hello"
        );
        assert_eq!(
            clean_pinned_text("<!--StartFragment--><p>fragment</p><!--EndFragment-->"),
            "fragment"
        );
        assert_eq!(
            clean_pinned_text("  <b>literal text</b>  "),
            "  <b>literal text</b>  "
        );
        assert_eq!(
            clean_pinned_text(
                "<meta http-equiv=\"content-type\" content=\"text/html\"><p>A&nbsp;&amp;&lt;&gt;&quot;&#39;</p>"
            ),
            "A &<>\"'"
        );
        assert_eq!(
            clean_pinned_text("<meta http-equiv=\"content-type\" content=\"text/html\">&amp;nbsp;"),
            "&nbsp;"
        );
    }

    #[test]
    fn pin_cleanup_caps_decoded_content() {
        assert_eq!(clean_pinned_text(&"x".repeat(8_001)).chars().count(), 8_000);
    }

    #[test]
    fn unpin_matching_uses_qml_preview_cutoff() {
        assert!(!pin_matches_preview("line one\ncontinued", "line one"));
        assert!(!pin_matches_preview("line one continued", "line one"));
        let prefix = "a".repeat(100);
        assert!(pin_matches_preview(&format!("{prefix} rest"), &prefix));
    }

    #[test]
    fn reorder_refresh_keeps_pin_identity_and_rejects_stale_queued_action() {
        use super::{material_pin_id, model_generation_is_current, resolve_material_pin};
        use crate::families::waffle::clipboard_provider::ConfiguredPin;

        let original = vec![
            ConfiguredPin {
                id: "configured-pin-0".into(),
                text: "private pin alpha".into(),
                preview: "private pin alpha".into(),
            },
            ConfiguredPin {
                id: "configured-pin-1".into(),
                text: "private pin beta".into(),
                preview: "private pin beta".into(),
            },
        ];
        let selected_id = material_pin_id(&original[0].text);
        let refreshed = vec![original[1].clone(), original[0].clone()];

        assert_eq!(
            resolve_material_pin(&refreshed, &selected_id).map(|pin| pin.text.as_str()),
            Some("private pin alpha")
        );
        assert!(!selected_id.contains("private pin alpha"));
        assert!(!model_generation_is_current(4, 3));
        assert!(model_generation_is_current(4, 4));
    }
}

#[cfg(test)]
mod display_privacy_tests {
    use super::{filter_browser_markup_for_copy, format_material_preview, material_entry_is_image};

    #[test]
    fn history_preview_removes_image_marker_markup_and_invisible_unicode() {
        assert!(material_entry_is_image("12\t[[ binary data 20x10 ]]"));
        assert_eq!(
            format_material_preview(
                "12\t[[ binary data 20x10 ]]  <b>A&nbsp;&amp;B</b>\u{200b}",
                true
            ),
            "A &B"
        );
        assert_eq!(
            format_material_preview(
                "13\t<meta http-equiv=\"content-type\" content=\"text/html\"><script>x</script><!--StartFragment--><p> Rich &amp; plain </p><!--EndFragment-->",
                false
            ),
            "x Rich & plain"
        );
        assert_eq!(
            format_material_preview(
                "14\t<meta http-equiv=\"content-type\" content=\"text/html\"><b></b>",
                false
            ),
            "Rich text"
        );
    }

    #[test]
    fn browser_copy_cleanup_preserves_plaintext_and_filters_html() {
        assert_eq!(
            filter_browser_markup_for_copy(b"plain text\n"),
            b"plain text\n"
        );
        let image = [0, 0xff, 0x89, 0x50, 0x4e, 0x47];
        assert_eq!(filter_browser_markup_for_copy(&image), image);
        let html = b"<meta http-equiv=\"content-type\" content=\"text/html\"><script>secret()</script><p>A&nbsp;&amp;B</p><img src=\"asset.png\">";
        assert_eq!(
            filter_browser_markup_for_copy(html),
            "A\u{00a0}&B\nasset.png".as_bytes()
        );
    }

    #[test]
    fn browser_copy_decodes_named_decimal_and_hex_html5_entities() {
        let html = b"<meta http-equiv=\"content-type\" content=\"text/html\"><p>&copy; &#169; &#x1F642; &eacute; &mdash;</p>";
        assert_eq!(
            filter_browser_markup_for_copy(html),
            "© © 🙂 é —".as_bytes()
        );
    }
}

fn material_labels() -> ClipboardLabels {
    ClipboardLabels {
        title: "Clipboard".into(),
        search_placeholder: "Search clipboard history".into(),
        loading: "Loading clipboard history…".into(),
        unavailable: "Clipboard history is unavailable".into(),
        empty: "Clipboard history is empty".into(),
        no_results: "No matching entries".into(),
        pinned: "Pinned".into(),
        image: "Image".into(),
        filter_mode: "Filter".into(),
        navigate_mode: "Navigate".into(),
        matches: "matches".into(),
        keyboard_hints: "Keyboard shortcuts".into(),
        clear: "Clear history".into(),
        clear_question: "Clear all clipboard history?".into(),
        confirm: "Clear".into(),
        cancel: "Cancel".into(),
        refresh: "Refresh".into(),
        close: "Close".into(),
        pin: "Pin".into(),
        unpin: "Unpin".into(),
        delete: "Delete".into(),
        unavailable_detail: "Check that cliphist and wl-copy are available.".into(),
        empty_detail: "Copied items will appear here.".into(),
        no_results_detail: "Try another search.".into(),
        hints: "Enter paste · Ctrl+C copy · Ctrl+P pin · Delete remove · Shift+Delete clear".into(),
    }
}

async fn handle_action(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
    generation: u64,
    model_generation: u64,
    action: ClipboardAction,
) -> bool {
    if !host_action_is_current(this, cx, generation, model_generation) {
        return false;
    }
    match action {
        ClipboardAction::SetQuery(query) => {
            let _ = this.update(cx, |view, cx| {
                view.query = query;
                view.selected = 0;
                cx.notify();
            });
        }
        ClipboardAction::SetSearchMode(mode) => {
            let _ = this.update(cx, |view, cx| {
                view.search_mode = mode;
                view.selected = 0;
                cx.notify();
            });
        }
        ClipboardAction::Select(selected) => {
            let _ = this.update(cx, |view, cx| {
                view.selected = selected;
                cx.notify();
            });
        }
        ClipboardAction::ToggleKeyboardHints => {
            let _ = this.update(cx, |view, cx| {
                view.show_keyboard_hints = !view.show_keyboard_hints;
                cx.notify();
            });
        }
        ClipboardAction::RequestClear => {
            let _ = this.update(cx, |view, cx| {
                if view.clear_confirmation {
                    // The second Shift+Delete confirms, matching ClipboardPanel.qml.
                    view.clear_confirmation = true;
                } else {
                    view.clear_confirmation = true;
                }
                cx.notify();
            });
        }
        ClipboardAction::CancelClear => {
            let _ = this.update(cx, |view, cx| {
                view.clear_confirmation = false;
                cx.notify();
            });
        }
        ClipboardAction::Dismiss => {
            emit_dismiss(this, cx, generation);
        }
        ClipboardAction::Refresh => {
            if let Some((provider, pins)) = host_data(this, cx) {
                let Some(model_generation) = begin_refresh(this, cx, generation) else {
                    return false;
                };
                let result = cx
                    .background_executor()
                    .spawn(async move { provider.list(&pins) })
                    .await;
                apply_snapshot(this, cx, generation, model_generation, result, false);
            }
        }
        ClipboardAction::Activate { id, source } => {
            let Some((provider, snapshot)) = host_snapshot(this, cx) else {
                return false;
            };
            let result = match source {
                ClipboardActionSource::History => {
                    let is_image = snapshot
                        .entries
                        .iter()
                        .find(|entry| entry.id == id)
                        .is_some_and(|entry| material_entry_is_image(&entry.source_row));
                    if is_image {
                        cx.background_executor()
                            .spawn({
                                let provider = provider.clone();
                                async move { provider.copy(&id) }
                            })
                            .await
                    } else {
                        let decoded = cx
                            .background_executor()
                            .spawn({
                                let provider = provider.clone();
                                let id = id.clone();
                                async move { provider.decode_for_copy(&id) }
                            })
                            .await;
                        if !host_action_is_current(this, cx, generation, model_generation) {
                            return false;
                        }
                        match decoded {
                            Ok(payload) => {
                                let filtered = filter_browser_markup_for_copy(&payload);
                                cx.background_executor()
                                    .spawn(async move { provider.copy_bytes(&filtered) })
                                    .await
                            }
                            Err(error) => Err(error),
                        }
                    }
                }
                ClipboardActionSource::Pinned => {
                    let text =
                        resolve_material_pin(&snapshot.pins, &id).map(|pin| pin.text.clone());
                    match text {
                        Some(text) => {
                            cx.background_executor()
                                .spawn(async move { provider.copy_pinned(&text) })
                                .await
                        }
                        None => return false,
                    }
                }
            };
            if !host_action_is_current(this, cx, generation, model_generation) {
                return false;
            }
            match result {
                Ok(()) => emit_dismiss(this, cx, generation),
                Err(_) => set_unavailable_error(this, cx, generation),
            }
        }
        ClipboardAction::DeleteHistory(id) => {
            let Some((provider, snapshot, pins)) = host_snapshot_with_pins(this, cx) else {
                return false;
            };
            let Some(entry) = snapshot
                .entries
                .iter()
                .find(|entry| entry.id == id)
                .cloned()
            else {
                return false;
            };
            let result = cx
                .background_executor()
                .spawn({
                    let provider = provider.clone();
                    async move { provider.delete(&entry) }
                })
                .await;
            if result.is_ok() {
                refresh_after_mutation(this, cx, generation, model_generation, provider, pins)
                    .await;
            } else if host_action_is_current(this, cx, generation, model_generation) {
                set_unavailable_error(this, cx, generation);
            }
        }
        ClipboardAction::SetPinned { id, pinned } => {
            if !host_action_is_current(this, cx, generation, model_generation) {
                return false;
            }
            let Some((provider, snapshot, _pins)) = host_snapshot_with_pins(this, cx) else {
                return false;
            };
            let mutation = if pinned {
                let result = cx
                    .background_executor()
                    .spawn({
                        let provider = provider.clone();
                        async move { provider.decode_for_pin(&id) }
                    })
                    .await;
                match result {
                    Ok(text) => {
                        let text = clean_pinned_text(&text);
                        if text.is_empty() {
                            return false;
                        }
                        PinMutation::Add(text)
                    }
                    Err(_) => {
                        if host_action_is_current(this, cx, generation, model_generation) {
                            set_unavailable_error(this, cx, generation);
                        }
                        return false;
                    }
                }
            } else if let Some(pin) = resolve_material_pin(&snapshot.pins, &id) {
                if !host_action_is_current(this, cx, generation, model_generation) {
                    return false;
                }
                PinMutation::Remove(pin.text.clone())
            } else if let Some(entry) = snapshot.entries.iter().find(|entry| entry.id == id) {
                let Some(pin) = snapshot
                    .pins
                    .iter()
                    .find(|pin| pin_matches_preview(&pin.text, &entry.preview))
                else {
                    return false;
                };
                PinMutation::Remove(pin.text.clone())
            } else {
                return false;
            };
            if !host_action_is_current(this, cx, generation, model_generation) {
                return false;
            }
            let persistence = cx
                .background_executor()
                .spawn(async move {
                    persist_clipboard_pin_mutation(&crate::config::runtime_path(), &mutation)
                })
                .await;
            let pins = match persistence {
                Ok(pins) => pins,
                Err(error) => {
                    if host_action_is_current(this, cx, generation, model_generation) {
                        set_operation_error(this, cx, generation, error);
                    }
                    return false;
                }
            };
            if !host_action_is_current(this, cx, generation, model_generation) {
                return false;
            }
            refresh_after_mutation(this, cx, generation, model_generation, provider, pins).await;
        }
        ClipboardAction::ConfirmClear => {
            if !host_action_is_current(this, cx, generation, model_generation) {
                return false;
            }
            let Some((provider, pins)) = host_data(this, cx) else {
                return false;
            };
            let confirmed = this
                .update(cx, |view, _| view.clear_confirmation)
                .unwrap_or(false);
            if !confirmed {
                return false;
            }
            let result = cx
                .background_executor()
                .spawn(async move { provider.wipe() })
                .await;
            if !host_action_is_current(this, cx, generation, model_generation) {
                return false;
            }
            if result.is_ok() {
                let _ = this.update(cx, |view, cx| {
                    view.clear_confirmation = false;
                    cx.notify();
                });
                let provider = this.update(cx, |view, _| view.provider.clone()).ok();
                if let Some(provider) = provider {
                    refresh_after_mutation(this, cx, generation, model_generation, provider, pins)
                        .await;
                }
                emit_dismiss(this, cx, generation);
            } else {
                set_unavailable_error(this, cx, generation);
            }
        }
    }
    false
}

fn host_data(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
) -> Option<(CliphistProvider, Vec<String>)> {
    this.update(cx, |view, _| {
        (view.provider.clone(), view.configured_pins.clone())
    })
    .ok()
}

fn host_generation_is_current(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
    generation: u64,
) -> bool {
    this.update(cx, |view, _| view.generation == generation)
        .unwrap_or(false)
}

fn host_action_is_current(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
    generation: u64,
    model_generation: u64,
) -> bool {
    this.update(cx, |view, _| {
        view.generation == generation
            && model_generation_is_current(view.model_generation, model_generation)
    })
    .unwrap_or(false)
}

fn host_snapshot(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
) -> Option<(CliphistProvider, Snapshot)> {
    this.update(cx, |view, _| {
        Some((view.provider.clone(), view.snapshot.clone()?))
    })
    .ok()
    .flatten()
}

fn host_snapshot_with_pins(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
) -> Option<(CliphistProvider, Snapshot, Vec<String>)> {
    this.update(cx, |view, _| {
        Some((
            view.provider.clone(),
            view.snapshot.clone()?,
            view.configured_pins.clone(),
        ))
    })
    .ok()
    .flatten()
}

fn normalize_pin_key(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn pin_matches_preview(pin: &str, preview: &str) -> bool {
    let key = normalize_pin_key(pin);
    let preview = normalize_pin_key(preview);
    if preview.is_empty() {
        return false;
    }
    if preview.chars().count() >= 100 {
        key.starts_with(&preview)
    } else {
        key == preview
    }
}

fn begin_refresh(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
    generation: u64,
) -> Option<u64> {
    this.update(cx, |view, cx| {
        if view.generation == generation {
            view.model_generation = view.model_generation.wrapping_add(1);
            view.state = ClipboardViewState::Loading;
            view.error = None;
            cx.notify();
            Some(view.model_generation)
        } else {
            None
        }
    })
    .ok()
    .flatten()
}

fn set_unavailable_error(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
    generation: u64,
) {
    let _ = this.update(cx, |view, cx| {
        if view.generation == generation {
            view.error = Some("Clipboard operation failed. Refresh and try again.".into());
            view.state = if view.snapshot.is_some() {
                ClipboardViewState::Populated
            } else {
                ClipboardViewState::Unavailable
            };
            cx.notify();
        }
    });
}

fn set_operation_error(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
    generation: u64,
    detail: String,
) {
    let _ = this.update(cx, |view, cx| {
        if view.generation == generation {
            view.error = Some(format!("Clipboard operation failed: {detail}"));
            view.state = if view.snapshot.is_some() {
                ClipboardViewState::Populated
            } else {
                ClipboardViewState::Unavailable
            };
            cx.notify();
        }
    });
}

async fn refresh_after_mutation(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
    generation: u64,
    action_model_generation: u64,
    provider: CliphistProvider,
    pins: Vec<String>,
) {
    if !host_action_is_current(this, cx, generation, action_model_generation) {
        return;
    }
    let Some(model_generation) = begin_refresh(this, cx, generation) else {
        return;
    };
    let result = cx
        .background_executor()
        .spawn(async move { provider.list(&pins) })
        .await;
    apply_snapshot(this, cx, generation, model_generation, result, true);
}

fn apply_snapshot(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
    generation: u64,
    model_generation: u64,
    result: Result<Snapshot, crate::families::waffle::clipboard_provider::ProviderError>,
    after_mutation: bool,
) {
    let _ = this.update(cx, |view, cx| {
        if view.generation != generation || view.model_generation != model_generation {
            return;
        }
        match result {
            Ok(snapshot) => {
                view.state = if snapshot.entries.is_empty() && snapshot.pins.is_empty() {
                    ClipboardViewState::Empty
                } else {
                    ClipboardViewState::Populated
                };
                view.snapshot = Some(snapshot);
                view.error = None;
            }
            Err(_) => {
                view.state = ClipboardViewState::Unavailable;
                if after_mutation {
                    view.snapshot = None;
                }
                view.error =
                    Some("Clipboard history is unavailable. Check cliphist and wl-copy.".into());
            }
        }
        view.selected = 0;
        view.clear_confirmation = false;
        cx.notify();
    });
}

fn emit_dismiss(
    this: &gpui::WeakEntity<IiClipboardHost>,
    cx: &mut gpui::AsyncApp,
    generation: u64,
) {
    let _ = this.update(cx, |view, _| {
        if view.generation != generation {
            return;
        }
        view.generation = view.generation.wrapping_add(1);
        let _ = view
            .events
            .try_send(crate::surface_manager::IiClipboardSurfaceEvent::Dismiss {
                display: view.display,
                generation: view.generation,
            });
    });
}
