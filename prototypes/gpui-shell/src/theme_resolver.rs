//! Conservative bridge from the existing QML configuration/theme inputs to
//! the GPUI token contract. ii dark mode uses the source-backed Material
//! defaults from Appearance.qml when its generated palette is absent. A
//! generated palette is completed from those defaults only when metadata
//! explicitly confirms dark mode; other families and schemes remain source-only.

use crate::{
    config::ConfigLoad,
    theme::{
        ColorRoles, Family, MaterialColorTokens, MaterialThemeContext, OpacityTokens, Scheme,
        ShapeTokens, SpacingTokens, ThemeInput, ThemeTokens, TypographyTokens, parse_hex_color,
    },
};
use serde_json::Value;

/// IDs used by the three QML surface families. These are intentionally exact:
/// unknown values do not become new visual dialects by accident.
pub fn family_from_config(config: &ConfigLoad) -> Family {
    match config.config.panel_family.as_deref() {
        Some("waffle") => Family::Waffle,
        Some("iris") => Family::Island,
        // `ii` is the default in defaults/config.json (`panelFamily`), and the
        // fallback used by the existing GPUI host for malformed/unknown IDs.
        _ => Family::Ii,
    }
}

/// Resolve only source-backed values. `generated_palette` should be the parsed
/// object at `Directories.generatedMaterialThemePath` (colors.json), and
/// `generated_theme_meta` the parsed `Directories.generatedThemeMetaPath`
/// (theme-meta.json). With no generated palette, ii uses the current built-in
/// dark palette from `Appearance.qml`. An incomplete generated palette is
/// replaced by that palette only when metadata explicitly confirms dark mode;
/// light and non-ii palettes are never synthesized.
pub fn resolve(
    config: &ConfigLoad,
    family: Family,
    generated_palette: Option<&Value>,
    generated_theme_meta: Option<&Value>,
) -> ThemeTokens {
    let appearance = &config.config.appearance;
    let style_id = match family {
        Family::Island => exact_island_style(appearance.island_preset.as_deref()),
        Family::Ii | Family::Waffle => exact_global_style(appearance.global_style.as_deref()),
    };
    let theme_id = match family {
        Family::Island => appearance
            .source_sections
            .get("iris.themeId")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id))
            .unwrap_or("iris"),
        Family::Ii | Family::Waffle => appearance
            .theme
            .as_deref()
            .filter(|id| valid_id(id))
            .unwrap_or("auto"),
    }
    .to_owned();

    // Iris has an independent scheme and density. Never let its section change
    // the shared Material mode or geometry for ii and Waffle.
    let iris_appearance = (family == Family::Island)
        .then(|| appearance.source_sections.get("iris.appearance"))
        .flatten();
    let iris_scheme = iris_appearance
        .and_then(|value| value.get("scheme"))
        .and_then(Value::as_str);
    let generated_dark_mode = generated_theme_meta.and_then(palette_mode) == Some(true);
    let scheme = match iris_scheme {
        Some("dark") => Scheme::Dark,
        Some("ink") => Scheme::Ink,
        Some("light") => Scheme::Light,
        _ => generated_theme_meta
            .and_then(palette_mode)
            .map(|dark| if dark { Scheme::Dark } else { Scheme::Light })
            .unwrap_or(Scheme::Dark),
    };

    let mut typography = TypographyTokens::default();
    if let Some(source_typography) = appearance.source_sections.get("typography") {
        set_font_family(source_typography, "mainFont", &mut typography.main);
        set_font_family(source_typography, "titleFont", &mut typography.title);
        set_font_family(
            source_typography,
            "monospaceFont",
            &mut typography.monospace,
        );
        if let Some(scale) = source_typography
            .get("sizeScale")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
        {
            typography.size_scale = scale as f32;
        }
        if let Some(axes) = source_typography
            .get("variableAxes")
            .and_then(Value::as_object)
        {
            for (name, range) in [
                ("wght", 100.0..=1000.0),
                ("wdth", 25.0..=151.0),
                ("grad", -200.0..=150.0),
            ] {
                if let Some(value) = axes
                    .get(name)
                    .and_then(Value::as_f64)
                    .filter(|value| value.is_finite() && range.contains(value))
                {
                    typography
                        .variable_axes
                        .insert(name.to_owned(), value as f32);
                }
            }
        }
    }
    // These are the prototype's current geometry defaults. Resolve them once
    // so the base context and compatibility token fields stay identical.
    let shape = ShapeTokens {
        density: iris_appearance
            .and_then(|value| value.get("density"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .map(|value| (value as f32).clamp(0.8, 1.35))
            .unwrap_or(1.0),
        ..ShapeTokens::default()
    };
    let spacing = SpacingTokens::default();
    // QML applies the configured values only when transparency is enabled and
    // automatic mode is explicitly off. Missing configuration retains the
    // automatic default, whose wallpaper analysis remains unported.
    let opacity = if appearance.transparency_enabled == Some(true)
        && appearance.transparency_automatic == Some(false)
    {
        OpacityTokens {
            background_transparency: manual_transparency(appearance.background_transparency),
            content_transparency: manual_transparency(appearance.content_transparency),
            ..OpacityTokens::default()
        }
    } else {
        OpacityTokens::default()
    };

    // The QML generator writes a complete Material palette. A sparse object
    // usually means the file is being replaced or is malformed, so mixing its
    // few values with unrelated component fallbacks creates an accidental
    // theme. For ii dark mode, accept generated values only when its core
    // background/surface/accent pairs are all valid and theme-meta explicitly
    // says dark; then fill optional roles from Appearance.qml. A complete
    // palette without a confirmed mode remains source-only, while an
    // incomplete one is ignored as a unit. If no palette exists, the current
    // prototype's dark default remains the source-backed baseline. Other
    // families/schemes have no ported fallback palette.
    let resolved_palette = generated_palette.map(|palette| palette.clone());
    let resolved_palette = match (family, scheme, resolved_palette) {
        (Family::Ii, Scheme::Dark, Some(palette)) if generated_dark_mode => {
            if has_material_core(&palette) {
                Some(merge_dark_material_defaults(palette))
            } else {
                Some(dark_material_defaults())
            }
        }
        (Family::Ii, Scheme::Dark, Some(palette)) => has_material_core(&palette).then_some(palette),
        (Family::Ii, Scheme::Dark, None) => Some(dark_material_defaults()),
        (_, _, palette) => palette,
    };

    ThemeTokens::resolve(ThemeInput {
        family,
        scheme,
        style_id: style_id.to_owned(),
        theme_id,
        material: MaterialThemeContext {
            colors: resolved_palette
                .as_ref()
                .map(material_color_tokens)
                .unwrap_or_default(),
            typography: typography.clone(),
            shape,
            spacing,
            overrides: ColorRoles::default(),
        },
        colors: resolved_palette
            .as_ref()
            .map(color_roles)
            .unwrap_or_default(),
        typography,
        // Geometry defaults remain the existing GPUI scaffold until the QML
        // family-specific geometry resolver is ported; no colors are implied.
        shape,
        spacing,
        opacity,
    })
}

fn manual_transparency(value: Option<f64>) -> f32 {
    value
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0) as f32
}

