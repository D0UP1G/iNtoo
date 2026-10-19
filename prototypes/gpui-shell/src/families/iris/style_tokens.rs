//! Source-backed IrisStyle palette values.
//!
//! Preset numbers and literal colours below are copied from IrisStyle.qml.
//! Wallpaper sampling, Lume contrast solving, user accent selection and glass
//! rendering need runtime services and are represented only when the resolver
//! supplies the resulting role. `preview()` is deliberately separate.

use crate::theme::{Family, Scheme, ThemeTokens, parse_hex_color};
use gpui::Hsla;

#[derive(Debug, Clone, Copy)]
pub struct IrisPresetTokens {
    pub fill: f32,
    pub shape: f32,
    pub text_strong: f32,
    pub text_secondary: f32,
    pub text_tertiary: f32,
    pub subtext: Hsla,
    pub muted: Hsla,
    pub hairline: Hsla,
    pub hairline_strong: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub struct IrisPalette {
    pub scheme: Scheme,
    pub preset: IrisPresetTokens,
    pub material: Hsla,
    pub material_high: Hsla,
    pub material_highest: Hsla,
    pub text: Hsla,
    pub accent: Hsla,
    pub fill: Hsla,
    pub fill_active: Hsla,
    pub hairline: Hsla,
}

impl IrisPalette {
    /// Resolve the source-backed subset available without wallpaper/Lume
    /// sampling. The unprovided values follow IrisStyle's literal defaults.
    pub fn from_theme(theme: &ThemeTokens) -> Option<Self> {
        if theme.family != Family::Island {
            return None;
        }
        let preset = preset(&theme.style_id)?;
        let (material, high, highest, text_hex) = match theme.scheme {
            Scheme::Dark => ("#000000", "#1c1c1e", "#2c2c2e", "#f5f5f7"),
            Scheme::Light => ("#ffffff", "#f2f2f7", "#e5e5ea", "#1d1d1f"),
            Scheme::Ink => ("#dcd7ca", "#cec9bc", "#c2bdb0", "#26231f"),
        };
        let material = parse_hex_color(material)?;
        let material_high = parse_hex_color(high)?;
        let material_highest = parse_hex_color(highest)?;
        let text = parse_hex_color(text_hex)?;
        // IrisStyle's default accent name is blue and identityChroma is 1 / .9
        // / .75 in dark / paper / ink respectively.
        let mut accent = parse_hex_color("#0a84ff")?;
        accent.s *= match theme.scheme {
            Scheme::Dark => 1.0,
            Scheme::Light => 0.9,
            Scheme::Ink => 0.75,
        };

        // IrisStyle.fillAlpha(level) = min(.5, level * preset.fill * tweak(fill));
        // tweak defaults to 1.0. Dark fillInk is the exact IrisStyle.text literal.
        let fill_ink = text;
        let fill = with_alpha(fill_ink, (0.12 * preset.fill).min(0.5));
        let fill_active = with_alpha(fill_ink, (0.26 * preset.fill).min(0.5));
        Some(Self {
            scheme: theme.scheme,
            preset,
            material,
            material_high,
            material_highest,
            text,
            accent,
            fill,
            fill_active,
            hairline: rule_line(preset.hairline, text, material),
        })
    }

    /// Neutral values for visual previews only. Production views should use
    /// `from_theme` and keep unresolved dynamic roles explicit.
    pub fn preview() -> Self {
        let color = |hex| parse_hex_color(hex).expect("valid preview color literal");
        Self {
            scheme: Scheme::Dark,
            preset: preset("iris").expect("built-in Iris preset"),
            material: color("#202126"),
            material_high: color("#32343b"),
            material_highest: color("#4b455d"),
            text: color("#f5f3f8"),
            accent: color("#c4a7ff"),
            fill: color("#32343b"),
            fill_active: color("#4b455d"),
            hairline: color("#77717f"),
        }
    }
}

fn preset(name: &str) -> Option<IrisPresetTokens> {
    let (fill, shape, strong, secondary, tertiary, subtext, muted, hairline, hairline_strong) =
        match name {
            "iris" => (
                1.0, 1.0, 0.82, 0.62, 0.40, "#aeaeb2", "#8e8e93", "#262628", "#48484a",
            ),
            "soft" => (
                0.78, 1.22, 0.78, 0.58, 0.36, "#a1a1a6", "#838388", "#1f1f21", "#3a3a3c",
            ),
            "crisp" => (
                1.12, 0.7, 0.86, 0.66, 0.42, "#b4b4b9", "#929297", "#2c2c2e", "#545456",
            ),
            "round" => (
                0.9, 1.5, 0.8, 0.6, 0.38, "#a8a8ad", "#8a8a8f", "#222224", "#404042",
            ),
            "angular" => (
                1.08, 0.42, 0.86, 0.64, 0.42, "#b0b0b5", "#909095", "#2a2a2c", "#505052",
            ),
            "contrast" => (
                1.6, 1.0, 0.94, 0.8, 0.6, "#d1d1d6", "#aeaeb2", "#3a3a3c", "#6c6c70",
            ),
            _ => return None,
        };
    Some(IrisPresetTokens {
        fill,
        shape,
        text_strong: strong,
        text_secondary: secondary,
        text_tertiary: tertiary,
        subtext: parse_hex_color(subtext)?,
        muted: parse_hex_color(muted)?,
        hairline: parse_hex_color(hairline)?,
        hairline_strong: parse_hex_color(hairline_strong)?,
    })
}

fn with_alpha(mut color: Hsla, alpha: f32) -> Hsla {
    color.a = alpha;
    color
}

/// IrisStyle.ruleOf + line at default tweak (1): mix foreground with the
/// opaque material using the source preset colour's mean brightness.
fn rule_line(reference: Hsla, foreground: Hsla, material: Hsla) -> Hsla {
    let reference_rgb = hsla_to_rgb(reference);
    let foreground_rgb = hsla_to_rgb(foreground);
    let material_rgb = hsla_to_rgb(material);
    let k = ((reference_rgb[0] + reference_rgb[1] + reference_rgb[2]) / 3.0 / 0.96).clamp(0.0, 1.0);
    let mixed =
        std::array::from_fn(|index| foreground_rgb[index] * (1.0 - k) + material_rgb[index] * k);
    rgb_to_hsla(mixed)
}

fn hsla_to_rgb(color: Hsla) -> [f32; 3] {
    let h = color.h;
    let s = color.s;
    let l = color.l;
    if s == 0.0 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    [
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    ]
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

fn rgb_to_hsla(rgb: [f32; 3]) -> Hsla {
    let (r, g, b) = (rgb[0], rgb[1], rgb[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    let (h, s) = if delta == 0.0 {
        (0.0, 0.0)
    } else {
        let s = delta / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / delta + 2.0) / 6.0
        } else {
            ((r - g) / delta + 4.0) / 6.0
        };
        (h, s)
    };
    Hsla { h, s, l, a: 1.0 }
}
