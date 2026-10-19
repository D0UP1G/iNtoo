//! Theme token contracts for the GPUI prototype.
//!
//! This module deliberately models resolved tokens separately from their source.
//! The production QML theme generator (Material You, custom palettes, wallpaper
//! sampling and family overrides) has not been ported yet, so callers must feed
//! resolved colors/configuration into `ThemeInput` rather than assuming parity.

use gpui::Hsla;
use std::collections::BTreeMap;

/// Existing visual families. Island is called `iris` in its QML implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Family {
    #[default]
    Ii,
    Waffle,
    Island,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheme {
    Light,
    /// Iris paper/ink scheme: warm material with dark ink, distinct from its
    /// regular light scheme in IrisStyle.qml.
    Ink,
    #[default]
    Dark,
}

/// Semantic palette. Values are GPUI HSLA colors; missing roles stay absent so
/// the view/component layer can choose a deliberate fallback.
#[derive(Debug, Clone, Default)]
pub struct ColorRoles {
    pub background: Option<Hsla>,
    pub surface: Option<Hsla>,
    pub surface_elevated: Option<Hsla>,
    pub foreground: Option<Hsla>,
    pub foreground_muted: Option<Hsla>,
    pub border: Option<Hsla>,
    pub accent: Option<Hsla>,
    pub accent_foreground: Option<Hsla>,
    pub selection: Option<Hsla>,
    pub danger: Option<Hsla>,
    pub warning: Option<Hsla>,
    /// Preserve unported Material 3 / Waffle / Iris roles without flattening
    /// their semantic names into this shared starter set.
    pub extra: BTreeMap<String, Hsla>,
}

/// Typed Material 3 palette shared by all environment plugins. Values remain
/// optional because the source generator may not have produced colors.json yet.
#[derive(Debug, Clone, Default)]
pub struct MaterialColorTokens {
    pub primary: Option<Hsla>,
    pub on_primary: Option<Hsla>,
    pub primary_container: Option<Hsla>,
    pub on_primary_container: Option<Hsla>,
    pub primary_fixed: Option<Hsla>,
    pub primary_fixed_dim: Option<Hsla>,
    pub on_primary_fixed: Option<Hsla>,
    pub on_primary_fixed_variant: Option<Hsla>,
    pub secondary: Option<Hsla>,
    pub on_secondary: Option<Hsla>,
    pub secondary_container: Option<Hsla>,
    pub on_secondary_container: Option<Hsla>,
    pub secondary_fixed: Option<Hsla>,
    pub secondary_fixed_dim: Option<Hsla>,
    pub on_secondary_fixed: Option<Hsla>,
    pub on_secondary_fixed_variant: Option<Hsla>,
    pub tertiary: Option<Hsla>,
    pub on_tertiary: Option<Hsla>,
    pub tertiary_container: Option<Hsla>,
    pub on_tertiary_container: Option<Hsla>,
    pub tertiary_fixed: Option<Hsla>,
    pub tertiary_fixed_dim: Option<Hsla>,
    pub on_tertiary_fixed: Option<Hsla>,
    pub on_tertiary_fixed_variant: Option<Hsla>,
    pub error: Option<Hsla>,
    pub on_error: Option<Hsla>,
    pub error_container: Option<Hsla>,
    pub on_error_container: Option<Hsla>,
    pub background: Option<Hsla>,
    pub on_background: Option<Hsla>,
    pub surface: Option<Hsla>,
    pub on_surface: Option<Hsla>,
    pub surface_variant: Option<Hsla>,
    pub on_surface_variant: Option<Hsla>,
    pub surface_dim: Option<Hsla>,
    pub surface_bright: Option<Hsla>,
    pub surface_container_lowest: Option<Hsla>,
    pub surface_container_low: Option<Hsla>,
    pub surface_container: Option<Hsla>,
    pub surface_container_high: Option<Hsla>,
    pub surface_container_highest: Option<Hsla>,
    pub outline: Option<Hsla>,
    pub outline_variant: Option<Hsla>,
    pub inverse_surface: Option<Hsla>,
    pub inverse_on_surface: Option<Hsla>,
    pub inverse_primary: Option<Hsla>,
    pub surface_tint: Option<Hsla>,
    pub scrim: Option<Hsla>,
    pub shadow: Option<Hsla>,
}

