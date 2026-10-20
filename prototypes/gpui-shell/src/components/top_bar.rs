//! Niri-backed ii-style top bar for the GPUI prototype.
//!
//! State is supplied by the compositor adapter. Missing services render as
//! empty states instead of fabricated desktop data.

use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    time::{Duration, Instant},
};

use crate::components::material;
use crate::components::media_controls_popup::{
    MediaControlsPopupColors, MediaControlsPopupProps, render_media_controls_popup,
};
use crate::components::vertical_bar::{BatteryState, MediaAction, MediaState};
use crate::config::ShellConfig;
use crate::niri;
use crate::resource_provider::{LinuxResourceProvider, ResourceSnapshot};
use crate::theme::ThemeTokens;
use gpui::popup::{PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupOptions};
use gpui::{
    Bounds, FocusHandle, FontWeight, InteractiveElement, KeyDownEvent, MouseButton, Role,
    ScrollAnchor, ScrollHandle, Size, StatefulInteractiveElement, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, point, prelude::*,
    px, rgba, white,
};

const SURFACE: u32 = 0x161b22ee;
const SURFACE_ACTIVE: u32 = 0x30363ddd;
const SUBTLE_TEXT: u32 = 0xb1bac4ff;

fn popup_id_matches<T: PartialEq>(current: Option<T>, closing: T) -> bool {
    current == Some(closing)
}

fn clear_popup_subscription_if_current<T: PartialEq, S>(
    current: Option<T>,
    closing: T,
    subscription: &mut Option<S>,
) -> bool {
    if popup_id_matches(current, closing) {
        subscription.take();
        true
    } else {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSlot {
    pub number: u8,
    pub occupied: bool,
    pub active: bool,
}

pub struct TopBar {
    display_id: gpui::DisplayId,
    active_window: Option<String>,
    workspaces: Vec<WorkspaceSlot>,
    clock: Option<String>,
    resources: ResourceSnapshot,
    battery: Option<BatteryState>,
    media_state: MediaState,
    media_actions: async_channel::Sender<MediaAction>,
    media_button_focus: FocusHandle,
    media_popup: Rc<RefCell<Option<gpui::WindowHandle<MediaControlsPopupView>>>>,
    media_popup_closed_subscription: Rc<RefCell<Option<gpui::Subscription>>>,
    media_popup_restore_focus: Rc<Cell<bool>>,
    status: Option<String>,
    config: ShellConfig,
    theme: ThemeTokens,
    mvp_settings: crate::mvp_settings::MvpSettings,
    plugins: crate::mvp_settings::PanelPluginRegistry,
    settings_open: bool,
    settings_status: Option<String>,
    settings_button_focus: FocusHandle,
    settings_dialog_focus: FocusHandle,
    settings_control_focuses: Vec<FocusHandle>,
    settings_scroll: ScrollHandle,
    settings_focus_subscriptions: Vec<gpui::Subscription>,
    wallpaper_editor:
        gpui::Entity<crate::families::waffle::taskbar::start_menu_search::StartMenuSearch>,
}

struct MediaControlsPopupView {
    props: MediaControlsPopupProps,
    did_focus: bool,
}

impl MediaControlsPopupView {
    fn set_media_state(&mut self, media: MediaState, cx: &mut Context<Self>) {
        self.props.media = Some(media);
        cx.notify();
    }
}

impl Render for MediaControlsPopupView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.did_focus {
            self.props.focus_handle.focus(window, cx);
            self.did_focus = true;
        }
        render_media_controls_popup(self.props.clone())
    }
}

