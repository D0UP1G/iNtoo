//! Blocking adapter around the existing Niri and Hyprland keybind parsers.
//!
//! The parser scripts remain the source of truth for compositor config
//! semantics. Call [`load_keybinds`] from a worker thread; it starts processes
//! and waits for their output. Hosts that already own a process/worker layer
//! can instead call the pure `decode_*_json` functions with captured output.

use std::{path::PathBuf, process::Command};

use serde_json::Value;

use super::cheatsheet::{KeybindCategory, KeybindProviderState, KeybindRecord, KeybindSnapshot};

/// Explicit script/config paths required to invoke the parser scripts.
///
/// No repository-relative or user-config path is inferred by the adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeybindProviderPaths {
    Niri {
        python: PathBuf,
        parser_script: PathBuf,
        config_file: PathBuf,
    },
    Hyprland {
        parser_script: PathBuf,
        default_config_file: PathBuf,
        user_config_file: PathBuf,
    },
}

/// Invoke an existing parser and normalize its JSON for the Cheatsheet view.
///
/// This function blocks on child processes and must run outside GPUI's UI
/// thread. It intentionally does not watch files or subscribe to compositor
/// reload events; the host owns refresh and lifecycle.
pub fn load_keybinds(paths: &KeybindProviderPaths) -> KeybindSnapshot {
    match paths {
        KeybindProviderPaths::Niri {
            python,
            parser_script,
            config_file,
        } => {
            let output = Command::new(python)
                .arg(parser_script)
                .arg(config_file)
                .output();
            decode_niri_command_output(output)
        }
        KeybindProviderPaths::Hyprland {
            parser_script,
            default_config_file,
            user_config_file,
        } => {
            let default_output = run_hypr_parser(parser_script, default_config_file);
            let user_output = run_hypr_parser(parser_script, user_config_file);
            let default_snapshot = decode_command_output(default_output, decode_hyprland_json);
            let user_snapshot = decode_command_output(user_output, decode_hyprland_json);
            combine_hyprland(default_snapshot, user_snapshot)
        }
    }
}

/// Decode the stdout payload emitted by `scripts/parse_niri_keybinds.py`.
///
/// The script emits categories as `children[]`, each containing another
/// `children[]` layer with `keybinds[]`. A flat `keybinds[]` category shape is
/// accepted as well for fixture/future compatibility.
pub fn decode_niri_json(json: &str) -> KeybindSnapshot {
    let value = match parse_json(json) {
        Ok(value) => value,
        Err(error) => return niri_fallback_with_detail(error),
    };
    if let Some(error) = parser_error(&value) {
        return niri_fallback_with_detail(error);
    }

    match parse_niri_categories(&value) {
        Ok(categories) => snapshot_from_categories(categories, None),
        Err(error) => niri_fallback_with_detail(error),
    }
}

/// Decode stdout from `scripts/hyprland/get_keybinds.py`.
///
/// The Hyprland parser emits a section object with ordered `children[]` and
/// `keybinds[]`; the top-level section has an empty name and is not displayed
/// as a category, matching the source model's structure.
pub fn decode_hyprland_json(json: &str) -> KeybindSnapshot {
    let value = match parse_json(json) {
        Ok(value) => value,
        Err(error) => return unavailable(error),
    };
    if let Some(error) = parser_error(&value) {
        return unavailable(error);
    }

    match parse_hypr_sections(&value) {
        Ok(categories) => snapshot_from_categories(categories, None),
        Err(error) => unavailable(error),
    }
}

fn decode_niri_command_output(output: std::io::Result<std::process::Output>) -> KeybindSnapshot {
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            return niri_fallback_with_detail(format!("Could not start keybind parser: {error}"));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let detail = if stderr.is_empty() {
            format!("Keybind parser exited with {}", output.status)
        } else {
            stderr
        };
        return niri_fallback_with_detail(detail);
    }
    if stdout.trim().is_empty() {
        return niri_fallback_with_detail("Keybind parser returned no output".into());
    }
    decode_niri_json(&stdout)
}

