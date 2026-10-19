//! Data-driven II sidebar drawer presentation.
//!
//! The component only renders supplied state. Adapters own configuration,
//! profile data, services, section loading, and persistence.

use std::sync::Arc;

use gpui::{
    AnyElement, App, ClickEvent, FontWeight, Hsla, IntoElement, KeyDownEvent, SharedString, Window,
    div, prelude::*, px,
};

use super::context_weather::{ContextWeatherInput, render_context_weather};
use super::controls_card::{ControlsCardInput, render_controls_card};
use super::crypto_widget::{CryptoWidgetInput, render_crypto_widget};
use super::media_player::{MediaPlayerInput, render_media_player};
use super::notification_panel::{NotificationPanelInput, render_notification_panel};
use super::quick_launch::{QuickLaunchInput, render_quick_launch};
use super::quick_note::QuickNoteCard;
use super::quick_sliders::{QuickSlidersInput, render_quick_sliders};
use super::quick_toggles::{QuickTogglesInput, render_quick_toggles};
use super::status_rings::{StatusRingsInput, render_status_rings};
use super::wallpaper_gallery::{WallpaperGalleryInput, render_wallpaper_gallery};
use super::week_strip::{WeekStripInput, render_week_strip};
use super::world_clock::{WorldClockInput, render_world_clock};