impl TopBar {
    pub fn new(
        output_name: Option<String>,
        config: ShellConfig,
        theme: ThemeTokens,
        display_id: gpui::DisplayId,
        media_actions: async_channel::Sender<MediaAction>,
        media_state: MediaState,
        cx: &mut Context<Self>,
    ) -> Self {
        let mvp_settings = config.gpui_mvp.clone();
        let mut plugins = crate::mvp_settings::PanelPluginRegistry::builtins();
        plugins.start_enabled(&mvp_settings);
        let settings_control_focuses = (0..plugins.descriptors().len() + 9)
            .map(|_| cx.focus_handle())
            .collect();
        let settings_button_focus = cx.focus_handle();
        let settings_dialog_focus = cx.focus_handle();
        let settings_scroll = ScrollHandle::new();
        let wallpaper_query = mvp_settings.wallpaper.clone().unwrap_or_default();
        let wallpaper_editor = cx.new(|cx| {
            crate::families::waffle::taskbar::start_menu_search::StartMenuSearch::new_with_labels(
                wallpaper_query,
                "Absolute image path".into(),
                "Desktop wallpaper path".into(),
                std::sync::Arc::new(|_| {}),
                cx,
            )
        });
        let mut theme = theme;
        if mvp_settings.configured {
            apply_mvp_theme(&mut theme, &mvp_settings);
        }
        let view = Self {
            display_id,
            active_window: None,
            workspaces: Vec::new(),
            clock: None,
            resources: ResourceSnapshot::default(),
            battery: None,
            media_state,
            media_actions,
            media_button_focus: cx.focus_handle(),
            media_popup: Rc::default(),
            media_popup_closed_subscription: Rc::default(),
            media_popup_restore_focus: Rc::new(Cell::new(true)),
            status: Some("Connecting to Niri…".into()),
            config: config.clone(),
            theme: theme.clone(),
            mvp_settings,
            plugins,
            settings_open: false,
            settings_status: None,
            settings_button_focus,
            settings_dialog_focus,
            settings_control_focuses,
            settings_scroll,
            settings_focus_subscriptions: Vec::new(),
            wallpaper_editor,
        };

        // Keep configuration mistakes visible to the developer while avoiding
        // placeholder widgets that would imply an unimplemented service exists.
        for module in configured_modules(&view.config) {
            if !matches!(
                module.as_str(),
                "activeWindow" | "workspaces" | "clock" | "resources" | "battery" | "media"
            ) && view
                .config
                .bar
                .modules
                .get(&module)
                .copied()
                .unwrap_or(true)
            {
                eprintln!(
                    "GPUI ii bar: configured module `{module}` omitted (no GPUI component yet)"
                );
            }
        }

        let battery_low = config.battery_low.unwrap_or(20.0);
        cx.spawn(async move |this, cx| {
            let mut resource_provider = LinuxResourceProvider::default();
            let mut desktop_provider = crate::vertical_provider::VerticalProvider::new(battery_low);
            let resource_interval = resource_provider.recommended_interval();
            let mut last_resource_sample: Option<Instant> = None;
            let mut resources = ResourceSnapshot::default();
            loop {
                let output_name = output_name.clone();
                let status_output_name = output_name.clone();
                let state = cx
                    .background_executor()
                    .spawn(async move {
                        let per_monitor = config.bar.workspace.per_monitor.unwrap_or(true);
                        let mut snapshot = niri::snapshot(if per_monitor {
                            output_name.as_deref()
                        } else {
                            None
                        });
                        if !config.bar.workspace.dynamic_count.unwrap_or(true) {
                            let count = config.bar.workspace.shown.unwrap_or(10).clamp(1, 20) as u8;
                            snapshot.workspaces.resize_with(usize::from(count), || {
                                niri::Workspace {
                                    id: 0,
                                    number: 0,
                                    name: None,
                                    occupied: false,
                                    active: false,
                                }
                            });
                            for (index, slot) in snapshot.workspaces.iter_mut().enumerate() {
                                slot.number = (index + 1) as u8;
                            }
                        }
                        snapshot
                    })
                    .await;
                // Keep procfs sampling at the provider's supported interval; the
                // first CPU value remains unknown until a second sample arrives.
                let should_sample_resources =
                    last_resource_sample.is_none_or(|last| last.elapsed() >= resource_interval);
                let mut battery_sample = None;
                if should_sample_resources {
                    resources = resource_provider.sample();
                    last_resource_sample = Some(Instant::now());
                    // Keep a local battery snapshot for the horizontal bar even
                    // while a vertical status-rings provider owns the shared cache.
                    let battery = desktop_provider.sample().await;
                    battery_sample = Some(battery.clone());
                    if !crate::families::ii::panel_surface::has_recent_vertical_status_rings_source(
                        status_output_name.as_deref(),
                    ) {
                        crate::families::ii::panel_surface::cache_top_status_rings_snapshot_if_unowned(
                            status_output_name.as_deref(),
                            &resources,
                            battery,
                        );
                    }
                }
                let resource_snapshot = resources.clone();
                if this
                    .update(cx, |view, cx| {
                        view.workspaces = state
                            .workspaces
                            .into_iter()
                            .map(|workspace| WorkspaceSlot {
                                number: workspace.number,
                                occupied: workspace.occupied,
                                active: workspace.active,
                            })
                            .collect();
                        view.active_window = state.active_window;
                        view.clock = state.clock;
                        view.resources = resource_snapshot;
                        if let Some(battery) = battery_sample {
                            view.battery = battery;
                        }
                        view.status = state.error.map(|_| "Niri unavailable".into());
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(1000))
                    .await;
            }
        })
        .detach();

        view
    }

    /// Replace the current MPRIS snapshot supplied by the shared media provider.
    pub fn set_media_state(&mut self, state: MediaState, cx: &mut Context<Self>) {
        if self.media_state != state {
            self.media_state = state;
            let popup_handle = { self.media_popup.borrow().as_ref().copied() };
            if let Some(handle) = popup_handle {
                let media = self.media_state.clone();
                if handle
                    .update(cx, |view, _, cx| view.set_media_state(media, cx))
                    .is_err()
                {
                    let mut popup = self.media_popup.borrow_mut();
                    let current_window_id = popup.as_ref().map(|current| current.window_id());
                    let mut subscription = self.media_popup_closed_subscription.borrow_mut();
                    if clear_popup_subscription_if_current(
                        current_window_id,
                        handle.window_id(),
                        &mut subscription,
                    ) {
                        popup.take();
                    }
                }
            }
            cx.notify();
        }
    }

    /// Current MPRIS snapshot for the horizontal media renderer.
    pub fn media_state(&self) -> &MediaState {
        &self.media_state
    }

    /// Queue used by the horizontal media controls; actions are consumed by
    /// the single MprisProvider owned by `surface_manager`.
    pub fn media_action_sender(&self) -> async_channel::Sender<MediaAction> {
        self.media_actions.clone()
    }
}

impl Render for TopBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.settings_focus_subscriptions.is_empty() {
            let editor_focus_index = 1 + self.plugins.descriptors().len();
            let handles = self.settings_control_focuses.clone();
            let scroll = self.settings_scroll.clone();
            self.settings_focus_subscriptions = handles
                .into_iter()
                .enumerate()
                .map(|(index, handle)| {
                    if index == editor_focus_index {
                        let anchor = ScrollAnchor::for_handle(scroll.clone());
                        cx.on_focus_in(&handle, window, move |_, window, cx| {
                            anchor.scroll_to(window, cx);
                        })
                    } else {
                        let scroll = scroll.clone();
                        cx.on_focus_in(&handle, window, move |_, _, _| {
                            // The title is the first direct child of the sheet;
                            // control handles follow the remaining child order.
                            scroll.scroll_to_item(index + 1);
                        })
                    }
                })
                .collect();
        }
        let this = cx.entity();
        let active_window = self.active_window.clone();
        let status = self.status.clone();
        let clock = self.clock.clone();
        let resources = self.resources.clone();
        let battery = self.battery.clone();
        let show_background = self.config.bar.show_background.unwrap_or(true);
        let background = self
            .theme
            .colors
            .surface_elevated
            .or(self.theme.colors.surface)
            .unwrap_or_else(|| rgba(SURFACE).into());
        let foreground_color = self
            .theme
            .colors
            .foreground
            .unwrap_or_else(|| white().into());
        let active_surface = self
            .theme
            .material
            .colors
            .primary_container
            .or(self.theme.colors.selection)
            .unwrap_or_else(|| rgba(SURFACE_ACTIVE).into());
        let active_foreground = self
            .theme
            .material
            .colors
            .on_primary_container
            .or(self.theme.colors.accent_foreground)
            .unwrap_or_else(|| white().into());
        let hover_surface = self
            .theme
            .material
            .colors
            .surface_container_high
            .unwrap_or_else(|| rgba(0xffffff18).into());
        let pressed_surface = self
            .theme
            .material
            .colors
            .surface_container_highest
            .unwrap_or_else(|| rgba(0xffffff30).into());
        let focus_outline = self
            .theme
            .material
            .colors
            .outline
            .or(self.theme.colors.accent)
            .unwrap_or(foreground_color);
        let occupied_marker = self
            .theme
            .colors
            .accent
            .unwrap_or_else(|| rgba(0x7aa2f7ff).into());
        let warning_color = self
            .theme
            .colors
            .warning
            .unwrap_or_else(|| rgba(0xd29922ff).into());
        let error_color = self
            .theme
            .material
            .colors
            .error
            .or(self.theme.colors.danger)
            .unwrap_or_else(|| rgba(0xf85149ff).into());
        let muted_foreground = self
            .theme
            .colors
            .foreground_muted
            .unwrap_or_else(|| rgba(SUBTLE_TEXT).into());
        let occupied_foreground = self
            .theme
            .material
            .colors
            .on_surface_variant
            .unwrap_or(foreground_color);
        let spacing = self.theme.material.spacing;
        let mut plugin_snapshot = crate::niri::Snapshot::default();
        plugin_snapshot.clock = clock.clone();
        plugin_snapshot.workspaces = self
            .workspaces
            .iter()
            .map(|slot| crate::niri::Workspace {
                id: 0,
                number: slot.number,
                name: None,
                occupied: slot.occupied,
                active: slot.active,
            })
            .collect();
        let plugin_widgets = self.plugins.widgets(&self.mvp_settings, &plugin_snapshot);
        let mut zones: [Vec<gpui::AnyElement>; 5] = std::array::from_fn(|_| Vec::new());
        let mut panel_layout = layout_zones(&self.config);
        if self.mvp_settings.configured {
            for (_, modules) in &mut panel_layout {
                modules.retain(|id| id != "clock" && id != "workspaces");
            }
            panel_layout[2]
                .1
                .extend(self.mvp_settings.panel_modules.iter().cloned());
        }
        for (zone_index, configured) in panel_layout {
            for module in configured {
                if !self
                    .config
                    .bar
                    .modules
                    .get(&module)
                    .copied()
                    .unwrap_or(true)
                {
                    continue;
                }
                if let Some(registered) = plugin_widgets
                    .iter()
                    .find(|registered| registered.plugin_id == module)
                {
                    match &registered.widget {
                        crate::mvp_settings::PanelWidget::Text {
                            text,
                            accessible_name,
                        } => {
                            zones[zone_index].push(
                                div()
                                    .id(format!("panel-plugin-{}", registered.plugin_id))
                                    .min_w_0()
                                    .max_w(px(260.))
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .aria_label(accessible_name.clone())
                                    .child(text.clone())
                                    .into_any_element(),
                            );
                        }
                        crate::mvp_settings::PanelWidget::Workspaces(workspaces) => {
                            if !workspaces.is_empty() {
                                let mut workspace_buttons = Vec::new();
                                for slot in workspaces {
                                    let label = slot.number.to_string();
                                    let workspace_number = slot.number;
                                    let background = if slot.active {
                                        active_surface
                                    } else {
                                        rgba(0x00000000).into()
                                    };
                                    let foreground = if slot.active {
                                        active_foreground
                                    } else if slot.occupied {
                                        occupied_foreground
                                    } else {
                                        muted_foreground
                                    };
                                    let workspace = div()
                                        .min_w(px(28.))
                                        .h(px(28.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(self.mvp_settings.corner_radius))
                                        .bg(background)
                                        .font_weight(if slot.active {
                                            FontWeight::BOLD
                                        } else if slot.occupied {
                                            FontWeight::MEDIUM
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .text_color(foreground)
                                        .id(format!(
                                            "panel-plugin-{}-workspace-{workspace_number}",
                                            registered.plugin_id
                                        ))
                                        .aria_label(workspace_accessible_name(
                                            slot.number,
                                            slot.occupied,
                                            slot.active,
                                        ))
                                        .role(Role::Button)
                                        .focusable()
                                        .tab_stop(true)
                                        .flex_shrink(0.)
                                        .focus_visible(move |style| {
                                            style.border_1().border_color(focus_outline)
                                        })
                                        .hover(move |style| style.bg(hover_surface))
                                        .active(move |style| style.bg(pressed_surface))
                                        .on_click(move |_, _, _| focus_workspace(workspace_number))
                                        .on_key_down(move |event, _, _| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) && !event.is_held
                                            {
                                                focus_workspace(workspace_number);
                                            }
                                        })
                                        .relative()
                                        .child(label)
                                        .when(slot.occupied && !slot.active, |button| {
                                            button.child(
                                                div()
                                                    .absolute()
                                                    .left_0()
                                                    .right_0()
                                                    .bottom(px(2.))
                                                    .flex()
                                                    .justify_center()
                                                    .child(
                                                        div()
                                                            .w(px(4.))
                                                            .h(px(4.))
                                                            .rounded_full()
                                                            .bg(occupied_marker),
                                                    ),
                                            )
                                        });
                                    let workspace = if slot.active {
                                        workspace.bg(active_surface)
                                    } else {
                                        workspace
                                    };
                                    workspace_buttons.push(
                                        material::focusable_button(
                                            workspace,
                                            &self.theme.material,
                                            active_surface,
                                        )
                                        .focus_visible(move |style| {
                                            style
                                                .bg(if slot.active {
                                                    active_surface
                                                } else {
                                                    active_surface.opacity(0.16)
                                                })
                                                .border_1()
                                                .border_color(focus_outline)
                                        })
                                        .into_any_element(),
                                    );
                                }
                                zones[zone_index].push(
                                    div()
                                        .id(format!(
                                            "panel-plugin-{}-workspaces",
                                            registered.plugin_id
                                        ))
                                        .role(Role::Group)
                                        .aria_label("Workspaces")
                                        .w_full()
                                        .min_w_0()
                                        .overflow_x_scroll()
                                        .flex()
                                        .items_center()
                                        .gap(px(spacing.xs))
                                        .children(workspace_buttons)
                                        .into_any_element(),
                                );
                            }
                        }
                    }
                    continue;
                }
                match module.as_str() {
                    "activeWindow" => {
                        if let Some(title) = active_window.as_ref() {
                            zones[zone_index].push(
                                div()
                                    .id("topbar-active-window-title")
                                    .min_w_0()
                                    .max_w(px(320.))
                                    .truncate()
                                    .text_color(foreground_color)
                                    .aria_label(title.clone())
                                    .child(title.clone())
                                    .into_any_element(),
                            );
                        }
                    }
                    "resources" => {
                        zones[zone_index].push(
                            resource_strip(
                                &resources,
                                &self.config,
                                muted_foreground,
                                warning_color,
                                error_color,
                                spacing,
                                px(260.),
                            )
                            .into_any_element(),
                        );
                    }
                    "battery" => {
                        // QML also hides this indicator for its shortened-form
                        // modes. That preference is not represented in parsed
                        // GPUI bar config yet, so this component follows module
                        // visibility and configured layout/order only.
                        if let Some(state) = battery.as_ref() {
                            let colors =
                                crate::components::horizontal_battery::HorizontalBatteryColors {
                                    backdrop: background,
                                    fill: self
                                        .theme
                                        .material
                                        .colors
                                        .on_background
                                        .or(self.theme.material.colors.on_surface)
                                        .unwrap_or(foreground_color),
                                    low_fill: self
                                        .theme
                                        .material
                                        .colors
                                        .error
                                        .or(self.theme.colors.danger)
                                        .unwrap_or(error_color),
                                    text: self
                                        .theme
                                        .material
                                        .colors
                                        .on_surface
                                        .unwrap_or(foreground_color),
                                    fill_text: self
                                        .theme
                                        .material
                                        .colors
                                        .background
                                        .or(self.theme.material.colors.inverse_on_surface)
                                        .unwrap_or(foreground_color),
                                    low_fill_text: self
                                        .theme
                                        .material
                                        .colors
                                        .on_error
                                        .or(self.theme.material.colors.background)
                                        .or(self.theme.material.colors.inverse_on_surface)
                                        .unwrap_or(foreground_color),
                                };
                            if let Some(element) =
                                crate::components::horizontal_battery::render_horizontal_battery(
                                    state, colors,
                                )
                            {
                                zones[zone_index].push(element);
                            }
                        }
                    }
                    "media" => {
                        // Match BarContent.qml's physical-output fallback: the
                        // indicator is hidden only in the tightest form.
                        let shortened_form =
                            shortened_form_for_width(window.bounds().size.width.as_f32());
                        if shortened_form < 2 {
                            let size_scale = self.theme.typography.size_scale;
                            let action_sender = self.media_actions.clone();
                            let on_action: Rc<dyn Fn(MediaAction)> = Rc::new(move |action| {
                                let _ = action_sender.try_send(action);
                            });
                            let popup_state = self.media_popup.clone();
                            let popup_closed_subscription =
                                self.media_popup_closed_subscription.clone();
                            let popup_restore_focus = self.media_popup_restore_focus.clone();
                            let popup_return_focus = self.media_button_focus.clone();
                            let popup_media = self.media_state.clone();
                            let popup_theme = self.theme.clone();
                            let popup_display = self.display_id;
                            let popup_bottom = self.config.bar.bottom.unwrap_or(false);
                            let popup_width = 380.0 * size_scale;
                            let popup_preferred_height = 190.0 * size_scale;
                            let popup_mode = self.config.bar.media_popup_mode;
                            let open_controls_action = self.media_actions.clone();
                            let popup_action_sender = self.media_actions.clone();
                            let on_open_controls: Rc<dyn Fn(&mut Window, &mut gpui::App, bool)> =
                                Rc::new(move |window, cx, grab| {
                                    if popup_mode == crate::config::MediaPopupMode::Dock {
                                        let _ = open_controls_action
                                            .try_send(MediaAction::OpenControls);
                                        return;
                                    }
                                    if let Some(handle) = popup_state.borrow_mut().take() {
                                        if handle
                                            .update(cx, |_, window, _| window.remove_window())
                                            .is_ok()
                                        {
                                            popup_closed_subscription.borrow_mut().take();
                                            let _ = open_controls_action
                                                .try_send(MediaAction::CloseControls);
                                            return;
                                        }
                                    }
                                    popup_restore_focus.set(true);

                                    let (anchor, gravity) = if popup_bottom {
                                        (PopupAnchor::Top, PopupGravity::Top)
                                    } else {
                                        (PopupAnchor::Bottom, PopupGravity::Bottom)
                                    };
                                    let popup_content_height =
                                        crate::components::media_controls_popup::card_stack_height(
                                            popup_width,
                                            popup_preferred_height,
                                            popup_media.players.len(),
                                        );
                                    let available_output_height = cx
                                        .displays()
                                        .into_iter()
                                        .find(|display| display.id() == popup_display)
                                        .map(|display| display.bounds().size.height.as_f32())
                                        .unwrap_or_else(|| window.bounds().size.height.as_f32());
                                    let popup_height =
                                        crate::components::media_controls_popup::bounded_popup_height(
                                            popup_content_height,
                                            available_output_height,
                                        );
                                    let options = WindowOptions {
                                        titlebar: None,
                                        display_id: Some(popup_display),
                                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                                            origin: point(px(0.), px(0.)),
                                            size: Size::new(px(popup_width), px(popup_height)),
                                        })),
                                        window_background: WindowBackgroundAppearance::Transparent,
                                        kind: WindowKind::AnchoredPopup(PopupOptions {
                                            parent: window.window_handle(),
                                            anchor_rect: window.bounds(),
                                            anchor,
                                            gravity,
                                            constraint_adjustment:
                                                PopupConstraintAdjustment::SLIDE_X
                                                    | PopupConstraintAdjustment::SLIDE_Y
                                                    | PopupConstraintAdjustment::FLIP_X
                                                    | PopupConstraintAdjustment::FLIP_Y,
                                            offset: point(px(0.), px(8. * size_scale)),
                                            grab,
                                        }),
                                        ..Default::default()
                                    };
                                    let colors = media_popup_colors(&popup_theme);
                                    let popup_window_id_cell = Rc::new(Cell::new(None));
                                    let close_window_id = popup_window_id_cell.clone();
                                    let close_popup_state = popup_state.clone();
                                    let close_action_sender = popup_action_sender.clone();
                                    let close: Rc<dyn Fn()> = Rc::new(move || {
                                        let Some(window_id) = close_window_id.get() else {
                                            return;
                                        };
                                        if close_popup_state.borrow().as_ref().is_some_and(
                                            |handle| {
                                                popup_id_matches(
                                                    Some(handle.window_id()),
                                                    window_id,
                                                )
                                            },
                                        ) {
                                            let _ = close_action_sender
                                                .try_send(MediaAction::CloseControls);
                                        }
                                    });
                                    let action_sender = popup_action_sender.clone();
                                    let props = MediaControlsPopupProps {
                                        media: Some(popup_media.clone()),
                                        colors,
                                        focus_handle: cx.focus_handle(),
                                        width_px: popup_width,
                                        height_px: popup_height,
                                        radius_px: 20. * size_scale,
                                        on_action: Some(Rc::new(move |action| {
                                            let _ = action_sender.try_send(action);
                                        })),
                                        on_close: Some(close),
                                    };
                                    match cx.open_window(options, move |_, cx| {
                                        cx.new(|_| MediaControlsPopupView {
                                            props,
                                            did_focus: false,
                                        })
                                    }) {
                                        Ok(handle) => {
                                            let popup_window_id = handle.window_id();
                                            popup_window_id_cell.set(Some(popup_window_id));
                                            let observed_popup_state = popup_state.clone();
                                            let observed_subscription =
                                                popup_closed_subscription.clone();
                                            let observed_restore_focus =
                                                popup_restore_focus.clone();
                                            let observed_parent_window = window.window_handle();
                                            let observed_focus = popup_return_focus.clone();
                                            let subscription =
                                                cx.on_window_closed(move |cx, closed_window_id| {
                                                    if closed_window_id == popup_window_id {
                                                        let is_current = observed_popup_state
                                                            .borrow()
                                                            .as_ref()
                                                            .is_some_and(|handle| {
                                                                popup_id_matches(
                                                                    Some(handle.window_id()),
                                                                    closed_window_id,
                                                                )
                                                            });
                                                        if is_current {
                                                            observed_popup_state
                                                                .borrow_mut()
                                                                .take();
                                                            observed_subscription
                                                                .borrow_mut()
                                                                .take();
                                                        }
                                                        if is_current
                                                            && observed_restore_focus.replace(true)
                                                        {
                                                            let _ = observed_parent_window.update(
                                                                cx,
                                                                |_, parent_window, cx| {
                                                                    observed_focus
                                                                        .focus(parent_window, cx);
                                                                },
                                                            );
                                                        }
                                                    }
                                                });
                                            *popup_state.borrow_mut() = Some(handle);
                                            *popup_closed_subscription.borrow_mut() =
                                                Some(subscription);
                                        }
                                        Err(error) => eprintln!(
                                            "Could not open horizontal media controls popup: {error:#}"
                                        ),
                                    }
                                });
                            let colors =
                                crate::components::horizontal_media::HorizontalMediaColors {
                                    foreground: foreground_color,
                                    muted: muted_foreground,
                                    accent: occupied_marker,
                                    track: muted_foreground.opacity(0.35),
                                    focus: focus_outline,
                                };
                            zones[zone_index].push(
                                crate::components::horizontal_media::render_horizontal_media(
                                    Some(&self.media_state),
                                    (self.config.bar.media_max_width as f32 * size_scale)
                                        .max(120.0 * size_scale),
                                    30.0,
                                    self.config.bar.verbose,
                                    size_scale,
                                    colors,
                                    self.media_button_focus.clone(),
                                    Some(on_action),
                                    Some(on_open_controls),
                                ),
                            );
                        }
                    }
                    "spacer" => {
                        let minimum_width = (self.config.bar.layout.spacer_width as f32
                            * self.theme.typography.size_scale.max(0.0))
                        .max(0.0);
                        let edge_zone = matches!(zone_index, 0 | 4);
                        let island_style =
                            self.config.bar.appearance_style.as_deref() == Some("islands");
                        let elastic = edge_zone
                            && !island_style
                            && self.config.bar.layout.spacer_mode
                                != crate::config::SpacerMode::Fixed;

                        // QML's auto and fill modes both use edge-zone slack;
                        // fixed mode and every center zone remain at minimum
                        // width. Island sections are content-sized, so their
                        // spacers also stay fixed like BarContent.qml.
                        if elastic || minimum_width > 0.0 {
                            let mut spacer = div()
                                .id(format!("topbar-spacer-{zone_index}"))
                                .h_full()
                                .min_w(px(minimum_width));
                            if elastic {
                                spacer = spacer.flex_1();
                            } else {
                                spacer = spacer.w(px(minimum_width));
                            }
                            zones[zone_index].push(spacer.into_any_element());
                        }
                    }
                    _ => {}
                }
            }
        }

        let [left, center_left, center, center_right, right] = zones;
        // BarContent.qml uses Qt.RightToLeft for the right edge RowLayout, so
        // its first configured module sits at the outer screen edge. GPUI lays
        // children left-to-right; reverse this edge's render order to match.
        let right = order_zone_children(4, right);
        let render_zone = |children: Vec<gpui::AnyElement>, alignment: u8| {
            let mut container = div().flex().items_center().gap(px(spacing.xs)).min_w_0();
            container = match alignment {
                1 => container.justify_end(),
                2 => container.justify_center(),
                _ => container,
            };
            container.children(children)
        };

        let settings_entity = this.clone();
        let launch_display = self.display_id;
        let launcher_button = material::action_button(
            div()
                .id("gpui-mvp-launcher-button")
                .role(Role::Button)
                .aria_label("Open app launcher")
                .focusable()
                .tab_stop(true)
                .on_click(move |_, _, _| {
                    crate::surface_manager::toggle_app_launcher(launch_display)
                })
                .on_key_down(move |event, _, _| {
                    if activation_pressed(event) {
                        crate::surface_manager::toggle_app_launcher(launch_display);
                    }
                })
                .child("Apps"),
            &self.theme.material,
            foreground_color,
            rgba(0xffffff22).into(),
            focus_outline,
            false,
        );
        let settings_keyboard_entity = this.clone();
        let settings_button = material::action_button(
            div()
                .id("gpui-mvp-settings-button")
                .role(Role::Button)
                .aria_label("Open shell settings")
                .aria_expanded(self.settings_open)
                .focusable()
                .tab_stop(true)
                .on_click(move |_, _, cx| {
                    let _ = settings_entity.update(cx, |view, cx| {
                        view.settings_open = !view.settings_open;
                        view.settings_status = None;
                        cx.notify();
                    });
                })
                .on_key_down(move |event, _, cx| {
                    if activation_pressed(event) {
                        let _ = settings_keyboard_entity.update(cx, |view, cx| {
                            view.settings_open = !view.settings_open;
                            view.settings_status = None;
                            cx.notify();
                        });
                    }
                })
                .track_focus(&self.settings_button_focus)
                .child("Settings"),
            &self.theme.material,
            foreground_color,
            rgba(0xffffff22).into(),
            focus_outline,
            self.settings_open,
        );

        let side_zone = |id: &'static str,
                         first: Vec<gpui::AnyElement>,
                         second: Vec<gpui::AnyElement>,
                         second_align: u8| {
            div()
                .id(id)
                .flex()
                .flex_1()
                .min_w_0()
                .overflow_x_scroll()
                .items_center()
                .gap(px(spacing.sm))
                .child(render_zone(first, 0).flex_1().min_w_0())
                .child(render_zone(second, second_align).flex_1().min_w_0())
        };

        let left_side = side_zone("topbar-left-side", left, center_left, 1);
        let right_configured = side_zone("topbar-configured-right-side", center_right, right, 1);
        let right_side = div()
            .id("topbar-right-side")
            .flex()
            .flex_1()
            .min_w_0()
            .overflow_x_scroll()
            .items_center()
            .gap(px(spacing.xs))
            .child(right_configured.flex_1().min_w_0())
            .when_some(status, |this, status| {
                this.child(
                    div()
                        .id("topbar-connection-status")
                        .w(px(88.))
                        .max_w(px(88.))
                        .min_w_0()
                        .flex_shrink(1.)
                        .truncate()
                        .text_color(muted_foreground)
                        .role(Role::Status)
                        .aria_label(status.clone())
                        .child(status),
                )
            })
            .child(launcher_button.flex_shrink(0.))
            .child(settings_button.flex_shrink(0.));

        let escape_entity = this.clone();
        let settings_dialog_focus = self.settings_dialog_focus.clone();
        let settings_button_focus = self.settings_button_focus.clone();
        let settings_overlay = if self.settings_open {
            let module_entity = this.clone();
            let wallpaper_entity = this.clone();
            let wallpaper_keyboard_entity = wallpaper_entity.clone();
            let wallpaper_editor = self.wallpaper_editor.clone();
            let wallpaper_keyboard_editor = wallpaper_editor.clone();
            let scheme = match self.mvp_settings.color_scheme {
                crate::mvp_settings::ColorScheme::Light => "Light theme",
                crate::mvp_settings::ColorScheme::Dark => "Dark theme",
            };
            let module_count = self.plugins.descriptors().len();
            let module_rows = self
                .plugins
                .descriptors()
                .into_iter()
                .enumerate()
                .map(|(index, (id, label))| {
                    let entity = module_entity.clone();
                    let keyboard_entity = module_entity.clone();
                    let focus_handle = self.settings_control_focuses[index + 1].clone();
                    let active = self
                        .mvp_settings
                        .enabled_modules
                        .iter()
                        .any(|enabled| enabled == id);
                    div()
                        .id(format!("gpui-mvp-module-{id}"))
                        .px(px(10.))
                        .py(px(7.))
                        .rounded_lg()
                        .text_color(foreground_color)
                        .role(Role::Button)
                        .aria_label(format!(
                            "{} {label}",
                            if active { "Disable" } else { "Enable" }
                        ))
                        .focusable()
                        .tab_stop(true)
                        .track_focus(&focus_handle)
                        .flex_shrink(0.)
                        .focus_visible(move |style| style.border_1().border_color(focus_outline))
                        .hover(move |style| style.bg(hover_surface))
                        .on_click(move |_, _, cx| {
                            let _ = entity.update(cx, |view, cx| {
                                if active {
                                    view.mvp_settings.enabled_modules.retain(|item| item != id);
                                } else if view
                                    .mvp_settings
                                    .panel_modules
                                    .iter()
                                    .any(|item| item == id)
                                {
                                    view.mvp_settings.enabled_modules.push(id.to_owned());
                                }
                                view.persist_mvp_settings();
                                cx.notify();
                            });
                        })
                        .on_key_down(move |event, _, cx| {
                            if activation_pressed(event) {
                                let _ = keyboard_entity.update(cx, |view, cx| {
                                    if active {
                                        view.mvp_settings.enabled_modules.retain(|item| item != id);
                                    } else if view
                                        .mvp_settings
                                        .panel_modules
                                        .iter()
                                        .any(|item| item == id)
                                    {
                                        view.mvp_settings.enabled_modules.push(id.to_owned());
                                    }
                                    view.persist_mvp_settings();
                                    cx.notify();
                                });
                            }
                        })
                        .child(format!("{}  {label}", if active { "✓" } else { "○" }))
                })
                .collect::<Vec<_>>();
            let status = self
                .settings_status
                .clone()
                .unwrap_or_else(|| "Settings apply to this panel immediately".into());
            let settings_error =
                status.starts_with("Could not save:") || status == "Wallpaper file does not exist";
            let editor_focus = self.settings_control_focuses[module_count + 1].clone();
            // The layer-shell surface is only as tall as the bar, so keep the
            // settings sheet inside that viewport and scroll through its controls.
            // The overlay is positioned relative to the padded bar root. Leave
            // room for that root's horizontal insets so its left edge remains
            // inside the output even when the sheet reaches its 900px cap.
            let settings_width =
                settings_overlay_width(window.bounds().size.width.as_f32(), spacing.md * 2.0 + 8.0);
            div()
                .id("gpui-mvp-settings-popover")
                .absolute()
                .right(px(4.))
                .bottom(px(0.))
                .w(px(settings_width))
                .h(px(self.mvp_settings.panel_height))
                .px(px(4.))
                .flex()
                .items_center()
                // The bar surface is only one row tall. Keep the controls in a
                // bounded horizontal viewport and let users scroll through the
                // full settings row instead of letting it paint past either edge.
                .overflow_x_scroll()
                .track_focus(&self.settings_dialog_focus)
                .tab_stop(false)
                .track_scroll(&self.settings_scroll)
                .role(Role::Dialog)
                .aria_label("Shell settings")
                .gap(px(4.))
                .rounded_lg()
                .border_1()
                .border_color(focus_outline)
                .bg(background)
                .text_color(foreground_color)
                .child(
                    div()
                        .px(px(8.))
                        .font_weight(FontWeight::BOLD)
                        .child("Settings"),
                )
                .child(settings_action_row(
                    "gpui-mvp-theme-toggle",
                    scheme.to_owned(),
                    this.clone(),
                    toggle_color_scheme,
                    self.settings_control_focuses[0].clone(),
                    foreground_color,
                    hover_surface,
                    focus_outline,
                ))
                .children(module_rows)
                .child(
                    div()
                        .id("gpui-mvp-wallpaper-path")
                        .w(px(300.))
                        .h(px(34.))
                        .flex_shrink(0.)
                        .px(px(8.))
                        .rounded_lg()
                        .border_1()
                        .border_color(focus_outline)
                        .track_focus(&editor_focus)
                        .tab_stop(false)
                        .anchor_scroll(Some(ScrollAnchor::for_handle(self.settings_scroll.clone())))
                        .child(self.wallpaper_editor.clone()),
                )
                .child(
                    div()
                        .id("gpui-mvp-wallpaper-apply")
                        .px(px(10.))
                        .py(px(7.))
                        .rounded_lg()
                        .text_color(foreground_color)
                        .role(Role::Button)
                        .aria_label("Apply desktop wallpaper path")
                        .focusable()
                        .tab_stop(true)
                        .track_focus(&self.settings_control_focuses[module_count + 2])
                        .flex_shrink(0.)
                        .focus_visible(move |style| style.border_1().border_color(focus_outline))
                        .hover(move |style| style.bg(hover_surface))
                        .on_click(move |_, _, cx| {
                            apply_wallpaper_from_editor(&wallpaper_entity, &wallpaper_editor, cx);
                        })
                        .on_key_down(move |event, _, cx| {
                            if activation_pressed(event) {
                                apply_wallpaper_from_editor(
                                    &wallpaper_keyboard_entity,
                                    &wallpaper_keyboard_editor,
                                    cx,
                                );
                            }
                        })
                        .child("Apply wallpaper"),
                )
                .child(settings_action_row(
                    "gpui-mvp-accent",
                    format!("Accent: {}", self.mvp_settings.accent),
                    this.clone(),
                    cycle_accent,
                    self.settings_control_focuses[module_count + 3].clone(),
                    foreground_color,
                    hover_surface,
                    focus_outline,
                ))
                .child(settings_action_row(
                    "gpui-mvp-text-size",
                    format!("Text size: {:.0}%", self.mvp_settings.text_scale * 100.0),
                    this.clone(),
                    cycle_text_size,
                    self.settings_control_focuses[module_count + 4].clone(),
                    foreground_color,
                    hover_surface,
                    focus_outline,
                ))
                .child(settings_action_row(
                    "gpui-mvp-radius",
                    format!("Corner radius: {:.0}", self.mvp_settings.corner_radius),
                    this.clone(),
                    cycle_corner_radius,
                    self.settings_control_focuses[module_count + 5].clone(),
                    foreground_color,
                    hover_surface,
                    focus_outline,
                ))
                .child(settings_action_row(
                    "gpui-mvp-edge",
                    format!(
                        "Panel edge: {}",
                        if self.mvp_settings.panel_edge == crate::mvp_settings::PanelEdge::Top {
                            "top"
                        } else {
                            "bottom"
                        }
                    ),
                    this.clone(),
                    toggle_panel_edge,
                    self.settings_control_focuses[module_count + 6].clone(),
                    foreground_color,
                    hover_surface,
                    focus_outline,
                ))
                .child(settings_action_row(
                    "gpui-mvp-height",
                    format!("Panel height: {:.0}px", self.mvp_settings.panel_height),
                    this.clone(),
                    cycle_panel_height,
                    self.settings_control_focuses[module_count + 7].clone(),
                    foreground_color,
                    hover_surface,
                    focus_outline,
                ))
                .child(settings_action_row(
                    "gpui-mvp-order",
                    "Rotate panel module order".into(),
                    this.clone(),
                    rotate_module_order,
                    self.settings_control_focuses[module_count + 8].clone(),
                    foreground_color,
                    hover_surface,
                    focus_outline,
                ))
                .child(
                    div()
                        .id("gpui-mvp-settings-status")
                        .px(px(8.))
                        .role(if settings_error {
                            Role::Alert
                        } else {
                            Role::Status
                        })
                        .aria_label(status.clone())
                        .text_color(if settings_error {
                            error_color
                        } else {
                            muted_foreground
                        })
                        .child(status),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let outside_dismiss_state = self.media_popup.clone();
        let outside_dismiss_restore_focus = self.media_popup_restore_focus.clone();
        let outside_dismiss_subscription = self.media_popup_closed_subscription.clone();
        let outside_dismiss_action = self.media_actions.clone();
        div()
            .size_full()
            .relative()
            .flex()
            .items_center()
            .gap(px(spacing.sm))
            .px(px(spacing.md))
            .bg(if show_background {
                material::surface_color(&self.theme.material, background)
            } else {
                rgba(0x00000000).into()
            })
            .text_color(foreground_color)
            .text_size(px(14.0 * self.mvp_settings.text_scale))
            // Three equal viewport columns keep the center group at the exact
            // panel midpoint. Each side preserves its configured zones and can
            // scroll instead of pushing the center away from the midpoint.
            .child(left_side)
            .child(
                render_zone(center, 2)
                    .id("topbar-center-zone")
                    .flex_1()
                    .min_w_0()
                    .overflow_x_scroll(),
            )
            .child(right_side)
            .child(settings_overlay)
            .on_key_down(move |event, window, cx| {
                if event.keystroke.key.eq_ignore_ascii_case("escape") {
                    let focus_is_in_settings = settings_dialog_focus.within_focused(window, cx);
                    let _ = escape_entity.update(cx, |view, cx| {
                        if view.settings_open {
                            if focus_is_in_settings {
                                settings_button_focus.focus(window, cx);
                            }
                            view.settings_open = false;
                            cx.notify();
                        }
                    });
                }
            })
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                if let Some(handle) = outside_dismiss_state.borrow_mut().take() {
                    outside_dismiss_restore_focus.set(false);
                    outside_dismiss_subscription.borrow_mut().take();
                    let _ = outside_dismiss_action.try_send(MediaAction::CloseControls);
                    let _ = handle.update(cx, |_, popup_window, _| popup_window.remove_window());
                }
            })
    }
}

