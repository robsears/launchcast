//! Wall-clock reference for a board with no RTC: capture one
//! `(wall_clock_ms, monotonic_ms)` pair from a GPS fix's UTC time, then
//! derive wall-clock time at any later moment from the board's own
//! free-running monotonic clock alone. Shared by both boards (`common`,
//! not either `-logic` crate) since each independently does the exact
//! same thing against its own GPS and its own clock -- there's no
//! cross-board synchronization here, just the same math done twice.
//!
//! Captured once, on the first fix that has both a valid position and a
//! decoded UTC time, and never recomputed after -- an RP2040's clock
//! drift over a bench/field session (minutes to a few hours) is nowhere
//! near enough to matter, so there's no reason to keep resyncing and
//! risk a later noisy fix corrupting an already-good reference. User
//! call, 2026-08-19.

use crate::nmea::{unix_ms, UtcDateTime};

/// A captured wall-clock reference. Opaque -- callers only ever
/// construct one via [`EpochOffset::capture`] and read wall-clock time
/// back out via [`EpochOffset::wall_clock_ms`], never the raw offset
/// itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EpochOffset(i64);

impl EpochOffset {
    /// Capture the offset from a UTC fix observed at `now_ms` on the
    /// caller's own monotonic clock.
    pub fn capture(utc: &UtcDateTime, now_ms: u32) -> Self {
        Self(unix_ms(utc) - now_ms as i64)
    }

    /// Wall-clock time (Unix ms) at `now_ms` on the same monotonic
    /// clock this offset was captured against. Only meaningful for a
    /// `now_ms` from *this* power-on session -- there's no persistence
    /// across a reboot (the monotonic clock resets to 0, and the offset
    /// with it, since it's never stored anywhere but RAM).
    pub fn wall_clock_ms(&self, now_ms: u32) -> i64 {
        self.0 + now_ms as i64
    }
}
