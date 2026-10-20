//! Per-display Material ii Cheatsheet keybind overlay host.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

use async_channel::Sender;
use gpui::{
    App, Context, DisplayId, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent, Render,
    ScrollHandle, Size, Window, div, prelude::*, px, rgba,
};

use crate::{
    families::{
        ii::{
            cheatsheet::{
                self, CheatsheetKeybindsInput, CheatsheetLabels, CheatsheetSearchInput,
                KeybindProviderState, KeybindSnapshot, QueryChange,
            },
            cheatsheet_elements::{self, PeriodicTableInput},
            keybind_provider::{self, KeybindProviderPaths},
        },
        waffle::taskbar::start_menu_search::StartMenuSearch,
    },
    theme::ThemeTokens,
};

static PROVIDER_LOAD_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static COPY_FEEDBACK_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheatsheetPage {
    Keybinds,
    Elements,
}

pub(crate) fn next_copy_feedback_generation() -> u64 {
    COPY_FEEDBACK_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCompositor {
    Niri,
    Hyprland,
}

/// Determine the compositor only from the session variables used by those
/// compositors. An absent or ambiguous signal never selects a parser.
pub fn detect_session_compositor(
    niri_socket: Option<&OsStr>,
    hyprland_signature: Option<&OsStr>,
) -> Result<SessionCompositor, String> {
    let has_niri = niri_socket.is_some_and(|value| !value.is_empty());
    let has_hyprland = hyprland_signature.is_some_and(|value| !value.is_empty());
    match (has_niri, has_hyprland) {
        (true, false) => Ok(SessionCompositor::Niri),
        (false, true) => Ok(SessionCompositor::Hyprland),
        (false, false) => Err("No supported Niri or Hyprland session signal is available".into()),
        (true, true) => Err("Conflicting Niri and Hyprland session signals are present".into()),
    }
}

/// Resolve explicit parser/config paths from the confirmed compositor and
/// current XDG config directory. Returns an error if the session or config
/// home cannot be identified.
pub fn provider_paths_from_session(
    niri_socket: Option<&OsStr>,
    hyprland_signature: Option<&OsStr>,
    scripts_dir: &Path,
    config_dir: Option<&Path>,
) -> Result<KeybindProviderPaths, String> {
    let compositor = detect_session_compositor(niri_socket, hyprland_signature)?;
    let config_dir =
        config_dir.ok_or_else(|| "XDG_CONFIG_HOME and HOME are unavailable".to_owned())?;
    match compositor {
        SessionCompositor::Niri => Ok(KeybindProviderPaths::Niri {
            python: PathBuf::from("/usr/bin/python3"),
            parser_script: scripts_dir.join("parse_niri_keybinds.py"),
            config_file: config_dir.join("niri/config.kdl"),
        }),
        SessionCompositor::Hyprland => Ok(KeybindProviderPaths::Hyprland {
            parser_script: scripts_dir.join("hyprland/get_keybinds.py"),
            default_config_file: config_dir.join("hypr/hyprland/keybinds.conf"),
            user_config_file: config_dir.join("hypr/custom/keybinds.conf"),
        }),
    }
}

pub(crate) fn current_provider_paths() -> Result<KeybindProviderPaths, String> {
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    let scripts_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
    provider_paths_from_session(
        std::env::var_os("NIRI_SOCKET").as_deref(),
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").as_deref(),
        &scripts_dir,
        config_dir.as_deref(),
    )
}

pub struct IiCheatsheetHost {
    display: DisplayId,
    output_size: Size<gpui::Pixels>,
    theme: ThemeTokens,
    snapshot: KeybindSnapshot,
    query: String,
    page: CheatsheetPage,
    copied_symbol: Option<String>,
    copied_generation: u64,
    elements_scroll: ScrollHandle,
    host_focus: FocusHandle,
    keybinds_tab_focus: FocusHandle,
    elements_tab_focus: FocusHandle,
    search_editor: Entity<StartMenuSearch>,
    events: Sender<crate::surface_manager::IiCheatsheetEvent>,
    provider_paths: Option<KeybindProviderPaths>,
    provider_error: Option<String>,
    load_generation: u64,
    focused_once: bool,
}

impl IiCheatsheetHost {
    pub fn new(
        display: DisplayId,
        output_size: Size<gpui::Pixels>,
        theme: ThemeTokens,
        paths: Result<KeybindProviderPaths, String>,
        events: Sender<crate::surface_manager::IiCheatsheetEvent>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (provider_paths, provider_error) = match paths {
            Ok(paths) => (Some(paths), None),
            Err(error) => (None, Some(error)),
        };
        let snapshot = provider_error.as_ref().map_or_else(
            || KeybindSnapshot {
                state: KeybindProviderState::Loading,
                categories: Vec::new(),
                detail: None,
            },
            |detail| unavailable_snapshot(detail.clone()),
        );
        let query_events = events.clone();
        let search_editor =
            cx.new(|cx| {
                StartMenuSearch::new_with_labels(
                    String::new(),
                    "Search keybinds…".into(),
                    "Search keybinds".into(),
                    Arc::new(move |query| {
                        let _ = query_events.try_send(
                            crate::surface_manager::IiCheatsheetEvent::Query { display, query },
                        );
                    }),
                    cx,
                )
            });
        Self {
            display,
            output_size,
            theme,
            snapshot,
            query: String::new(),
            page: CheatsheetPage::Keybinds,
            copied_symbol: None,
            copied_generation: 0,
            elements_scroll: ScrollHandle::new(),
            host_focus: cx.focus_handle(),
            keybinds_tab_focus: cx.focus_handle(),
            elements_tab_focus: cx.focus_handle(),
            search_editor,
            events,
            provider_paths,
            provider_error,
            load_generation: 0,
            focused_once: false,
        }
    }

    /// Starts parser subprocesses on a dedicated OS thread. Never call the
    /// blocking provider from the GPUI UI thread.
    pub fn refresh_provider(&mut self, cx: &mut Context<Self>) {
        let Some(paths) = self.provider_paths.clone() else {
            if let Some(error) = &self.provider_error {
                self.snapshot = unavailable_snapshot(error.clone());
            }
            cx.notify();
            return;
        };
        self.load_generation = PROVIDER_LOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let generation = self.load_generation;
        self.snapshot = KeybindSnapshot {
            state: KeybindProviderState::Loading,
            categories: Vec::new(),
            detail: None,
        };
        cx.notify();
        let display = self.display;
        let events = self.events.clone();
        let _ = thread::Builder::new()
            .name("ii-cheatsheet-keybind-provider".into())
            .spawn(move || {
                let snapshot = keybind_provider::load_keybinds(&paths);
                let _ = events.send_blocking(crate::surface_manager::IiCheatsheetEvent::Loaded {
                    display,
                    generation,
                    snapshot,
                });
            });
    }

    pub fn apply_snapshot(
        &mut self,
        generation: u64,
        snapshot: KeybindSnapshot,
        cx: &mut Context<Self>,
    ) {
        if generation != self.load_generation {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }

    pub fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.query != query {
            self.query = query;
            cx.notify();
        }
    }

    pub fn set_page(&mut self, show_elements: bool, cx: &mut Context<Self>) {
        let page = if show_elements {
            CheatsheetPage::Elements
        } else {
            CheatsheetPage::Keybinds
        };
        if self.page != page {
            self.page = page;
            cx.notify();
        }
    }

    pub fn set_copied_symbol(&mut self, symbol: String, generation: u64, cx: &mut Context<Self>) {
        self.copied_symbol = Some(symbol);
        self.copied_generation = generation;
        cx.notify();
    }

    pub fn clear_copy_feedback(&mut self, generation: u64, cx: &mut Context<Self>) {
        if self.copied_generation == generation {
            self.copied_symbol = None;
            cx.notify();
        }
    }

    pub fn is_copy_feedback_current(&self, generation: u64) -> bool {
        self.copied_generation == generation && self.copied_symbol.is_some()
    }

    pub fn set_theme(&mut self, theme: ThemeTokens, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    pub fn set_output_size(&mut self, output_size: Size<gpui::Pixels>, cx: &mut Context<Self>) {
        self.output_size = output_size;
        cx.notify();
    }

    fn is_constrained(&self) -> bool {
        self.output_size.width.as_f32() < 688.0 || self.output_size.height.as_f32() < 408.0
    }

    fn input(&self, cx: &Context<Self>) -> CheatsheetKeybindsInput {
        let query_events = self.events.clone();
        let display = self.display;
        CheatsheetKeybindsInput {
            snapshot: self.snapshot.clone(),
            query: self.query.clone(),
            search: Some(CheatsheetSearchInput {
                editor: self.search_editor.clone().into_any_element(),
                focus: self.search_editor.focus_handle(cx),
            }),
            theme: self.theme.material.clone(),
            labels: CheatsheetLabels::default(),
            on_query_change: Arc::new(move |query| {
                let _ = query_events
                    .try_send(crate::surface_manager::IiCheatsheetEvent::Query { display, query });
            }) as QueryChange,
        }
    }
}

impl Focusable for IiCheatsheetHost {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.is_constrained() {
            self.host_focus.clone()
        } else {
            self.search_editor.focus_handle(cx)
        }
    }
}

impl Render for IiCheatsheetHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            let focus = if self.is_constrained() {
                self.host_focus.clone()
            } else {
                self.search_editor.focus_handle(cx)
            };
            window.focus(&focus, cx);
            self.focused_once = true;
        }
        let query = self.query.clone();
        let events = self.events.clone();
        let display = self.display;
        self.search_editor.update(cx, |editor, cx| {
            let editor_events = events.clone();
            editor.sync_controlled_query(
                query.clone(),
                Arc::new(move |query| {
                    let _ =
                        editor_events.try_send(crate::surface_manager::IiCheatsheetEvent::Query {
                            display,
                            query,
                        });
                }),
                cx,
            );
        });
        let width = (self.output_size.width.as_f32() - 48.0).clamp(640.0, 1120.0);
        let height = (self.output_size.height.as_f32() - 48.0).clamp(360.0, 920.0);
        if self.output_size.width.as_f32() < 688.0 || self.output_size.height.as_f32() < 408.0 {
            let available_width = (self.output_size.width.as_f32() - 32.0).max(200.0);
            let available_height = (self.output_size.height.as_f32() - 32.0).max(120.0);
            let close_events = self.events.clone();
            let backdrop_events = self.events.clone();
            let display = self.display;
            return div()
                .id("ii.cheatsheet.host.constrained")
                .track_focus(&self.host_focus)
                .focusable()
                .size_full()
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0x00000066))
                .role(gpui::Role::Dialog)
                .aria_label("ii Cheatsheet")
                .on_key_down(move |event: &KeyDownEvent, _, cx| {
                    if !event.is_held && event.keystroke.key == "escape" {
                        let _ = close_events
                            .try_send(crate::surface_manager::IiCheatsheetEvent::Close { display });
                        cx.stop_propagation();
                    }
                })
                .child(
                    div()
                        .id("ii.cheatsheet.backdrop.constrained")
                        .absolute()
                        .inset_0()
                        .on_click(move |_, _, _| {
                            let _ = backdrop_events.try_send(
                                crate::surface_manager::IiCheatsheetEvent::Close { display },
                            );
                        }),
                )
                .child(
                    div()
                        .id("ii.cheatsheet.constrained-message")
                        .w(px(available_width))
                        .h(px(available_height))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(12.))
                        .rounded(px(16.))
                        .bg(self
                            .theme
                            .material
                            .colors
                            .surface_container_low
                            .unwrap_or_else(|| rgba(0xf4f1f8ff).into()))
                        .text_color(
                            self.theme
                                .material
                                .colors
                                .on_surface
                                .unwrap_or_else(|| rgba(0x1b1b1fff).into()),
                        )
                        .role(gpui::Role::Status)
                        .aria_label("Cheatsheet requires a larger display")
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child("Cheatsheet needs at least 688 × 408 pixels")
                        .child("Resize this output or open it on a larger display."),
                );
        }
        let focus = self.search_editor.focus_handle(cx);
        let query_is_empty = self.query.is_empty();
        let close_on_escape = query_is_empty || self.page == CheatsheetPage::Elements;
        let close_events = self.events.clone();
        let close_for_backdrop = self.events.clone();
        let tab_events = self.events.clone();
        let find_keybinds_events = self.events.clone();
        let on_copy_events = self.events.clone();
        let copy_feedback = self.copied_symbol.clone();
        let selected_page = self.page;
        let keybinds_tab_focus = self.keybinds_tab_focus.clone();
        let elements_tab_focus = self.elements_tab_focus.clone();
        let display = self.display;
        let page_content = match self.page {
            CheatsheetPage::Keybinds => cheatsheet::render_cheatsheet_keybinds(self.input(cx)),
            CheatsheetPage::Elements => {
                cheatsheet_elements::render_periodic_table(PeriodicTableInput {
                    theme: self.theme.material.clone(),
                    scroll: self.elements_scroll.clone(),
                    copied_symbol: copy_feedback,
                    on_copy: Arc::new(move |symbol| {
                        let _ = on_copy_events.try_send(
                            crate::surface_manager::IiCheatsheetEvent::CopyElement {
                                display,
                                symbol,
                            },
                        );
                    }),
                })
            }
        };
        let tab_color = self
            .theme
            .material
            .colors
            .on_surface
            .unwrap_or_else(|| rgba(0x1b1b1fff).into());
        let tab_surface = self
            .theme
            .material
            .colors
            .surface_container
            .unwrap_or_else(|| rgba(0xece6f0ff).into());
        let tab_accent = self
            .theme
            .material
            .colors
            .primary
            .unwrap_or_else(|| rgba(0x6750a4ff).into());
        let keybinds_tab = page_tab(
            "ii.cheatsheet.tab.keybinds",
            "Keybinds",
            CheatsheetPage::Keybinds,
            selected_page,
            keybinds_tab_focus.clone(),
            elements_tab_focus.clone(),
            tab_surface,
            tab_color,
            tab_accent,
            tab_events.clone(),
            display,
        );
        let elements_tab = page_tab(
            "ii.cheatsheet.tab.elements",
            "Periodic table",
            CheatsheetPage::Elements,
            selected_page,
            elements_tab_focus,
            keybinds_tab_focus,
            tab_surface,
            tab_color,
            tab_accent,
            tab_events,
            display,
        );
        div()
            .id("ii.cheatsheet.host")
            .track_focus(&self.host_focus)
            .focusable()
            .size_full()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000066))
            .role(gpui::Role::Dialog)
            .aria_label("ii Cheatsheet")
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if !event.is_held
                    && event.keystroke.key == "f"
                    && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
                {
                    focus.focus(window, cx);
                    let _ = find_keybinds_events.try_send(
                        crate::surface_manager::IiCheatsheetEvent::SetPage {
                            display,
                            page: crate::surface_manager::IiCheatsheetPage::Keybinds,
                        },
                    );
                    cx.stop_propagation();
                } else if !event.is_held && event.keystroke.key == "escape" && close_on_escape {
                    let _ = close_events
                        .try_send(crate::surface_manager::IiCheatsheetEvent::Close { display });
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .id("ii.cheatsheet.backdrop")
                    .absolute()
                    .inset_0()
                    .on_click(move |_, _, _| {
                        let _ = close_for_backdrop
                            .try_send(crate::surface_manager::IiCheatsheetEvent::Close { display });
                    }),
            )
            .child(
                div()
                    .id("ii.cheatsheet.card")
                    .w(px(width))
                    .h(px(height))
                    .min_w(px(640.))
                    .flex()
                    .flex_col()
                    .rounded(px(20.))
                    .shadow_lg()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id("ii.cheatsheet.tabs")
                            .w_full()
                            .flex_shrink_0()
                            .flex()
                            .gap(px(8.))
                            .p(px(10.))
                            .role(gpui::Role::TabList)
                            .aria_label("Cheatsheet pages")
                            .child(keybinds_tab)
                            .child(elements_tab),
                    )
                    .child(
                        div()
                            .id("ii.cheatsheet.tab-panel")
                            .w_full()
                            .flex_1()
                            .min_h_0()
                            .role(gpui::Role::TabPanel)
                            .aria_label(match selected_page {
                                CheatsheetPage::Keybinds => "Keybinds",
                                CheatsheetPage::Elements => "Periodic table",
                            })
                            .child(page_content),
                    ),
            )
    }
}