impl TopBar {
    fn persist_mvp_settings(&mut self) {
        match self.mvp_settings.write_to(crate::config::runtime_path()) {
            Ok(()) => {
                self.mvp_settings.configured = true;
                self.settings_status = Some("Saved".into());
                apply_mvp_theme(&mut self.theme, &self.mvp_settings);
                self.plugins.stop_all();
                self.plugins.start_enabled(&self.mvp_settings);
            }
            Err(error) => self.settings_status = Some(format!("Could not save: {error}")),
        }
    }
}

fn settings_action_row(
    id: &'static str,
    label: String,
    entity: gpui::Entity<TopBar>,
    action: fn(&mut TopBar),
    focus_handle: FocusHandle,
    foreground: gpui::Hsla,
    hover_surface: gpui::Hsla,
    focus_outline: gpui::Hsla,
) -> impl IntoElement {
    let keyboard_entity = entity.clone();
    div()
        .id(id)
        .px(px(10.))
        .py(px(7.))
        .rounded_lg()
        .text_color(foreground)
        .role(Role::Button)
        .focusable()
        .tab_stop(true)
        .track_focus(&focus_handle)
        .flex_shrink(0.)
        .focus_visible(move |style| style.border_1().border_color(focus_outline))
        .hover(move |style| style.bg(hover_surface))
        .on_click(move |_, _, cx| {
            let _ = entity.update(cx, |view, cx| {
                action(view);
                cx.notify();
            });
        })
        .on_key_down(move |event, _, cx| {
            if activation_pressed(event) {
                let _ = keyboard_entity.update(cx, |view, cx| {
                    action(view);
                    cx.notify();
                });
            }
        })
        .child(label)
}

