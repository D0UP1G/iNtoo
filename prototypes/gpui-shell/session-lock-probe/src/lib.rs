//! Protocol-independent guards used by the isolated live Wayland probe.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockPhase {
    Idle,
    Requested,
    Locked,
    Denied,
    Released,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockRequest {
    Destroy,
    UnlockAndDestroy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LockFsm {
    phase: LockPhase,
}

impl Default for LockFsm {
    fn default() -> Self {
        Self {
            phase: LockPhase::Idle,
        }
    }
}

impl LockFsm {
    pub fn phase(self) -> LockPhase {
        self.phase
    }

    pub fn request(&mut self) -> Result<(), &'static str> {
        if self.phase != LockPhase::Idle {
            return Err("lock request is only valid from Idle");
        }
        self.phase = LockPhase::Requested;
        Ok(())
    }

    pub fn locked(&mut self) -> Result<(), &'static str> {
        if self.phase != LockPhase::Requested {
            return Err("locked event is only valid after request");
        }
        self.phase = LockPhase::Locked;
        Ok(())
    }

    /// Respond to `finished` according to whether `locked` was observed.
    pub fn finished(&mut self) -> Result<LockRequest, &'static str> {
        match self.phase {
            LockPhase::Requested => {
                self.phase = LockPhase::Denied;
                Ok(LockRequest::Destroy)
            }
            LockPhase::Locked => {
                self.phase = LockPhase::Released;
                Ok(LockRequest::UnlockAndDestroy)
            }
            _ => Err("finished event is invalid in the current phase"),
        }
    }

    pub fn unlock(&mut self) -> Result<LockRequest, &'static str> {
        if self.phase != LockPhase::Locked {
            return Err("unlock requires a received locked event");
        }
        self.phase = LockPhase::Released;
        Ok(LockRequest::UnlockAndDestroy)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Configure {
    pub serial: u32,
    pub width: u32,
    pub height: u32,
}

impl Configure {
    pub fn validate(self) -> Result<usize, &'static str> {
        if self.width == 0 || self.height == 0 {
            return Err("zero-sized lock surface configure");
        }
        let pixels = (self.width as usize)
            .checked_mul(self.height as usize)
            .ok_or("configure size overflow")?;
        // Keep accidental/malicious allocations bounded to 16 megapixels (64 MiB at 4 B/pixel).
        if pixels > 16 * 1024 * 1024 {
            return Err("lock surface exceeds 16 megapixels");
        }
        pixels.checked_mul(4).ok_or("buffer size overflow")
    }

    pub fn accepts_buffer(self, width: u32, height: u32, is_null: bool) -> bool {
        !is_null && self.width == width && self.height == height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_request_uses_destroy_before_locked() {
        let mut fsm = LockFsm::default();
        fsm.request().unwrap();
        assert_eq!(fsm.finished().unwrap(), LockRequest::Destroy);
        assert_eq!(fsm.phase(), LockPhase::Denied);
        assert!(fsm.unlock().is_err());
    }

    #[test]
    fn successful_lock_uses_unlock_and_destroy() {
        let mut fsm = LockFsm::default();
        fsm.request().unwrap();
        fsm.locked().unwrap();
        assert_eq!(fsm.unlock().unwrap(), LockRequest::UnlockAndDestroy);
        assert_eq!(fsm.phase(), LockPhase::Released);
        assert!(fsm.finished().is_err());
    }

    #[test]
    fn finished_after_locked_also_uses_unlock_and_destroy() {
        let mut fsm = LockFsm::default();
        fsm.request().unwrap();
        fsm.locked().unwrap();
        assert_eq!(fsm.finished().unwrap(), LockRequest::UnlockAndDestroy);
    }

    #[test]
    fn configure_rejects_zero_overflow_and_oversized_buffers() {
        assert!(
            Configure {
                serial: 1,
                width: 0,
                height: 10
            }
            .validate()
            .is_err()
        );
        assert!(
            Configure {
                serial: 1,
                width: u32::MAX,
                height: u32::MAX
            }
            .validate()
            .is_err()
        );
        assert!(
            Configure {
                serial: 1,
                width: 8192,
                height: 8192
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn configure_requires_non_null_exact_dimensions() {
        let c = Configure {
            serial: 9,
            width: 1920,
            height: 1080,
        };
        assert!(c.accepts_buffer(1920, 1080, false));
        assert!(!c.accepts_buffer(1920, 1080, true));
        assert!(!c.accepts_buffer(1080, 1920, false));
    }
}