fn run_hypr_parser(
    parser_script: &PathBuf,
    config_file: &PathBuf,
) -> std::io::Result<std::process::Output> {
    Command::new(parser_script)
        .arg("--path")
        .arg(config_file)
        .output()
}

fn decode_command_output(
    output: std::io::Result<std::process::Output>,
    decoder: fn(&str) -> KeybindSnapshot,
) -> KeybindSnapshot {
    let output = match output {
        Ok(output) => output,
        Err(error) => return unavailable(format!("Could not start keybind parser: {error}")),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let detail = if stderr.is_empty() {
            format!("Keybind parser exited with {}", output.status)
        } else {
            stderr
        };
        return unavailable(detail);
    }
    if stdout.trim().is_empty() {
        return unavailable("Keybind parser returned no output".into());
    }
    decoder(&stdout)
}

fn combine_hyprland(
    default_snapshot: KeybindSnapshot,
    user_snapshot: KeybindSnapshot,
) -> KeybindSnapshot {
    let default_ready = is_loaded(&default_snapshot);
    let user_ready = is_loaded(&user_snapshot);
    if !default_ready && !user_ready {
        let details = [default_snapshot.detail, user_snapshot.detail]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        return unavailable(if details.is_empty() {
            "Both Hyprland keybind sources are unavailable".into()
        } else {
            details.join("; ")
        });
    }

    let mut categories = Vec::new();
    if default_ready {
        categories.extend(default_snapshot.categories);
    }
    if user_ready {
        categories.extend(user_snapshot.categories);
    }
    let detail = [
        (!default_ready)
            .then_some(default_snapshot.detail)
            .flatten(),
        (!user_ready).then_some(user_snapshot.detail).flatten(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    snapshot_from_categories(categories, (!detail.is_empty()).then(|| detail.join("; ")))
}

fn is_loaded(snapshot: &KeybindSnapshot) -> bool {
    matches!(
        snapshot.state,
        KeybindProviderState::Ready | KeybindProviderState::Empty
    )
}

fn parse_json(json: &str) -> Result<Value, String> {
    serde_json::from_str(json.trim()).map_err(|error| format!("Invalid keybind JSON: {error}"))
}

fn niri_fallback_with_detail(detail: String) -> KeybindSnapshot {
    let mut snapshot = niri_default_snapshot();
    snapshot.detail = Some(detail);
    snapshot
}

/// QML's `defaultKeybinds` model used while its parser cannot load binds.
/// The snapshot is `Ready` so the Cheatsheet continues rendering these rows;
/// `detail` is empty here and set by callers handling a load failure.
pub fn niri_default_snapshot() -> KeybindSnapshot {
    let categories = vec![
        category(
            "System",
            &[
                (&["Super"], "Tab", "Niri Overview"),
                (&["Super", "Shift"], "E", "Quit Niri"),
                (&["Super"], "Escape", "Toggle shortcuts inhibit"),
                (&["Super", "Shift"], "O", "Power off monitors"),
            ],
        ),
        category(
            "iNtoo Shell",
            &[
                (&["Super"], "Space", "iNtoo Overview"),
                (&["Super"], "G", "iNtoo Overlay"),
                (&["Super"], "V", "Clipboard"),
                (&["Super"], "Comma", "Settings"),
                (&["Super"], "Slash", "Cheatsheet"),
                (&["Super", "Alt"], "L", "Lock Screen"),
                (&["Ctrl", "Alt"], "T", "Wallpaper Selector"),
                (&["Super", "Shift"], "W", "Cycle panel style"),
                (&["Super", "Shift"], "Q", "Session dialog"),
            ],
        ),
        category(
            "Window Switcher",
            &[
                (&["Alt"], "Tab", "Next window"),
                (&["Alt", "Shift"], "Tab", "Previous window"),
            ],
        ),
        category(
            "Region Tools",
            &[
                (&["Super", "Shift"], "S", "Screenshot region"),
                (&["Super", "Shift"], "X", "OCR region"),
                (&["Super", "Shift"], "A", "Reverse image search"),
            ],
        ),
        category(
            "Applications",
            &[
                (&["Super"], "T", "Terminal"),
                (&["Super"], "Return", "Terminal"),
                (&["Super"], "E", "File manager"),
                (&["Super"], "W", "Browser"),
            ],
        ),
        category(
            "Window Management",
            &[
                (&["Super"], "Q", "Close window"),
                (&["Super"], "D", "Maximize column"),
                (&["Super"], "F", "Fullscreen"),
                (&["Super"], "A", "Toggle floating"),
                (&["Super", "Shift"], "V", "Switch float/tile focus"),
                (&["Super"], "[", "Consume/expel left"),
                (&["Super"], "]", "Consume/expel right"),
            ],
        ),
        category(
            "Layout",
            &[
                (&["Super"], "R", "Cycle column width"),
                (&["Super", "Shift"], "R", "Cycle window height"),
                (&["Super", "Ctrl"], "R", "Reset window height"),
                (&["Super"], "C", "Center column"),
            ],
        ),
        category(
            "Resize",
            &[
                (&["Super"], "-", "Shrink column 10%"),
                (&["Super"], "=", "Grow column 10%"),
                (&["Super", "Shift"], "-", "Shrink window 10%"),
                (&["Super", "Shift"], "=", "Grow window 10%"),
            ],
        ),
        category(
            "Focus",
            &[
                (&["Super"], "←/→/↑/↓", "Focus direction"),
                (&["Super"], "H/J/K/L", "Focus (vim)"),
                (&["Super"], "Home", "Focus first column"),
                (&["Super"], "End", "Focus last column"),
            ],
        ),
        category(
            "Move Windows",
            &[
                (&["Super", "Shift"], "←/→/↑/↓", "Move direction"),
                (&["Super", "Shift"], "H/J/K/L", "Move (vim)"),
                (&["Super", "Ctrl"], "Home", "Move to first"),
                (&["Super", "Ctrl"], "End", "Move to last"),
            ],
        ),
        category(
            "Monitors",
            &[
                (&["Super", "Ctrl"], "←/→/↑/↓", "Focus monitor"),
                (&["Super", "Ctrl", "Shift"], "←/→/↑/↓", "Move to monitor"),
            ],
        ),
        category(
            "Workspaces",
            &[
                (&["Super"], "1-9", "Focus workspace"),
                (&["Super", "Ctrl"], "1-9", "Move to workspace"),
                (&["Super"], "PgUp/PgDn", "Workspace up/down"),
            ],
        ),
        category(
            "Screenshots",
            &[
                (&[], "Print", "Screenshot (select)"),
                (&["Ctrl"], "Print", "Screenshot screen"),
                (&["Alt"], "Print", "Screenshot window"),
            ],
        ),
        category(
            "Media",
            &[
                (&[], "Vol+", "Volume up"),
                (&[], "Vol-", "Volume down"),
                (&[], "Mute", "Mute audio"),
                (&["Ctrl", "Super"], "Space", "Play/Pause"),
                (&["Super", "Alt"], "N", "Next track"),
                (&["Super", "Alt"], "P", "Previous track"),
            ],
        ),
    ];
    KeybindSnapshot {
        state: KeybindProviderState::Ready,
        categories,
        detail: None,
    }
}

fn category(name: &str, rows: &[(&[&str], &str, &str)]) -> KeybindCategory {
    KeybindCategory {
        name: name.to_owned(),
        bindings: rows
            .iter()
            .map(|(mods, key, comment)| KeybindRecord {
                key: (*key).to_owned(),
                modifiers: mods.iter().map(|modifier| (*modifier).to_owned()).collect(),
                comment: (*comment).to_owned(),
            })
            .collect(),
    }
}

fn parser_error(value: &Value) -> Option<String> {
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Some(error.to_owned());
    }
    if value.as_str() == Some("error") {
        return Some("Keybind parser could not read the configured file".into());
    }
    None
}

fn parse_niri_categories(value: &Value) -> Result<Vec<KeybindCategory>, String> {
    let root = value
        .as_object()
        .ok_or_else(|| "Malformed Niri parser result: expected an object".to_owned())?;
    let children = root
        .get("children")
        .ok_or_else(|| "Malformed Niri parser result: missing children array".to_owned())?
        .as_array()
        .ok_or_else(|| "Malformed Niri parser result: children must be an array".to_owned())?;
    let mut categories = Vec::with_capacity(children.len());
    for (index, category) in children.iter().enumerate() {
        let category = category.as_object().ok_or_else(|| {
            format!("Malformed Niri category at index {index}: expected an object")
        })?;
        let name = category
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("Malformed Niri category at index {index}: missing string name")
            })?;
        let bindings = if let Some(value) = category.get("keybinds") {
            parse_bindings(
                value,
                &format!("Niri category `{name}`"),
                BindingFormat::Niri,
            )?
        } else {
            let nested = category.get("children").ok_or_else(|| {
                format!("Malformed Niri category `{name}`: missing children/keybinds")
            })?;
            let nested = nested.as_array().ok_or_else(|| {
                format!("Malformed Niri category `{name}`: children must be an array")
            })?;
            let mut bindings = Vec::new();
            for (child_index, child) in nested.iter().enumerate() {
                let child = child.as_object().ok_or_else(|| {
                    format!(
                        "Malformed Niri category `{name}` child {child_index}: expected an object"
                    )
                })?;
                if let Some(child_bindings) = child.get("keybinds") {
                    bindings.extend(parse_bindings(
                        child_bindings,
                        &format!("Niri category `{name}` child {child_index}"),
                        BindingFormat::Niri,
                    )?);
                } else if let Some(grandchildren) = child.get("children") {
                    let grandchildren = grandchildren.as_array().ok_or_else(|| {
                        format!("Malformed Niri category `{name}` child {child_index}: children must be an array")
                    })?;
                    for (grandchild_index, grandchild) in grandchildren.iter().enumerate() {
                        let grandchild = grandchild.as_object().ok_or_else(|| {
                            format!("Malformed Niri category `{name}` child {child_index}.{grandchild_index}: expected an object")
                        })?;
                        if let Some(items) = grandchild.get("keybinds") {
                            bindings.extend(parse_bindings(
                                items,
                                &format!(
                                    "Niri category `{name}` child {child_index}.{grandchild_index}"
                                ),
                                BindingFormat::Niri,
                            )?);
                        } else {
                            return Err(format!(
                                "Malformed Niri category `{name}` child {child_index}.{grandchild_index}: missing keybinds array"
                            ));
                        }
                    }
                } else {
                    return Err(format!(
                        "Malformed Niri category `{name}` child {child_index}: missing keybinds array"
                    ));
                }
            }
            bindings
        };
        categories.push(KeybindCategory {
            name: name.to_owned(),
            bindings,
        });
    }
    Ok(categories)
}