/// Shared base plus an intentionally empty-by-default plugin overlay slot.
/// Plugins can specialize roles without forking the base token contract.
#[derive(Debug, Clone, Default)]
pub struct MaterialThemeContext {
    pub colors: MaterialColorTokens,
    pub typography: TypographyTokens,
    pub shape: ShapeTokens,
    pub spacing: SpacingTokens,
    pub overrides: ColorRoles,
}

#[derive(Debug, Clone)]
pub struct TypographyTokens {
    pub main: String,
    pub title: String,
    pub monospace: String,
    pub size_scale: f32,
    pub variable_axes: BTreeMap<String, f32>,
}

impl Default for TypographyTokens {
    fn default() -> Self {
        // Confirmed defaults in modules/common/Config.qml.
        Self {
            main: "Roboto Flex".into(),
            title: "Gabarito".into(),
            monospace: "JetBrainsMono Nerd Font".into(),
            size_scale: 1.0,
            variable_axes: [
                ("wght".into(), 300.0),
                ("wdth".into(), 105.0),
                ("grad".into(), 150.0),
            ]
            .into(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ShapeTokens {
    /// Density-adjusted radius values are logical pixels.
    pub small: f32,
    pub medium: f32,
    pub large: f32,
    pub pill: f32,
    pub density: f32,
}

impl Default for ShapeTokens {
    fn default() -> Self {
        // Neutral scaffold only; family-specific source values are populated
        // by a future config adapter (Waffle and Iris differ substantially).
        Self {
            small: 4.0,
            medium: 8.0,
            large: 12.0,
            pill: 9999.0,
            density: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SpacingTokens {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
}

impl Default for SpacingTokens {
    fn default() -> Self {
        Self {
            xs: 4.0,
            sm: 8.0,
            md: 12.0,
            lg: 16.0,
            xl: 24.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OpacityTokens {
    /// Fractions indicate amount of transparency, matching QML config naming.
    pub background_transparency: f32,
    pub content_transparency: f32,
    pub effects_enabled: bool,
    pub animations_enabled: bool,
}

impl Default for OpacityTokens {
    fn default() -> Self {
        Self {
            background_transparency: 0.0,
            content_transparency: 0.0,
            effects_enabled: true,
            animations_enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThemeTokens {
    pub family: Family,
    pub scheme: Scheme,
    /// Existing appearance.globalStyle (for ii/Waffle) or Iris preset id.
    pub style_id: String,
    /// Existing appearance.theme or iris.appearance.themeId when supplied.
    pub theme_id: String,
    /// Common Material foundation supplied to ii, Waffle, and Iris plugins.
    pub material: MaterialThemeContext,
    pub colors: ColorRoles,
    pub typography: TypographyTokens,
    pub shape: ShapeTokens,
    pub spacing: SpacingTokens,
    pub opacity: OpacityTokens,
}

/// Input boundary for a future Config/ThemePresets adapter. `colors` should be
/// produced by the family-aware theme resolver; this struct does not fabricate
/// generated Material You or wallpaper colors.
#[derive(Debug, Clone)]
pub struct ThemeInput {
    pub family: Family,
    pub scheme: Scheme,
    pub style_id: String,
    pub theme_id: String,
    pub material: MaterialThemeContext,
    pub colors: ColorRoles,
    pub typography: TypographyTokens,
    pub shape: ShapeTokens,
    pub spacing: SpacingTokens,
    pub opacity: OpacityTokens,
}

impl ThemeTokens {
    pub fn resolve(input: ThemeInput) -> Self {
        Self {
            family: input.family,
            scheme: input.scheme,
            style_id: input.style_id,
            theme_id: input.theme_id,
            material: input.material,
            colors: input.colors,
            typography: input.typography,
            shape: input.shape,
            spacing: input.spacing,
            opacity: input.opacity,
        }
    }
}

/// Built-in appearance typography presets, matching `StylePresets.qml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypographyPreset {
    Default,
    Compact,
    Spacious,
    Sharp,
    Soft,
    Minimal,
}

impl TypographyPreset {
    pub fn id(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Compact => "compact",
            Self::Spacious => "spacious",
            Self::Sharp => "sharp",
            Self::Soft => "soft",
            Self::Minimal => "minimal",
        }
    }

    pub fn size_scale(self) -> f32 {
        match self {
            Self::Default | Self::Sharp | Self::Soft => 1.0,
            Self::Compact => 0.9,
            Self::Spacious => 1.15,
            Self::Minimal => 0.95,
        }
    }

    pub fn variable_axes(self) -> BTreeMap<String, f32> {
        let (wght, wdth, grad) = match self {
            Self::Default => (300.0, 105.0, 175.0),
            Self::Compact => (350.0, 100.0, 150.0),
            Self::Spacious => (280.0, 110.0, 175.0),
            Self::Sharp => (400.0, 100.0, 200.0),
            Self::Soft => (250.0, 108.0, 100.0),
            Self::Minimal => (300.0, 100.0, 150.0),
        };
        [
            ("wght".into(), wght),
            ("wdth".into(), wdth),
            ("grad".into(), grad),
        ]
        .into()
    }

    pub fn apply(self, typography: &mut TypographyTokens) {
        typography.size_scale = self.size_scale();
        typography.variable_axes = self.variable_axes();
    }
}

/// Parse #RRGGBB or #RRGGBBAA into GPUI's normalized HSLA representation.
pub fn parse_hex_color(value: &str) -> Option<Hsla> {
    let hex = value.strip_prefix('#')?;
    let bytes = hex.as_bytes();
    if bytes.len() != 6 && bytes.len() != 8 {
        return None;
    }
    let r = f32::from(parse_hex_pair(bytes, 0)?) / 255.0;
    let g = f32::from(parse_hex_pair(bytes, 2)?) / 255.0;
    let b = f32::from(parse_hex_pair(bytes, 4)?) / 255.0;
    let a = if bytes.len() == 8 {
        f32::from(parse_hex_pair(bytes, 6)?) / 255.0
    } else {
        1.0
    };
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    let s = if d == 0.0 {
        0.0
    } else {
        d / (1.0 - (2.0 * l - 1.0).abs())
    };
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    Some(Hsla { h, s, l, a })
}

fn parse_hex_pair(bytes: &[u8], offset: usize) -> Option<u8> {
    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    Some((nibble(*bytes.get(offset)?)? << 4) | nibble(*bytes.get(offset + 1)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_alpha() {
        let color = parse_hex_color("#ff000080").unwrap();
        assert!((color.h - 0.0).abs() < 0.001);
        assert!((color.s - 1.0).abs() < 0.001);
        assert!((color.l - 0.5).abs() < 0.001);
        assert!((color.a - 128.0 / 255.0).abs() < 0.001);
        assert!(parse_hex_color("red").is_none());
        assert!(parse_hex_color("#aéabc").is_none());
        assert!(parse_hex_color("#aéabcde").is_none());
    }

    #[test]
    fn preset_values_match_qml_contract() {
        assert_eq!(TypographyPreset::Compact.size_scale(), 0.9);
        assert_eq!(TypographyPreset::Spacious.variable_axes()["wdth"], 110.0);
        assert_eq!(TypographyPreset::Sharp.variable_axes()["wght"], 400.0);
    }
}
