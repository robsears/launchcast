//! Real, explicitly-cached list of the rocket's stored flights --
//! replaces trusting a live telemetry byte, which can't distinguish "the
//! rocket really has N flights" from "the rocket power-cycled and lost
//! its RAM-only stored flights since we last heard from it." User call,
//! 2026-08-19, after noticing exactly that gap.
//!
//! Two related caches, invalidated together:
//!   - the flight index itself (ordered ARM timestamps, fetched via
//!     `Command::GET_FLIGHT_INDEX`) -- what flights exist, and at what
//!     index.
//!   - individual flight summaries. Two paths populate this cache: a
//!     manual FLIGHTS selection (`summary_request.rs` tracks that
//!     specific request's transient UI status; this module additionally
//!     persists the *result* so revisiting an already-viewed flight
//!     costs no further radio round trip) and an automatic background
//!     prefetch (below) that walks every not-yet-cached index in
//!     sequence as soon as the index itself is `Ready`, so a full review
//!     of every flight works later with the rocket powered off entirely
//!     -- user call, 2026-08-19.
//!
//! Prefetch is a slow background drip, not a burst: at most one
//! `GET_SUMMARY_BASE` request is ever in flight at a time (shares the
//! same one-radio, half-duplex constraint every other request on this
//! link already respects), paced by `main.rs`'s core0 loop the same way
//! the index fetch itself is. An index whose prefetch attempt times out
//! is marked `abandoned` (not retried automatically) so a genuinely
//! unreachable entry can't stall the rest of the list forever -- matches
//! `cmdlog`/`summary_request`'s existing "failures need a deliberate
//! retry, not silent infinite spam" philosophy; the user can still
//! recover an abandoned entry manually by selecting it on FLIGHTS, which
//! goes through the ordinary on-demand path untouched by any of this.
//!
//! Invalidated in two ways, both clearing everything (not just the
//! affected entries): indices only actually shift once the rocket's own
//! 32-flight cap forces an eviction (rare), so a full clear-and-refetch
//! is simpler than trying to reason about partial staleness, for a cost
//! that's trivial given how rarely this fires.
//!   - automatically, whenever a RECOVER command succeeds (`main.rs`
//!     watches `cmdlog::poll`'s return value for
//!     `PollOutcome::RecoverSucceeded`) -- a new flight just got
//!     archived, so whatever's cached is stale.
//!   - manually, from the DIAG screen (CHIRP button repurposed there --
//!     see `screen_footer.rs`) for whenever the user wants to force a
//!     rebuild without waiting for the next RECOVER.
//!
//! Owned entirely by core0 (the side that sends requests and sees
//! responses), same cross-core `Mutex` pattern as `link::LINK`/
//! `cmdlog::CMD_LOG`.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use launchcast_common as common;
use launchcast_ground_logic::LinkStatus;

/// Matches `cmdlog::CMD_CONFIRM_FRAMES`/`summary_request::
/// CONFIRM_FRAMES`'s reasoning: how many telemetry frames to see arrive
/// before giving up on a response that hasn't come.
const CONFIRM_FRAMES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IndexState {
    /// Never fetched, or just invalidated. Not itself a "loading"
    /// signal -- `main.rs`'s auto-fetch check picks this up (and moves
    /// it to `Pending`) essentially immediately whenever the FLIGHTS
    /// screen is showing and the link looks alive; screens render this
    /// the same as `Pending` unless the link is dead (see
    /// `screen_flights.rs`).
    Idle,
    Pending { packets_at_send: u32 },
    /// Fetched successfully, at least one flight.
    Ready { count: u8 },
    /// Fetched successfully, zero flights stored.
    Empty,
    Failed,
}

/// Background per-flight-summary prefetch, walking every not-yet-cached
/// index once the flight index itself is `Ready` -- see module docs.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Prefetch {
    /// Nothing currently in flight -- either there's nothing left to
    /// fetch, or the last attempt just resolved (either way) and the
    /// next core0 tick's check decides what happens next.
    Idle,
    Requesting { index: u8, packets_at_send: u32 },
}

pub struct FlightIndexState {
    pub state: IndexState,
    timestamps: [u32; common::MAX_STORED_FLIGHTS as usize],
    summaries: [Option<common::Summary>; common::MAX_STORED_FLIGHTS as usize],
    prefetch: Prefetch,
    /// Bitmask, bit `i` set = a prefetch attempt for index `i` timed out
    /// this cache generation and won't be retried automatically. Cleared
    /// alongside everything else on invalidate/refetch.
    abandoned: u32,
}

impl FlightIndexState {
    pub fn count(&self) -> u8 {
        match self.state {
            IndexState::Ready { count } => count,
            _ => 0,
        }
    }

    pub fn cached_summary(&self, index: u8) -> Option<common::Summary> {
        self.summaries.get(index as usize).copied().flatten()
    }

    /// The lowest index that still needs a prefetch attempt -- `None`
    /// both when a prefetch request is already in flight (only one at a
    /// time, see module docs) and when there's simply nothing left to
    /// try (everything's either cached or abandoned).
    fn lowest_uncached_index(&self) -> Option<u8> {
        if matches!(self.prefetch, Prefetch::Requesting { .. }) {
            return None;
        }
        let count = self.count();
        (0..count).find(|&i| self.summaries[i as usize].is_none() && self.abandoned & (1 << i) == 0)
    }

