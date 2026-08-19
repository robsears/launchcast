//! Rolling mean of the most recent [`WINDOW_SAMPLES`] GPS lat/lon
//! samples, via a fixed-capacity ring buffer -- not an unbounded running
//! sum, and not a periodic snapshot-and-reset window either (an earlier
//! version of this module worked that way; see git history). Hardware-
//! free -- *whether* to feed a sample in at all right now is each
//! board's own GPS module's call (`ground/src/gps.rs`, `rocket/src/
//! gps.rs`, which has flight-phase knowledge the rocket needs), this is
//! just the buffer.
//!
//! **2026-08-19, real hardware**: the previous design (an unbounded sum,
//! snapshotted and reset every `FIX_AVERAGE_WINDOW_MS`) settled to a
//! stable reading, but took a few real-world minutes to do it after a
//! cold start. The averaging *window* itself wasn't actually the cause
//! (each window only ever held a few seconds of samples before
//! resetting -- there was no code path where an early bad reading could
//! literally persist for minutes), but a fixed few-second window still
//! forces a full window's worth of latency before a newly-accurate GPS
//! reading is even reflected at all, on top of whatever the GPS chip
//! itself needs to converge (WAAS/SBAS lock and ephemeris acquisition
//! commonly do take a couple of minutes after cold start, independent of
//! any software averaging). A ring buffer publishes a continuously
//! updated mean instead: each new sample evicts the single oldest one
//! immediately, so a stale/bad early reading clears out after
//! `WINDOW_SAMPLES` more samples arrive, not after however long the next
//! window boundary happens to be -- lower worst-case lag for the same
//! amount of smoothing, and no separate window-deadline/clock bookkeeping
//! for callers to manage at all.
//!
//! A plain arithmetic mean of degrees, not a proper geographic centroid
//! (great-circle midpoint, etc.) -- deliberately: this only ever averages
//! samples a few meters apart (a stationary GPS's own noise spread), and
//! at that scale degrees behave linearly enough that the distinction is
//! well below the GPS's own noise floor (same reasoning as this
//! codebase's choice to keep `haversine_m` over a flat-earth
//! approximation for *distance* -- curvature/projection effects that
//! matter at km scale are irrelevant at meter scale, just from the
//! opposite direction: here it means the simpler math is already exact
//! enough, not that the fancier math would be wasted).

/// User-specified range was "10-20"; picked the middle. Both boards'
/// `POLL_PERIOD_MS`/fix rate differ, so this is a sample *count*, not
/// tied to a specific time window -- how much wall-clock time it
/// represents varies with how fast valid fixes actually arrive.
pub const WINDOW_SAMPLES: usize = 15;

#[derive(Debug, Clone, Copy)]
pub struct FixAverage {
    samples: [(f32, f32); WINDOW_SAMPLES],
    /// Index the *next* `add` will write to (and, once the buffer is
    /// full, the index of the oldest sample being evicted).
    write_idx: usize,
    /// How many slots are actually populated so far, 0..=WINDOW_SAMPLES
    /// -- lets `mean()` divide by the true sample count while the buffer
    /// is still ramping up, instead of assuming it's always full.
    filled: usize,
    sum_lat: f32,
    sum_lon: f32,
}

impl Default for FixAverage {
    fn default() -> Self {
        Self::new()
    }
}

impl FixAverage {
    pub fn new() -> Self {
        Self {
            samples: [(0.0, 0.0); WINDOW_SAMPLES],
            write_idx: 0,
            filled: 0,
            sum_lat: 0.0,
            sum_lon: 0.0,
        }
    }

    pub fn add(&mut self, lat: f32, lon: f32) {
        if self.filled == WINDOW_SAMPLES {
            let (old_lat, old_lon) = self.samples[self.write_idx];
            self.sum_lat -= old_lat;
            self.sum_lon -= old_lon;
        } else {
            self.filled += 1;
        }
        self.samples[self.write_idx] = (lat, lon);
        self.sum_lat += lat;
        self.sum_lon += lon;
        self.write_idx = (self.write_idx + 1) % WINDOW_SAMPLES;
    }

    pub fn count(&self) -> u32 {
        self.filled as u32
    }

    /// Mean lat/lon of the most recent [`WINDOW_SAMPLES`] samples added
    /// (fewer, if `count()` hasn't reached that yet). `None` if nothing
    /// has been added since construction or the last `reset`.
    pub fn mean(&self) -> Option<(f32, f32)> {
        if self.filled == 0 {
            return None;
        }
        Some((self.sum_lat / self.filled as f32, self.sum_lon / self.filled as f32))
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}