fn activation_pressed(event: &KeyDownEvent) -> bool {
    !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space")
}

fn apply_wallpaper_from_editor(
    entity: &gpui::Entity<TopBar>,
    editor: &gpui::Entity<crate::families::waffle::taskbar::start_menu_search::StartMenuSearch>,
    cx: &mut gpui::App,
) {
    let path = editor.read(cx).current_query().trim().to_owned();
    let _ = entity.update(cx, |view, cx| {
        if path.is_empty() || std::path::Path::new(&path).is_file() {
            view.mvp_settings.wallpaper = (!path.is_empty()).then_some(path);
            view.persist_mvp_settings();
        } else {
            view.settings_status = Some("Wallpaper file does not exist".into());
        }
        cx.notify();
    });
}

fn toggle_color_scheme(view: &mut TopBar) {
    view.mvp_settings.color_scheme = match view.mvp_settings.color_scheme {
        crate::mvp_settings::ColorScheme::Dark => crate::mvp_settings::ColorScheme::Light,
        crate::mvp_settings::ColorScheme::Light => crate::mvp_settings::ColorScheme::Dark,
    };
    view.persist_mvp_settings();
}

fn cycle_accent(view: &mut TopBar) {
    view.mvp_settings.accent = match view.mvp_settings.accent.as_str() {
        "#7aa2f7" => "#8bd5ca".into(),
        "#8bd5ca" => "#f5a97f".into(),
        _ => "#7aa2f7".into(),
    };
    view.persist_mvp_settings();
}

