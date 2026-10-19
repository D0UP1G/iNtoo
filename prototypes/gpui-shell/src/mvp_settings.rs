//! Small persisted settings model for the basic GPUI shell.
//!
//! Settings live in the existing runtime config under `gpuiMvp`. Updates read
//! and merge the complete JSON object before an atomic replacement, preserving
//! fields this prototype does not understand.

use serde_json::Value;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const DEFAULT_CONFIG: &str = include_str!("../../../defaults/config.json");
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    #[default]
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelEdge {
    Top,
    #[default]
    Bottom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MvpSettings {
    pub configured: bool,
    pub color_scheme: ColorScheme,
    pub accent: String,
    pub text_scale: f32,
    pub corner_radius: f32,
    pub panel_edge: PanelEdge,
    pub panel_height: f32,
    pub wallpaper: Option<String>,
    pub panel_modules: Vec<String>,
    pub enabled_modules: Vec<String>,
    /// Raw per-plugin JSON values from `gpuiMvp.<plugin_id>`.
    pub plugin_settings: std::collections::HashMap<String, Value>,
}

impl Default for MvpSettings {
    fn default() -> Self {
        Self {
            configured: false,
            color_scheme: ColorScheme::Dark,
            accent: "#7aa2f7".into(),
            text_scale: 1.0,
            corner_radius: 12.0,
            panel_edge: PanelEdge::Top,
            panel_height: 48.0,
            wallpaper: None,
            panel_modules: vec!["workspaces".into(), "clock".into()],
            enabled_modules: vec!["workspaces".into(), "clock".into()],
            plugin_settings: std::collections::HashMap::new(),
        }
    }
}

impl MvpSettings {
    pub fn from_json(root: &Value) -> (Self, Vec<String>) {
        let mut settings = Self::default();
        let mut diagnostics = Vec::new();
        let Some(config) = root.get("gpuiMvp") else {
            return (settings, diagnostics);
        };
        if !config.is_object() {
            diagnostics.push("gpuiMvp must be an object; using defaults".into());
            return (settings, diagnostics);
        }
        settings.configured = true;

        if let Some(value) = config.get("colorScheme") {
            settings.color_scheme = match value.as_str() {
                Some("light") => ColorScheme::Light,
                Some("dark") => ColorScheme::Dark,
                _ => {
                    diagnostics.push("gpuiMvp.colorScheme must be light or dark".into());
                    settings.color_scheme
                }
            };
        }
        if let Some(value) = config.get("accent") {
            match value.as_str().filter(|value| valid_hex_color(value)) {
                Some(value) => settings.accent = value.to_ascii_lowercase(),
                None => diagnostics.push("gpuiMvp.accent must be a #RRGGBB color".into()),
            }
        }
        settings.text_scale = bounded_number(
            config,
            "textScale",
            0.75,
            1.5,
            settings.text_scale,
            &mut diagnostics,
        );
        settings.corner_radius = bounded_number(
            config,
            "cornerRadius",
            0.0,
            32.0,
            settings.corner_radius,
            &mut diagnostics,
        );
        let default_bar = serde_json::from_str::<Value>(DEFAULT_CONFIG).ok();
        if let Some(bottom) = root
            .get("bar")
            .and_then(|bar| bar.get("bottom"))
            .and_then(Value::as_bool)
            .or_else(|| {
                default_bar
                    .as_ref()
                    .and_then(|defaults| defaults.get("bar"))
                    .and_then(|bar| bar.get("bottom"))
                    .and_then(Value::as_bool)
            })
        {
            settings.panel_edge = if bottom {
                PanelEdge::Bottom
            } else {
                PanelEdge::Top
            };
        }
        let bar_height = root
            .get("bar")
            .and_then(|bar| bar.get("height"))
            .and_then(Value::as_f64)
            .or_else(|| {
                default_bar
                    .as_ref()
                    .and_then(|defaults| defaults.get("bar"))
                    .and_then(|bar| bar.get("height"))
                    .and_then(Value::as_f64)
            });
        if let Some(height) = bar_height {
            if height.is_finite() && (24.0..=128.0).contains(&height) {
                settings.panel_height = height as f32;
            } else {
                diagnostics.push("bar.height must be between 24 and 128".into());
            }
        }
        if let Some(value) = config.get("wallpaper") {
            if !value.is_null() {
                match value.as_str() {
                    Some(path) if path.len() <= 4096 && !path.contains('\0') => {
                        settings.wallpaper = (!path.is_empty()).then(|| path.to_owned());
                    }
                    _ => diagnostics.push("gpuiMvp.wallpaper must be a path string".into()),
                }
            }
        }
        if settings.wallpaper.is_none() {
            settings.wallpaper = root
                .get("background")
                .and_then(|background| background.get("wallpaperPath"))
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty() && path.len() <= 4096 && !path.contains('\0'))
                .map(str::to_owned);
        }
        read_module_list(
            config.get("panelModules"),
            "panelModules",
            &mut settings.panel_modules,
            &mut diagnostics,
        );
        read_module_list(
            config.get("enabledModules"),
            "enabledModules",
            &mut settings.enabled_modules,
            &mut diagnostics,
        );
        settings
            .enabled_modules
            .retain(|id| settings.panel_modules.contains(id));
        settings.plugin_settings = settings
            .panel_modules
            .iter()
            .filter_map(|id| config.get(id).cloned().map(|value| (id.clone(), value)))
            .collect();
        (settings, diagnostics)
    }

    pub fn write_to(&self, path: impl AsRef<Path>) -> Result<(), String> {
        if !valid_hex_color(&self.accent) {
            return Err("accent must be a #RRGGBB color".into());
        }
        let path = path.as_ref();
        let (mut root, target_mode) = match fs::read(path) {
            Ok(bytes) => {
                let mode = fs::metadata(path)
                    .map_err(|error| format!("cannot read configuration permissions: {error}"))?
                    .permissions()
                    .mode()
                    & 0o777;
                (
                    serde_json::from_slice::<Value>(&bytes)
                        .map_err(|error| format!("configuration JSON is invalid: {error}"))?,
                    mode,
                )
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
                serde_json::from_str::<Value>(DEFAULT_CONFIG)
                    .map_err(|error| format!("bundled defaults are invalid: {error}"))?,
                0o600,
            ),
            Err(error) => return Err(format!("cannot read configuration: {error}")),
        };
        let object = root
            .as_object_mut()
            .ok_or("configuration root must be a JSON object")?;
        let mut settings = object
            .remove("gpuiMvp")
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        settings.insert(
            "colorScheme".into(),
            Value::String(
                match self.color_scheme {
                    ColorScheme::Light => "light",
                    ColorScheme::Dark => "dark",
                }
                .into(),
            ),
        );
        settings.insert(
            "accent".into(),
            Value::String(self.accent.to_ascii_lowercase()),
        );
        settings.insert(
            "textScale".into(),
            json_number(self.text_scale.clamp(0.75, 1.5))?,
        );
        settings.insert(
            "cornerRadius".into(),
            json_number(self.corner_radius.clamp(0.0, 32.0))?,
        );
        settings.insert(
            "wallpaper".into(),
            self.wallpaper
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        settings.insert("panelModules".into(), string_array(&self.panel_modules));
        settings.insert("enabledModules".into(), string_array(&self.enabled_modules));
        object.insert("gpuiMvp".into(), Value::Object(settings));
        let bar = object
            .entry("bar")
            .or_insert_with(|| Value::Object(serde_json::Map::new()))
            .as_object_mut()
            .ok_or("bar config must be an object")?;
        bar.insert(
            "bottom".into(),
            Value::Bool(self.panel_edge == PanelEdge::Bottom),
        );
        bar.insert(
            "height".into(),
            json_number(self.panel_height.clamp(24.0, 128.0))?,
        );
        {
            let background = object
                .entry("background")
                .or_insert_with(|| Value::Object(serde_json::Map::new()))
                .as_object_mut()
                .ok_or("background config must be an object")?;
            background.insert(
                "wallpaperPath".into(),
                Value::String(self.wallpaper.clone().unwrap_or_default()),
            );
        }

        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create config directory: {error}"))?;
        let temp_path = temporary_path(path);
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                // Keep the temporary config private until its final mode is set.
                .mode(0o600)
                .open(&temp_path)
                .map_err(|error| format!("cannot create temporary config: {error}"))?;
            serde_json::to_writer_pretty(&mut file, &root)
                .map_err(|error| format!("cannot serialize config: {error}"))?;
            file.write_all(b"\n")
                .map_err(|error| format!("cannot finish config: {error}"))?;
            fs::set_permissions(&temp_path, fs::Permissions::from_mode(target_mode))
                .map_err(|error| format!("cannot set configuration permissions: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("cannot sync config: {error}"))?;
            fs::rename(&temp_path, path)
                .map_err(|error| format!("cannot replace config: {error}"))?;
            if let Ok(directory) = OpenOptions::new().read(true).open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }
}

/// Data a panel module contributes to the shared panel renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelWidget {
    Text {
        text: String,
        accessible_name: String,
    },
    Workspaces(Vec<PluginWorkspace>),
}

/// A widget paired with the registered module that contributed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredPanelWidget {
    pub plugin_id: &'static str,
    pub widget: PanelWidget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginWorkspace {
    pub number: u8,
    pub occupied: bool,
    pub active: bool,
}

/// Small service/config context supplied by the host to built-in modules.
pub struct PanelPluginContext<'a> {
    pub snapshot: &'a crate::niri::Snapshot,
    /// The registered plugin's `gpuiMvp.<plugin_id>` JSON value, or `null`
    /// when that namespace is not configured.
    #[allow(dead_code)] // Built-in widgets need not consume plugin-specific settings.
    pub plugin_settings: &'a Value,
}

/// Statically linked panel module contract. Runtime resources owned by a
/// plugin must be released in `stop` before its view is dropped.
pub trait PanelPlugin {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn start(&mut self) {}
    fn stop(&mut self) {}
    fn namespaced_settings(&self, settings: &MvpSettings) -> Value {
        settings
            .plugin_settings
            .get(self.id())
            .cloned()
            .unwrap_or(Value::Null)
    }
    fn widget(&self, context: &PanelPluginContext<'_>) -> Option<PanelWidget>;
}

pub struct ClockPlugin;
impl PanelPlugin for ClockPlugin {
    fn id(&self) -> &'static str {
        "clock"
    }
    fn name(&self) -> &'static str {
        "Часы"
    }
    fn widget(&self, context: &PanelPluginContext<'_>) -> Option<PanelWidget> {
        context
            .snapshot
            .clock
            .as_ref()
            .map(|text| PanelWidget::Text {
                text: text.clone(),
                accessible_name: format!("Время: {text}"),
            })
    }
}

