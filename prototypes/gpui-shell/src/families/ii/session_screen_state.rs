//! Pure host lifecycle model for the Material ii session screen.
//!
//! This module tracks visibility, selection, close-timer generations, and
//! warning/capability presentation state. It does not perform session,
//! compositor, lock, or power operations.

use super::session_screen::{SessionAction, SessionScreenEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionScreenVisibility {
    Closed,
    Open,
    Closing { generation: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HibernateAvailability {
    /// Capability detection has not returned; QML keeps Hibernate visible.
    Unknown,
    Available,
    Unavailable,
}

impl HibernateAvailability {
    pub const fn is_visible(self) -> bool {
        !matches!(self, Self::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionScreenWarnings {
    pub package_manager_running: bool,
    pub download_running: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionScreenState {
    visibility: SessionScreenVisibility,
    close_generation: u64,
    selected: SessionAction,
    hibernate: HibernateAvailability,
    warnings: SessionScreenWarnings,
}

impl Default for SessionScreenState {
    fn default() -> Self {
        Self {
            visibility: SessionScreenVisibility::Closed,
            close_generation: 0,
            selected: SessionAction::Lock,
            hibernate: HibernateAvailability::Unknown,
            warnings: SessionScreenWarnings::default(),
        }
    }
}

impl SessionScreenState {
    pub const fn visibility(&self) -> SessionScreenVisibility {
        self.visibility
    }

    /// Monotonically advances whenever an outstanding close timer is created
    /// or invalidated. A timer must present the generation it was issued with.
    pub const fn close_generation(&self) -> u64 {
        self.close_generation
    }

    /// Seed a newly constructed per-output host state from the generation
    /// epoch retained outside a replaceable surface entity.
    pub fn seed_close_generation(&mut self, generation: u64) {
        self.close_generation = self.close_generation.max(generation);
    }

    pub const fn selected(&self) -> SessionAction {
        self.selected
    }

    pub const fn hibernate_availability(&self) -> HibernateAvailability {
        self.hibernate
    }

    pub const fn warnings(&self) -> SessionScreenWarnings {
        self.warnings
    }

    /// Open or reopen the surface. Reopening during the close grace period
    /// invalidates that timer without allowing it to close the new session.
    /// QML focuses Lock whenever the surface becomes visible, so every open
    /// also resets selection to Lock.
    pub fn open(&mut self) {
        if matches!(self.visibility, SessionScreenVisibility::Closing { .. }) {
            self.advance_generation();
        }
        self.visibility = SessionScreenVisibility::Open;
        self.selected = SessionAction::Lock;
    }

    /// Apply a typed view event. Activation is only a request; the host must
    /// acknowledge a real action before beginning the close grace period.
    /// Returns a close generation only when a fresh grace-period timer should
    /// be scheduled.
    pub fn handle_view_event(&mut self, event: SessionScreenEvent) -> Option<u64> {
        match event {
            SessionScreenEvent::Select(action) => {
                if self.is_open() {
                    self.select(action);
                }
                None
            }
            SessionScreenEvent::Dismiss => self.begin_close(),
            SessionScreenEvent::Activate(action) => {
                if self.is_open() && self.action_is_visible(action) {
                    self.selected = action;
                    None
                } else {
                    None
                }
            }
        }
    }

    /// Called only after an injected session-action handler acknowledges the
    /// request. Missing handlers and rejected requests leave the menu open.
    pub fn action_acknowledged(&mut self, action: SessionAction) -> Option<u64> {
        if self.is_open() && self.action_is_visible(action) {
            self.selected = action;
            self.begin_close()
        } else {
            None
        }
    }

    /// Begin the close grace period after the shared host confirms that the
    /// lock became active. A request to lock alone is not confirmation.
    pub fn confirmed_locked(&mut self) -> Option<u64> {
        self.begin_close()
    }

    /// Update capability visibility. Unknown remains visible, matching QML;
    /// only an explicit unavailable result hides Hibernate.
    pub fn set_hibernate_availability(&mut self, availability: HibernateAvailability) {
        self.hibernate = availability;
        self.normalize_selection();
    }

    /// Update both warnings independently so one service can stop while the
    /// other remains active.
    pub fn set_warnings(&mut self, warnings: SessionScreenWarnings) {
        self.warnings = warnings;
    }

    /// Ignore stale timers. Returns true only when this timer closed the
    /// currently closing surface.
    pub fn close_timer_elapsed(&mut self, generation: u64) -> bool {
        if self.visibility == (SessionScreenVisibility::Closing { generation })
            && generation == self.close_generation
        {
            self.visibility = SessionScreenVisibility::Closed;
            true
        } else {
            false
        }
    }

    fn select(&mut self, action: SessionAction) {
        self.selected = if self.action_is_visible(action) {
            action
        } else {
            SessionAction::Lock
        };
    }

    fn normalize_selection(&mut self) {
        if !self.action_is_visible(self.selected) {
            self.selected = SessionAction::Lock;
        }
    }

    fn action_is_visible(&self, action: SessionAction) -> bool {
        action != SessionAction::Hibernate || self.hibernate.is_visible()
    }

    fn is_open(&self) -> bool {
        self.visibility == SessionScreenVisibility::Open
    }

    fn begin_close(&mut self) -> Option<u64> {
        if !self.is_open() {
            return None;
        }
        self.advance_generation();
        self.visibility = SessionScreenVisibility::Closing {
            generation: self.close_generation,
        };
        Some(self.close_generation)
    }

    fn advance_generation(&mut self) {
        self.close_generation = self
            .close_generation
            .checked_add(1)
            .expect("session screen close generation exhausted");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dismiss_and_activate_start_a_close_grace_period() {
        let mut state = SessionScreenState::default();
        state.open();

        let dismiss_generation = state.handle_view_event(SessionScreenEvent::Dismiss);
        assert_eq!(dismiss_generation, Some(1));
        assert_eq!(
            state.visibility(),
            SessionScreenVisibility::Closing { generation: 1 }
        );
        assert!(state.close_timer_elapsed(1));
        assert_eq!(state.visibility(), SessionScreenVisibility::Closed);

        state.open();
        let activate_generation =
            state.handle_view_event(SessionScreenEvent::Activate(SessionAction::Shutdown));
        assert_eq!(activate_generation, None);
        assert_eq!(state.visibility(), SessionScreenVisibility::Open);
        let activate_generation = state.action_acknowledged(SessionAction::Shutdown);
        assert_eq!(activate_generation, Some(2));
        assert_eq!(state.selected(), SessionAction::Shutdown);
    }

    #[test]
    fn reopening_invalidates_old_timer_and_generations_increase() {
        let mut state = SessionScreenState::default();
        state.open();
        state.handle_view_event(SessionScreenEvent::Select(SessionAction::Shutdown));
        let old_timer = state
            .handle_view_event(SessionScreenEvent::Dismiss)
            .expect("open screen starts close timer");

        state.open();
        assert_eq!(state.visibility(), SessionScreenVisibility::Open);
        assert_eq!(state.selected(), SessionAction::Lock);
        assert!(state.close_generation() > old_timer);
        assert!(!state.close_timer_elapsed(old_timer));

        let new_timer = state
            .handle_view_event(SessionScreenEvent::Dismiss)
            .expect("reopened screen can close again");
        assert!(new_timer > old_timer);
        assert!(!state.close_timer_elapsed(old_timer));
        assert!(state.close_timer_elapsed(new_timer));
    }

    #[test]
    fn recreated_surface_keeps_timer_generations_unique() {
        let mut first_surface = SessionScreenState::default();
        first_surface.open();
        let first_timer = first_surface
            .handle_view_event(SessionScreenEvent::Dismiss)
            .expect("first dismiss schedules a timer");

        // Explicit Open replaces the surface and its state object. The
        // per-display epoch must be seeded into the replacement before it is
        // opened, otherwise both outstanding close timers can be generation 1.
        let mut replacement = SessionScreenState::default();
        replacement.seed_close_generation(first_surface.close_generation());
        replacement.open();
        let second_timer = replacement
            .handle_view_event(SessionScreenEvent::Dismiss)
            .expect("replacement dismiss schedules a new timer");

        assert!(second_timer > first_timer);
        assert!(!replacement.close_timer_elapsed(first_timer));
        assert!(replacement.close_timer_elapsed(second_timer));
    }

    #[test]
    fn opening_after_a_completed_close_resets_selection_to_lock() {
        let mut state = SessionScreenState::default();
        state.open();
        state.handle_view_event(SessionScreenEvent::Select(SessionAction::Reboot));
        let generation = state
            .handle_view_event(SessionScreenEvent::Dismiss)
            .expect("open screen starts close timer");
        assert!(state.close_timer_elapsed(generation));

        state.open();
        assert_eq!(state.visibility(), SessionScreenVisibility::Open);
        assert_eq!(state.selected(), SessionAction::Lock);
    }

    #[test]
    fn accepted_lock_request_starts_qml_close_grace() {
        let mut state = SessionScreenState::default();
        state.open();

        let generation = state.action_acknowledged(SessionAction::Lock);
        assert_eq!(generation, Some(1));
        assert_eq!(
            state.visibility(),
            SessionScreenVisibility::Closing { generation: 1 }
        );
    }

    #[test]
    fn confirmed_lock_state_can_start_close_grace() {
        let mut state = SessionScreenState::default();
        state.open();
        assert_eq!(state.confirmed_locked(), Some(1));
        assert_eq!(
            state.visibility(),
            SessionScreenVisibility::Closing { generation: 1 }
        );
    }

    #[test]
    fn hibernate_is_visible_until_explicitly_unavailable_and_selection_falls_back() {
        let mut state = SessionScreenState::default();
        assert!(state.hibernate_availability().is_visible());
        state.open();
        state.handle_view_event(SessionScreenEvent::Select(SessionAction::Hibernate));
        assert_eq!(state.selected(), SessionAction::Hibernate);

        state.set_hibernate_availability(HibernateAvailability::Unavailable);
        assert!(!state.hibernate_availability().is_visible());
        assert_eq!(state.selected(), SessionAction::Lock);
        state.handle_view_event(SessionScreenEvent::Select(SessionAction::Hibernate));
        assert_eq!(state.selected(), SessionAction::Lock);
    }

    #[test]
    fn hidden_hibernate_cannot_be_activated() {
        let mut state = SessionScreenState::default();
        state.open();
        state.set_hibernate_availability(HibernateAvailability::Unavailable);

        assert_eq!(
            state.handle_view_event(SessionScreenEvent::Activate(SessionAction::Hibernate)),
            None
        );
        assert_eq!(state.visibility(), SessionScreenVisibility::Open);
    }

    #[test]
    fn warning_flags_remain_independent() {
        let mut state = SessionScreenState::default();
        state.set_warnings(SessionScreenWarnings {
            package_manager_running: true,
            download_running: false,
        });
        assert_eq!(
            state.warnings(),
            SessionScreenWarnings {
                package_manager_running: true,
                download_running: false,
            }
        );
        state.set_warnings(SessionScreenWarnings {
            package_manager_running: false,
            download_running: true,
        });
        assert_eq!(
            state.warnings(),
            SessionScreenWarnings {
                package_manager_running: false,
                download_running: true,
            }
        );
    }
}