fn cycle_text_size(view: &mut TopBar) {
    view.mvp_settings.text_scale = match view.mvp_settings.text_scale {
        scale if scale < 0.9 => 0.9,
        scale if scale < 1.05 => 1.1,
        _ => 0.8,
    };
    view.persist_mvp_settings();
}

fn cycle_corner_radius(view: &mut TopBar) {
    view.mvp_settings.corner_radius = match view.mvp_settings.corner_radius {
        radius if radius < 9.0 => 12.0,
        radius if radius < 15.0 => 20.0,
        _ => 6.0,
    };
    view.persist_mvp_settings();
}

fn toggle_panel_edge(view: &mut TopBar) {
    view.mvp_settings.panel_edge = match view.mvp_settings.panel_edge {
        crate::mvp_settings::PanelEdge::Top => crate::mvp_settings::PanelEdge::Bottom,
        crate::mvp_settings::PanelEdge::Bottom => crate::mvp_settings::PanelEdge::Top,
    };
    view.persist_mvp_settings();
}

fn cycle_panel_height(view: &mut TopBar) {
    view.mvp_settings.panel_height = match view.mvp_settings.panel_height {
        height if height < 40.0 => 48.0,
        height if height < 56.0 => 64.0,
        _ => 36.0,
    };
    view.persist_mvp_settings();
}

