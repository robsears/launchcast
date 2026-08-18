//! Cross-core shared "what do we know about the rocket" state -- the
//! parts of `code.py`'s `Link` class that need a real clock
//! (`embassy_time::Instant` isn't hardware-free, which is why this lives
//! in the firmware crate and not `ground-logic`). The freshness
//! *bucketing* rule itself (WAITING/LIVE/STALE/LOST) is hardware-free and
//! lives in `launchcast_ground_logic::link_status` -- this module just
//! tracks the raw timestamps that rule gets applied to.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Instant;
use launchcast_common as common;

/// The rocket's last known GPS fix, latched separately from live
/// telemetry so it survives the rocket going silent -- matches
/// `code.py`'s own docstring: "the single most valuable feature in the
/// file."
#[derive(Debug, Clone, Copy)]
pub struct LatchedFix {
    pub lat: f32,
    pub lon: f32,
    pub latched_at: Instant,
}

/// `Clone`/`Copy` so `display_task` can snapshot the whole thing out from
/// behind the lock in one deref, matching this codebase's existing
/// pattern for cross-core "latest value" state -- never held across the
/// ~50ms SPI render that follows.
#[derive(Clone, Copy)]
pub struct LinkState {
    pub latest: Option<(common::Telemetry, i16, i16)>, // telemetry, rssi, snr
    pub last_rx: Option<Instant>,
    pub fix: Option<LatchedFix>,
}

pub static LINK: Mutex<CriticalSectionRawMutex, LinkState> = Mutex::new(LinkState {
    latest: None,
    last_rx: None,
    fix: None,
});

impl LinkState {
    /// Record a newly received, successfully decoded frame. Matches
    /// `code.py`'s `Link.ingest`: always updates `latest`/`last_rx`; only
    /// (re)latches the fix when this frame reports one (`has_fix && lat
    /// != 0.0`) -- an unfixed or all-zero frame leaves the previous latch
    /// untouched, exactly like Python's `Link.ingest` only ever
    /// *assigning* `fix_lat`/`fix_lon`/`fix_age_ms`, never clearing them.
    pub fn record_rx(&mut self, telemetry: common::Telemetry, rssi: i16, snr: i16, now: Instant) {
        if telemetry.has_fix && telemetry.lat != 0.0 {
            self.fix = Some(LatchedFix {
                lat: telemetry.lat,
                lon: telemetry.lon,
                latched_at: now,
            });
        }
        self.latest = Some((telemetry, rssi, snr));
        self.last_rx = Some(now);
    }

    /// Age of the last received frame, in ms. `None` if nothing has ever
    /// been received -- matches `code.py`'s `Link.age_ms()`.
    pub fn age_ms(&self, now: Instant) -> Option<u32> {
        self.last_rx.map(|t| (now - t).as_millis() as u32)
    }
}
