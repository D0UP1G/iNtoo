//! Isolated GPUI presentation slice for the Iris Dock.
//!
//! The source Dock derives its entries from TaskbarApps/IrisDockOrder and its
//! geometry from IrisFrame/IrisStyle. This renderer accepts those resolved
//! values and never invents an application list or touches global state.

use std::sync::Arc;

use gpui::{
    Context, Hsla, IntoElement, KeyDownEvent, Render, Role, SharedString, Window, div, prelude::*,
    px,
};

use crate::theme::ThemeTokens;
use crate::{
    families::waffle::taskbar::start_menu_adapter::desktop_entries::XdgDesktopEntrySource, niri,
};

#[path = "style_tokens.rs"]
mod style_tokens;
use style_tokens::IrisPalette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IrisDockEdge {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

impl IrisDockEdge {
    fn vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Debug, Clone)]
pub struct IrisDockApp {
    /// Stable desktop/app id from TaskbarApps.
    pub id: String,
    /// Resolved display name from the desktop entry; may be absent while data loads.
    pub name: Option<String>,
    /// A glyph/icon fallback supplied by the host's icon resolver.
    pub glyph: Option<String>,
    pub running: bool,
    pub focused: bool,
    pub urgent: bool,
    pub badge_count: u32,
}

/// Derive Dock task records from the real Niri window list and XDG desktop
/// catalog. Apps without an `app_id` are omitted because no stable task key is
/// available; app names/icons are resolved from desktop metadata when present.
pub fn apps_from_windows(
    windows: &[niri::Window],
    catalog: &XdgDesktopEntrySource,
) -> Vec<IrisDockApp> {
    let mut apps = std::collections::BTreeMap::<String, IrisDockApp>::new();
    for window in windows {
        let Some(app_id) = window
            .app_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let key = app_id.to_lowercase();
        let name = catalog.resolve_app(app_id).map(|(name, _icon_name)| name);
        let app = apps.entry(key).or_insert_with(|| IrisDockApp {
            id: app_id.to_owned(),
            name,
            // Dock's current renderer accepts text glyphs, not icon-theme
            // names or image handles; avoid drawing a misleading fake icon.
            glyph: None,
            running: true,
            focused: false,
            urgent: false,
            badge_count: 0,
        });
        app.focused |= window.focused;
        app.urgent |= window.urgent;
    }
    apps.into_values().collect()
}

#[derive(Debug, Clone)]
pub struct IrisDockPiece {
    pub id: String,
    pub label: String,
    pub glyph: String,
    pub active: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct IrisDockGeometry {
    /// Available width/height of the host's output-local content area.
    pub width: f32,
    pub height: f32,
    pub edge: IrisDockEdge,
    /// Density-scaled size equivalent to IrisDock's QML `iconSize` property.
    pub icon_size: f32,
    pub notch: bool,
}

#[derive(Debug, Clone)]
pub struct IrisDockStyle {
    pub body: Hsla,
    pub icon_plate: Hsla,
    pub icon_plate_hover: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub outline: Hsla,
    pub radius: f32,
    pub density: f32,
}

impl IrisDockStyle {
    /// Resolve the static theme roles that are available without wallpaper
    /// sampling or compositor glass. Material effects remain host-owned.
    pub fn from_theme(theme: &ThemeTokens, material_alpha: f32) -> Option<Self> {
        let palette = IrisPalette::from_theme(theme)?;
        let density = theme.shape.density.clamp(0.8, 1.35);
        let mut body = palette.material_high;
        body.a = material_alpha.clamp(0.45, 1.0);
        Some(Self {
            body,
            icon_plate: palette.fill,
            icon_plate_hover: palette.fill_active,
            text: palette.text,
            muted: palette.preset.subtext,
            accent: palette.accent,
            outline: palette.hairline,
            radius: 18.0,
            density,
        })
    }
}

#[derive(Clone)]
pub struct IrisDockInput {
    pub geometry: IrisDockGeometry,
    pub style: IrisDockStyle,
    pub show_launcher: bool,
    pub apps: Vec<IrisDockApp>,
    pub pieces: Vec<IrisDockPiece>,
    pub on_launch: Option<Arc<dyn Fn() + Send + Sync>>,
    pub on_activate: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_piece_activate: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

/// Updateable GPUI entity wrapper intended for one output-local Dock surface.
/// The host owns entity lifetime, output selection, reveal state and shell
/// services; `set_input` replaces all resolved presentation data atomically.
pub struct IrisDockView {
    input: IrisDockInput,
}

impl IrisDockView {
    pub fn new(input: IrisDockInput) -> Self {
        Self { input }
    }