pub struct NiriWorkspacesPlugin;
impl PanelPlugin for NiriWorkspacesPlugin {
    fn id(&self) -> &'static str {
        "workspaces"
    }
    fn name(&self) -> &'static str {
        "Рабочие столы"
    }
    fn widget(&self, context: &PanelPluginContext<'_>) -> Option<PanelWidget> {
        let workspaces = context
            .snapshot
            .workspaces
            .iter()
            .map(|workspace| PluginWorkspace {
                number: workspace.number,
                occupied: workspace.occupied,
                active: workspace.active,
            })
            .collect::<Vec<_>>();
        (!workspaces.is_empty()).then_some(PanelWidget::Workspaces(workspaces))
    }
}

/// Registry owns built-in module lifecycles and applies configured order and
/// enablement. Adding a module only requires a registration here.
pub struct PanelPluginRegistry {
    plugins: Vec<Box<dyn PanelPlugin>>,
    started: std::collections::HashSet<&'static str>,
}

impl PanelPluginRegistry {
    pub fn builtins() -> Self {
        Self {
            plugins: vec![Box::new(NiriWorkspacesPlugin), Box::new(ClockPlugin)],
            started: std::collections::HashSet::new(),
        }
    }

    pub fn start_enabled(&mut self, settings: &MvpSettings) {
        for plugin in &mut self.plugins {
            if settings.enabled_modules.iter().any(|id| id == plugin.id())
                && self.started.insert(plugin.id())
            {
                plugin.start();
            }
        }
    }