fn rotate_module_order(view: &mut TopBar) {
    if view.mvp_settings.panel_modules.len() > 1 {
        view.mvp_settings.panel_modules.rotate_left(1);
    }
    view.persist_mvp_settings();
}

pub fn apply_mvp_theme(theme: &mut ThemeTokens, settings: &crate::mvp_settings::MvpSettings) {
    let dark = settings.color_scheme == crate::mvp_settings::ColorScheme::Dark;
    let background = if dark { 0x11151cff } else { 0xf3f4f6ff };
    let surface = if dark { 0x1a202aff } else { 0xffffffff };
    let foreground = if dark { 0xe8edf5ff } else { 0x20252eff };
    let muted = if dark { 0xaab4c2ff } else { 0x606a78ff };
    let accent = u32::from_str_radix(settings.accent.trim_start_matches('#'), 16)
        .map(|rgb| (rgb << 8) | 0xff)
        .unwrap_or(0x7aa2f7ff);
    theme.colors.background = Some(rgba(background).into());
    theme.colors.surface = Some(rgba(surface).into());
    theme.colors.surface_elevated = Some(rgba(surface).into());
    theme.colors.foreground = Some(rgba(foreground).into());
    theme.colors.foreground_muted = Some(rgba(muted).into());
    theme.colors.accent = Some(rgba(accent).into());
    theme.material.colors.primary = Some(rgba(accent).into());
    theme.typography.size_scale = settings.text_scale;
    theme.material.typography.size_scale = settings.text_scale;
    theme.shape.medium = settings.corner_radius;
    theme.material.shape.medium = settings.corner_radius;
}

