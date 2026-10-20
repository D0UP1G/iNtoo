//! GPUI entity for the Material ii session screen. The surface manager owns
//! compositor routing and feeds state changes into this view.

use async_channel::Sender;
use gpui::{Context, FocusHandle, Focusable, IntoElement, Render, Window};

use super::{
    session_screen::{self, SessionAction, SessionScreenInput, SessionScreenLabels},
    session_screen_state::{HibernateAvailability, SessionScreenState},
};
use crate::theme::ThemeTokens;

pub struct SessionScreenHost {
    input: SessionScreenInput,
    focus: FocusHandle,
    focused_once: bool,
}

impl SessionScreenHost {
    pub fn new(
        theme: ThemeTokens,
        events: Sender<crate::surface_manager::IiSessionScreenEvent>,
        display: gpui::DisplayId,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let sender = events.clone();
        Self {
            input: SessionScreenInput {
                labels: SessionScreenLabels {
                    title: "Session".into(),
                    instructions:
                        "Arrow keys to navigate, Enter to select\nEsc or click anywhere to cancel"
                            .into(),
                    session_action_description: "Choose a session action".into(),
                    selected_session_action_description: "Selected action".into(),
                    lock: "Lock".into(),
                    sleep: "Sleep".into(),
                    logout: "Logout".into(),
                    task_manager: "Task Manager".into(),
                    hibernate: "Hibernate".into(),
                    shutdown: "Shutdown".into(),
                    reboot: "Reboot".into(),
                    firmware_reboot: "Reboot to firmware settings".into(),
                    package_manager_warning: "Your package manager is running".into(),
                    download_warning: "There might be a download in progress".into(),
                },
                selected: SessionAction::Lock,
                show_hibernate: HibernateAvailability::Unknown.is_visible(),
                package_manager_running: false,
                download_running: false,
                closing: false,
                editorial_style: false,
                zzz_style: false,
                wallpaper_preview: None,
                theme: theme.material,
                focus: focus.clone(),
                on_event: std::sync::Arc::new(move |event| {
                    let _ = sender.try_send(crate::surface_manager::IiSessionScreenEvent::View {
                        display,
                        event,
                    });
                }),
            },
            focus,
            focused_once: false,
        }
    }

    pub fn set_state(
        &mut self,
        state: &SessionScreenState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input.selected = state.selected();
        self.input.show_hibernate = state.hibernate_availability().is_visible();
        let warnings = state.warnings();
        self.input.package_manager_running = warnings.package_manager_running;
        self.input.download_running = warnings.download_running;
        self.input.closing = matches!(
            state.visibility(),
            super::session_screen_state::SessionScreenVisibility::Closing { .. }
        );
        if !self.focused_once {
            window.focus(&self.focus, cx);
            self.focused_once = true;
        }
        cx.notify();
    }
}

impl Focusable for SessionScreenHost {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SessionScreenHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        session_screen::render(self.input.clone())
    }
}