    pub fn stop_all(&mut self) {
        for plugin in &mut self.plugins {
            if self.started.remove(plugin.id()) {
                plugin.stop();
            }
        }
    }

    pub fn descriptors(&self) -> Vec<(&'static str, &'static str)> {
        self.plugins
            .iter()
            .map(|plugin| (plugin.id(), plugin.name()))
            .collect()
    }

    pub fn widgets(
        &self,
        settings: &MvpSettings,
        snapshot: &crate::niri::Snapshot,
    ) -> Vec<RegisteredPanelWidget> {
        settings
            .panel_modules
            .iter()
            .filter(|id| settings.enabled_modules.contains(id))
            .filter_map(|id| self.plugins.iter().find(|plugin| plugin.id() == id))
            .filter_map(|plugin| {
                let plugin_settings = plugin.namespaced_settings(settings);
                let context = PanelPluginContext {
                    snapshot,
                    plugin_settings: &plugin_settings,
                };
                plugin.widget(&context).map(|widget| RegisteredPanelWidget {
                    plugin_id: plugin.id(),
                    widget,
                })
            })
            .collect()
    }
}

impl Drop for PanelPluginRegistry {
    fn drop(&mut self) {
        self.stop_all();
    }
}

fn json_number(value: f32) -> Result<Value, String> {
    serde_json::Number::from_f64(f64::from(value))
        .map(Value::Number)
        .ok_or_else(|| "setting must be a finite number".to_owned())
}