fn set_font_family(source: &Value, key: &str, target: &mut String) {
    if let Some(value) = source
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        *target = value.to_owned();
    }
}

fn has_material_core(palette: &Value) -> bool {
    [
        "background",
        "on_background",
        "surface",
        "on_surface",
        "primary",
        "on_primary",
    ]
    .into_iter()
    .all(|role| material_color(palette, role).is_some())
}

fn merge_dark_material_defaults(mut palette: Value) -> Value {
    let defaults = dark_material_defaults();
    let Some(target) = palette.as_object_mut() else {
        return defaults;
    };
    let Some(defaults) = defaults.as_object() else {
        return palette;
    };

    for (name, value) in defaults {
        let camel = material_camel_name(name);
        let has_valid_value = [name.as_str(), camel.as_str()].into_iter().any(|key| {
            target
                .get(key)
                .and_then(Value::as_str)
                .and_then(parse_hex_color)
                .is_some()
        });
        if !has_valid_value {
            target.insert(name.clone(), value.clone());
        }
    }
    palette
}

fn dark_material_defaults() -> Value {
    // Exact baseline values from modules/common/Appearance.qml's m3colors
    // object. Keep this fallback confined to ii's dark Material base; generated
    // wallpaper palettes, Waffle, and Iris retain their independent schemes.
    serde_json::Value::Object(
        [
            ("background", "#06070b"),
            ("on_background", "#E3E6F0"),
            ("surface", "#06070b"),
            ("surface_dim", "#05060a"),
            ("surface_bright", "#141825"),
            ("surface_container_lowest", "#040508"),
            ("surface_container_low", "#0B0E16"),
            ("surface_container", "#111522"),
            ("surface_container_high", "#161B28"),
            ("surface_container_highest", "#1B2233"),
            ("on_surface", "#E3E6F0"),
            ("surface_variant", "#3D455A"),
            ("on_surface_variant", "#C3CAD9"),
            ("inverse_surface", "#E3E6F0"),
            ("inverse_on_surface", "#151822"),
            ("outline", "#707894"),
            ("outline_variant", "#3D455A"),
            ("shadow", "#000000"),
            ("scrim", "#000000"),
            ("surface_tint", "#1F6CFF"),
            ("primary", "#1F6CFF"),
            ("on_primary", "#0A1020"),
            ("primary_container", "#12244A"),
            ("on_primary_container", "#C7D4FF"),
            ("inverse_primary", "#8CACFF"),
            ("secondary", "#9AA5C0"),
            ("on_secondary", "#151925"),
            ("secondary_container", "#242C40"),
            ("on_secondary_container", "#D6DDF0"),
            ("tertiary", "#d1c3c6"),
            ("on_tertiary", "#372e30"),
            ("tertiary_container", "#31292b"),
            ("on_tertiary_container", "#c1b4b7"),
            ("error", "#ffb4ab"),
            ("on_error", "#690005"),
            ("error_container", "#93000a"),
            ("on_error_container", "#ffdad6"),
            ("primary_fixed", "#e7e0e7"),
            ("primary_fixed_dim", "#cbc4cb"),
            ("on_primary_fixed", "#1d1b1f"),
            ("on_primary_fixed_variant", "#49454b"),
            ("secondary_fixed", "#e6e1e4"),
            ("secondary_fixed_dim", "#cac5c8"),
            ("on_secondary_fixed", "#1d1b1d"),
            ("on_secondary_fixed_variant", "#484648"),
            ("tertiary_fixed", "#eddfe1"),
            ("tertiary_fixed_dim", "#d1c3c6"),
            ("on_tertiary_fixed", "#211a1c"),
            ("on_tertiary_fixed_variant", "#4e4447"),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), Value::String(value.to_owned())))
        .collect(),
    )
}

