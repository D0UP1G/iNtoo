//! Family-aware GPUI composition boundary.
//!
//! QML roots: `modules/bar/BarContent.qml` (ii),
//! `modules/waffle/bar/WaffleBarContent.qml`, and
//! `modules/iris/bar/IrisIsland.qml`. The QML roots also depend on Quickshell
//! services and family-specific state; those dependencies must be supplied by
//! an adapter through the input contracts below.

use async_channel::Sender;
use gpui::{
    Context, DisplayId, Entity, FocusHandle, IntoElement, Pixels, Size, Window, div, prelude::*,
};
use std::sync::Arc;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::{
    components::{top_bar::TopBar, vertical_bar::VerticalBar},
    config::ShellConfig,
    families::{
        ii::panel_surface::{self, IiPanelEvent, IiPanelLauncher},
        iris::runtime::IrisRuntime,
        waffle::{
            controller::{
                ActionCenterController, ActionCenterEvent, ClipboardController, ClipboardEvent,
                NotificationCenterController, NotificationCenterEvent, StartMenuEvent,
                TaskViewController, TaskViewEvent,
            },
            runtime::WaffleRuntime,
            taskbar::start_menu_adapter::desktop_entries::XdgDesktopEntrySource,
        },
    },
    theme::ThemeTokens,
};

/// Fully resolved data required to display one family root. Host geometry,
/// layer-shell anchoring and output filtering remain the surface manager's job.
pub enum FamilyComposition {
    IiHorizontal {
        output_name: Option<String>,
        config: ShellConfig,
        theme: ThemeTokens,
        display_id: DisplayId,
        panel_events: Sender<IiPanelEvent>,
    },
    IiVertical {
        output_name: Option<String>,
        config: ShellConfig,
        theme: ThemeTokens,
        display_id: DisplayId,
        panel_events: Sender<IiPanelEvent>,
        media_actions: async_channel::Sender<crate::components::vertical_bar::MediaAction>,
    },
    /// Waffle uses XDG desktop entries and live clock state; tasks, tray, and
    /// notifications stay empty until their platform adapters are connected.
    Waffle {
        theme: ThemeTokens,
        start_menu_enabled: bool,
        notifications_enabled: bool,
        task_view_enabled: bool,
        display_id: DisplayId,
        controller: Sender<StartMenuEvent>,
        notification_events: Sender<NotificationCenterEvent>,
        notification_controller: Rc<RefCell<NotificationCenterController>>,
        task_view_events: Sender<TaskViewEvent>,
        task_view_controller: Rc<RefCell<TaskViewController>>,
        action_center_enabled: bool,
        action_center_events: Sender<ActionCenterEvent>,
        action_center_controller: Rc<RefCell<ActionCenterController>>,
        clipboard_enabled: bool,
        clipboard_events: Sender<ClipboardEvent>,
        clipboard_controller: Rc<RefCell<ClipboardController>>,
        output_name: Option<String>,
        source: Option<Arc<XdgDesktopEntrySource>>,
    },
    IrisRuntime {
        theme: ThemeTokens,
        options: Option<serde_json::Value>,
        dock_options: Option<serde_json::Value>,
        output_size: Size<Pixels>,
        dock_enabled: bool,
        output_name: Option<String>,
    },
}

