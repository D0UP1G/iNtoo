//! Output-sized GPUI host for Waffle Task View.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_channel::Sender;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, Render, Window, div, prelude::*,
};

use crate::{niri, theme::ThemeTokens};

use super::{
    controller::TaskViewEvent,
    runtime::{start_menu_style, taskbar_style},
    task_view::{
        TaskViewInput, TaskViewLoadState, TaskViewMode, TaskViewStyle, TaskViewWindow,
        TaskViewWorkspace, WaffleTaskView,
    },
    taskbar::start_menu_search::StartMenuSearch,
};

pub struct WaffleTaskViewOverlay {
    task_view: Entity<WaffleTaskView>,
    search_editor: Entity<StartMenuSearch>,
    query: Arc<Mutex<String>>,
    query_changed: Arc<dyn Fn(String) + Send + Sync>,
    theme: ThemeTokens,
    output_name: Option<String>,
    display_id: gpui::DisplayId,
    mode: TaskViewMode,
    close_on_select: bool,
    selected_workspace: Option<u8>,
    snapshot: niri::Snapshot,
    events: Sender<TaskViewEvent>,
    on_close: Arc<dyn Fn() + Send + Sync>,
    focus_on_first_render: bool,
}

impl WaffleTaskViewOverlay {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        theme: ThemeTokens,
        output_name: Option<String>,
        display_id: gpui::DisplayId,
        mode: TaskViewMode,
        close_on_select: bool,
        selected_workspace: Option<u8>,
        events: Sender<TaskViewEvent>,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = Arc::new(Mutex::new(String::new()));
        let query_changed = query_action(&query);
        let search_editor = cx.new(|cx| {
            StartMenuSearch::new_with_labels(
                String::new(),
                "Search windows".to_owned(),
                "Search windows".to_owned(),
                query_changed.clone(),
                cx,
            )
        });
        let snapshot = niri::Snapshot::default();
        let on_close = close_action(events.clone(), display_id);
        let input = make_input(
            &theme,
            mode,
            close_on_select,
            selected_workspace,
            display_id,
            events.clone(),
            snapshot.clone(),
            TaskViewLoadState::Loading,
            String::new(),
            Some(search_editor.clone()),
            Arc::clone(&query_changed),
            Arc::clone(&on_close),
        );
        let task_view = cx.new(|cx| WaffleTaskView::new(input, cx));
        let overlay = Self {
            task_view,
            search_editor,
            query,
            query_changed,
            theme,
            output_name,
            display_id,
            mode,
            close_on_select,
            selected_workspace,
            snapshot,
            events,
            on_close,
            focus_on_first_render: true,
        };
        cx.spawn(async move |this, cx| {
            loop {
                let output = this
                    .update(cx, |view, _| view.output_name.clone())
                    .unwrap_or_default();
                let snapshot = cx
                    .background_executor()
                    .spawn(async move { niri::snapshot(output.as_deref()) })
                    .await;
                let update = this.update(cx, |view, cx| {
                    view.snapshot = snapshot;
                    view.publish_input(cx, TaskViewLoadState::Ready);
                });
                if update.is_err() {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(900))
                    .await;
            }
        })
        .detach();
        overlay
    }

    pub fn select_workspace(&mut self, number: u8, cx: &mut Context<Self>) {
        self.selected_workspace = Some(number);
        self.publish_input(cx, TaskViewLoadState::Ready);
    }

    fn publish_input(&mut self, cx: &mut Context<Self>, load_state: TaskViewLoadState) {
        let query = self
            .query
            .lock()
            .map(|query| query.clone())
            .unwrap_or_default();
        let input = make_input(
            &self.theme,
            self.mode,
            self.close_on_select,
            self.selected_workspace,
            self.display_id,
            self.events.clone(),
            self.snapshot.clone(),
            load_state,
            query,
            Some(self.search_editor.clone()),
            Arc::clone(&self.query_changed),
            Arc::clone(&self.on_close),
        );
        self.task_view
            .update(cx, |task_view, cx| task_view.set_input(input, cx));
        cx.notify();
    }
}