fn parse_hypr_sections(value: &Value) -> Result<Vec<KeybindCategory>, String> {
    let root = value
        .as_object()
        .ok_or_else(|| "Malformed Hyprland parser result: expected a section object".to_owned())?;
    let mut categories = Vec::new();
    parse_hypr_section(root, "root", true, &mut categories)?;
    Ok(categories)
}

fn parse_hypr_section(
    section: &serde_json::Map<String, Value>,
    path: &str,
    is_root: bool,
    categories: &mut Vec<KeybindCategory>,
) -> Result<(), String> {
    let name = section
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Malformed Hyprland section `{path}`: missing string name"))?;
    let items = section
        .get("keybinds")
        .ok_or_else(|| format!("Malformed Hyprland section `{path}`: missing keybinds array"))?;
    let bindings = parse_bindings(
        items,
        &format!("Hyprland section `{path}`"),
        BindingFormat::Hyprland,
    )?;
    if !is_root && !name.is_empty() {
        categories.push(KeybindCategory {
            name: name.to_owned(),
            bindings,
        });
    }
    let children = section
        .get("children")
        .ok_or_else(|| format!("Malformed Hyprland section `{path}`: missing children array"))?
        .as_array()
        .ok_or_else(|| format!("Malformed Hyprland section `{path}`: children must be an array"))?;
    for (index, child) in children.iter().enumerate() {
        let child = child.as_object().ok_or_else(|| {
            format!("Malformed Hyprland section `{path}` child {index}: expected an object")
        })?;
        let child_name = child.get("name").and_then(Value::as_str).unwrap_or("?");
        parse_hypr_section(child, &format!("{path}/{child_name}"), false, categories)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum BindingFormat {
    Niri,
    Hyprland,
}

fn parse_bindings(
    value: &Value,
    context: &str,
    format: BindingFormat,
) -> Result<Vec<KeybindRecord>, String> {
    let entries = value
        .as_array()
        .ok_or_else(|| format!("Malformed {context}: keybinds must be an array"))?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            normalize_binding(entry, &format!("{context} keybind {index}"), format)
        })
        .collect()
}

