//! Typed events and per-output state for the manager-owned Waffle launcher.
use gpui::DisplayId;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartMenuEvent {
    Toggle { display: DisplayId },
    Close { display: DisplayId },
    Refresh { display: DisplayId },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartMenuState {
    #[default]
    Closed,
    Open {
        restore_focus_to_start: bool,
    },
}

#[derive(Default)]
pub struct StartMenuController {
    outputs: HashMap<DisplayId, StartMenuState>,
}

impl StartMenuController {
    pub fn state(&self, display: DisplayId) -> StartMenuState {
        self.outputs.get(&display).copied().unwrap_or_default()
    }

    pub fn toggle(&mut self, display: DisplayId) -> StartMenuState {
        let next = match self.state(display) {
            StartMenuState::Closed => StartMenuState::Open {
                restore_focus_to_start: true,
            },
            StartMenuState::Open { .. } => StartMenuState::Closed,
        };
        self.outputs.insert(display, next);
        next
    }

    pub fn close(&mut self, display: DisplayId) -> StartMenuState {
        self.outputs.insert(display, StartMenuState::Closed);
        StartMenuState::Closed
    }

    pub fn remove(&mut self, display: DisplayId) {
        self.outputs.remove(&display);
    }
}

/// Per-output lifecycle events for the Waffle Notification Center surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationCenterEvent {
    Toggle { display: DisplayId },
    Close { display: DisplayId },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotificationCenterState {
    #[default]
    Closed,
    Open,
}

#[derive(Default)]
pub struct NotificationCenterController {
    outputs: HashMap<DisplayId, NotificationCenterState>,
}

impl NotificationCenterController {
    pub fn state(&self, display: DisplayId) -> NotificationCenterState {
        self.outputs.get(&display).copied().unwrap_or_default()
    }

    pub fn toggle(&mut self, display: DisplayId) -> NotificationCenterState {
        let next = match self.state(display) {
            NotificationCenterState::Closed => NotificationCenterState::Open,
            NotificationCenterState::Open => NotificationCenterState::Closed,
        };
        self.outputs.insert(display, next);
        next
    }

    pub fn close(&mut self, display: DisplayId) {
        self.outputs
            .insert(display, NotificationCenterState::Closed);
    }

    pub fn remove(&mut self, display: DisplayId) {
        self.outputs.remove(&display);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskViewEvent {
    Toggle { display: DisplayId },
    Close { display: DisplayId },
    SelectWorkspace { display: DisplayId, number: u8 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskViewState {
    #[default]
    Closed,
    Open,
}

#[derive(Default)]
pub struct TaskViewController {
    outputs: HashMap<DisplayId, TaskViewState>,
    selected_workspaces: HashMap<DisplayId, u8>,
}

impl TaskViewController {
    pub fn state(&self, display: DisplayId) -> TaskViewState {
        self.outputs.get(&display).copied().unwrap_or_default()
    }

    pub fn toggle(&mut self, display: DisplayId) -> TaskViewState {
        let next = match self.state(display) {
            TaskViewState::Closed => TaskViewState::Open,
            TaskViewState::Open => TaskViewState::Closed,
        };
        self.outputs.insert(display, next);
        next
    }

    pub fn close(&mut self, display: DisplayId) {
        self.outputs.insert(display, TaskViewState::Closed);
    }

    pub fn selected_workspace(&self, display: DisplayId) -> Option<u8> {
        self.selected_workspaces.get(&display).copied()
    }

    pub fn select_workspace(&mut self, display: DisplayId, number: u8) {
        self.selected_workspaces.insert(display, number);
    }

    pub fn remove(&mut self, display: DisplayId) {
        self.outputs.remove(&display);
        self.selected_workspaces.remove(&display);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionCenterEvent {
    Toggle { display: DisplayId },
    Close { display: DisplayId },
    SelectPage { display: DisplayId, page: usize },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActionCenterState {
    #[default]
    Closed,
    Open,
}

#[derive(Default)]
pub struct ActionCenterController {
    outputs: HashMap<DisplayId, ActionCenterState>,
    pages: HashMap<DisplayId, usize>,
}

impl ActionCenterController {
    pub fn state(&self, display: DisplayId) -> ActionCenterState {
        self.outputs.get(&display).copied().unwrap_or_default()
    }

    pub fn toggle(&mut self, display: DisplayId) -> ActionCenterState {
        let state = match self.state(display) {
            ActionCenterState::Closed => ActionCenterState::Open,
            ActionCenterState::Open => ActionCenterState::Closed,
        };
        self.outputs.insert(display, state);
        state
    }

    pub fn close(&mut self, display: DisplayId) {
        self.outputs.insert(display, ActionCenterState::Closed);
    }

    pub fn page(&self, display: DisplayId) -> usize {
        self.pages.get(&display).copied().unwrap_or(0)
    }

    pub fn select_page(&mut self, display: DisplayId, page: usize) {
        self.pages.insert(display, page);
    }

    pub fn remove(&mut self, display: DisplayId) {
        self.outputs.remove(&display);
        self.pages.remove(&display);
    }
}

/// Per-output lifecycle events for the Waffle clipboard history surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardEvent {
    Toggle { display: DisplayId },
    Close { display: DisplayId },
    PinsChanged { pins: Vec<String> },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipboardOverlayState {
    #[default]
    Closed,
    Open,
}

#[derive(Default)]
pub struct ClipboardController {
    outputs: HashMap<DisplayId, ClipboardOverlayState>,
    pins: Vec<String>,
}

impl ClipboardController {
    pub fn state(&self, display: DisplayId) -> ClipboardOverlayState {
        self.outputs.get(&display).copied().unwrap_or_default()
    }

    pub fn toggle(&mut self, display: DisplayId) -> ClipboardOverlayState {
        let next = match self.state(display) {
            ClipboardOverlayState::Closed => ClipboardOverlayState::Open,
            ClipboardOverlayState::Open => ClipboardOverlayState::Closed,
        };
        self.outputs.insert(display, next);
        next
    }

    pub fn close(&mut self, display: DisplayId) {
        self.outputs.insert(display, ClipboardOverlayState::Closed);
    }

    pub fn remove(&mut self, display: DisplayId) {
        self.outputs.remove(&display);
    }

    pub fn pins(&self) -> &[String] {
        &self.pins
    }

    pub fn set_pins(&mut self, pins: Vec<String>) {
        self.pins = pins;
    }
}