fn bounded_number(
    config: &Value,
    key: &str,
    min: f32,
    max: f32,
    default: f32,
    diagnostics: &mut Vec<String>,
) -> f32 {
    let Some(value) = config.get(key) else {
        return default;
    };
    match value
        .as_f64()
        .filter(|value| value.is_finite() && *value >= f64::from(min) && *value <= f64::from(max))
    {
        Some(value) => value as f32,
        None => {
            diagnostics.push(format!("gpuiMvp.{key} must be between {min} and {max}"));
            default
        }
    }
}

fn valid_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn read_module_list(
    value: Option<&Value>,
    key: &str,
    target: &mut Vec<String>,
    diagnostics: &mut Vec<String>,
) {
    let Some(value) = value else { return };
    let Some(entries) = value.as_array() else {
        diagnostics.push(format!("gpuiMvp.{key} must be an array"));
        return;
    };
    let mut parsed = Vec::new();
    for entry in entries {
        let Some(id) = entry.as_str().filter(|id| {
            !id.is_empty()
                && id.len() <= 64
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }) else {
            diagnostics.push(format!("gpuiMvp.{key} contains an invalid module id"));
            continue;
        };
        if !parsed.iter().any(|known| known == id) {
            parsed.push(id.to_owned());
        }
    }
    *target = parsed;
}

