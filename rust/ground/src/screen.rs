//! Which screen is currently showing (`code.py`'s `screen`/`SCREEN_*`
//! constants, extended 2026-08-19 with FLIGHTS/SUMMARY). Lives on core1
//! (owns both buttons and the display), read by `button_task` (to decide
//! what a button means right now) and `display_task` (to pick which
//! screen to render) -- but also, since these are plain atomics (not a
//! `Mutex`), safely readable from core0 too, which needs to know "is the
//! user currently on FLIGHTS" to decide what a CHIRP-button tap means
//! (see `main.rs`'s button-forwarding handler).
//!
//! FLIGHT/RECOVERY/DIAG/FLIGHTS cycle normally via MENU (`advance`/
//! `back`, modulo `COUNT`). SUMMARY is deliberately *not* part of that
//! rotation -- it's reachable only by selecting a flight from FLIGHTS,
//! and MENU from SUMMARY goes back to FLIGHTS specifically
//! (`to_flights`), not the next screen in the normal cycle. User call,
//! 2026-08-19.

use portable_atomic::{AtomicU8, Ordering};

pub const FLIGHT: u8 = 0;
pub const RECOVERY: u8 = 1;
pub const DIAG: u8 = 2;
pub const FLIGHTS: u8 = 3;
/// How many screens participate in the normal MENU-cycling rotation.
/// SUMMARY is intentionally outside it -- see module docs.
pub const COUNT: u8 = 4;
pub const SUMMARY: u8 = 4;
const NAMES: [&str; 5] = ["FLIGHT", "RECOVERY", "DIAG", "FLIGHTS", "SUMMARY"];

static CURRENT: AtomicU8 = AtomicU8::new(FLIGHT);
/// Cursor position within the FLIGHTS list -- see `cycle_selected`.
static SELECTED: AtomicU8 = AtomicU8::new(0);

pub fn current() -> u8 {
    CURRENT.load(Ordering::Relaxed)
}

/// Direct lookup, not modulo `COUNT` -- unlike `next_name`/`prev_name`
/// (which are specifically about the cycle), this needs to resolve
/// SUMMARY (index 4, outside `COUNT`) correctly too.
pub fn name(index: u8) -> &'static str {
    NAMES[index as usize]
}

pub fn current_name() -> &'static str {
    name(current())
}

/// SUMMARY has no "next" in the normal sense -- its MENU action is
/// always "back to FLIGHTS" (see `to_flights`), so that's what this
/// reports too, keeping the footer's "MENU>..." label honest about what
/// actually happens on a tap.
pub fn next_name() -> &'static str {
    if current() == SUMMARY {
        name(FLIGHTS)
    } else {
        name((current() + 1) % COUNT)
    }
}

pub fn prev_name() -> &'static str {
    name((current() + COUNT - 1) % COUNT)
}

/// MENU on any screen in the normal rotation -- matches `code.py`'s
/// `screen = (screen + 1) % SCREEN_COUNT`. Never called while on
/// SUMMARY; see `to_flights`.
pub fn advance() {
    CURRENT.store((current() + 1) % COUNT, Ordering::Relaxed);
}

/// ARM/DISARM-as-BACK, off FLIGHT -- matches `code.py`'s `screen =
/// (screen - 1) % SCREEN_COUNT`.
pub fn back() {
    CURRENT.store((current() + COUNT - 1) % COUNT, Ordering::Relaxed);
}

/// Enter SUMMARY -- called when a flight is selected on FLIGHTS. Not
/// part of the normal cycle, so a direct `store`, not `advance()`.
pub fn to_summary() {
    CURRENT.store(SUMMARY, Ordering::Relaxed);
}

/// MENU's meaning specifically on SUMMARY -- back to FLIGHTS, not the
/// next screen in the normal rotation.
pub fn to_flights() {
    CURRENT.store(FLIGHTS, Ordering::Relaxed);
}

pub fn selected() -> u8 {
    SELECTED.load(Ordering::Relaxed)
}

/// Advance the FLIGHTS-list cursor, wrapping at `flight_count` (the
/// live count from telemetry -- FLIGHTS has no fixed size). A
/// `flight_count` of 0 leaves the cursor at 0 rather than dividing by
/// zero; there's nothing to select yet in that case anyway.
pub fn cycle_selected(flight_count: u8) {
    if flight_count == 0 {
        SELECTED.store(0, Ordering::Relaxed);
        return;
    }
    let next = (SELECTED.load(Ordering::Relaxed) + 1) % flight_count;
    SELECTED.store(next, Ordering::Relaxed);
}