/// API version for statically linked environment plugins. These plugins supply
/// family content while GPUI windows and layer-shell lifecycle stay in the host.
pub const ENVIRONMENT_PLUGIN_API_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnvironmentId(&'static str);

impl EnvironmentId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnvironmentDescriptor {
    pub id: EnvironmentId,
    pub api_version: u16,
    pub display_name: &'static str,
}

/// Statically linked content provider. Implementations must not create or own
/// top-level GPUI windows; the host embeds their content in host-managed views.
pub trait EnvironmentPlugin {
    fn descriptor(&self) -> &'static EnvironmentDescriptor;

    fn create_composition(
        &self,
        inputs: ConfiguredFamilyInputs,
        runtime: FamilyRuntimeInputs,
    ) -> Result<FamilyComposition, FamilyInputError>;
}

/// Live adapters and host-resolved geometry for one requested root. These are
/// deliberately host-created; plugins only assemble them into family content.
pub enum FamilyRuntimeInputs {
    IiHorizontal {
        display_id: DisplayId,
        panel_events: Sender<IiPanelEvent>,
    },
    IiVertical {
        display_id: DisplayId,
        panel_events: Sender<IiPanelEvent>,
        media_actions: async_channel::Sender<crate::components::vertical_bar::MediaAction>,
    },
    Waffle {
        display_id: DisplayId,
        controller: Sender<StartMenuEvent>,
        notification_events: Sender<NotificationCenterEvent>,
        notification_controller: Rc<RefCell<NotificationCenterController>>,
        task_view_events: Sender<TaskViewEvent>,
        task_view_controller: Rc<RefCell<TaskViewController>>,
        action_center_events: Sender<ActionCenterEvent>,
        action_center_controller: Rc<RefCell<ActionCenterController>>,
        clipboard_events: Sender<ClipboardEvent>,
        clipboard_controller: Rc<RefCell<ClipboardController>>,
        source: Option<Arc<XdgDesktopEntrySource>>,
    },
    Iris {
        output_size: Size<Pixels>,
        options: Option<serde_json::Value>,
        dock_options: Option<serde_json::Value>,
        dock_enabled: bool,
    },
}

struct IiEnvironment;
struct WaffleEnvironment;
struct IrisEnvironment;

static II_DESCRIPTOR: EnvironmentDescriptor = EnvironmentDescriptor {
    id: EnvironmentId::new("ii"),
    api_version: ENVIRONMENT_PLUGIN_API_VERSION,
    display_name: "Material",
};
static WAFFLE_DESCRIPTOR: EnvironmentDescriptor = EnvironmentDescriptor {
    id: EnvironmentId::new("waffle"),
    api_version: ENVIRONMENT_PLUGIN_API_VERSION,
    display_name: "Waffle",
};
static IRIS_DESCRIPTOR: EnvironmentDescriptor = EnvironmentDescriptor {
    id: EnvironmentId::new("iris"),
    api_version: ENVIRONMENT_PLUGIN_API_VERSION,
    display_name: "Iris",
};

impl EnvironmentPlugin for IiEnvironment {
    fn descriptor(&self) -> &'static EnvironmentDescriptor {
        &II_DESCRIPTOR
    }

    fn create_composition(
        &self,
        inputs: ConfiguredFamilyInputs,
        runtime: FamilyRuntimeInputs,
    ) -> Result<FamilyComposition, FamilyInputError> {
        let composition = match (inputs.config.bar.vertical.unwrap_or(false), runtime) {
            (
                true,
                FamilyRuntimeInputs::IiVertical {
                    display_id,
                    panel_events,
                    media_actions,
                },
            ) => FamilyComposition::IiVertical {
                output_name: inputs.output_name,
                config: inputs.config,
                theme: inputs.theme,
                display_id,
                panel_events,
                media_actions,
            },
            (
                false,
                FamilyRuntimeInputs::IiHorizontal {
                    display_id,
                    panel_events,
                },
            ) => FamilyComposition::IiHorizontal {
                output_name: inputs.output_name,
                config: inputs.config,
                theme: inputs.theme,
                display_id,
                panel_events,
            },
            _ => return Err(FamilyInputError::RuntimeInputMismatch("ii")),
        };
        Ok(composition)
    }
}