    pub fn set_input(&mut self, input: IrisDockInput, cx: &mut Context<Self>) {
        self.input = input;
        cx.notify();
    }

    pub fn input(&self) -> &IrisDockInput {
        &self.input
    }
}

impl Render for IrisDockView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        render(self.input.clone())
    }
}

/// Render a compact app dock. Launcher and app ids are forwarded to the host;
/// ordering, notifications, focus/window cycling, pinning and launch semantics
/// remain adapters because QML delegates each to separate services.
pub fn render(input: IrisDockInput) -> impl IntoElement {
    let style = input.style.clone();
    let geometry = input.geometry;
    let scale = style.density;
    let vertical = geometry.edge.vertical();
    let icon_size = geometry.icon_size.clamp(28.0, 64.0) * scale;
    let slot = icon_size + 10.0 * scale;
    let padding = 10.0 * scale;
    let spacing = 2.0 * scale;
    let radius = style.radius.min((icon_size + 18.0 * scale) / 2.0);

    let mut cells: Vec<gpui::AnyElement> = Vec::new();
    if input.show_launcher {
        let activate = input
            .on_launch
            .clone()
            .map(|launch| Arc::new(move || launch()) as Arc<dyn Fn() + Send + Sync>);
        cells.push(
            dock_cell(
                "iris.dock.launcher",
                "Applications",
                "▦",
                icon_size,
                slot,
                &style,
                false,
                false,
                false,
                false,
                0,
                activate,
            )
            .into_any_element(),
        );
    }
    for app in &input.apps {
        let id = app.id.clone();
        let label = app.name.clone().unwrap_or_else(|| app.id.clone());
        let glyph = app.glyph.clone().unwrap_or_else(|| "•".to_owned());
        let callback = input.on_activate.clone();
        let accessible_label = app_accessible_label(&label, app);
        let activate = callback.map(|callback| {
            let id = id.clone();
            Arc::new(move || callback(id.clone())) as Arc<dyn Fn() + Send + Sync>
        });
        cells.push(
            dock_cell(
                &format!("iris.dock.app.{id}"),
                &accessible_label,
                &glyph,
                icon_size,
                slot,
                &style,
                app.running,
                app.focused,
                app.urgent,
                false,
                app.badge_count,
                activate,
            )
            .into_any_element(),
        );
    }
    // A separator only appears when both app entries and edge pieces exist,
    // matching IrisDock's source grouping rule.
    if !input.apps.is_empty() && !input.pieces.is_empty() {
        cells.push(separator(vertical, icon_size, &style).into_any_element());
    }
    for piece in &input.pieces {
        let id = piece.id.clone();
        let accessible_label = if piece.active {
            format!("{}, active", piece.label)
        } else {
            piece.label.clone()
        };
        let callback = input.on_piece_activate.clone();
        let activate = callback.map(|callback| {
            let id = id.clone();
            Arc::new(move || callback(id.clone())) as Arc<dyn Fn() + Send + Sync>
        });
        cells.push(
            dock_cell(
                &format!("iris.dock.piece.{}", piece.id),
                &accessible_label,
                &piece.glyph,
                icon_size * 0.92,
                slot,
                &style,
                false,
                false,
                false,
                piece.active,
                0,
                activate,
            )
            .into_any_element(),
        );
    }

    let row = div()
        .id("iris.dock")
        .flex()
        .when(vertical, |this| this.flex_col())
        .when(!vertical, |this| this.flex_row())
        .items_center()
        .justify_center()
        .gap(px(spacing))
        .px(px(padding))
        .py(px(padding))
        .rounded(px(radius))
        .bg(style.body)
        .border_1()
        .border_color(style.outline)
        .children(cells);

    // Fill the output-local surface and derive the dock's edge alignment from
    // the typed geometry. Host controls its reveal/autohide visibility.
    div()
        .w(px(geometry.width.max(1.0)))
        .h(px(geometry.height.max(1.0)))
        .flex()
        .when(vertical, |this| {
            this.flex_row()
                .items_center()
                .when(geometry.edge == IrisDockEdge::Left, |this| {
                    this.justify_start()
                })
                .when(geometry.edge == IrisDockEdge::Right, |this| {
                    this.justify_end()
                })
                .when(!geometry.notch, |this| this.px(px(8.0 * scale)))
        })
        .when(!vertical, |this| {
            this.flex_col()
                .items_center()
                .when(geometry.edge == IrisDockEdge::Top, |this| {
                    this.justify_start()
                })
                .when(geometry.edge == IrisDockEdge::Bottom, |this| {
                    this.justify_end()
                })
                .when(!geometry.notch, |this| this.py(px(8.0 * scale)))
        })
        .child(row)
}