pub type Action = Arc<dyn Fn() + Send + Sync>;
pub type IdAction = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerState {
    Closed,
    Open,
    Expanded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentState {
    Ready,
    Loading,
    Empty,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSizeMode {
    Full,
    Content,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarHeaderStyle {
    Profile,
    Classic,
}

#[derive(Debug, Clone)]
pub struct SidebarStyle {
    pub surface: Hsla,
    pub card: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub border: Hsla,
    pub radius: f32,
    pub card_radius: f32,
    pub padding: f32,
    pub gap: f32,
    /// Width chosen by the output/layout adapter; zero keeps intrinsic width.
    pub width: f32,
    /// Resolved viewport/content height, as in SidebarHost size modes.
    pub height: f32,
    pub compact: bool,
    pub header_style: SidebarHeaderStyle,
    pub size_mode: SidebarSizeMode,
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub identity: String,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SidebarItem {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub glyph: String,
    pub selected: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct SidebarSection {
    pub id: String,
    pub title: String,
    pub state: ContentState,
    pub message: Option<String>,
    pub items: Vec<SidebarItem>,
    /// Specialized notification presentation; other sections leave this unset.
    pub notification_panel: Option<NotificationPanelInput>,
    /// Specialized quick slider presentation; other sections leave this unset.
    pub quick_sliders: Option<QuickSlidersInput>,
    /// Specialized quick toggle presentation; other sections leave this unset.
    pub quick_toggles: Option<QuickTogglesInput>,
    /// Specialized media card; other sections leave this unset.
    pub media_player: Option<MediaPlayerInput>,
    /// Resource and battery rings; populated only when their adapters exist.
    pub status_rings: Option<StatusRingsInput>,
    /// Configured quick launch shortcuts; no defaults are synthesized here.
    pub quick_launch: Option<QuickLaunchInput>,
    /// Stateful editor for the persisted current QuickNote tab.
    pub quick_note: Option<gpui::Entity<QuickNoteCard>>,
    /// Locale-derived date strip; date values come from the system adapter.
    pub week_strip: Option<WeekStripInput>,
    /// Timezone rows supplied by the system-backed clock adapter.
    pub world_clock: Option<WorldClockInput>,
    /// Read-only wallpaper previews from the local directory adapter.
    pub wallpaper_gallery: Option<WallpaperGalleryInput>,
    /// Disabled ControlsCard glyphs while matching providers/actions are absent.
    pub controls_card: Option<ControlsCardInput>,
    /// Current weather supplied by the weather adapter.
    pub context_weather: Option<ContextWeatherInput>,
    /// Configured cryptocurrency quotes supplied by the market adapter.
    pub crypto_widget: Option<CryptoWidgetInput>,
}

#[derive(Clone)]
pub struct SidebarInput {
    pub side: DrawerSide,
    pub drawer: DrawerState,
    pub title: String,
    pub profile: Option<Profile>,
    pub navigation: Vec<SidebarItem>,
    pub actions: Vec<SidebarItem>,
    pub sections: Vec<SidebarSection>,
    pub style: SidebarStyle,
    pub on_close: Action,
    pub on_expand: Action,
    pub on_navigation: IdAction,
    pub on_action: IdAction,
    pub on_section_item: IdAction,
    pub on_retry: IdAction,
}

/// QML `SidebarHost` presentation facts resolved by the runtime adapter.
/// `enabled_screen` must already apply `SidebarLeft/Right.targetScreens`
/// semantics (configured list, falling back to all outputs when no configured
/// name matches). The UI deliberately does not query global state itself.
#[derive(Debug, Clone)]
pub struct SidebarHostState {
    pub screen_name: String,
    pub role_open: bool,
    pub other_role_open: bool,
    pub role_hold_open: bool,
    pub role_target_output: String,
    pub editor_active: bool,
    pub screen_locked: bool,
    pub fullscreen_covered: bool,
    pub enabled_screen: bool,
    /// `SidebarHost` applies expansion only to the feature/left semantic role.
    pub feature_role: bool,
    pub role_expanded: bool,
}

/// Dashboard cards are supplied by the adapter. The component never fabricates
/// widget values; unavailable/loading/empty states are explicit per card.
#[derive(Debug, Clone)]
pub struct DashboardCard {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub glyph: String,
    pub state: ContentState,
    pub message: Option<String>,
    pub lines: Vec<String>,
}

#[derive(Clone)]
pub struct DashboardInput {
    pub title: Option<String>,
    pub columns: [Vec<DashboardCard>; 3],
    pub style: SidebarStyle,
}

/// Three-column dashboard composition matching DashboardContent's left,
/// center, and right user-defined zones. Empty columns naturally collapse.
pub fn render_dashboard(input: DashboardInput) -> AnyElement {
    let style = input.style;
    let columns = input
        .columns
        .into_iter()
        .enumerate()
        .filter(|(_, cards)| !cards.is_empty())
        .map(|(column, cards)| {
            div()
                .id(SharedString::from(format!("ii.dashboard.column.{column}")))
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(style.gap))
                .children(cards.into_iter().map(move |card| {
                    let id = card.id.clone();
                    let title = card.title.clone();
                    let subtitle = card.subtitle.clone();
                    let lines = card.lines.clone();
                    let glyph = card.glyph.clone();
                    let state = card.state;
                    let message = card.message.clone().unwrap_or_else(|| match state {
                        ContentState::Ready => String::new(),
                        ContentState::Loading => "Loading…".into(),
                        ContentState::Empty => "No items".into(),
                        ContentState::Error => "Could not load".into(),
                    });
                    div()
                        .id(SharedString::from(format!("ii.dashboard.card.{id}")))
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(style.gap))
                        .p(px(style.padding))
                        .bg(style.card)
                        .rounded(px(style.card_radius))
                        .text_color(style.foreground)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(style.gap))
                                .child(div().text_color(style.accent).child(glyph))
                                .child(
                                    div()
                                        .flex_1()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(title),
                                )
                                .when_some(subtitle, |this, text| {
                                    this.child(div().text_sm().text_color(style.muted).child(text))
                                }),
                        )
                        .when(state == ContentState::Ready, |this| {
                            this.children(
                                lines.into_iter().map(|line| {
                                    div().text_sm().text_color(style.muted).child(line)
                                }),
                            )
                        })
                        .when(state != ContentState::Ready, |this| {
                            this.child(div().text_sm().text_color(style.muted).child(message))
                        })
                }))
        });
    div()
        .id("ii.dashboard")
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .gap(px(style.gap))
        .p(px(style.padding))
        .bg(style.surface)
        .when_some(input.title, |this, title| {
            this.child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(style.foreground)
                    .child(title),
            )
        })
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .gap(px(style.gap))
                .children(columns),
        )
        .into_any_element()
}