fn page_tab(
    id: &'static str,
    label: &'static str,
    page: CheatsheetPage,
    selected_page: CheatsheetPage,
    focus: FocusHandle,
    other_focus: FocusHandle,
    background: gpui::Hsla,
    foreground: gpui::Hsla,
    accent: gpui::Hsla,
    events: Sender<crate::surface_manager::IiCheatsheetEvent>,
    display: DisplayId,
) -> impl IntoElement {
    let selected = tab_is_tab_stop(page, selected_page);
    let click_events = events.clone();
    let key_events = events;
    let focus_for_click = focus.clone();
    let focus_for_key = focus.clone();
    let other_focus_for_key = other_focus.clone();
    div()
        .id(id)
        .track_focus(&focus)
        .px(px(14.))
        .py(px(7.))
        .rounded(px(18.))
        .bg(if selected { accent } else { background })
        .text_color(if selected {
            gpui::hsla(0., 0., 1., 1.)
        } else {
            foreground
        })
        .role(gpui::Role::Tab)
        .aria_label(label)
        .aria_selected(selected)
        .tab_stop(selected)
        .cursor_pointer()
        .on_click(move |_, window, cx| {
            focus_for_click.focus(window, cx);
            send_tab_selection(&click_events, display, page);
        })
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if event.is_held {
                return;
            }
            match tab_keyboard_action(page, &event.keystroke.key) {
                Some(TabKeyboardAction::Activate) => {
                    send_tab_selection(&key_events, display, page);
                    cx.stop_propagation();
                }
                Some(TabKeyboardAction::Navigate(target)) => {
                    if target == page {
                        focus_for_key.focus(window, cx);
                    } else {
                        other_focus_for_key.focus(window, cx);
                    }
                    send_tab_selection(&key_events, display, target);
                    cx.stop_propagation();
                }
                None => {}
            }
        })
        .child(label)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabKeyboardAction {
    Activate,
    Navigate(CheatsheetPage),
}