fn material_camel_name(snake_name: &str) -> String {
    let mut camel = String::from("m3");
    let mut uppercase_next = false;
    for character in snake_name.chars() {
        if character == '_' {
            uppercase_next = true;
        } else if uppercase_next {
            camel.extend(character.to_uppercase());
            uppercase_next = false;
        } else {
            camel.push(character);
        }
    }
    camel
}

/// Resolve the complete generated Material 3 palette through a stable typed
/// surface. Both matugen's snake_case names and the QML `m3camelCase` names
/// emitted by older theme generators are accepted.
fn material_color_tokens(palette: &Value) -> MaterialColorTokens {
    MaterialColorTokens {
        primary: material_color(palette, "primary"),
        on_primary: material_color(palette, "on_primary"),
        primary_container: material_color(palette, "primary_container"),
        on_primary_container: material_color(palette, "on_primary_container"),
        primary_fixed: material_color(palette, "primary_fixed"),
        primary_fixed_dim: material_color(palette, "primary_fixed_dim"),
        on_primary_fixed: material_color(palette, "on_primary_fixed"),
        on_primary_fixed_variant: material_color(palette, "on_primary_fixed_variant"),
        secondary: material_color(palette, "secondary"),
        on_secondary: material_color(palette, "on_secondary"),
        secondary_container: material_color(palette, "secondary_container"),
        on_secondary_container: material_color(palette, "on_secondary_container"),
        secondary_fixed: material_color(palette, "secondary_fixed"),
        secondary_fixed_dim: material_color(palette, "secondary_fixed_dim"),
        on_secondary_fixed: material_color(palette, "on_secondary_fixed"),
        on_secondary_fixed_variant: material_color(palette, "on_secondary_fixed_variant"),
        tertiary: material_color(palette, "tertiary"),
        on_tertiary: material_color(palette, "on_tertiary"),
        tertiary_container: material_color(palette, "tertiary_container"),
        on_tertiary_container: material_color(palette, "on_tertiary_container"),
        tertiary_fixed: material_color(palette, "tertiary_fixed"),
        tertiary_fixed_dim: material_color(palette, "tertiary_fixed_dim"),
        on_tertiary_fixed: material_color(palette, "on_tertiary_fixed"),
        on_tertiary_fixed_variant: material_color(palette, "on_tertiary_fixed_variant"),
        error: material_color(palette, "error"),
        on_error: material_color(palette, "on_error"),
        error_container: material_color(palette, "error_container"),
        on_error_container: material_color(palette, "on_error_container"),
        background: material_color(palette, "background"),
        on_background: material_color(palette, "on_background"),
        surface: material_color(palette, "surface"),
        on_surface: material_color(palette, "on_surface"),
        surface_variant: material_color(palette, "surface_variant"),
        on_surface_variant: material_color(palette, "on_surface_variant"),
        surface_dim: material_color(palette, "surface_dim"),
        surface_bright: material_color(palette, "surface_bright"),
        surface_container_lowest: material_color(palette, "surface_container_lowest"),
        surface_container_low: material_color(palette, "surface_container_low"),
        surface_container: material_color(palette, "surface_container"),
        surface_container_high: material_color(palette, "surface_container_high"),
        surface_container_highest: material_color(palette, "surface_container_highest"),
        outline: material_color(palette, "outline"),
        outline_variant: material_color(palette, "outline_variant"),
        inverse_surface: material_color(palette, "inverse_surface"),
        inverse_on_surface: material_color(palette, "inverse_on_surface"),
        inverse_primary: material_color(palette, "inverse_primary"),
        surface_tint: material_color(palette, "surface_tint"),
        scrim: material_color(palette, "scrim"),
        shadow: material_color(palette, "shadow"),
    }
}