fn resource_strip(
    snapshot: &ResourceSnapshot,
    config: &ShellConfig,
    muted_foreground: gpui::Hsla,
    warning_color: gpui::Hsla,
    error_color: gpui::Hsla,
    spacing: crate::theme::SpacingTokens,
    max_width: gpui::Pixels,
) -> impl IntoElement {
    let mut indicators = Vec::new();
    let resources = &config.bar.resources;
    let mut push = |shown: Option<bool>,
                    name: &'static str,
                    icon: &'static str,
                    value: Option<f64>,
                    caution_threshold: Option<f64>,
                    threshold: Option<f64>,
                    fallback: f64| {
        if !shown.unwrap_or(true) {
            return;
        }
        let percent = value.map(|value| value.clamp(0.0, 100.0));
        let warning = percent.is_some_and(|value| value >= threshold.unwrap_or(fallback));
        let caution = percent
            .is_some_and(|value| caution_threshold.is_some_and(|limit| value >= limit) && !warning);
        let color = if warning {
            error_color
        } else if caution {
            warning_color
        } else {
            muted_foreground
        };
        let value_label = percent
            .map(|value| format!("{value:.0}%"))
            .unwrap_or_else(|| "—".into());
        let accessible = percent
            .map(|value| format!("{name}: {value:.0}%"))
            .unwrap_or_else(|| format!("{name}: unavailable"));
        indicators.push(
            div()
                .min_w_0()
                .flex_shrink(1.)
                .flex()
                .items_center()
                .gap(px(spacing.xs))
                .id(format!("topbar-resource-{name}"))
                .aria_label(accessible)
                .when_some(percent, |element, percent| {
                    element
                        .role(Role::ProgressIndicator)
                        .aria_min_numeric_value(0.0)
                        .aria_max_numeric_value(100.0)
                        .aria_numeric_value(percent)
                })
                .text_color(color)
                .child(
                    div()
                        .flex_shrink(0.)
                        .font_family("Material Symbols Rounded")
                        .child(icon),
                )
                .child(div().min_w_0().truncate().child(value_label))
                .into_any_element(),
        );
    };
    push(
        resources.show_memory_indicator,
        "Memory",
        "memory",
        snapshot.memory_percent,
        None,
        resources.memory_warning_threshold,
        90.0,
    );
    push(
        resources.show_swap_indicator,
        "Swap",
        "swap_horiz",
        snapshot.swap_percent,
        None,
        resources.swap_warning_threshold,
        90.0,
    );
    // The QML module includes temperature between memory and CPU. No sensor
    // provider is connected, so this remains an explicit unavailable reading.
    push(
        resources.show_temp_indicator,
        "Temperature",
        "thermostat",
        None,
        resources.temp_caution_threshold.or(Some(65.0)),
        resources.temp_warning_threshold,
        80.0,
    );
    push(
        resources.show_cpu_indicator,
        "CPU",
        "planner_review",
        snapshot.cpu_percent,
        None,
        resources.cpu_warning_threshold,
        90.0,
    );
    // GPU remains visible when configured, with an explicit unavailable marker
    // until a supported telemetry provider is wired into the prototype.
    push(
        resources.show_gpu_indicator,
        "GPU",
        "memory_alt",
        None,
        None,
        resources.gpu_warning_threshold,
        90.0,
    );
    div()
        .flex()
        .items_center()
        .gap(px(spacing.sm))
        .min_w_0()
        .max_w(max_width)
        .children(indicators)
}

