//! Output-scoped host for the ii Overview prototype.

use std::time::Duration;

use async_channel::Sender;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, Render, Window, div, prelude::*,
    rgba,
};

use crate::{
    families::ii::{
        overview::{
            self, DashboardSection, DataState, OverviewGroup, OverviewInput, OverviewItem,
            OverviewMode,
        },
        panel_surface::IiPanelEvent,
    },
    families::waffle::taskbar::start_menu_adapter::{
        StartMenuDataSource, desktop_entries::XdgDesktopEntrySource,
    },
    families::waffle::taskbar::start_menu_search::StartMenuSearch,
    niri,
    theme::ThemeTokens,
};

pub struct IiOverviewOverlay {
    display: gpui::DisplayId,
    output: Option<String>,
    theme: ThemeTokens,
    events: Sender<IiPanelEvent>,
    focus: FocusHandle,
    mode: OverviewMode,
    query: String,
    launch_error: Option<String>,
    search_editor: Entity<StartMenuSearch>,
    snapshot: niri::Snapshot,
    applications: Option<std::sync::Arc<XdgDesktopEntrySource>>,
    first: bool,
}

impl IiOverviewOverlay {
    pub fn new(
        display: gpui::DisplayId,
        output: Option<String>,
        theme: ThemeTokens,
        events: Sender<IiPanelEvent>,
        cx: &mut Context<Self>,
    ) -> Self {
        let applications = XdgDesktopEntrySource::new().map(std::sync::Arc::new);
        let query_events = events.clone();
        let query_changed: super::overview::StringAction = std::sync::Arc::new(move |query| {
            let _ = query_events.try_send(IiPanelEvent::SetOverviewQuery { display, query });
        });
        let search_editor = cx.new(|cx| {
            StartMenuSearch::new_with_labels(
                String::new(),
                "Search applications".into(),
                "Search applications".into(),
                query_changed,
                cx,
            )
        });
        let view = Self {
            display,
            output,
            theme,
            events,
            focus: cx.focus_handle(),
            mode: OverviewMode::Workspaces,
            query: String::new(),
            launch_error: None,
            search_editor,
            snapshot: niri::Snapshot::default(),
            applications,
            first: true,
        };
        let output = view.output.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let name = output.clone();
                let snapshot = cx
                    .background_executor()
                    .spawn(async move { niri::snapshot(name.as_deref()) })
                    .await;
                if this
                    .update(cx, |view, cx| {
                        view.snapshot = snapshot;
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

    pub fn set_mode(&mut self, mode: OverviewMode, cx: &mut Context<Self>) {
        self.mode = mode;
        cx.notify();
    }

    pub fn view_state(&self) -> (OverviewMode, String) {
        (self.mode, self.query.clone())
    }

    pub fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.query = query.clone();
        self.launch_error = None;
        if let Some(source) = &self.applications {
            source.query_changed(query);
        }
        cx.notify();
    }

    pub fn set_launch_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.launch_error = Some(error);
        cx.notify();
    }

    pub fn launch_application(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(source) = &self.applications else {
            self.set_launch_error("Application launcher is unavailable".into(), cx);
            return;
        };
        let child = match source.spawn_app(&id) {
            Ok(child) => child,
            Err(error) => {
                self.set_launch_error(error, cx);
                return;
            }
        };
        let events = self.events.clone();
        let display = self.display;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut child = child;
                    child.wait()
                })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok(status) if status.success() => {
                    let _ = events.try_send(IiPanelEvent::CloseOverview { display });
                }
                Ok(status) => {
                    view.set_launch_error(format!("Application launcher exited with {status}"), cx)
                }
                Err(error) => {
                    view.set_launch_error(format!("Application launch failed: {error}"), cx)
                }
            });
        })
        .detach();
    }

    fn input(&self) -> OverviewInput {
        let application_data = self
            .applications
            .as_ref()
            .and_then(|source| source.snapshot());
        let applications = application_data
            .as_ref()
            .map(|data| {
                data.results
                    .iter()
                    .map(|application| OverviewItem {
                        id: format!("app:{}", application.id),
                        title: application.name.clone(),
                        subtitle: Some(application.category.clone()),
                        glyph: Some("◉".into()),
                        selected: false,
                        detail: (!application.detail.is_empty())
                            .then(|| application.detail.clone()),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let groups = self
            .snapshot
            .workspaces
            .iter()
            .map(|workspace| {
                let windows = self
                    .snapshot
                    .windows
                    .iter()
                    .filter(|window| window.workspace_id == Some(workspace.id))
                    .map(|window| OverviewItem {
                        id: format!("window:{}", window.id),
                        title: window
                            .title
                            .clone()
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| "Untitled window".into()),
                        subtitle: window.app_id.clone(),
                        glyph: None,
                        selected: window.focused,
                        detail: window.urgent.then(|| "Needs attention".into()),
                    })
                    .collect::<Vec<_>>();
                let mut items = vec![OverviewItem {
                    id: format!("workspace:{}", workspace.id),
                    title: workspace
                        .name
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| format!("Workspace {}", workspace.number)),
                    subtitle: Some(format!("{} window(s)", windows.len())),
                    glyph: Some("▦".into()),
                    selected: workspace.active,
                    detail: None,
                }];
                items.extend(windows);
                OverviewGroup {
                    id: workspace.id.to_string(),
                    title: format!("Workspace {}", workspace.number),
                    items,
                }
            })
            .collect::<Vec<_>>();
        let workspace_numbers = self
            .snapshot
            .workspaces
            .iter()
            .map(|workspace| (workspace.id, workspace.number))
            .collect::<std::collections::HashMap<_, _>>();
        let events = self.events.clone();
        let mode_events = events.clone();
        let application_events = events.clone();
        let display = self.display;
        let colors = &self.theme.colors;
        let foreground = colors.foreground.unwrap_or_else(|| rgba(0xf2f4f8ff).into());
        let muted = colors
            .foreground_muted
            .unwrap_or_else(|| rgba(0xb2bac8ff).into());
        let surface = colors
            .surface_elevated
            .or(colors.surface)
            .unwrap_or_else(|| rgba(0x171a21f5).into());
        let card = colors.surface.unwrap_or_else(|| rgba(0x252a34ff).into());
        let accent = colors.accent.unwrap_or_else(|| rgba(0x84aaffff).into());
        let border = colors.border.unwrap_or_else(|| rgba(0x505969ff).into());
        let state = if self.snapshot.error.is_some() {
            DataState::Unavailable
        } else if groups.is_empty() {
            DataState::Empty
        } else {
            DataState::Ready
        };
        let status = self
            .snapshot
            .error
            .clone()
            .or_else(|| Some("Live workspace and window metadata from Niri".into()));
        let clock_section = match self.snapshot.clock.as_deref() {
            Some(clock) => DashboardSection {
                id: "time-date".into(),
                title: "Time and date".into(),
                state: DataState::Ready,
                status_message: None,
                items: vec![OverviewItem {
                    id: "local-clock".into(),
                    title: clock.to_owned(),
                    subtitle: Some("Local time · system date command".into()),
                    glyph: Some("◷".into()),
                    selected: false,
                    detail: None,
                }],
            },
            None => DashboardSection {
                id: "time-date".into(),
                title: "Time and date".into(),
                state: DataState::Unavailable,
                status_message: Some("The host could not read the local clock".into()),
                items: Vec::new(),
            },
        };
        let unavailable = |id: &str, title: &str, reason: &str| DashboardSection {
            id: id.into(),
            title: title.into(),
            state: DataState::Unavailable,
            status_message: Some(reason.into()),
            items: Vec::new(),
        };
        let dashboard = vec![
            clock_section,
            unavailable(
                "quick-toggles",
                "Quick toggles",
                "Wi-Fi, Bluetooth, DND, appearance, and session controls are not connected",
            ),
            unavailable(
                "sliders",
                "Volume and brightness",
                "Audio and display brightness providers are not connected",
            ),
            unavailable("media", "Media", "No media session provider is connected"),
            unavailable("weather", "Weather", "No weather provider is connected"),
            unavailable(
                "system",
                "System resources",
                "No resource monitor provider is connected",
            ),
        ];
        OverviewInput {
            mode: self.mode,
            state,
            status_message: status,
            query: self.query.clone(),
            groups,
            applications,
            actions: Vec::new(),
            dashboard,
            dashboard_state: DataState::Ready,
            dashboard_status_message: None,
            style: overview::OverviewStyle {
                backdrop: rgba(0x080a0dcc).into(),
                surface,
                card,
                hover: colors.selection.unwrap_or(accent),
                foreground,
                muted,
                accent,
                border,
                radius: self.theme.shape.medium,
                gap: self.theme.spacing.md,
            },
            on_mode: Some(std::sync::Arc::new(move |mode| {
                let _ = mode_events.try_send(IiPanelEvent::SetOverviewMode { display, mode });
            })),
            on_select_item: Some(std::sync::Arc::new(move |id| {
                if let Some(application_id) = id.strip_prefix("app:") {
                    let _ = application_events.try_send(IiPanelEvent::LaunchOverviewApplication {
                        display,
                        application_id: application_id.to_owned(),
                    });
                    return;
                }
                if let Some(raw) = id.strip_prefix("window:").and_then(|s| s.parse().ok()) {
                    let _ = niri::focus_window(raw);
                }
                if let Some(raw) = id
                    .strip_prefix("workspace:")
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    if let Some(number) = workspace_numbers.get(&raw) {
                        let _ = niri::focus_workspace(*number);
                    }
                }
            })),
            on_move_selection: None,
            on_query_change: self.applications.as_ref().map(|_| {
                let events = self.events.clone();
                let display = self.display;
                std::sync::Arc::new(move |query| {
                    let _ = events.try_send(IiPanelEvent::SetOverviewQuery { display, query });
                }) as super::overview::StringAction
            }),
            on_close: Some(std::sync::Arc::new(move || {
                let _ = events.try_send(IiPanelEvent::CloseOverview { display });
            })),
        }
    }
}

