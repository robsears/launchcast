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