impl Focusable for WaffleTaskViewOverlay {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search_editor.focus_handle(cx)
    }
}

impl Render for WaffleTaskViewOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_on_first_render {
            window.focus(&self.search_editor.focus_handle(cx), cx);
            self.focus_on_first_render = false;
        }
        let query = self
            .query
            .lock()
            .map(|query| query.clone())
            .unwrap_or_default();
        let query_changed = Arc::clone(&self.query_changed);
        self.search_editor.update(cx, |search, cx| {
            search.sync_controlled_query(query, query_changed, cx);
        });
        div()
            .id("waffle.task-view.overlay")
            .size_full()
            .child(self.task_view.clone())
    }
}

fn close_action(
    events: Sender<TaskViewEvent>,
    display: gpui::DisplayId,
) -> Arc<dyn Fn() + Send + Sync> {
    Arc::new(move || {
        let _ = events.try_send(TaskViewEvent::Close { display });
    })
}

fn query_action(query: &Arc<Mutex<String>>) -> Arc<dyn Fn(String) + Send + Sync> {
    let query = Arc::clone(query);
    Arc::new(move |value| {
        if let Ok(mut current) = query.lock() {
            *current = value;
        }
    })
}

fn task_view_style(theme: &ThemeTokens) -> TaskViewStyle {
    let bar = taskbar_style(theme);
    let menu = start_menu_style(theme, &bar);
    TaskViewStyle {
        backdrop: menu.background.opacity(0.88),
        panel: menu.background,
        card: menu.surface,
        card_hover: menu.surface_hover,
        foreground: menu.foreground,
        muted: menu.muted,
        accent: menu.accent,
        border: menu.border,
        radius: menu.radius,
        scale: 1.,
    }
}

#[allow(clippy::too_many_arguments)]
fn make_input(
    theme: &ThemeTokens,
    mode: TaskViewMode,
    close_on_select: bool,
    selected_workspace: Option<u8>,
    display_id: gpui::DisplayId,
    events: Sender<TaskViewEvent>,
    snapshot: niri::Snapshot,
    load_state: TaskViewLoadState,
    query: String,
    search_editor: Option<Entity<StartMenuSearch>>,
    on_query_change: Arc<dyn Fn(String) + Send + Sync>,
    on_close: Arc<dyn Fn() + Send + Sync>,
) -> TaskViewInput {
    let task_view_style = task_view_style(theme);
    let workspaces = snapshot
        .workspaces
        .iter()
        .map(|workspace| TaskViewWorkspace {
            number: workspace.number,
            name: workspace.name.clone(),
            active: workspace.active,
            windows: snapshot
                .windows
                .iter()
                .filter(|window| window.workspace_id == Some(workspace.id))
                .map(|window| TaskViewWindow {
                    id: window.id,
                    title: window.title.clone(),
                    app_id: window.app_id.clone(),
                    focused: window.focused,
                    urgent: window.urgent,
                })
                .collect(),
        })
        .collect();
    let select_events = events;
    TaskViewInput {
        style: task_view_style,
        mode,
        load_state: if snapshot.error.is_some() {
            TaskViewLoadState::Unavailable
        } else {
            load_state
        },
        workspaces,
        selected_workspace,
        close_on_select,
        query,
        search_editor,
        on_select_workspace: Arc::new(move |number| {
            let _ = select_events.try_send(TaskViewEvent::SelectWorkspace {
                display: display_id,
                number,
            });
        }),
        on_switch_workspace: Arc::new(|number| {
            let _ = niri::focus_workspace(number);
        }),
        on_focus_window: Arc::new(|id| {
            let _ = niri::focus_window(id);
        }),
        on_close_window: Arc::new(|id| {
            let _ = niri::close_window(id);
        }),
        on_query_change,
        on_close,
    }
}