fn workspace_accessible_name(number: u8, occupied: bool, active: bool) -> String {
    match (active, occupied) {
        (true, true) => format!("Workspace {number}, active, has windows"),
        (true, false) => format!("Workspace {number}, active, empty"),
        (false, true) => format!("Workspace {number}, has windows"),
        (false, false) => format!("Workspace {number}, empty"),
    }
}

fn layout_zones(config: &ShellConfig) -> [(usize, Vec<String>); 5] {
    let layout = &config.bar.layout;
    let has_active_window = layout
        .left
        .iter()
        .chain(&layout.center_left)
        .chain(&layout.center)
        .chain(&layout.center_right)
        .chain(&layout.right)
        .any(|id| id == "activeWindow");
    let normalize = |ids: &[String]| {
        ids.iter()
            .filter_map(|id| match id.as_str() {
                "taskbar" if !has_active_window => Some("activeWindow".to_owned()),
                "taskbar" => None,
                _ => Some(id.clone()),
            })
            .collect()
    };
    [
        (0, normalize(&layout.left)),
        (1, normalize(&layout.center_left)),
        (2, normalize(&layout.center)),
        (3, normalize(&layout.center_right)),
        (4, normalize(&layout.right)),
    ]
}

fn order_zone_children<T>(zone_index: usize, mut children: Vec<T>) -> Vec<T> {
    if zone_index == 4 {
        children.reverse();
    }
    children
}

fn configured_modules(config: &ShellConfig) -> Vec<String> {
    layout_zones(config)
        .into_iter()
        .flat_map(|(_, modules)| modules)
        .collect()
}

fn shortened_form_for_width(width: f32) -> u8 {
    if width <= 1000.0 {
        2
    } else if width <= 1200.0 {
        1
    } else {
        0
    }
}

fn settings_overlay_width(output_width: f32, horizontal_inset: f32) -> f32 {
    (output_width - horizontal_inset).clamp(1.0, 900.0)
}

pub(crate) fn media_popup_colors(theme: &ThemeTokens) -> MediaControlsPopupColors {
    let foreground = theme
        .material
        .colors
        .on_surface
        .or(theme.colors.foreground)
        .unwrap_or_else(|| white().into());
    let muted = theme
        .material
        .colors
        .on_surface_variant
        .or(theme.colors.foreground_muted)
        .unwrap_or_else(|| rgba(SUBTLE_TEXT).into());
    let surface = theme
        .material
        .colors
        .surface_container
        .or(theme.colors.surface_elevated)
        .or(theme.colors.surface)
        .unwrap_or_else(|| rgba(SURFACE).into());
    let accent = theme
        .material
        .colors
        .primary
        .or(theme.colors.accent)
        .unwrap_or(foreground);
    let outline = theme
        .material
        .colors
        .outline
        .or(theme.colors.accent)
        .unwrap_or(muted);
    MediaControlsPopupColors {
        surface,
        foreground,
        muted,
        accent,
        track: muted.opacity(0.28),
        outline,
        control_surface: theme
            .material
            .colors
            .surface_container_high
            .unwrap_or(surface),
        hover_surface: theme
            .material
            .colors
            .surface_container_highest
            .unwrap_or(surface),
        focus: outline,
    }
}

fn focus_workspace(number: u8) {
    let _ = Command::new("niri")
        .args(["msg", "action", "focus-workspace"])
        .arg(number.to_string())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::{
        clear_popup_subscription_if_current, order_zone_children, popup_id_matches,
        settings_overlay_width, shortened_form_for_width,
    };

    #[test]
    fn stale_popup_close_callback_does_not_match_reopened_window() {
        assert!(popup_id_matches(Some(41_u64), 41));
        assert!(!popup_id_matches(Some(42_u64), 41));
        assert!(!popup_id_matches(None::<u64>, 41));
    }

    #[test]
    fn stale_media_update_clears_only_its_own_subscription() {
        let mut current_subscription = Some("current popup subscription");
        assert!(clear_popup_subscription_if_current(
            Some(41_u64),
            41,
            &mut current_subscription,
        ));
        assert_eq!(current_subscription, None);

        let mut reopened_subscription = Some("reopened popup subscription");
        assert!(!clear_popup_subscription_if_current(
            Some(42_u64),
            41,
            &mut reopened_subscription,
        ));
        assert_eq!(reopened_subscription, Some("reopened popup subscription"));
    }

    #[test]
    fn right_edge_zone_reverses_render_order_without_reordering_other_zones() {
        let configured = vec!["clock", "spacer", "tray"];
        assert_eq!(
            order_zone_children(4, configured.clone()),
            ["tray", "spacer", "clock"]
        );
        assert_eq!(order_zone_children(3, configured.clone()), configured);
    }

    #[test]
    fn shortened_form_matches_qml_output_width_boundaries() {
        assert_eq!(shortened_form_for_width(1000.0), 2);
        assert_eq!(shortened_form_for_width(1001.0), 1);
        assert_eq!(shortened_form_for_width(1200.0), 1);
        assert_eq!(shortened_form_for_width(1201.0), 0);
    }

    #[test]
    fn settings_overlay_stays_inside_output_and_respects_its_maximum_width() {
        assert_eq!(settings_overlay_width(1920.0, 32.0), 900.0);
        assert_eq!(settings_overlay_width(640.0, 32.0), 608.0);
        assert_eq!(settings_overlay_width(24.0, 32.0), 1.0);
    }
}