impl EnvironmentPlugin for WaffleEnvironment {
    fn descriptor(&self) -> &'static EnvironmentDescriptor {
        &WAFFLE_DESCRIPTOR
    }

    fn create_composition(
        &self,
        inputs: ConfiguredFamilyInputs,
        runtime: FamilyRuntimeInputs,
    ) -> Result<FamilyComposition, FamilyInputError> {
        let FamilyRuntimeInputs::Waffle {
            display_id,
            controller,
            notification_events,
            notification_controller,
            task_view_events,
            task_view_controller,
            action_center_events,
            action_center_controller,
            clipboard_events,
            clipboard_controller,
            source,
        } = runtime
        else {
            return Err(FamilyInputError::RuntimeInputMismatch("waffle"));
        };
        Ok(FamilyComposition::Waffle {
            theme: inputs.theme,
            start_menu_enabled: inputs
                .config
                .enabled_panels
                .iter()
                .any(|panel| panel == "wStartMenu"),
            notifications_enabled: inputs
                .config
                .enabled_panels
                .iter()
                .any(|panel| panel == "wNotificationCenter"),
            task_view_enabled: inputs
                .config
                .enabled_panels
                .iter()
                .any(|panel| panel == "wTaskView"),
            display_id,
            controller,
            notification_events,
            notification_controller,
            task_view_events,
            task_view_controller,
            action_center_enabled: inputs
                .config
                .enabled_panels
                .iter()
                .any(|panel| panel == "wActionCenter"),
            action_center_events,
            action_center_controller,
            clipboard_enabled: inputs
                .config
                .enabled_panels
                .iter()
                .any(|panel| panel == "wClipboard"),
            clipboard_events,
            clipboard_controller,
            output_name: inputs.output_name,
            source,
        })
    }
}

impl EnvironmentPlugin for IrisEnvironment {
    fn descriptor(&self) -> &'static EnvironmentDescriptor {
        &IRIS_DESCRIPTOR
    }

    fn create_composition(
        &self,
        inputs: ConfiguredFamilyInputs,
        runtime: FamilyRuntimeInputs,
    ) -> Result<FamilyComposition, FamilyInputError> {
        let FamilyRuntimeInputs::Iris {
            output_size,
            options,
            dock_options,
            dock_enabled,
        } = runtime
        else {
            return Err(FamilyInputError::RuntimeInputMismatch("iris"));
        };
        Ok(FamilyComposition::IrisRuntime {
            theme: inputs.theme,
            options,
            dock_options,
            output_size,
            dock_enabled,
            output_name: inputs.output_name,
        })
    }
}

static II_ENVIRONMENT: IiEnvironment = IiEnvironment;
static WAFFLE_ENVIRONMENT: WaffleEnvironment = WaffleEnvironment;
static IRIS_ENVIRONMENT: IrisEnvironment = IrisEnvironment;

/// Immutable v1 registry. Adding a statically linked environment requires a
/// descriptor and registration here; dynamic library loading is intentionally
/// outside the current contract.
pub struct EnvironmentRegistry {
    plugins: Vec<&'static dyn EnvironmentPlugin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateId(&'static str),
    UnsupportedApiVersion { id: &'static str, version: u16 },
    MissingMaterialEnvironment,
}

impl EnvironmentRegistry {
    pub fn builtins() -> Result<Self, RegistryError> {
        Self::new(vec![
            &II_ENVIRONMENT,
            &WAFFLE_ENVIRONMENT,
            &IRIS_ENVIRONMENT,
        ])
    }

    fn new(plugins: Vec<&'static dyn EnvironmentPlugin>) -> Result<Self, RegistryError> {
        let mut ids = std::collections::BTreeSet::new();
        for plugin in &plugins {
            let descriptor = plugin.descriptor();
            if descriptor.api_version != ENVIRONMENT_PLUGIN_API_VERSION {
                return Err(RegistryError::UnsupportedApiVersion {
                    id: descriptor.id.as_str(),
                    version: descriptor.api_version,
                });
            }
            if !ids.insert(descriptor.id) {
                return Err(RegistryError::DuplicateId(descriptor.id.as_str()));
            }
        }
        if !ids.contains(&EnvironmentId::new("ii")) {
            return Err(RegistryError::MissingMaterialEnvironment);
        }
        Ok(Self { plugins })
    }

