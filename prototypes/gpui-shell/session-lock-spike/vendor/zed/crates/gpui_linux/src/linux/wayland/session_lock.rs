//! Protocol role and ordering state for GPUI's Wayland session-lock path.
//!
//! This module is intentionally not wired into `WaylandClient::open_window` yet.
//! Its callers must keep `WindowKind::SessionLock` fail-closed until GPUI can
//! defer its synchronous first draw until after a lock-surface configure.

use std::collections::HashMap;

use gpui::{Pixels, Size, px};
use wayland_client::{
    Dispatch, Proxy, QueueHandle,
    backend::ObjectId,
    globals::{BindError, GlobalList},
    protocol::{wl_output::WlOutput, wl_surface::WlSurface},
};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, ext_session_lock_v1::ExtSessionLockV1,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLockPhase {
    Idle,
    Requested,
    Locked,
    UnlockRequested,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLockStateError {
    AlreadyRequested,
    NotRequested,
    NotLocked,
    Finished,
    UnlockRequested,
    InvalidConfigure,
    ConfigureRequired,
    ConfigureSizeMismatch,
    NullFirstBuffer,
    DuplicateOutput,
    UnknownSurface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishedAction {
    Destroy,
    UnlockAndDestroy,
    AlreadyUnlockRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLockState {
    phase: SessionLockPhase,
}

impl Default for SessionLockState {
    fn default() -> Self {
        Self {
            phase: SessionLockPhase::Idle,
        }
    }
}

impl SessionLockState {
    pub fn phase(self) -> SessionLockPhase {
        self.phase
    }

    pub fn request(&mut self) -> Result<(), SessionLockStateError> {
        if self.phase != SessionLockPhase::Idle {
            return Err(SessionLockStateError::AlreadyRequested);
        }
        self.phase = SessionLockPhase::Requested;
        Ok(())
    }

    pub fn locked(&mut self) -> Result<(), SessionLockStateError> {
        if self.phase != SessionLockPhase::Requested {
            return Err(match self.phase {
                SessionLockPhase::Finished => SessionLockStateError::Finished,
                SessionLockPhase::UnlockRequested => SessionLockStateError::UnlockRequested,
                SessionLockPhase::Idle | SessionLockPhase::Locked => {
                    SessionLockStateError::NotRequested
                }
                SessionLockPhase::Requested => unreachable!(),
            });
        }
        self.phase = SessionLockPhase::Locked;
        Ok(())
    }

    pub fn finished(&mut self) -> Result<FinishedAction, SessionLockStateError> {
        let action = match self.phase {
            SessionLockPhase::Requested => FinishedAction::Destroy,
            SessionLockPhase::Locked => FinishedAction::UnlockAndDestroy,
            SessionLockPhase::Idle => return Err(SessionLockStateError::NotRequested),
            SessionLockPhase::UnlockRequested => FinishedAction::AlreadyUnlockRequested,
            SessionLockPhase::Finished => return Err(SessionLockStateError::Finished),
        };
        self.phase = SessionLockPhase::Finished;
        Ok(action)
    }

    pub fn unlock(&mut self) -> Result<(), SessionLockStateError> {
        if self.phase != SessionLockPhase::Locked {
            return Err(match self.phase {
                SessionLockPhase::Finished => SessionLockStateError::Finished,
                SessionLockPhase::UnlockRequested => SessionLockStateError::UnlockRequested,
                SessionLockPhase::Idle | SessionLockPhase::Requested => {
                    SessionLockStateError::NotLocked
                }
                SessionLockPhase::Locked => unreachable!(),
            });
        }
        self.phase = SessionLockPhase::UnlockRequested;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LockSurfacePhase {
    AwaitingConfigure,
    ReadyToCommit { size: Size<Pixels> },
    Mapped { size: Size<Pixels> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LockSurfaceState {
    phase: LockSurfacePhase,
}

impl Default for LockSurfaceState {
    fn default() -> Self {
        Self {
            phase: LockSurfacePhase::AwaitingConfigure,
        }
    }
}

impl LockSurfaceState {
    pub fn phase(self) -> LockSurfacePhase {
        self.phase
    }

    pub fn configured(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<Size<Pixels>, SessionLockStateError> {
        if width == 0 || height == 0 {
            return Err(SessionLockStateError::InvalidConfigure);
        }
        let size = Size {
            width: px(width as f32),
            height: px(height as f32),
        };
        self.phase = LockSurfacePhase::ReadyToCommit { size };
        Ok(size)
    }

    pub fn first_buffer_committed(
        &mut self,
        size: Size<Pixels>,
        non_null: bool,
    ) -> Result<(), SessionLockStateError> {
        let LockSurfacePhase::ReadyToCommit { size: configured } = self.phase else {
            return Err(match self.phase {
                LockSurfacePhase::AwaitingConfigure => SessionLockStateError::ConfigureRequired,
                LockSurfacePhase::ReadyToCommit { .. } | LockSurfacePhase::Mapped { .. } => {
                    SessionLockStateError::ConfigureSizeMismatch
                }
            });
        };
        if size != configured {
            return Err(SessionLockStateError::ConfigureSizeMismatch);
        }
        if !non_null {
            return Err(SessionLockStateError::NullFirstBuffer);
        }
        self.phase = LockSurfacePhase::Mapped { size };
        Ok(())
    }
}

struct LockSurfaceRole {
    output_id: ObjectId,
    proxy: ExtSessionLockSurfaceV1,
    state: LockSurfaceState,
}

/// A protocol-only ext-session-lock role. It is not yet connected to GPUI windows.
pub struct SessionLockProtocol {
    manager_global_name: u32,
    manager: ExtSessionLockManagerV1,
    lock: Option<ExtSessionLockV1>,
    state: SessionLockState,
    surfaces: HashMap<ObjectId, LockSurfaceRole>,
}

impl SessionLockProtocol {
    /// Binds the optional v1 manager. `BindError::NotPresent` means the
    /// compositor does not advertise ext-session-lock-v1.
    pub fn bind<State>(globals: &GlobalList, qh: &QueueHandle<State>) -> Result<Self, BindError>
    where
        State: Dispatch<ExtSessionLockManagerV1, ()> + 'static,
    {
        let manager_global_name = globals.contents().with_list(|globals| {
            globals
                .iter()
                .find(|global| global.interface == "ext_session_lock_manager_v1")
                .map(|global| global.name)
        });
        let manager = globals.bind::<ExtSessionLockManagerV1, _, _>(qh, 1..=1, ())?;
        let manager_global_name = manager_global_name.ok_or(BindError::NotPresent)?;
        Ok(Self {
            manager_global_name,
            manager,
            lock: None,
            state: SessionLockState::default(),
            surfaces: HashMap::new(),
        })
    }

    pub fn manager_global_name(&self) -> u32 {
        self.manager_global_name
    }

    pub fn phase(&self) -> SessionLockPhase {
        self.state.phase()
    }

    /// Requests the single session lock owned by this protocol role.
    pub fn request_lock<State>(
        &mut self,
        qh: &QueueHandle<State>,
    ) -> Result<(), SessionLockStateError>
    where
        State: Dispatch<ExtSessionLockV1, ()> + 'static,
    {
        self.state.request()?;
        self.lock = Some(self.manager.lock(qh, ()));
        Ok(())
    }

    /// Creates one lock-surface role for an output. The supplied `wl_surface`
    /// must be fresh: it must have no other role and must not have been
    /// committed or assigned a buffer before this call.
    pub fn add_output<State>(
        &mut self,
        surface: &WlSurface,
        output: &WlOutput,
        qh: &QueueHandle<State>,
    ) -> Result<ObjectId, SessionLockStateError>
    where
        State: Dispatch<ExtSessionLockSurfaceV1, ObjectId> + 'static,
    {
        let output_id = output.id();
        if self
            .surfaces
            .values()
            .any(|existing| existing.output_id == output_id)
        {
            return Err(SessionLockStateError::DuplicateOutput);
        }
        let lock = self
            .lock
            .as_ref()
            .ok_or(SessionLockStateError::NotRequested)?;
        let surface_id = surface.id();
        let proxy = lock.get_lock_surface(surface, output, qh, surface_id.clone());
        self.surfaces.insert(
            surface_id.clone(),
            LockSurfaceRole {
                output_id,
                proxy,
                state: LockSurfaceState::default(),
            },
        );
        Ok(surface_id)
    }

    /// Acknowledges this configure before the caller schedules a render/commit.
    pub fn configure(
        &mut self,
        surface_id: &ObjectId,
        serial: u32,
        width: u32,
        height: u32,
    ) -> Result<Size<Pixels>, SessionLockStateError> {
        let role = self
            .surfaces
            .get_mut(surface_id)
            .ok_or(SessionLockStateError::UnknownSurface)?;
        let size = role.state.configured(width, height)?;
        role.proxy.ack_configure(serial);
        Ok(size)
    }

    /// Records a successful GPUI commit after the caller has attached a non-null buffer.
    pub fn first_buffer_committed(
        &mut self,
        surface_id: &ObjectId,
        size: Size<Pixels>,
        non_null: bool,
    ) -> Result<(), SessionLockStateError> {
        let role = self
            .surfaces
            .get_mut(surface_id)
            .ok_or(SessionLockStateError::UnknownSurface)?;
        role.state.first_buffer_committed(size, non_null)
    }

    /// Destroys the role associated with a removed output.
    pub fn remove_output(&mut self, output_id: &ObjectId) -> bool {
        let surface_id = self.surfaces.iter().find_map(|(surface_id, role)| {
            (&role.output_id == output_id).then(|| surface_id.clone())
        });
        if let Some(surface_id) = surface_id {
            if let Some(role) = self.surfaces.remove(&surface_id) {
                role.proxy.destroy();
            }
            true
        } else {
            false
        }
    }

    /// Applies the compositor's `locked` event. The caller must schedule
    /// configured lock-surface frames before treating the lock as established.
    pub fn locked(&mut self) -> Result<(), SessionLockStateError> {
        self.state.locked()
    }

    /// Sends `unlock_and_destroy` only after a `locked` event was observed.
    pub fn unlock_and_destroy(&mut self) -> Result<(), SessionLockStateError> {
        self.state.unlock()?;
        if let Some(lock) = self.lock.take() {
            lock.unlock_and_destroy();
        }
        self.destroy_surfaces();
        Ok(())
    }

    /// Handles compositor `finished`, selecting the only legal destructor for
    /// the current protocol state. If an earlier client unlock request is
    /// already in flight, this only records the terminal event and does not
    /// send a second destructor.
    pub fn finished(&mut self) -> Result<FinishedAction, SessionLockStateError> {
        let action = self.state.finished()?;
        if let Some(lock) = self.lock.take() {
            match action {
                FinishedAction::Destroy => lock.destroy(),
                FinishedAction::UnlockAndDestroy => lock.unlock_and_destroy(),
                FinishedAction::AlreadyUnlockRequested => {}
            }
        }
        self.destroy_surfaces();
        Ok(action)
    }

    fn destroy_surfaces(&mut self) {
        for (_, role) in self.surfaces.drain() {
            role.proxy.destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_surface_requires_acknowledged_configure_size_before_non_null_first_buffer() {
        let configured_size = Size {
            width: px(1920.0),
            height: px(1080.0),
        };
        let mut surface = LockSurfaceState::default();

        assert_eq!(
            surface.first_buffer_committed(configured_size, true),
            Err(SessionLockStateError::ConfigureRequired)
        );
        assert_eq!(
            surface.configured(0, 1080),
            Err(SessionLockStateError::InvalidConfigure)
        );
        assert_eq!(surface.configured(1920, 1080), Ok(configured_size));
        assert_eq!(
            surface.first_buffer_committed(
                Size {
                    width: px(1280.0),
                    height: px(720.0),
                },
                true,
            ),
            Err(SessionLockStateError::ConfigureSizeMismatch)
        );
        assert_eq!(
            surface.first_buffer_committed(configured_size, false),
            Err(SessionLockStateError::NullFirstBuffer)
        );
        assert_eq!(
            surface.first_buffer_committed(configured_size, true),
            Ok(())
        );
        assert_eq!(
            surface.phase(),
            LockSurfacePhase::Mapped {
                size: configured_size
            }
        );
    }

    #[test]
    fn release_is_gated_by_the_locked_event() {
        let mut lock = SessionLockState::default();
        assert_eq!(lock.unlock(), Err(SessionLockStateError::NotLocked));
        assert_eq!(lock.request(), Ok(()));
        assert_eq!(
            lock.finished(),
            Ok(FinishedAction::Destroy),
            "finished before locked must use destroy"
        );
        assert_eq!(lock.unlock(), Err(SessionLockStateError::Finished));
    }

    #[test]
    fn locked_event_enables_unlock_and_finished_requires_unlock_and_destroy() {
        let mut lock = SessionLockState::default();
        lock.request().unwrap();
        lock.locked().unwrap();
        assert_eq!(
            lock.finished(),
            Ok(FinishedAction::UnlockAndDestroy),
            "finished after locked must use unlock_and_destroy"
        );
        assert_eq!(lock.phase(), SessionLockPhase::Finished);
        assert_eq!(lock.unlock(), Err(SessionLockStateError::Finished));
        assert_eq!(lock.finished(), Err(SessionLockStateError::Finished));

        let mut unlock_request = SessionLockState::default();
        unlock_request.request().unwrap();
        unlock_request.locked().unwrap();
        unlock_request.unlock().unwrap();
        assert_eq!(unlock_request.phase(), SessionLockPhase::UnlockRequested);
        assert_eq!(
            unlock_request.unlock(),
            Err(SessionLockStateError::UnlockRequested)
        );
        assert_eq!(
            unlock_request.finished(),
            Ok(FinishedAction::AlreadyUnlockRequested)
        );
        assert_eq!(unlock_request.phase(), SessionLockPhase::Finished);
        assert_eq!(
            unlock_request.finished(),
            Err(SessionLockStateError::Finished)
        );
    }

    #[test]
    fn duplicate_and_out_of_order_lock_events_are_errors() {
        let mut lock = SessionLockState::default();
        assert_eq!(lock.locked(), Err(SessionLockStateError::NotRequested));
        lock.request().unwrap();
        assert_eq!(lock.request(), Err(SessionLockStateError::AlreadyRequested));
        lock.locked().unwrap();
        assert_eq!(lock.locked(), Err(SessionLockStateError::NotRequested));
        assert_eq!(lock.unlock(), Ok(()));
        assert_eq!(lock.phase(), SessionLockPhase::UnlockRequested);
        assert_eq!(lock.unlock(), Err(SessionLockStateError::UnlockRequested));
        assert_eq!(lock.finished(), Ok(FinishedAction::AlreadyUnlockRequested));
        assert_eq!(lock.phase(), SessionLockPhase::Finished);
        assert_eq!(lock.finished(), Err(SessionLockStateError::Finished));
    }
}