fn dock_cell(
    id: &str,
    label: &str,
    glyph: &str,
    icon_size: f32,
    slot: f32,
    style: &IrisDockStyle,
    running: bool,
    focused: bool,
    urgent: bool,
    active: bool,
    badge_count: u32,
    activate: Option<Arc<dyn Fn() + Send + Sync>>,
) -> impl IntoElement {
    let background = if focused || urgent || active {
        style.icon_plate_hover
    } else {
        style.icon_plate
    };
    let marker = if urgent { style.accent } else { style.muted };
    div()
        .id(id.to_owned())
        .aria_label(SharedString::from(label))
        .when_some(activate, |this, activate| {
            this.role(Role::Button)
                .focusable()
                .tab_stop(true)
                .on_click({
                    let activate = activate.clone();
                    move |_, _, _| activate()
                })
                .on_key_down(move |event: &KeyDownEvent, _, _| {
                    if is_activate_key(event) {
                        activate();
                    }
                })
        })
        .w(px(slot))
        .h(px(slot))
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .rounded(px((icon_size + 6.0) / 2.0))
        .bg(background)
        .text_color(style.text)
        .child(
            div()
                .text_size(px(icon_size.min(30.0)))
                .child(glyph.to_owned()),
        )
        .when(running, |this| {
            this.child(
                div()
                    .absolute()
                    .bottom(px(2.0))
                    .w(px(4.0))
                    .h(px(3.0))
                    .rounded_full()
                    .bg(if focused { style.accent } else { marker }),
            )
        })
        .when(badge_count > 0, |this| {
            this.child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .right(px(0.0))
                    .min_w(px(15.0))
                    .h(px(15.0))
                    .px(px(3.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(style.accent)
                    .text_color(style.text)
                    .text_xs()
                    .child(badge_count.to_string()),
            )
        })
}

fn is_activate_key(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held
}

fn app_accessible_label(name: &str, app: &IrisDockApp) -> String {
    let mut parts = vec![name.to_owned()];
    if app.running {
        parts.push("running".to_owned());
    }
    if app.focused {
        parts.push("focused".to_owned());
    }
    if app.urgent {
        parts.push("urgent".to_owned());
    }
    if app.badge_count > 0 {
        parts.push(format!("{} notifications", app.badge_count));
    }
    parts.join(", ")
}

fn separator(vertical: bool, icon_size: f32, style: &IrisDockStyle) -> impl IntoElement {
    let mark = div()
        .rounded_full()
        .bg(style.muted)
        .when(vertical, |this| this.w(px(icon_size * 0.56)).h(px(1.0)))
        .when(!vertical, |this| this.w(px(1.0)).h(px(icon_size * 0.56)));
    div()
        .w(px(if vertical {
            icon_size + 10.0
        } else {
            13.0 * style.density
        }))
        .h(px(if vertical {
            13.0 * style.density
        } else {
            icon_size + 10.0
        }))
        .flex()
        .items_center()
        .justify_center()
        .child(mark)
}
