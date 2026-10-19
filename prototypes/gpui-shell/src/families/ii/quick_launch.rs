//! Configured QuickLaunch button row for the ii Widgets sidebar.
//!
//! The caller supplies configured shortcut records, running state, and the
//! launch callback. This component never invents defaults or launches commands.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use gpui::{
    AnyElement, FontWeight, IntoElement, KeyDownEvent, Role, SharedString, div, prelude::*, px,
};

use super::sidebars::SidebarStyle;

pub type LaunchAction = Arc<dyn Fn(String) + Send + Sync>;

/// Launches a configured command as an argv vector, never through a shell.
/// Plain commands and single/double-quoted arguments are accepted; shell
/// operators and substitutions are rejected rather than interpreted.
pub fn spawn_configured_command(source: &str) -> Result<(), String> {
    let mut argv = parse_command_line(source)?;
    if argv.is_empty() {
        return Err("Command is empty".into());
    }
    let executable = argv.remove(0);
    Command::new(executable)
        .args(argv)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not launch configured command: {error}"))
}

fn parse_command_line(source: &str) -> Result<Vec<String>, String> {
    let mut argv = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;
    for ch in source.chars() {
        if escaped {
            word.push(ch);
            escaped = false;
            started = true;
            continue;
        }
        if quote == Some('\'') {
            if ch == '\'' {
                quote = None;
            } else {
                word.push(ch);
            }
            started = true;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            started = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                word.push(ch);
            }
            started = true;
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                started = true;
            }
            c if c.is_whitespace() => {
                if started {
                    argv.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            '|' | '&' | ';' | '<' | '>' | '$' | '`' | '(' | ')' | '*' | '?' | '[' | ']' | '{'
            | '}' | '~' | '#' | '\n' | '\r' => {
                return Err(format!(
                    "Shell syntax `{ch}` is not supported in QuickLaunch commands"
                ));
            }
            _ => {
                word.push(ch);
                started = true;
            }
        }
    }
    if escaped {
        return Err("Command ends with an incomplete escape".into());
    }
    if quote.is_some() {
        return Err("Command has an unclosed quote".into());
    }
    if started {
        argv.push(word);
    }
    Ok(argv)
}

/// Returns the executable basename from the same argv parsing used to launch.
pub fn command_basename(source: &str) -> Option<String> {
    let argv = parse_command_line(source).ok()?;
    let executable = argv.first()?;
    Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

#[derive(Clone, Debug)]
pub struct QuickLaunchEntry {
    /// Configured command, also used as the callback identity.
    pub command: String,
    pub name: String,
    /// Material Symbols name from the shortcut configuration.
    pub icon: String,
    pub running: bool,
}

#[derive(Clone)]
pub struct QuickLaunchInput {
    pub entries: Vec<QuickLaunchEntry>,
    pub style: SidebarStyle,
    pub on_launch: LaunchAction,
}

impl std::fmt::Debug for QuickLaunchInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QuickLaunchInput")
            .field("entries", &self.entries)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

pub fn render_quick_launch(input: QuickLaunchInput) -> AnyElement {
    let style = input.style;
    let buttons = input
        .entries
        .into_iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            if entry.command.trim().is_empty() {
                return None;
            }
            let command = entry.command;
            let keyboard_command = command.clone();
            let name = if entry.name.trim().is_empty() {
                command_basename(&command).unwrap_or_else(|| "Application".to_owned())
            } else {
                entry.name
            };
            let accessible_name = if entry.running {
                format!("{name}, running")
            } else {
                name.clone()
            };
            let on_click = input.on_launch.clone();
            let on_key = input.on_launch.clone();
            let button_bg = if entry.running {
                style.selected
            } else {
                style.surface.opacity(0.)
            };
            let button_hover = if entry.running {
                style.selected.opacity(0.82)
            } else {
                style.hover
            };
            let foreground = if entry.running {
                style.accent
            } else {
                style.foreground
            };
            let indicator = style.accent;
            let focus_border = style.accent;
            Some(
                div()
                    .id(SharedString::from(format!("ii.quick-launch.{index}")))
                    .w(px(52.))
                    .h(px(56.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(2.))
                    .role(Role::Button)
                    .aria_label(SharedString::from(accessible_name))
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .focus_visible(move |element| element.border_1().border_color(focus_border))
                    .hover(move |element| element.bg(button_hover))
                    .rounded(px(style.radius))
                    .bg(button_bg)
                    .on_click(move |_, _, _| on_click(command.clone()))
                    .on_key_down(move |event: &KeyDownEvent, _, cx| {
                        if !event.is_held
                            && matches!(event.keystroke.key.as_str(), "enter" | "space")
                        {
                            on_key(keyboard_command.clone());
                            cx.stop_propagation();
                        }
                    })
                    .child(
                        div()
                            .w(px(48.))
                            .h(px(48.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(foreground)
                            .font_family("Material Symbols Rounded")
                            .text_size(px(24.))
                            .child(entry.icon),
                    )
                    .when(entry.running, |button| {
                        button.child(div().w(px(6.)).h(px(6.)).rounded(px(3.)).bg(indicator))
                    })
                    .when(!entry.running, |button| {
                        button.child(
                            div()
                                .w(px(6.))
                                .h(px(6.))
                                .rounded(px(3.))
                                .bg(style.surface.opacity(0.)),
                        )
                    })
                    .font_weight(FontWeight::NORMAL)
                    .into_any_element(),
            )
        })
        .collect::<Vec<_>>();

    div()
        .id("ii.quick-launch")
        .w_full()
        .px(px(12.))
        .flex()
        .justify_center()
        .gap(px(0.))
        .children(buttons)
        .into_any_element()
}