impl SidebarHostState {
    /// Mirrors the QML output-scoped open/editor gate and native-surface
    /// visibility guards. GPUI currently has no QML exit animation state, so
    /// the fullscreen gate has no animation exception.
    pub fn should_present(&self) -> bool {
        let role_open_here = self.role_open && self.role_target_output == self.screen_name;
        let presentation_open = role_open_here || (self.editor_active && !role_open_here);
        self.enabled_screen
            && !self.screen_locked
            && (!self.fullscreen_covered || presentation_open)
            && presentation_open
    }
}

/// Host adapter boundary: applies per-output lifecycle gates, then delegates
/// all visible content and callbacks to the injected component contract.
pub fn render_host(host: &SidebarHostState, mut input: SidebarInput) -> impl IntoElement {
    render_host_with_header(host, input, None)
}

pub fn render_host_with_header(
    host: &SidebarHostState,
    mut input: SidebarInput,
    header: Option<AnyElement>,
) -> impl IntoElement {
    let present = host.should_present();
    let left = input.side == DrawerSide::Left;
    if present {
        input.drawer = if host.feature_role && host.role_expanded {
            DrawerState::Expanded
        } else {
            DrawerState::Open
        };
    }
    let dismiss = input.on_close.clone();
    let can_dismiss =
        host.role_open && !host.editor_active && !host.role_hold_open && !host.other_role_open;
    div()
        .id("ii.sidebar.host")
        .size_full()
        .when(present, |this| {
            this.flex()
                .when(!left, |this| this.justify_end())
                .when(can_dismiss, |this| this.on_click(move |_, _, _| dismiss()))
                .child(render_with_header(input, header))
        })
}

/// Closed drawers remain externally controlled and render no content. Open
/// and expanded use the same content model; the adapter supplies resolved
/// dimensions because the QML host owns width/height and presentation motion.
pub fn render(input: SidebarInput) -> AnyElement {
    render_with_header(input, None)
}

pub fn render_with_header(input: SidebarInput, custom_header: Option<AnyElement>) -> AnyElement {
    if input.drawer == DrawerState::Closed {
        return div().size_full().into_any_element();
    }
    let style = input.style.clone();
    let title = input.title.clone();
    let close = input.on_close.clone();
    let expand = input.on_expand.clone();
    let left = input.side == DrawerSide::Left;
    let navigation = input
        .navigation
        .iter()
        .map(|item| render_item(item, &style, input.on_navigation.clone()));
    let actions = input
        .actions
        .iter()
        .map(|item| render_item(item, &style, input.on_action.clone()));
    let mut header = custom_header.or_else(|| {
        input
            .profile
            .as_ref()
            .map(|p| render_profile(p, &style).into_any_element())
    });
    let left_header = if left { header.take() } else { None };
    let sections = input
        .sections
        .into_iter()
        .filter(|section| {
            section.media_player.as_ref().is_none_or(|player| {
                player.media.as_ref().is_some_and(|media| {
                    media.status != crate::components::vertical_bar::MediaStatus::NoPlayer
                })
            })
        })
        .map(|section| {
            if section.id == "system-header" {
                header.take().unwrap_or_else(|| div().into_any_element())
            } else {
                render_section(
                    &section,
                    &style,
                    input.on_section_item.clone(),
                    input.on_retry.clone(),
                )
                .into_any_element()
            }
        });
    let side_name = match input.side {
        DrawerSide::Left => "feature",
        DrawerSide::Right => "system",
    };
    let close_click = close.clone();
    let close_key = close.clone();
    let expand_click = expand.clone();
    let expand_key = expand.clone();
    div()
        .id(SharedString::from(format!("ii.sidebar.{side_name}")))
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .when(style.width > 0.0, |this| this.w(px(style.width)))
        .when(style.height > 0.0, |this| this.h(px(style.height)))
        .gap(px(style.gap))
        .p(px(style.padding))
        .bg(style.surface)
        .text_color(style.foreground)
        .rounded(px(style.radius))
        .when(!left, |this| this.aria_label("System sidebar"))
        .when(left, |this| {
            this.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(action_button(
                        "close",
                        "Close sidebar",
                        "×",
                        &style,
                        move |_, _, _| close_click(),
                        move |e, _, _| activate(e, &close_key),
                    ))
                    .when(left, |this| {
                        this.child(action_button(
                            "expand",
                            "Expand sidebar",
                            "↗",
                            &style,
                            move |_, _, _| expand_click(),
                            move |e, _, _| activate(e, &expand_key),
                        ))
                    }),
            )
            .when_some(left_header, |this, header| this.child(header))
        })
        .on_click(|_, _, cx| cx.stop_propagation())
        .children(navigation)
        .children(actions)
        .children(sections)
        .into_any_element()
}