    pub fn get(&self, id: &str) -> Option<&'static dyn EnvironmentPlugin> {
        self.plugins
            .iter()
            .copied()
            .find(|plugin| plugin.descriptor().id.as_str() == id)
    }

    pub fn select(&self, requested: Option<&str>) -> Option<&'static dyn EnvironmentPlugin> {
        requested
            .and_then(|id| self.get(id))
            .or_else(|| self.get("ii"))
    }

    /// Keep only registered IDs, removing duplicates while preserving cycle order.
    pub fn filter_cycle<'a>(&self, requested: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        let cycle = requested
            .into_iter()
            .filter(|id| self.get(id).is_some() && seen.insert((*id).to_owned()))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if cycle.is_empty() {
            vec!["ii".into()]
        } else {
            cycle
        }
    }
}

/// Errors when selecting a family from source configuration without the
/// required family-owned view model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FamilyInputError {
    Registry(RegistryError),
    RuntimeInputMismatch(&'static str),
}

/// Inputs from which source configuration selects a composition. Iris page
/// content remains adapter supplied; Waffle owns its real desktop-entry and
/// clock adapters.
pub struct ConfiguredFamilyInputs {
    pub output_name: Option<String>,
    pub config: ShellConfig,
    pub theme: ThemeTokens,
}

impl ConfiguredFamilyInputs {
    /// Resolve persisted `panelFamily` through the versioned registry. Unknown
    /// or absent values are diagnosed and fall back to the Material (`ii`)
    /// plugin without rewriting source configuration.
    pub fn select(
        config: &ShellConfig,
    ) -> Result<&'static dyn EnvironmentPlugin, FamilyInputError> {
        let registry = EnvironmentRegistry::builtins().map_err(FamilyInputError::Registry)?;
        let requested = config.panel_family.as_deref();
        let selected = registry
            .select(requested)
            .ok_or_else(|| FamilyInputError::Registry(RegistryError::MissingMaterialEnvironment))?;
        if requested.is_none_or(|id| registry.get(id).is_none()) {
            eprintln!(
                "GPUI environment registry: panelFamily {:?} is missing, unknown, or incompatible; using Material ('ii')",
                requested.unwrap_or("<unset>")
            );
        }
        Ok(selected)
    }

    /// Create content for a selected plugin. The host still owns the top-level
    /// GPUI window and layer-shell lifecycle.
    pub fn create_composition(
        self,
        runtime: FamilyRuntimeInputs,
    ) -> Result<FamilyComposition, FamilyInputError> {
        let plugin = Self::select(&self.config)?;
        plugin.create_composition(self, runtime)
    }
}

enum HostedFamily {
    IiHorizontal(Entity<TopBar>, IiPanelLauncher),
    IiVertical(Entity<VerticalBar>, IiPanelLauncher),
    Waffle(Entity<WaffleRuntime>),
    IrisRuntime(Entity<IrisRuntime>),
    IrisUnavailable,
}

/// GPUI view that composes the selected family root without owning global
/// services. `new` should be called from the GPUI app/window context.
pub struct FamilyHost {
    family: HostedFamily,
    overview_launcher_focus: FocusHandle,
}

impl FamilyHost {
    /// Restore focus to the ii Overview launcher in the bar's own GPUI window.
    pub fn focus_overview_launcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overview_launcher_focus.focus(window, cx);
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub fn set_media_state(
        &mut self,
        state: crate::components::vertical_bar::MediaState,
        cx: &mut Context<Self>,
    ) {
        if let HostedFamily::IiVertical(view, _) = &self.family {
            let _ = view.update(cx, |view, cx| view.set_media_state(state, cx));
        }
    }

    pub fn set_left_sidebar_open(&mut self, open: bool, cx: &mut Context<Self>) {
        match &mut self.family {
            HostedFamily::IiHorizontal(_, launcher) | HostedFamily::IiVertical(_, launcher) => {
                launcher.left_open = open;
                cx.notify();
            }
            _ => {}
        }
    }

