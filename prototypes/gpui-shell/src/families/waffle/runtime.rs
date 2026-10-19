//! Live GPUI binding for the Waffle taskbar and its XDG-backed Start Menu.
//!
//! This view owns only its local Start Menu visibility and refresh cadence.
//! Task lists use the live Niri/XDG adapters; tray items and notifications
//! remain empty until real adapters for those services are available.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};

use async_channel::Sender;
use gpui::{Context, DisplayId, IntoElement, Render, ScrollHandle, Window, div, prelude::*};

use crate::{niri, theme::ThemeTokens};

use super::controller::{
    ActionCenterController, ActionCenterEvent, ActionCenterState, ClipboardController,
    ClipboardEvent, ClipboardOverlayState, NotificationCenterController, NotificationCenterEvent,
    NotificationCenterState, StartMenuEvent, TaskViewController, TaskViewEvent, TaskViewState,
};
use super::taskbar::{self, ButtonAction, TaskAction, WaffleTaskbarInput};
use super::taskbar::{
    start_menu::StartMenuStyle,
    start_menu_adapter,
    start_menu_adapter::{StartMenuDataSource, desktop_entries::XdgDesktopEntrySource},
};

/// Preview colors are used only for roles absent from the resolved theme.
/// They are a neutral scaffold, not a claim of QML theme parity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleOrigin {
    ResolvedTheme,
    PreviewFallback,
}

#[derive(Clone, Copy)]
enum RuntimeEvent {
    ToggleStart,
    CloseStart,
    Refresh,
}

pub struct WaffleRuntime {
    input: WaffleTaskbarInput,
    theme: ThemeTokens,
    source: Option<Arc<XdgDesktopEntrySource>>,
    start_open: bool,
    clock_time: Option<String>,
    clock_date: Option<String>,
    style_origin: StyleOrigin,
    event_sender: Sender<RuntimeEvent>,
    start_menu_enabled: bool,
    notifications_enabled: bool,
    display_id: DisplayId,
    controller: Sender<StartMenuEvent>,
    notification_events: Sender<NotificationCenterEvent>,
    notification_controller: Rc<RefCell<NotificationCenterController>>,
    task_view_enabled: bool,
    task_view_events: Sender<TaskViewEvent>,
    task_view_controller: Rc<RefCell<TaskViewController>>,
    action_center_enabled: bool,
    action_center_events: Sender<ActionCenterEvent>,
    action_center_controller: Rc<RefCell<ActionCenterController>>,
    output_name: Option<String>,
    taskbar_icon_size: f32,
    task_windows: HashMap<String, Vec<u64>>,
    task_scroll: ScrollHandle,
    clipboard_enabled: bool,
    clipboard_events: Sender<ClipboardEvent>,
    clipboard_controller: Rc<RefCell<ClipboardController>>,
}

