//! Radio link freshness bucketing. Port of `code.py`'s `Link.status()`
//! (the WAITING/LIVE/STALE/LOST logic), split out as hardware-free,
//! host-testable logic -- the "now"/"last received" timestamps themselves
//! come from `embassy_time` in the firmware crate, but the bucketing
//! rule itself doesn't need to.

/// No packet for this long -> shown as stale. Matches `code.py`'s
/// `LINK_STALE_MS`.
pub const LINK_STALE_MS: u32 = 3000;
/// No packet for this long -> shown as LOST. Matches `code.py`'s
/// `LINK_LOST_MS`.
pub const LINK_LOST_MS: u32 = 15000;

/// No packet for this long -> the ground station shows the MISSING screen
/// (see `ground/src/screen_missing.rs`) in place of whatever the current
/// screen would otherwise render, rather than leaving stale FLIGHT/
/// RECOVERY/DIAG content on screen looking current. Deliberately much
/// coarser than `LINK_LOST_MS` above (a live-screen "link degraded"
/// indicator) -- this is "there's nothing current to show at all."
pub const TELEMETRY_MISSING_MS: u32 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    /// No packet has ever been received.
    Waiting,
    Live,
    Stale,
    Lost,
}

impl LinkStatus {
    pub fn name(self) -> &'static str {
        match self {
            LinkStatus::Waiting => "WAITING",
            LinkStatus::Live => "LIVE",
            LinkStatus::Stale => "STALE",
            LinkStatus::Lost => "LOST",
        }
    }
}

/// Bucket the age (in ms) of the last received packet into a
/// [`LinkStatus`]. `None` -> `Waiting`, matching `code.py`'s
/// `Link.age_ms()` returning `None` when `last_rx_ms == 0`.
pub fn link_status(age_ms: Option<u32>) -> LinkStatus {
    match age_ms {
        None => LinkStatus::Waiting,
        Some(age) if age > LINK_LOST_MS => LinkStatus::Lost,
        Some(age) if age > LINK_STALE_MS => LinkStatus::Stale,
        Some(_) => LinkStatus::Live,
    }
}

/// Whether the MISSING screen should replace the current screen's normal
/// content: nothing has ever been received, or the last frame is older
/// than [`TELEMETRY_MISSING_MS`].
pub fn telemetry_missing(age_ms: Option<u32>) -> bool {
    match age_ms {
        None => true,
        Some(age) => age >= TELEMETRY_MISSING_MS,
    }
}