fn string_array(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn temporary_path(path: &Path) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.{}.{}.tmp", std::process::id(), sequence))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct GreetingPlugin;

    impl PanelPlugin for GreetingPlugin {
        fn id(&self) -> &'static str {
            "greeting"
        }

        fn name(&self) -> &'static str {
            "Greeting"
        }

        fn widget(&self, context: &PanelPluginContext<'_>) -> Option<PanelWidget> {
            let greeting = context
                .plugin_settings
                .get("message")
                .and_then(Value::as_str)?;
            Some(PanelWidget::Text {
                text: greeting.into(),
                accessible_name: "Greeting".into(),
            })
        }
    }

    fn test_config_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "intoo-gpui-{label}-{}-{}.json",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn invalid_values_use_defaults_and_report_diagnostics() {
        let input = serde_json::json!({ "gpuiMvp": {
            "textScale": 9.0,
            "accent": "red",
            "panelModules": ["clock", "clock", "bad/id"],
            "enabledModules": ["workspaces", "clock"]
        }, "bar": { "height": 12 }});
        let (settings, diagnostics) = MvpSettings::from_json(&input);
        assert_eq!(settings.text_scale, 1.0);
        assert_eq!(settings.panel_height, 48.0);
        assert_eq!(settings.accent, "#7aa2f7");
        assert_eq!(settings.panel_modules, ["clock"]);
        assert_eq!(settings.enabled_modules, ["clock"]);
        assert_eq!(diagnostics.len(), 4);
    }

    #[test]
    fn atomic_update_preserves_unknown_root_and_settings_fields() {
        let root = test_config_path("config");
        fs::write(
            &root,
            r#"{"futureRoot":{"keep":true},"gpuiMvp":{"futureSetting":42}}"#,
        )
        .unwrap();
        let mut settings = MvpSettings::default();
        settings.color_scheme = ColorScheme::Light;
        settings.panel_edge = PanelEdge::Bottom;
        settings.panel_height = 64.0;
        settings.write_to(&root).unwrap();
        let saved: Value = serde_json::from_slice(&fs::read(&root).unwrap()).unwrap();
        assert_eq!(saved["futureRoot"]["keep"], true);
        assert_eq!(saved["gpuiMvp"]["futureSetting"], 42);
        assert_eq!(saved["gpuiMvp"]["colorScheme"], "light");
        assert_eq!(saved["bar"]["bottom"], true);
        assert_eq!(saved["bar"]["height"], 64.0);
        let (reloaded, diagnostics) = MvpSettings::from_json(&saved);
        assert!(diagnostics.is_empty());
        assert_eq!(reloaded.panel_edge, PanelEdge::Bottom);
        assert_eq!(reloaded.panel_height, 64.0);
        let _ = fs::remove_file(root);
    }

    #[test]
    fn atomic_update_preserves_existing_config_mode() {
        let path = test_config_path("mode");
        fs::write(&path, r#"{"futureRoot":true}"#).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        MvpSettings::default().write_to(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn new_config_file_uses_private_mode() {
        let path = test_config_path("new-mode");
        MvpSettings::default().write_to(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_file_is_never_replaced() {
        let path = test_config_path("malformed");
        fs::write(&path, "not json").unwrap();
        assert!(MvpSettings::default().write_to(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn plugin_lists_keep_order_and_remove_duplicates() {
        let input = serde_json::json!({ "gpuiMvp": {
            "panelModules": ["clock", "workspaces", "clock"],
            "enabledModules": ["workspaces", "unknown", "clock"]
        }});
        let (settings, _) = MvpSettings::from_json(&input);
        assert_eq!(settings.panel_modules, ["clock", "workspaces"]);
        assert_eq!(settings.enabled_modules, ["workspaces", "clock"]);
    }

    #[test]
    fn builtins_are_registered_in_configured_order_and_respect_enablement() {
        let input = serde_json::json!({ "gpuiMvp": {
            "panelModules": ["clock", "workspaces"],
            "enabledModules": ["clock"]
        }});
        let (settings, _) = MvpSettings::from_json(&input);
        let mut registry = PanelPluginRegistry::builtins();
        registry.start_enabled(&settings);
        let snapshot = crate::niri::Snapshot {
            clock: Some("12:34".into()),
            workspaces: vec![crate::niri::Workspace {
                id: 1,
                number: 2,
                name: None,
                occupied: true,
                active: true,
            }],
            ..Default::default()
        };
        assert_eq!(
            registry.widgets(&settings, &snapshot),
            [RegisteredPanelWidget {
                plugin_id: "clock",
                widget: PanelWidget::Text {
                    text: "12:34".into(),
                    accessible_name: "Время: 12:34".into(),
                }
            }]
        );
        registry.stop_all();
    }

    #[test]
    fn registered_plugin_receives_its_namespaced_settings() {
        let input = serde_json::json!({ "gpuiMvp": {
            "panelModules": ["greeting"],
            "enabledModules": ["greeting"],
            "greeting": { "message": "Привет из настроек" }
        }});
        let (settings, diagnostics) = MvpSettings::from_json(&input);
        assert!(diagnostics.is_empty());

        let registry = PanelPluginRegistry {
            plugins: vec![Box::new(GreetingPlugin)],
            started: std::collections::HashSet::new(),
        };
        let widgets = registry.widgets(&settings, &crate::niri::Snapshot::default());

        assert_eq!(
            widgets,
            [RegisteredPanelWidget {
                plugin_id: "greeting",
                widget: PanelWidget::Text {
                    text: "Привет из настроек".into(),
                    accessible_name: "Greeting".into(),
                },
            }]
        );
    }
}