impl WaffleRuntime {
    /// Construct a live taskbar. XDG desktop entries are read from the real
    /// XDG application directories; a missing catalog yields an unavailable
    /// Start Menu state with no fabricated apps.
    pub fn new(
        theme: ThemeTokens,
        start_menu_enabled: bool,
        notifications_enabled: bool,
        display_id: DisplayId,
        controller: Sender<StartMenuEvent>,
        notification_events: Sender<NotificationCenterEvent>,
        notification_controller: Rc<RefCell<NotificationCenterController>>,
        task_view_enabled: bool,
        task_view_events: Sender<TaskViewEvent>,
        task_view_controller: Rc<RefCell<TaskViewController>>,
        action_center_enabled: bool,
        action_center_events: Sender<ActionCenterEvent>,
        action_center_controller: Rc<RefCell<ActionCenterController>>,
        output_name: Option<String>,
        clipboard_enabled: bool,
        clipboard_events: Sender<ClipboardEvent>,
        clipboard_controller: Rc<RefCell<ClipboardController>>,
        source: Option<Arc<XdgDesktopEntrySource>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (event_sender, event_receiver) = async_channel::unbounded();
        let style_origin = if theme.colors.background.is_some()
            && theme.colors.foreground.is_some()
            && theme.colors.accent.is_some()
        {
            StyleOrigin::ResolvedTheme
        } else {
            StyleOrigin::PreviewFallback
        };
        let view = Self {
            input: empty_seed(),
            theme,
            source,
            start_open: false,
            clock_time: None,
            clock_date: None,
            style_origin,
            event_sender,
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
            taskbar_icon_size: 26.0,
            task_windows: HashMap::new(),
            task_scroll: ScrollHandle::new(),
            clipboard_enabled,
            clipboard_events,
            clipboard_controller,
        };

        cx.spawn(async move |this, cx| {
            while let Ok(event) = event_receiver.recv().await {
                let update = this.update(cx, |view, cx| {
                    match event {
                        RuntimeEvent::ToggleStart => view.start_open = !view.start_open,
                        RuntimeEvent::CloseStart => view.start_open = false,
                        RuntimeEvent::Refresh => {}
                    }
                    cx.notify();
                });
                if update.is_err() {
                    break;
                }
            }
        })
        .detach();

        let task_output = view.output_name.clone();
        let task_catalog = view.source.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let output = task_output.clone();
                let catalog = task_catalog.clone();
                let (snapshot, task_snapshot, icon_size) = cx
                    .background_executor()
                    .spawn(async move {
                        let snapshot = niri::snapshot(output.as_deref());
                        let config =
                            crate::config::ConfigLoad::from_path(crate::config::runtime_path());
                        let pinned_apps = configured_pinned_apps(&config);
                        let icon_size = config
                            .config
                            .waffle
                            .bar
                            .icon_size
                            .unwrap_or(26.0)
                            .clamp(20.0, 40.0) as f32;
                        let tasks = super::taskbar::niri_tasks::from_snapshot_with_icon_size(
                            snapshot.clone(),
                            &pinned_apps,
                            catalog.as_deref(),
                            icon_size.round() as u32,
                        );
                        (snapshot, tasks, icon_size)
                    })
                    .await;
                let update = this.update(cx, |view, cx| {
                    if let Some(clock) = snapshot.clock {
                        let mut fields = clock.splitn(2, " · ");
                        view.clock_time = fields.next().map(str::to_owned);
                        view.clock_date = fields.next().map(str::to_owned);
                    } else {
                        view.clock_time = None;
                        view.clock_date = None;
                    }
                    view.input.tasks = task_snapshot.tasks;
                    view.task_windows = task_snapshot.windows_by_app;
                    view.taskbar_icon_size = icon_size;
                    cx.notify();
                });
                if update.is_err() {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
        .detach();

        view
    }

    pub fn style_origin(&self) -> StyleOrigin {
        self.style_origin
    }

    fn taskbar_input(&self) -> WaffleTaskbarInput {
        let mut input = self.input.clone();
        let mut style = taskbar_style(&self.theme);
        style.icon_size = self.taskbar_icon_size;
        input.style = style.clone();
        input.start_open = self.start_open;
        input.task_view_open =
            self.task_view_controller.borrow().state(self.display_id) == TaskViewState::Open;
        input.action_center_open = self
            .action_center_controller
            .borrow()
            .state(self.display_id)
            == ActionCenterState::Open;
        input.clipboard_open = self.clipboard_controller.borrow().state(self.display_id)
            == ClipboardOverlayState::Open;
        let menu = self.start_menu_enabled.then(|| {
            let mut menu = start_menu_adapter::input(
                start_menu_style(&self.theme, &style),
                self.source.as_ref().map(|source| {
                    Arc::clone(source) as Arc<dyn start_menu_adapter::StartMenuDataSource>
                }),
            );
            let close_source = self.source.clone();
            let close_events = self.event_sender.clone();
            let controller = self.controller.clone();
            let display_id = self.display_id;
            menu.on_close = Arc::new(move || {
                if let Some(source) = &close_source {
                    source.close();
                }
                let _ = close_events.try_send(RuntimeEvent::CloseStart);
                let _ = controller.try_send(StartMenuEvent::Close {
                    display: display_id,
                });
            });
            if let Some(source) = &self.source {
                let query_source = Arc::clone(source);
                let query_events = self.event_sender.clone();
                let controller = self.controller.clone();
                let display_id = self.display_id;
                menu.on_query_changed = Arc::new(move |query| {
                    query_source.query_changed(query);
                    let _ = query_events.try_send(RuntimeEvent::Refresh);
                    let _ = controller.try_send(StartMenuEvent::Refresh {
                        display: display_id,
                    });
                });
                let select_source = Arc::clone(source);
                let select_events = self.event_sender.clone();
                menu.on_select_result = Arc::new(move |id| {
                    select_source.select_result(id);
                    let _ = select_events.try_send(RuntimeEvent::Refresh);
                });
                let all_source = Arc::clone(source);
                let all_events = self.event_sender.clone();
                menu.on_all_apps = Arc::new(move || {
                    all_source.show_all_apps();
                    let _ = all_events.try_send(RuntimeEvent::Refresh);
                });
                let back_source = Arc::clone(source);
                let back_events = self.event_sender.clone();
                menu.on_back = Arc::new(move || {
                    back_source.back();
                    let _ = back_events.try_send(RuntimeEvent::Refresh);
                });
            }
            menu
        });
        // The launcher is hosted in a separate, full-output Overlay surface.
        // Keep the taskbar's Start button as the focus-return target only.
        let _ = menu;
        input.start_menu = None;

        let start_events = self.event_sender.clone();
        let controller = self.controller.clone();
        let display_id = self.display_id;
        input.on_start = if self.start_menu_enabled {
            Arc::new(move || {
                let _ = start_events.try_send(RuntimeEvent::ToggleStart);
                let _ = controller.try_send(StartMenuEvent::Toggle {
                    display: display_id,
                });
            })
        } else {
            Arc::new(|| {})
        };
        input.tray_items.clear();
        input.unread_notifications = 0;
        input.notifications_silent = false;
        input.notifications_open = self.notification_controller.borrow().state(self.display_id)
            == NotificationCenterState::Open;
        let notification_controller = self.notification_events.clone();
        let notification_refresh = self.event_sender.clone();
        let display_id = self.display_id;
        input.on_notifications = if self.notifications_enabled {
            Arc::new(move || {
                let _ = notification_controller.try_send(NotificationCenterEvent::Toggle {
                    display: display_id,
                });
                let _ = notification_refresh.try_send(RuntimeEvent::Refresh);
            })
        } else {
            Arc::new(|| {})
        };
        let task_view_events = self.task_view_events.clone();
        let task_view_refresh = self.event_sender.clone();
        let display_id = self.display_id;
        input.on_task_view = if self.task_view_enabled {
            Arc::new(move || {
                let _ = task_view_events.try_send(TaskViewEvent::Toggle {
                    display: display_id,
                });
                let _ = task_view_refresh.try_send(RuntimeEvent::Refresh);
            })
        } else {
            Arc::new(|| {})
        };
        let action_center_events = self.action_center_events.clone();
        let action_center_refresh = self.event_sender.clone();
        let display_id = self.display_id;
        input.on_action_center = if self.action_center_enabled {
            Arc::new(move || {
                let _ = action_center_events.try_send(ActionCenterEvent::Toggle {
                    display: display_id,
                });
                let _ = action_center_refresh.try_send(RuntimeEvent::Refresh);
            })
        } else {
            Arc::new(|| {})
        };
        let clipboard_events = self.clipboard_events.clone();
        let clipboard_refresh = self.event_sender.clone();
        let display_id = self.display_id;
        input.on_clipboard = if self.clipboard_enabled {
            Arc::new(move || {
                let _ = clipboard_events.try_send(ClipboardEvent::Toggle {
                    display: display_id,
                });
                let _ = clipboard_refresh.try_send(RuntimeEvent::Refresh);
            })
        } else {
            Arc::new(|| {})
        };
        input.clock_time = self.clock_time.clone();
        input.clock_date = self.clock_date.clone();
        let task_windows = self.task_windows.clone();
        let active_tasks = input
            .tasks
            .iter()
            .filter(|task| task.active)
            .map(|task| task.app_id.to_lowercase())
            .collect::<std::collections::HashSet<_>>();
        let catalog = self.source.clone();
        input.on_task = Arc::new(move |app_id| {
            let key = app_id.to_lowercase();
            // QML minimizes the active window here. Until GPUI has the
            // MinimizedWindows provider, leave an already active task alone.
            if active_tasks.contains(&key) {
                return;
            }
            let windows = task_windows.get(&key).cloned().unwrap_or_default();
            if let Some(window_id) = windows.first() {
                if !niri::focus_window(*window_id) {
                    eprintln!("Waffle taskbar: could not focus Niri window {window_id}");
                }
            } else if let Some(catalog) = &catalog {
                if let Err(error) = catalog.spawn_app_id(&app_id) {
                    eprintln!("Waffle taskbar: {error}");
                }
            } else {
                eprintln!("Waffle taskbar: application catalog unavailable for {app_id}");
            }
        });
        input
    }
}

fn configured_pinned_apps(config: &crate::config::ConfigLoad) -> Vec<String> {
    config
        .config
        .appearance
        .source_sections
        .get("ii.dock")
        .and_then(|dock| dock.get("pinnedApps"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn empty_seed() -> WaffleTaskbarInput {
    let noop: ButtonAction = Arc::new(|| {});
    let noop_item: TaskAction = Arc::new(|_| {});
    WaffleTaskbarInput {
        style: preview_palette(),
        start_open: false,
        task_view_open: false,
        start_menu: None,
        tasks: Vec::new(),
        tray_items: Vec::new(),
        clock_time: None,
        clock_date: None,
        unread_notifications: 0,
        notifications_silent: false,
        notifications_open: false,
        action_center_open: false,
        clipboard_open: false,
        on_start: Arc::clone(&noop),
        on_task_view: Arc::clone(&noop),
        on_task: Arc::clone(&noop_item),
        on_tray_item: noop_item,
        on_notifications: noop,
        on_action_center: Arc::new(|| {}),
        on_clipboard: Arc::new(|| {}),
    }
}

impl Render for WaffleRuntime {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = self.taskbar_input();
        div()
            .size_full()
            .child(taskbar::render(input, self.task_scroll.clone(), window, cx))
    }
}

pub(crate) fn taskbar_style(theme: &ThemeTokens) -> taskbar::WaffleTaskbarStyle {
    let fallback = preview_palette();
    taskbar::WaffleTaskbarStyle {
        // Family-resolved Waffle roles take precedence. The shared Material
        // palette fills only roles that the Waffle resolver has not supplied.
        material: theme.material.clone(),
        background: theme.colors.background.unwrap_or_else(|| {
            crate::components::material::surface_color(&theme.material, fallback.background)
        }),
        foreground: theme
            .colors
            .foreground
            .or(theme.material.colors.on_surface)
            .unwrap_or(fallback.foreground),
        muted_foreground: theme
            .colors
            .foreground_muted
            .or(theme.material.colors.on_surface_variant)
            .unwrap_or(fallback.muted_foreground),
        accent: theme
            .colors
            .accent
            .or(theme.material.colors.primary)
            .unwrap_or(fallback.accent),
        active_surface: theme
            .colors
            .selection
            .or(theme.material.colors.surface_container_high)
            .unwrap_or(fallback.active_surface),
        hover_surface: theme
            .colors
            .surface_elevated
            .or(theme.material.colors.surface_container)
            .unwrap_or(fallback.hover_surface),
        border: theme
            .colors
            .border
            .or(theme.material.colors.outline_variant)
            .unwrap_or(fallback.border),
        // WaffleBarContent.qml: Looks.scaledBar(48, panelScreen). The runtime
        // lacks per-output geometry here, so use the unscaled QML base value.
        bar_height: 48.0,
        icon_size: 26.0,
        button_radius: theme.shape.medium,
        horizontal_padding: theme.spacing.sm,
        item_gap: 0.0,
    }
}

pub(crate) fn start_menu_style(
    theme: &ThemeTokens,
    style: &taskbar::WaffleTaskbarStyle,
) -> StartMenuStyle {
    StartMenuStyle {
        material: theme.material.clone(),
        background: style.background,
        surface: theme.colors.surface.unwrap_or(style.active_surface),
        surface_hover: style.hover_surface,
        foreground: style.foreground,
        muted: style.muted_foreground,
        accent: style.accent,
        border: style.border,
        radius: theme.shape.large,
    }
}

fn preview_palette() -> taskbar::WaffleTaskbarStyle {
    let color = |hex| crate::theme::parse_hex_color(hex).expect("valid preview palette color");
    taskbar::WaffleTaskbarStyle {
        material: crate::theme::MaterialThemeContext::default(),
        background: color("#202124"),
        foreground: color("#f1f3f4"),
        muted_foreground: color("#aeb3b8"),
        accent: color("#8ab4f8"),
        active_surface: color("#3c4043"),
        hover_surface: color("#303134"),
        border: color("#45484c"),
        bar_height: 48.0,
        icon_size: 26.0,
        button_radius: 8.0,
        horizontal_padding: 8.0,
        item_gap: 0.0,
    }
}
