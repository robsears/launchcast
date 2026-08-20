//! Hardware-free accumulator for one flight's "highlights" -- the
//! wireless-transferable digest, distinct from (and much smaller than)
//! the full raw log a physical pull retrieves. Orthogonal to
//! [`FlightState`](crate::FlightState): that owns "what state am I in
//! right now," this owns "how did this flight go, start to finish."
//!
//! Fed by `rocket/src/main.rs`'s flight loop at four points:
//!   - [`FlightSummary::on_armed`] once, at IDLE -> ARMED (resets to a
//!     fresh flight, captures the ARM GPS fix).
//!   - [`FlightSummary::on_transition`] on every state change from BOOST
//!     onward -- closes out whichever named phase duration just ended.
//!   - [`FlightSummary::observe`] every loop tick from ARMED onward --
//!     running maxes and the log record count.
//!   - [`FlightSummary::lock_in_landed_fix`] once, but *not* at the
//!     LANDED transition itself -- at RECOVER (see module docs below for
//!     why).
//!
//! **Why the LANDED GPS fix is locked in at RECOVER, not at LANDED**:
//! GPS averaging (`common::fix_average`) only runs while the rocket is
//! classified stationary, and the ring buffer resets the instant that
//! classification flips -- including DESCENT -> LANDED. Snapshotting
//! immediately at that transition would capture an empty-or-just-reset
//! buffer, not a settled position. The rocket is expected to sit in
//! LANDED for however long it takes a person to walk over and confirm
//! recovery -- typically seconds to minutes, comfortably enough for the
//! average to settle -- and RECOVER (the explicit "I have it" signal,
//! see `rocket/src/main.rs`'s DISARM-from-LANDED handling) is exactly
//! the right moment to freeze whatever the average has converged to by
//! then. User call, 2026-08-19.

use launchcast_common::State;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlightSummary {
    /// Start-of-current-phase marker, advanced on every call to
    /// `on_transition` (and set once by `on_armed`) -- not part of the
    /// public/wire data, just bookkeeping for computing each duration as
    /// `now_ms - phase_start_ms` at the moment its phase ends.
    phase_start_ms: u32,

    pub wait_ms: u32,
    pub boost_ms: u32,
    pub coast_ms: u32,
    pub descent_ms: u32,

    pub arm_lat: f32,
    pub arm_lon: f32,
    /// Unix seconds at the moment of ARM, from the caller's own
    /// `common::epoch::EpochOffset` -- `0` if no wall-clock reference
    /// was available yet (no GPS fix since boot). Resolved by the
    /// caller, not computed here: this type stays synchronous/hardware-
    /// free, and reading the epoch offset needs an async mutex lock
    /// (see `rocket/src/gps.rs::EPOCH_OFFSET`).
    pub arm_epoch_s: u32,
    pub landed_lat: f32,
    pub landed_lon: f32,

    pub max_speed_mps: f32,
    pub max_alt_m: f32,
    /// Temperature/pressure at the moment `max_alt_m` was last set, not
    /// independently-tracked extremes of their own -- "what was the
    /// weather like up at apogee," not "what's the single hottest/
    /// highest-pressure instant of the flight." User call, 2026-08-19.
    pub temp_at_max_alt_c: f32,
    pub pressure_at_max_alt_hpa: f32,
    pub max_accel_g: f32,
    pub max_gyro_dps: f32,

    pub record_count: u32,
}

impl Default for FlightSummary {
    fn default() -> Self {
        Self {
            phase_start_ms: 0,
            wait_ms: 0,
            boost_ms: 0,
            coast_ms: 0,
            descent_ms: 0,
            arm_lat: 0.0,
            arm_lon: 0.0,
            arm_epoch_s: 0,
            landed_lat: 0.0,
            landed_lon: 0.0,
            max_speed_mps: 0.0,
            max_alt_m: 0.0,
            temp_at_max_alt_c: 0.0,
            pressure_at_max_alt_hpa: 0.0,
            max_accel_g: 0.0,
            max_gyro_dps: 0.0,
            record_count: 0,
        }
    }
}

impl FlightSummary {
    /// Start tracking a new flight -- call once, at IDLE -> ARMED. Wipes
    /// any previous flight's data (this type only ever represents the
    /// flight currently in progress; `rocket/src/main.rs` is responsible
    /// for archiving a completed one before starting the next).
    pub fn on_armed(now_ms: u32, arm_fix: Option<(f32, f32)>, arm_epoch_s: u32) -> Self {
        let mut s = Self { phase_start_ms: now_ms, arm_epoch_s, ..Self::default() };
        if let Some((lat, lon)) = arm_fix {
            s.arm_lat = lat;
            s.arm_lon = lon;
        }
        s
    }

    /// Call on every state transition from BOOST onward (i.e. every time
    /// `FlightState::update` or an explicit `FlightState::transition`
    /// returns `true` after ARM). Closes out the duration bucket for
    /// whichever named phase just ended; always advances the phase-start
    /// marker regardless of which state was entered, since APOGEE ->
    /// DESCENT needs a fresh marker for `descent_ms` even though the
    /// "time at apogee" dwell itself isn't a field this reports.
    pub fn on_transition(&mut self, new_state: u8, now_ms: u32) {
        let elapsed = now_ms.wrapping_sub(self.phase_start_ms);
        match new_state {
            State::BOOST => self.wait_ms = elapsed,
            State::COAST => self.boost_ms = elapsed,
            State::APOGEE => self.coast_ms = elapsed,
            State::LANDED => self.descent_ms = elapsed,
            _ => {}
        }
        self.phase_start_ms = now_ms;
    }

    /// Call every loop tick from ARMED onward -- same gating as
    /// `rocket/src/main.rs`'s own log-write condition, so `record_count`
    /// stays in lockstep with how many entries actually made it into the
    /// flash log this flight (a cheap, immediate signal for "is a full
    /// pull even going to have much in it"). `alt_m` is tracked here
    /// rather than reusing `FlightState::max_alt_m` directly -- that
    /// field never resets between arm cycles (it's a whole-power-session
    /// max, not a per-flight one, an existing, unrelated behavior this
    /// type doesn't change), so it would silently leak an earlier
    /// flight's peak into this one's summary. `temp_c`/`pressure_hpa`
    /// are whatever the caller's own most recent barometer reading is --
    /// on the rocket, that's always read in the same pass that produces
    /// `alt_m` itself (see `main.rs`'s barometer block), so they're
    /// already correctly paired by the time they get here, not
    /// independently timestamped values that could be stale relative to
    /// each other.
    pub fn observe(&mut self, alt_m: f32, speed_mps: f32, accel_g: f32, gyro_dps: f32, temp_c: f32, pressure_hpa: f32) {
        if alt_m > self.max_alt_m {
            self.max_alt_m = alt_m;
            self.temp_at_max_alt_c = temp_c;
            self.pressure_at_max_alt_hpa = pressure_hpa;
        }
        let speed_mps = libm::fabsf(speed_mps);
        if speed_mps > self.max_speed_mps {
            self.max_speed_mps = speed_mps;
        }
        if accel_g > self.max_accel_g {
            self.max_accel_g = accel_g;
        }
        if gyro_dps > self.max_gyro_dps {
            self.max_gyro_dps = gyro_dps;
        }
        self.record_count += 1;
    }

    /// Freeze the LANDED position -- call once, at RECOVER. See module
    /// docs for why this isn't done at the LANDED transition itself.
    pub fn lock_in_landed_fix(&mut self, lat: f32, lon: f32) {
        self.landed_lat = lat;
        self.landed_lon = lon;
    }
}