fn tab_keyboard_action(page: CheatsheetPage, key: &str) -> Option<TabKeyboardAction> {
    match key {
        "enter" | "space" => Some(TabKeyboardAction::Activate),
        "left" if page == CheatsheetPage::Elements => {
            Some(TabKeyboardAction::Navigate(CheatsheetPage::Keybinds))
        }
        "right" if page == CheatsheetPage::Keybinds => {
            Some(TabKeyboardAction::Navigate(CheatsheetPage::Elements))
        }
        "home" => Some(TabKeyboardAction::Navigate(CheatsheetPage::Keybinds)),
        "end" => Some(TabKeyboardAction::Navigate(CheatsheetPage::Elements)),
        _ => None,
    }
}

fn tab_is_tab_stop(tab: CheatsheetPage, selected: CheatsheetPage) -> bool {
    tab == selected
}

fn send_tab_selection(
    events: &Sender<crate::surface_manager::IiCheatsheetEvent>,
    display: DisplayId,
    page: CheatsheetPage,
) {
    let page = match page {
        CheatsheetPage::Keybinds => crate::surface_manager::IiCheatsheetPage::Keybinds,
        CheatsheetPage::Elements => crate::surface_manager::IiCheatsheetPage::Elements,
    };
    let _ = events.try_send(crate::surface_manager::IiCheatsheetEvent::SetPage { display, page });
}