fn material_color(palette: &Value, snake_name: &str) -> Option<gpui::Hsla> {
    let camel = material_camel_name(snake_name);
    [snake_name, camel.as_str()].into_iter().find_map(|key| {
        palette
            .get(key)
            .and_then(Value::as_str)
            .and_then(parse_hex_color)
    })
}

/// Use the family selected by exact `panelFamily` matching.
pub fn resolve_for_config(
    config: &ConfigLoad,
    generated_palette: Option<&Value>,
    generated_theme_meta: Option<&Value>,
) -> ThemeTokens {
    resolve(
        config,
        family_from_config(config),
        generated_palette,
        generated_theme_meta,
    )
}

/// Extract semantic roles represented in `defaults/matugen/templates/colors.json`
/// and in `ThemePresets.qml`'s generated Material palette. No role is synthesized.
pub fn color_roles(palette: &Value) -> ColorRoles {
    let mut roles = ColorRoles::default();
    roles.background = color(palette, &["background", "m3background"]);
    roles.surface = color(palette, &["surface", "m3surface"]);
    roles.surface_elevated = color(
        palette,
        &["surface_container_high", "m3surfaceContainerHigh"],
    );
    roles.foreground = color(palette, &["on_surface", "m3onSurface"]);
    roles.foreground_muted = color(palette, &["on_surface_variant", "m3onSurfaceVariant"]);
    roles.border = color(palette, &["outline_variant", "m3outlineVariant"]);
    roles.accent = color(palette, &["primary", "m3primary"]);
    roles.accent_foreground = color(palette, &["on_primary", "m3onPrimary"]);
    roles.selection = color(palette, &["primary_container", "m3primaryContainer"]);
    roles.danger = color(palette, &["error", "m3error"]);
    roles.warning = color(palette, &["tertiary", "m3tertiary"]);
    roles
}

