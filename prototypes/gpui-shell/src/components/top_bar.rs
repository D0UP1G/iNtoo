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
    FontWeight, InteractiveElement, Role, StatefulInteractiveElement, Window, div, prelude::*, px,
    rgba, white,
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
    active_window: Option<String>,
    workspaces: Vec<WorkspaceSlot>,
    clock: Option<String>,
    resources: ResourceSnapshot,
    status: Option<String>,
    config: ShellConfig,
    theme: ThemeTokens,
}

impl TopBar {
    pub fn new(
        output_name: Option<String>,
        config: ShellConfig,
        theme: ThemeTokens,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = Self {
            active_window: None,
            workspaces: Vec::new(),
            clock: None,
            resources: ResourceSnapshot::default(),
            status: Some("Connecting to Niri…".into()),
            config: config.clone(),
            theme: theme.clone(),
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
        for (zone_index, configured) in layout_zones(&self.config) {
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
                    "workspaces" => {
                        if !self.workspaces.is_empty() {
                            let mut workspace_buttons = Vec::new();
                            for slot in &self.workspaces {
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
                                    .rounded_lg()
                                    .bg(background)
                                    .font_weight(if slot.active {
                                        FontWeight::BOLD
                                    } else if slot.occupied {
                                        FontWeight::MEDIUM
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(foreground)
                                    .id(("workspace", u32::from(workspace_number)))
                                    .aria_label(workspace_accessible_name(slot))
                                    .role(Role::Button)
                                    .focusable()
                                    .tab_stop(true)
                                    .hover(move |style| style.bg(hover_surface))
                                    .active(move |style| style.bg(pressed_surface))
                                    .on_click(move |_, _, _| focus_workspace(workspace_number))
                                    .on_key_down(move |event, _, _| {
                                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                            && !event.is_held
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
                                    .id("topbar-workspaces-rail")
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
                    "clock" => {
                        if let Some(clock) = clock.as_ref() {
                            zones[zone_index].push(
                                div()
                                    .min_w_0()
                                    .max_w(px(260.))
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(clock.clone())
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

        div()
            .size_full()
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
            .child(render_zone(left, 0).flex_1())
            .child(render_zone(center_left, 1).flex_1())
            .child(render_zone(center, 2).flex_1())
            .child(render_zone(center_right, 0).flex_1())
            .child(render_zone(right, 1).flex_1())
    }
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

fn workspace_accessible_name(slot: &WorkspaceSlot) -> String {
    match (slot.active, slot.occupied) {
        (true, true) => format!("Workspace {}, active, has windows", slot.number),
        (true, false) => format!("Workspace {}, active, empty", slot.number),
        (false, true) => format!("Workspace {}, has windows", slot.number),
        (false, false) => format!("Workspace {}, empty", slot.number),
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