impl Focusable for IiOverviewOverlay {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for IiOverviewOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.first {
            window.focus(&self.focus, cx);
            self.first = false;
        }
        let display = self.display;
        let close = self.events.clone();
        let backdrop_close = close.clone();
        let clear_query = close.clone();
        let current_query = self.query.clone();
        let state = if self.snapshot.error.is_some() {
            DataState::Unavailable
        } else if self.snapshot.workspaces.is_empty() {
            DataState::Empty
        } else {
            DataState::Ready
        };
        // Modes without connected providers remain explicit unavailable states.
        let mut input = self.input();
        match self.mode {
            OverviewMode::Applications => match (&self.applications, &input.applications) {
                _ if self.launch_error.is_some() => {
                    input.state = DataState::Error;
                    input.status_message = self.launch_error.clone();
                }
                (None, _) => {
                    input.state = DataState::Unavailable;
                    input.status_message =
                        Some("Application catalog and launcher are unavailable".into());
                }
                (Some(_), apps) if apps.is_empty() => {
                    input.state = DataState::Empty;
                    input.status_message = Some("No desktop applications are visible".into());
                }
                (Some(_), _) => {
                    input.state = DataState::Ready;
                    input.status_message = None;
                }
            },
            OverviewMode::Dashboard => {
                input.state = DataState::Unavailable;
                input.status_message = Some("Dashboard data adapters are not connected".into());
            }
            OverviewMode::Actions => {
                input.state = DataState::Unavailable;
                input.status_message = Some("System action adapters are not connected".into());
            }
            OverviewMode::Workspaces => input.state = state,
        }
        let search_editor = if self.mode == OverviewMode::Applications {
            input.on_query_change.clone().map(|action| {
                self.search_editor.update(cx, |search, cx| {
                    search.sync_controlled_query(self.query.clone(), action, cx);
                });
                self.search_editor.clone().into_any_element()
            })
        } else {
            None
        };
        div()
            .id("ii.overview.overlay")
            .size_full()
            .focusable()
            .tab_stop(true)
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "escape" {
                    if current_query.is_empty() {
                        let _ = close.try_send(IiPanelEvent::CloseOverview { display });
                    } else {
                        let _ = clear_query.try_send(IiPanelEvent::SetOverviewQuery {
                            display,
                            query: String::new(),
                        });
                    }
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .id("ii.overview.backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x05070b99))
                    .on_click(move |_, _, _| {
                        let _ = backdrop_close.try_send(IiPanelEvent::CloseOverview { display });
                    }),
            )
            .child(
                div()
                    .absolute()
                    .inset_8()
                    .child(overview::render(input, search_editor)),
            )
    }
}