fn color(palette: &Value, keys: &[&str]) -> Option<gpui::Hsla> {
    keys.iter().find_map(|key| {
        palette
            .get(*key)
            .and_then(Value::as_str)
            .and_then(parse_hex_color)
    })
}

fn palette_mode(palette: &Value) -> Option<bool> {
    // ThemePresets.qml::buildThemeMeta writes `mode` to the separate generated
    // theme-meta.json file; colors.json itself intentionally has no mode key.
    palette
        .get("mode")
        .and_then(Value::as_str)
        .and_then(|mode| match mode {
            "dark" => Some(true),
            "light" => Some(false),
            _ => None,
        })
        .or_else(|| palette.get("darkmode").and_then(Value::as_bool))
}

fn exact_global_style(value: Option<&str>) -> &'static str {
    // Anchors: Appearance.qml globalStyle and its per-style predicates. The
    // Config.qml comment mentions legacy `cards`, but Appearance.qml does not
    // implement it as a global surface dialect, so it is not accepted here.
    match value {
        Some("inir") => "inir",
        Some("aurora") => "aurora",
        Some("angel") => "angel",
        Some("regalia") => "regalia",
        Some("zzz") => "zzz",
        Some("cookie") => "cookie",
        Some("editorial") => "editorial",
        // Appearance.qml:60 default.
        _ => "material",
    }
}

fn exact_island_style(value: Option<&str>) -> &'static str {
    // Anchor: IrisStyle.qml presets map (`iris`, `soft`, `crisp`, `round`,
    // `angular`, `contrast`).
    match value {
        Some("soft") => "soft",
        Some("crisp") => "crisp",
        Some("round") => "round",
        Some("angular") => "angular",
        Some("contrast") => "contrast",
        _ => "iris",
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_ii_dark_palette_uses_source_backed_material_defaults() {
        let resolved = resolve_for_config(&ConfigLoad::from_defaults(), None, None);
        assert!(resolved.colors.accent.is_some());
        assert!(resolved.colors.background.is_some());
        assert!(resolved.material.colors.surface_container.is_some());
    }

    #[test]
    fn accepts_only_exact_family_style_ids() {
        assert_eq!(exact_global_style(Some("angel")), "angel");
        assert_eq!(exact_global_style(Some("Angel")), "material");
        assert_eq!(exact_island_style(Some("contrast")), "contrast");
        assert_eq!(exact_island_style(Some("contrast ")), "iris");
    }

    #[test]
    fn generated_colors_are_parsed_without_deriving_missing_roles() {
        let value = serde_json::json!({"primary": "#123456", "surface": "#ffffff"});
        let colors = color_roles(&value);
        assert!(colors.accent.is_some());
        assert!(colors.surface.is_some());
        assert!(colors.foreground.is_none());
    }
}
