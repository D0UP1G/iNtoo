//! Niri-backed ii-style top bar for the GPUI prototype.
//!
//! State is supplied by the compositor adapter. Missing services render as
//! empty states instead of fabricated desktop data.

use std::{
    process::Command,
    time::{Duration, Instant},
};

use crate::components::material;
use crate::config::ShellConfig;
use crate::niri;
use crate::resource_provider::{LinuxResourceProvider, ResourceSnapshot};
use crate::theme::ThemeTokens;
use gpui::{
    FontWeight, InteractiveElement, KeyDownEvent, Role, StatefulInteractiveElement, Window, div,
    prelude::*, px, rgba, white,
};

const SURFACE: u32 = 0x161b22ee;
const SURFACE_ACTIVE: u32 = 0x30363ddd;
const SUBTLE_TEXT: u32 = 0xb1bac4ff;

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
    status: Option<String>,
    config: ShellConfig,
    theme: ThemeTokens,
    mvp_settings: crate::mvp_settings::MvpSettings,
    plugins: crate::mvp_settings::PanelPluginRegistry,
    settings_open: bool,
    settings_status: Option<String>,
    wallpaper_editor:
        gpui::Entity<crate::families::waffle::taskbar::start_menu_search::StartMenuSearch>,
}

impl TopBar {
    pub fn new(
        output_name: Option<String>,
        config: ShellConfig,
        theme: ThemeTokens,
        display_id: gpui::DisplayId,
        cx: &mut Context<Self>,
    ) -> Self {
        let mvp_settings = config.gpui_mvp.clone();
        let mut plugins = crate::mvp_settings::PanelPluginRegistry::builtins();
        plugins.start_enabled(&mvp_settings);
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
            status: Some("Connecting to Niri…".into()),
            config: config.clone(),
            theme: theme.clone(),
            mvp_settings,
            plugins,
            settings_open: false,
            settings_status: None,
            wallpaper_editor,
        };

        // Keep configuration mistakes visible to the developer while avoiding
        // placeholder widgets that would imply an unimplemented service exists.
        for module in configured_modules(&view.config) {
            if !matches!(
                module.as_str(),
                "activeWindow" | "workspaces" | "clock" | "resources"
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
                if should_sample_resources {
                    resources = resource_provider.sample();
                    last_resource_sample = Some(Instant::now());
                    if !crate::families::ii::panel_surface::has_recent_vertical_status_rings_source(
                        status_output_name.as_deref(),
                    ) {
                        let battery = desktop_provider.sample().await;
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
}

impl Render for TopBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        let active_window = self.active_window.clone();
        let status = self.status.clone();
        let clock = self.clock.clone();
        let resources = self.resources.clone();
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
        if let Some(status) = status {
            zones[4].push(
                div()
                    .id("topbar-connection-status")
                    .min_w_0()
                    .max_w(px(180.))
                    .truncate()
                    .text_color(muted_foreground)
                    .role(Role::Status)
                    .child(status)
                    .into_any_element(),
            );
        }
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
                    _ => {}
                }
            }
        }

        let [left, center_left, center, center_right, right] = zones;
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
        let launcher_button = div()
            .id("gpui-mvp-launcher-button")
            .px(px(10.))
            .py(px(6.))
            .rounded_lg()
            .border_1()
            .border_color(rgba(0xffffff22))
            .text_color(foreground_color)
            .role(Role::Button)
            .aria_label("Open app launcher")
            .focusable()
            .tab_stop(true)
            .focus_visible(|style| style.border_1().border_color(rgba(0x7aa2f7ff)))
            .on_click(move |_, _, _| crate::surface_manager::toggle_app_launcher(launch_display))
            .on_key_down(move |event, _, _| {
                if activation_pressed(event) {
                    crate::surface_manager::toggle_app_launcher(launch_display);
                }
            })
            .child("Apps");
        let settings_keyboard_entity = this.clone();
        let settings_button = div()
            .id("gpui-mvp-settings-button")
            .px(px(10.))
            .py(px(6.))
            .rounded_lg()
            .border_1()
            .border_color(if self.settings_open {
                focus_outline
            } else {
                rgba(0xffffff22).into()
            })
            .text_color(foreground_color)
            .role(Role::Button)
            .aria_label("Open shell settings")
            .focusable()
            .tab_stop(true)
            .focus_visible(move |style| style.border_1().border_color(focus_outline))
            .hover(move |style| style.bg(hover_surface))
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
            .child("Settings");

        let escape_entity = this.clone();
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
            let module_rows = self
                .plugins
                .descriptors()
                .into_iter()
                .map(|(id, label)| {
                    let entity = module_entity.clone();
                    let keyboard_entity = module_entity.clone();
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
            div()
                .id("gpui-mvp-settings-popover")
                .absolute()
                .left(px(4.))
                .right(px(4.))
                .bottom(px(0.))
                .h(px(self.mvp_settings.panel_height))
                .px(px(4.))
                .flex()
                .flex_row()
                .items_center()
                .overflow_x_scroll()
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
                ))
                .child(settings_action_row(
                    "gpui-mvp-text-size",
                    format!("Text size: {:.0}%", self.mvp_settings.text_scale * 100.0),
                    this.clone(),
                    cycle_text_size,
                ))
                .child(settings_action_row(
                    "gpui-mvp-radius",
                    format!("Corner radius: {:.0}", self.mvp_settings.corner_radius),
                    this.clone(),
                    cycle_corner_radius,
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
                ))
                .child(settings_action_row(
                    "gpui-mvp-height",
                    format!("Panel height: {:.0}px", self.mvp_settings.panel_height),
                    this.clone(),
                    cycle_panel_height,
                ))
                .child(settings_action_row(
                    "gpui-mvp-order",
                    "Rotate panel module order".into(),
                    this.clone(),
                    rotate_module_order,
                ))
                .child(div().px(px(8.)).text_color(muted_foreground).child(status))
                .into_any_element()
        } else {
            div().into_any_element()
        };

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
            .child(render_zone(left, 0).flex_1())
            .child(render_zone(center_left, 1).flex_1())
            .child(render_zone(center, 2).flex_1())
            .child(render_zone(center_right, 0).flex_1())
            .child(render_zone(right, 1).flex_1())
            .child(launcher_button)
            .child(settings_button)
            .child(settings_overlay)
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key.eq_ignore_ascii_case("escape") {
                    let _ = escape_entity.update(cx, |view, cx| {
                        if view.settings_open {
                            view.settings_open = false;
                            cx.notify();
                        }
                    });
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
) -> impl IntoElement {
    let keyboard_entity = entity.clone();
    div()
        .id(id)
        .px(px(10.))
        .py(px(7.))
        .rounded_lg()
        .text_color(white())
        .role(Role::Button)
        .focusable()
        .tab_stop(true)
        .flex_shrink(0.)
        .focus_visible(|style| style.border_1().border_color(rgba(0x7aa2f7ff)))
        .hover(|style| style.bg(rgba(0xffffff18)))
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

fn configured_modules(config: &ShellConfig) -> Vec<String> {
    layout_zones(config)
        .into_iter()
        .flat_map(|(_, modules)| modules)
        .collect()
}

fn focus_workspace(number: u8) {
    let _ = Command::new("niri")
        .args(["msg", "action", "focus-workspace"])
        .arg(number.to_string())
        .spawn();
}