    /// `(cached, total)` while background prefetch still has work left
    /// to do -- `None` once every index is resolved (cached or
    /// abandoned), so `screen_flights.rs` knows when to stop showing the
    /// "FETCHING..." banner.
    pub fn prefetch_progress(&self) -> Option<(u8, u8)> {
        let total = self.count();
        if total == 0 {
            return None;
        }
        let cached = self.summaries[..total as usize].iter().filter(|s| s.is_some()).count() as u32;
        let resolved = cached + self.abandoned.count_ones();
        if resolved >= total as u32 {
            None
        } else {
            Some((cached as u8, total))
        }
    }
}

pub static FLIGHT_INDEX: Mutex<CriticalSectionRawMutex, FlightIndexState> = Mutex::new(FlightIndexState {
    state: IndexState::Idle,
    timestamps: [0; common::MAX_STORED_FLIGHTS as usize],
    summaries: [None; common::MAX_STORED_FLIGHTS as usize],
    prefetch: Prefetch::Idle,
    abandoned: 0,
});

/// Call right after successfully sending a `GET_FLIGHT_INDEX` command.
pub async fn start_fetch(packets_now: u32) {
    FLIGHT_INDEX.lock().await.state = IndexState::Pending { packets_at_send: packets_now };
}

/// Call when a `PKT_FLIGHT_INDEX` frame arrives. Ignored (not just
/// silently overwritten) if nothing was actually pending -- a stale/
/// late response has nothing to update.
pub async fn record_response(timestamps: &[u32]) {
    let mut s = FLIGHT_INDEX.lock().await;
    if !matches!(s.state, IndexState::Pending { .. }) {
        return;
    }
    let count = timestamps.len().min(common::MAX_STORED_FLIGHTS as usize);
    s.timestamps[..count].copy_from_slice(&timestamps[..count]);
    // A fresh index invalidates any previously cached summaries -- see
    // module docs on why this is a full clear, not an attempt to keep
    // still-valid-looking entries.
    s.summaries = [None; common::MAX_STORED_FLIGHTS as usize];
    s.prefetch = Prefetch::Idle;
    s.abandoned = 0;
    s.state = if count == 0 { IndexState::Empty } else { IndexState::Ready { count: count as u8 } };
}

/// Call right after successfully sending a background prefetch
/// `GET_SUMMARY_BASE` request for `index`.
pub async fn start_prefetch(index: u8, packets_now: u32) {
    FLIGHT_INDEX.lock().await.prefetch = Prefetch::Requesting { index, packets_at_send: packets_now };
}

/// The next index background prefetch should request, or `None` if
/// nothing should happen right now (already busy, or nothing left) --
/// called once per core0 loop tick alongside the other auto-fetch
/// checks.
pub async fn next_to_prefetch() -> Option<u8> {
    FLIGHT_INDEX.lock().await.lowest_uncached_index()
}

/// Persist a fetched summary into the cache, keyed by its own
/// `flight_index` -- called for both a manual FLIGHTS-selection
/// response (alongside `summary_request::record_response`) and a
/// background prefetch response. Clears the matching in-flight prefetch
/// marker too, if this response happens to be the one prefetch was
/// waiting on -- if it isn't (a manual selection resolved instead), the
/// prefetch request is left alone to resolve on its own next.
pub async fn record_summary(summary: common::Summary) {
    let mut s = FLIGHT_INDEX.lock().await;
    if let Some(slot) = s.summaries.get_mut(summary.flight_index as usize) {
        *slot = Some(summary);
    }
    if let Prefetch::Requesting { index, .. } = s.prefetch {
        if index == summary.flight_index {
            s.prefetch = Prefetch::Idle;
        }
    }
}

/// Call once per core0 loop iteration, same reasoning as `cmdlog::poll`.
pub async fn poll_timeout(packets_now: u32, status: LinkStatus) {
    let mut s = FLIGHT_INDEX.lock().await;
    if let IndexState::Pending { packets_at_send } = s.state {
        if packets_now.wrapping_sub(packets_at_send) >= CONFIRM_FRAMES || status == LinkStatus::Lost {
            s.state = IndexState::Failed;
        }
    }
    if let Prefetch::Requesting { index, packets_at_send } = s.prefetch {
        if packets_now.wrapping_sub(packets_at_send) >= CONFIRM_FRAMES || status == LinkStatus::Lost {
            // Abandon, don't retry -- see module docs. The next core0
            // tick's next_to_prefetch() check moves on to whatever
            // index is next.
            s.abandoned |= 1 << index;
            s.prefetch = Prefetch::Idle;
        }
    }
}

/// Clear both caches -- see module docs on the two triggers for this.
pub async fn invalidate() {
    let mut s = FLIGHT_INDEX.lock().await;
    s.state = IndexState::Idle;
    s.summaries = [None; common::MAX_STORED_FLIGHTS as usize];
    s.prefetch = Prefetch::Idle;
    s.abandoned = 0;
}