fn normalize_binding(
    value: &Value,
    context: &str,
    format: BindingFormat,
) -> Result<KeybindRecord, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("Malformed {context}: expected an object"))?;
    let key = object
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Malformed {context}: key must be a string"))?
        .to_owned();
    let modifiers = match object.get("mods") {
        Some(Value::Array(values)) if values.iter().all(Value::is_string) => {
            normalize_modifiers(object.get("mods"))
        }
        Some(Value::Array(_)) => {
            return Err(format!(
                "Malformed {context}: mods array entries must be strings"
            ));
        }
        Some(Value::String(_)) => normalize_modifiers(object.get("mods")),
        Some(_) => {
            return Err(format!(
                "Malformed {context}: mods must be an array or string"
            ));
        }
        None => return Err(format!("Malformed {context}: missing mods")),
    };
    if let Some(comment) = object.get("comment") {
        if !comment.is_string() {
            return Err(format!("Malformed {context}: comment must be a string"));
        }
    }
    if let Some(action) = object.get("action") {
        if !action.is_string() {
            return Err(format!("Malformed {context}: action must be a string"));
        }
    }
    let comment = object
        .get("comment")
        .and_then(Value::as_str)
        .filter(|comment| !comment.is_empty())
        .or_else(|| object.get("action").and_then(Value::as_str))
        .map(str::to_owned)
        .or_else(|| match format {
            BindingFormat::Niri => None,
            BindingFormat::Hyprland => {
                let dispatcher = object.get("dispatcher")?.as_str()?;
                let params = object.get("params")?.as_str()?;
                Some(if params.is_empty() {
                    dispatcher.to_owned()
                } else {
                    format!("{dispatcher} {params}")
                })
            }
        })
        .unwrap_or_default();
    if let BindingFormat::Hyprland = format {
        for field in ["dispatcher", "params"] {
            if !object.get(field).is_some_and(Value::is_string) {
                return Err(format!("Malformed {context}: {field} must be a string"));
            }
        }
    }
    Ok(KeybindRecord {
        key,
        modifiers,
        comment,
    })
}