fn unavailable_snapshot(detail: String) -> KeybindSnapshot {
    KeybindSnapshot {
        state: KeybindProviderState::Unavailable,
        categories: Vec::new(),
        detail: Some(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CheatsheetPage, SessionCompositor, TabKeyboardAction, detect_session_compositor,
        provider_paths_from_session, tab_is_tab_stop, tab_keyboard_action,
    };
    use std::{ffi::OsStr, path::Path};

    #[test]
    fn compositor_selection_requires_one_unambiguous_session_signal() {
        assert_eq!(
            detect_session_compositor(Some(OsStr::new("/run/niri.sock")), None),
            Ok(SessionCompositor::Niri)
        );
        assert_eq!(
            detect_session_compositor(None, Some(OsStr::new("hypr-session"))),
            Ok(SessionCompositor::Hyprland)
        );
        assert!(detect_session_compositor(None, None).is_err());
        assert!(
            detect_session_compositor(
                Some(OsStr::new("/run/niri.sock")),
                Some(OsStr::new("hypr-session"))
            )
            .is_err()
        );
    }

    #[test]
    fn compositor_paths_are_explicit_and_match_qml_parser_inputs() {
        let scripts = Path::new("/source/scripts");
        let config = Path::new("/config");
        let niri = provider_paths_from_session(
            Some(OsStr::new("/run/niri.sock")),
            None,
            scripts,
            Some(config),
        )
        .unwrap();
        assert!(
            matches!(niri, crate::families::ii::keybind_provider::KeybindProviderPaths::Niri { parser_script, config_file, .. }
            if parser_script == scripts.join("parse_niri_keybinds.py") && config_file == config.join("niri/config.kdl"))
        );
        let hypr = provider_paths_from_session(
            None,
            Some(OsStr::new("hypr-session")),
            scripts,
            Some(config),
        )
        .unwrap();
        assert!(
            matches!(hypr, crate::families::ii::keybind_provider::KeybindProviderPaths::Hyprland { parser_script, default_config_file, user_config_file }
            if parser_script == scripts.join("hyprland/get_keybinds.py")
                && default_config_file == config.join("hypr/hyprland/keybinds.conf")
                && user_config_file == config.join("hypr/custom/keybinds.conf"))
        );
    }

    #[test]
    fn cheatsheet_tabs_support_activation_and_roving_keyboard_navigation() {
        assert_eq!(
            tab_keyboard_action(CheatsheetPage::Keybinds, "enter"),
            Some(TabKeyboardAction::Activate)
        );
        assert_eq!(
            tab_keyboard_action(CheatsheetPage::Elements, "space"),
            Some(TabKeyboardAction::Activate)
        );
        assert_eq!(
            tab_keyboard_action(CheatsheetPage::Keybinds, "right"),
            Some(TabKeyboardAction::Navigate(CheatsheetPage::Elements))
        );
        assert_eq!(
            tab_keyboard_action(CheatsheetPage::Elements, "left"),
            Some(TabKeyboardAction::Navigate(CheatsheetPage::Keybinds))
        );
        assert_eq!(
            tab_keyboard_action(CheatsheetPage::Keybinds, "end"),
            Some(TabKeyboardAction::Navigate(CheatsheetPage::Elements))
        );
        assert_eq!(tab_keyboard_action(CheatsheetPage::Elements, "tab"), None);
        assert!(tab_is_tab_stop(
            CheatsheetPage::Elements,
            CheatsheetPage::Elements
        ));
        assert!(!tab_is_tab_stop(
            CheatsheetPage::Keybinds,
            CheatsheetPage::Elements
        ));
    }
}