fn render_profile(profile: &Profile, style: &SidebarStyle) -> impl IntoElement {
    let name = profile.name.clone();
    let identity = profile.identity.clone();
    let status = profile.status.clone();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(style.gap))
        .p(px(style.padding))
        .bg(style.card)
        .rounded(px(style.card_radius))
        .child(
            div()
                .size(px(44.))
                .rounded(px(22.))
                .bg(style.selected)
                .flex()
                .items_center()
                .justify_center()
                .text_color(style.accent)
                .child(
                    profile
                        .avatar
                        .as_ref()
                        .map(|_| "●".to_owned())
                        .unwrap_or_else(|| initials(&name)),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .text_color(style.foreground)
                        .when(style.header_style == SidebarHeaderStyle::Profile, |this| {
                            this.child(name.clone())
                        })
                        .when(style.header_style == SidebarHeaderStyle::Classic, |this| {
                            this.child(name)
                        }),
                )
                .when(style.header_style == SidebarHeaderStyle::Profile, |this| {
                    this.child(div().text_sm().text_color(style.muted).child(identity))
                })
                .children(status.map(|s| div().text_xs().text_color(style.muted).child(s))),
        )
}

fn render_item(item: &SidebarItem, style: &SidebarStyle, action: IdAction) -> impl IntoElement {
    let id = item.id.clone();
    let key_id = id.clone();
    let click_action = action.clone();
    let keyboard_action = action;
    let enabled = item.enabled;
    let bg = if item.selected {
        style.selected
    } else {
        style.surface
    };
    let hover = style.hover;
    let border = style.border;
    let label = item.label.clone();
    let detail = item.detail.clone();
    let glyph = item.glyph.clone();
    div()
        .id(SharedString::from(format!("ii.sidebar.item.{id}")))
        .w_full()
        .min_h(px(if style.compact { 34. } else { 42. }))
        .px(px(style.padding))
        .gap(px(style.gap))
        .flex()
        .items_center()
        .bg(bg)
        .rounded(px(style.card_radius))
        .text_color(if enabled {
            style.foreground
        } else {
            style.muted
        })
        .aria_label(SharedString::from(label.clone()))
        .focusable()
        .tab_stop(enabled)
        .focus_visible(move |s| s.border_1().border_color(border))
        .hover(move |s| s.bg(hover))
        .when(enabled, |this| {
            this.on_click(move |_, _, _| click_action(id.clone()))
        })
        .on_key_down(move |e: &KeyDownEvent, _, _| {
            if enabled && is_activate(e) {
                keyboard_action(key_id.clone());
            }
        })
        .child(div().w(px(22.)).child(glyph))
        .child(div().flex_1().child(label))
        .children(detail.map(|d| div().text_sm().text_color(style.muted).child(d)))
}

