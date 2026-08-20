//! Pending-request tracking for a FLIGHTS-screen summary fetch --
//! mirrors `cmdlog.rs`'s ARM/DISARM pending-confirmation shape, but the
//! completion condition is different: "a `PKT_SUMMARY` response echoing
//! the same flight index arrived" rather than "telemetry now reports
//! state X". Kept as its own small module rather than folded into
//! `cmdlog.rs` since that shape (`Pending { want: u8 }`, compared
//! against live telemetry state) doesn't fit this case at all.
//!
//! Owned entirely by core0 (the side that sends the request and sees
//! the response), same cross-core `Mutex` pattern as `link::LINK`/
//! `cmdlog::CMD_LOG` -- core1's display task clones the small `Copy`
//! snapshot out from behind the lock rather than holding it across a
//! render.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use launchcast_common as common;
use launchcast_ground_logic::LinkStatus;

/// Matches `cmdlog::CMD_CONFIRM_FRAMES`'s reasoning: how many telemetry
/// frames to see arrive before giving up on a response that hasn't come.
const CONFIRM_FRAMES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SummaryRequest {
    /// Nothing requested yet, or the last request resolved and the user
    /// has since navigated away.
    Idle,
    Pending { index: u8, packets_at_send: u32 },
    Ready(common::Summary),
    Failed { index: u8 },
}

pub static REQUEST: Mutex<CriticalSectionRawMutex, SummaryRequest> = Mutex::new(SummaryRequest::Idle);

/// Call right after successfully sending a `GET_SUMMARY_BASE + index`
/// command.
pub async fn start(index: u8, packets_now: u32) {
    *REQUEST.lock().await = SummaryRequest::Pending { index, packets_at_send: packets_now };
}

/// Call when a `PKT_SUMMARY` frame arrives off the radio. A response
/// for an index that doesn't match the currently pending request (a
/// stale/late reply, or nothing was pending at all) is silently
/// dropped -- there's nothing to update.
pub async fn record_response(summary: common::Summary) {
    let mut r = REQUEST.lock().await;
    if let SummaryRequest::Pending { index, .. } = *r {
        if index == summary.flight_index {
            *r = SummaryRequest::Ready(summary);
        }
    }
}

/// Call once per core0 loop iteration, same reasoning as `cmdlog::poll`:
/// the link-lost branch has to fire even when nothing is arriving at all
/// to count against `CONFIRM_FRAMES`.
pub async fn poll_timeout(packets_now: u32, status: LinkStatus) {
    let mut r = REQUEST.lock().await;
    if let SummaryRequest::Pending { index, packets_at_send } = *r {
        if packets_now.wrapping_sub(packets_at_send) >= CONFIRM_FRAMES || status == LinkStatus::Lost {
            *r = SummaryRequest::Failed { index };
        }
    }
}

/// Reset to `Idle` -- call when the user leaves SUMMARY back to
/// FLIGHTS, so a stale `Ready`/`Failed` from a previous selection
/// doesn't flash up again before its own request resolves.
pub async fn reset() {
    *REQUEST.lock().await = SummaryRequest::Idle;
}

/// Show an already-cached summary directly, with no radio request at
/// all -- see `flight_index.rs`'s docs on the per-flight summary cache.
pub async fn show_cached(summary: common::Summary) {
    *REQUEST.lock().await = SummaryRequest::Ready(summary);
}