fn normalize_modifiers(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(value)) => value
            .split(['+', ' '])
            .filter(|modifier| !modifier.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn snapshot_from_categories(
    categories: Vec<KeybindCategory>,
    detail: Option<String>,
) -> KeybindSnapshot {
    let is_empty = categories
        .iter()
        .all(|category| category.bindings.is_empty());
    KeybindSnapshot {
        state: if is_empty {
            KeybindProviderState::Empty
        } else {
            KeybindProviderState::Ready
        },
        categories,
        detail,
    }
}

fn unavailable(detail: String) -> KeybindSnapshot {
    KeybindSnapshot {
        state: KeybindProviderState::Unavailable,
        categories: Vec::new(),
        detail: Some(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn niri_fixture_preserves_category_binding_order_and_fields() {
        let snapshot = decode_niri_json(
            r#"{"children":[
                {"name":"System","children":[{"keybinds":[
                    {"mods":["Super","Shift"],"key":"Q","comment":"Quit shell","action":"quit"},
                    {"mods":["Alt"],"key":"F4","action":"close-window"}
                ]}]},
                {"name":"Windows","children":[{"keybinds":[
                    {"mods":[],"key":"Tab","comment":"Next window"}
                ]}]}
            ]}"#,
        );
        assert_eq!(snapshot.state, KeybindProviderState::Ready);
        assert_eq!(snapshot.categories.len(), 2);
        assert_eq!(snapshot.categories[0].name, "System");
        assert_eq!(snapshot.categories[0].bindings[0].key, "Q");
        assert_eq!(
            snapshot.categories[0].bindings[0].modifiers,
            ["Super", "Shift"]
        );
        assert_eq!(snapshot.categories[0].bindings[0].comment, "Quit shell");
        assert_eq!(snapshot.categories[0].bindings[1].comment, "close-window");
        assert_eq!(snapshot.categories[1].bindings[0].key, "Tab");
    }

    #[test]
    fn hypr_fixture_keeps_nested_order_comments_and_action_fields() {
        let snapshot = decode_hyprland_json(
            r#"{"name":"","keybinds":[],"children":[
                {"name":"Windows","keybinds":[
                    {"mods":["SUPER"],"key":"Q","dispatcher":"killactive","params":"","comment":"Close window"}
                ],"children":[]},
                {"name":"Custom","keybinds":[
                    {"mods":"ALT SHIFT","key":"R","action":"reload shell","comment":"","dispatcher":"exec","params":"reload shell"}
                ],"children":[]}
            ]}"#,
        );
        assert_eq!(snapshot.state, KeybindProviderState::Ready);
        assert_eq!(snapshot.categories.len(), 2);
        assert_eq!(snapshot.categories[0].name, "Windows");
        assert_eq!(snapshot.categories[0].bindings[0].comment, "Close window");
        assert_eq!(
            snapshot.categories[1].bindings[0].modifiers,
            ["ALT", "SHIFT"]
        );
        assert_eq!(snapshot.categories[1].bindings[0].comment, "reload shell");
    }

    #[test]
    fn empty_invalid_and_parser_error_are_distinct_states() {
        assert_eq!(
            decode_niri_json(r#"{"children":[]}"#).state,
            KeybindProviderState::Empty
        );
        assert_eq!(
            decode_hyprland_json(r#"{"error":"Config not found"}"#).state,
            KeybindProviderState::Unavailable
        );
        assert_eq!(
            decode_niri_json("not json").state,
            KeybindProviderState::Ready
        );
        assert!(decode_niri_json("not json").detail.is_some());
    }

    #[test]
    fn malformed_niri_schema_uses_visible_defaults_and_keeps_diagnostic() {
        let malformed = [
            r#"{"children":"wrong"}"#,
            r#"{"children":[{"name":7,"children":[]}] }"#,
            r#"{"children":[{"name":"Broken","children":[{"keybinds":{}}]}]}"#,
            r#"{"children":[{"name":"Broken","children":[{"keybinds":[{"key":8,"mods":[],"comment":"bad"}]}]}]}"#,
            r#"{"children":[{"name":"Broken","children":[{"keybinds":[{"key":"X","mods":["Super",4],"comment":"bad"}]}]}]}"#,
        ];
        for payload in malformed {
            let snapshot = decode_niri_json(payload);
            assert_eq!(snapshot.state, KeybindProviderState::Ready, "{payload}");
            assert_eq!(snapshot.categories.len(), 14, "{payload}");
            assert!(
                snapshot
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("Malformed")),
                "{payload}"
            );
        }
    }

    #[test]
    fn niri_defaults_match_qml_category_counts_and_exact_rows() {
        let snapshot = niri_default_snapshot();
        assert_eq!(snapshot.state, KeybindProviderState::Ready);
        assert_eq!(snapshot.detail, None);
        assert_eq!(
            snapshot
                .categories
                .iter()
                .map(|category| category.name.as_str())
                .collect::<Vec<_>>(),
            [
                "System",
                "iNtoo Shell",
                "Window Switcher",
                "Region Tools",
                "Applications",
                "Window Management",
                "Layout",
                "Resize",
                "Focus",
                "Move Windows",
                "Monitors",
                "Workspaces",
                "Screenshots",
                "Media",
            ]
        );
        assert_eq!(
            snapshot
                .categories
                .iter()
                .map(|category| category.bindings.len())
                .collect::<Vec<_>>(),
            [4, 9, 2, 3, 4, 7, 4, 4, 4, 4, 2, 3, 3, 6]
        );
        assert_eq!(
            snapshot
                .categories
                .iter()
                .map(|category| category.bindings.len())
                .sum::<usize>(),
            59
        );
        let system = &snapshot.categories[0].bindings;
        assert_eq!(
            system[0],
            KeybindRecord {
                key: "Tab".into(),
                modifiers: vec!["Super".into()],
                comment: "Niri Overview".into()
            }
        );
        assert_eq!(
            system[1],
            KeybindRecord {
                key: "E".into(),
                modifiers: vec!["Super".into(), "Shift".into()],
                comment: "Quit Niri".into()
            }
        );
        let shell = &snapshot.categories[1].bindings;
        assert_eq!(
            shell[4],
            KeybindRecord {
                key: "Slash".into(),
                modifiers: vec!["Super".into()],
                comment: "Cheatsheet".into()
            }
        );
        assert_eq!(
            shell[6],
            KeybindRecord {
                key: "T".into(),
                modifiers: vec!["Ctrl".into(), "Alt".into()],
                comment: "Wallpaper Selector".into()
            }
        );
        let media = &snapshot.categories[13].bindings;
        assert_eq!(
            media[0],
            KeybindRecord {
                key: "Vol+".into(),
                modifiers: vec![],
                comment: "Volume up".into()
            }
        );
        assert_eq!(
            media[3],
            KeybindRecord {
                key: "Space".into(),
                modifiers: vec!["Ctrl".into(), "Super".into()],
                comment: "Play/Pause".into()
            }
        );
        assert_eq!(
            media[5],
            KeybindRecord {
                key: "P".into(),
                modifiers: vec!["Super".into(), "Alt".into()],
                comment: "Previous track".into()
            }
        );
    }

    #[test]
    fn parser_failures_return_visible_niri_defaults_with_diagnostic() {
        let snapshot = niri_fallback_with_detail("Config not found".into());
        assert_eq!(snapshot.state, KeybindProviderState::Ready);
        assert_eq!(snapshot.categories.len(), 14);
        assert_eq!(snapshot.detail.as_deref(), Some("Config not found"));
        let parser_error = decode_niri_json(r#"{"error":"No binds block found","children":[]}"#);
        assert_eq!(parser_error.categories.len(), 14);
        assert_eq!(parser_error.detail.as_deref(), Some("No binds block found"));
    }

    #[test]
    fn malformed_hyprland_schema_is_unavailable_with_specific_detail() {
        for payload in [
            r#"{"name":"","keybinds":[],"children":"wrong"}"#,
            r#"{"name":"","keybinds":[],"children":[{"name":"Broken","keybinds":[{"key":4,"mods":[],"comment":"bad","dispatcher":"exec","params":""}],"children":[]}] }"#,
        ] {
            let snapshot = decode_hyprland_json(payload);
            assert_eq!(snapshot.state, KeybindProviderState::Unavailable);
            assert!(
                snapshot
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("Malformed"))
            );
        }
    }

    #[test]
    fn hyprland_combines_available_sources_and_reports_partial_failure() {
        let available = decode_hyprland_json(
            r#"{"name":"","keybinds":[],"children":[{"name":"User","keybinds":[{"key":"T","mods":[],"comment":"Terminal","dispatcher":"exec","params":"terminal"}],"children":[]}] }"#,
        );
        let failed = unavailable("default config missing".into());
        let combined = combine_hyprland(failed, available);
        assert_eq!(combined.state, KeybindProviderState::Ready);
        assert_eq!(combined.categories[0].name, "User");
        assert_eq!(combined.detail.as_deref(), Some("default config missing"));
    }
}