fn render_section(
    section: &SidebarSection,
    style: &SidebarStyle,
    item_action: IdAction,
    retry: IdAction,
) -> AnyElement {
    if let Some(player) = section.media_player.clone() {
        return render_media_player(player);
    }
    if let Some(rings) = section.status_rings.clone() {
        return render_status_rings(rings);
    }
    if let Some(launch) = section.quick_launch.clone() {
        return render_quick_launch(launch);
    }
    if let Some(note) = section.quick_note.clone() {
        return div()
            .id("ii.sidebar.section.quick-note")
            .w_full()
            .child(note)
            .into_any_element();
    }
    if let Some(week_strip) = section.week_strip.clone() {
        return render_week_strip(week_strip);
    }
    if let Some(world_clock) = section.world_clock.clone() {
        return render_world_clock(world_clock);
    }
    if let Some(context_weather) = section.context_weather.clone() {
        return render_context_weather(context_weather);
    }
    if let Some(wallpaper_gallery) = section.wallpaper_gallery.clone() {
        return render_wallpaper_gallery(wallpaper_gallery);
    }
    if let Some(controls_card) = section.controls_card.clone() {
        return render_controls_card(controls_card);
    }
    if let Some(crypto_widget) = section.crypto_widget.clone() {
        return render_crypto_widget(crypto_widget);
    }
    if let Some(panel) = section.notification_panel.clone() {
        return div()
            .id(SharedString::from(format!(
                "ii.sidebar.section.{}",
                section.id
            )))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(style.gap))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(style.muted)
                    .child(section.title.clone()),
            )
            .child(render_notification_panel(panel))
            .into_any_element();
    }
    if let Some(panel) = section.quick_sliders.clone() {
        return div()
            .id(SharedString::from(format!(
                "ii.sidebar.section.{}",
                section.id
            )))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(style.gap))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(style.muted)
                    .child(section.title.clone()),
            )
            .child(render_quick_sliders(panel))
            .into_any_element();
    }
    if let Some(panel) = section.quick_toggles.clone() {
        return div()
            .id(SharedString::from(format!(
                "ii.sidebar.section.{}",
                section.id
            )))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(style.gap))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(style.muted)
                    .child(section.title.clone()),
            )
            .child(render_quick_toggles(panel))
            .into_any_element();
    }
    let title = section.title.clone();
    let state = section.state;
    let message = section.message.clone();
    let id = section.id.clone();
    let retry_id = id.clone();
    let items = section
        .items
        .iter()
        .map(|item| render_item(item, style, item_action.clone()));
    let fallback = match state {
        ContentState::Ready => None,
        ContentState::Loading => Some(message.unwrap_or_else(|| "Loading…".into())),
        ContentState::Empty => Some(message.unwrap_or_else(|| "No items".into())),
        ContentState::Error => {
            Some(message.unwrap_or_else(|| "Could not load this section".into()))
        }
    };
    let retry_button = if state == ContentState::Error {
        let retry_key = retry.clone();
        let retry_click = retry.clone();
        let retry_id_click = retry_id.clone();
        Some(action_button(
            format!("retry.{id}"),
            "Retry loading section",
            "Retry",
            style,
            move |_, _, _| retry_click(retry_id_click.clone()),
            move |e, _, _| {
                if is_activate(e) {
                    retry_key(retry_id.clone())
                }
            },
        ))
    } else {
        None
    };
    div()
        .id(SharedString::from(format!(
            "ii.sidebar.section.{}",
            section.id
        )))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(style.gap))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(style.muted)
                .child(title),
        )
        .when_some(fallback, |this, message| {
            this.child(
                div()
                    .p(px(style.padding))
                    .text_sm()
                    .text_color(style.muted)
                    .child(message),
            )
        })
        .children(retry_button)
        .children(items)
        .into_any_element()
}

fn action_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    text: impl Into<SharedString>,
    style: &SidebarStyle,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    key: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id = id.into();
    let label = label.into();
    let text = text.into();
    let hover = style.hover;
    let border = style.border;
    div()
        .id(id)
        .min_h(px(36.))
        .min_w(px(36.))
        .px(px(8.))
        .rounded(px(style.card_radius))
        .flex()
        .items_center()
        .justify_center()
        .bg(style.card)
        .text_color(style.foreground)
        .aria_label(label)
        .focusable()
        .tab_stop(true)
        .focus_visible(move |s| s.border_1().border_color(border))
        .hover(move |s| s.bg(hover))
        .on_click(click)
        .on_key_down(key)
        .child(text)
}

fn is_activate(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held
}

fn activate(event: &KeyDownEvent, action: &Action) {
    if is_activate(event) {
        action();
    }
}

fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}