    pub fn new(composition: FamilyComposition, cx: &mut Context<Self>) -> Self {
        let overview_launcher_focus = cx.focus_handle();
        let family = match composition {
            FamilyComposition::IiHorizontal {
                output_name,
                config,
                theme,
                display_id,
                panel_events,
            } => {
                let launcher = panel_launcher(
                    display_id,
                    panel_events,
                    &config,
                    overview_launcher_focus.clone(),
                );
                HostedFamily::IiHorizontal(
                    cx.new(|cx| TopBar::new(output_name, config, theme, display_id, cx)),
                    launcher,
                )
            }
            FamilyComposition::IiVertical {
                output_name,
                config,
                theme,
                display_id,
                panel_events,
                media_actions,
            } => {
                let launcher = panel_launcher(
                    display_id,
                    panel_events,
                    &config,
                    overview_launcher_focus.clone(),
                );
                HostedFamily::IiVertical(
                    cx.new(|cx| VerticalBar::live(output_name, config, theme, media_actions, cx)),
                    launcher,
                )
            }
            FamilyComposition::Waffle {
                theme,
                start_menu_enabled,
                notifications_enabled,
                task_view_enabled,
                display_id,
                controller,
                notification_events,
                notification_controller,
                task_view_events,
                task_view_controller,
                action_center_enabled,
                action_center_events,
                action_center_controller,
                clipboard_enabled,
                clipboard_events,
                clipboard_controller,
                output_name,
                source,
            } => HostedFamily::Waffle(cx.new(|cx| {
                WaffleRuntime::new(
                    theme,
                    start_menu_enabled,
                    notifications_enabled,
                    display_id,
                    controller,
                    notification_events,
                    notification_controller,
                    task_view_enabled,
                    task_view_events,
                    task_view_controller,
                    action_center_enabled,
                    action_center_events,
                    action_center_controller,
                    output_name,
                    clipboard_enabled,
                    clipboard_events,
                    clipboard_controller,
                    source,
                    cx,
                )
            })),
            FamilyComposition::IrisRuntime {
                theme,
                options,
                dock_options,
                output_size,
                dock_enabled,
                output_name,
            } => {
                let initialized = Rc::new(Cell::new(false));
                let initialized_in_entity = initialized.clone();
                let view = cx.new(move |iris_cx| {
                    match IrisRuntime::new(
                        theme,
                        options.as_ref(),
                        dock_options.as_ref(),
                        output_size,
                        dock_enabled,
                        output_name,
                        iris_cx,
                    ) {
                        Some(view) => {
                            initialized_in_entity.set(true);
                            view
                        }
                        None => IrisRuntime::unavailable(),
                    }
                });
                if initialized.get() {
                    HostedFamily::IrisRuntime(view)
                } else {
                    eprintln!(
                        "GPUI Iris runtime: constructor returned None after preflight; content suppressed"
                    );
                    HostedFamily::IrisUnavailable
                }
            }
        };
        Self {
            family,
            overview_launcher_focus,
        }
    }
}

impl Render for FamilyHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        match &self.family {
            HostedFamily::IiHorizontal(view, launcher) => div()
                .size_full()
                .relative()
                .child(view.clone())
                .child(panel_surface::render_launchers(launcher.clone()))
                .into_any_element(),
            HostedFamily::IiVertical(view, launcher) => div()
                .size_full()
                .relative()
                .child(view.clone())
                .child(panel_surface::render_launchers(launcher.clone()))
                .into_any_element(),
            HostedFamily::Waffle(view) => div().size_full().child(view.clone()).into_any_element(),
            HostedFamily::IrisRuntime(view) => {
                div().size_full().child(view.clone()).into_any_element()
            }
            HostedFamily::IrisUnavailable => div().size_full().into_any_element(),
        }
    }
}

fn panel_launcher(
    display_id: DisplayId,
    events: Sender<IiPanelEvent>,
    config: &ShellConfig,
    overview_focus: FocusHandle,
) -> IiPanelLauncher {
    let enabled = |name: &str| config.enabled_panels.iter().any(|panel| panel == name);
    IiPanelLauncher {
        display: display_id,
        events,
        left_enabled: enabled("iiSidebarLeft"),
        left_open: false,
        right_enabled: enabled("iiSidebarRight"),
        dashboard_enabled: enabled("iiDashboard") && config.dashboard.enabled,
        overview_enabled: enabled("iiOverview"),
        overview_focus,
        volume_mixer_enabled: true,
    }
}
