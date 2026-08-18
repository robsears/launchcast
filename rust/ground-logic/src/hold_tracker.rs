//! Turns button press/release edges into "tap" / "hold" events.
//!
//! Port of `ground/hold_tracker.py`. On real hardware, `keypad.Keys`
//! debounces and timestamps presses in the background (a supervisor-level
//! scan, not the main loop), so an edge is never missed just because the
//! loop is stuck in a slow GPS or display call. `HoldTracker` only adds the
//! piece that isn't an edge: "still held after `hold_ms`" has to be checked
//! every pass rather than read off an event queue.
//!
//! A tap fires on release, so a hold does not also register as a tap.
//!
//! Unlike the Python version, keys are addressed by `key_number` (a small
//! array index) rather than a `name` string looked up through a `names`
//! table -- there are at most a handful of physical buttons, an array
//! indexed by key number is the natural no_std/no-alloc shape, and mapping
//! a fired event's `key_number` back to a name (`"ARM"`, `"CHIRP"`, ...) for
//! logging is a firmware-layer concern, not this state machine's.

pub const DEFAULT_HOLD_MS: u32 = 2000;
pub const DEFAULT_GRACE_MS: u32 = 250;

/// A raw press/release edge, as read off a debounced key-scan queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key_number: usize,
    pub pressed: bool,
}

/// A dispatched button event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Tap,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyState {
    /// ms timestamp of the ORIGINAL press, while the key is considered down.
    down_since: Option<u32>,
    /// ms timestamp of a not-yet-finalized release.
    released_at: Option<u32>,
    hold_fired: bool,
}

impl KeyState {
    const EMPTY: Self = Self {
        down_since: None,
        released_at: None,
        hold_fired: false,
    };
}

/// Tracks up to `N` independent keys. `N` is the number of physical
/// buttons (3 on the handheld: ARM/DISARM, CHIRP, MENU).
pub struct HoldTracker<const N: usize> {
    hold_ms: u32,
    grace_ms: u32,
    keys: [KeyState; N],
}

impl<const N: usize> Default for HoldTracker<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> HoldTracker<N> {
    pub fn new() -> Self {
        Self::with_timing(DEFAULT_HOLD_MS, DEFAULT_GRACE_MS)
    }

    pub fn with_timing(hold_ms: u32, grace_ms: u32) -> Self {
        Self {
            hold_ms,
            grace_ms,
            keys: [KeyState::EMPTY; N],
        }
    }

    pub fn hold_ms(&self) -> u32 {
        self.hold_ms
    }

    pub fn grace_ms(&self) -> u32 {
        self.grace_ms
    }

    /// Drain queued edges and check for newly-expired holds.
    ///
    /// A release isn't finalized into a tap immediately -- it's held for
    /// `grace_ms` in case a same-key re-press arrives right behind it. A
    /// cheap switch or marginal contact can drop out for a moment mid-hold;
    /// without this, that one glitch would restart the whole `hold_ms`
    /// countdown and a genuine hold could take several retries (many real
    /// seconds) to ever register, even though tap (which only needs one
    /// clean edge) is fine.
    ///
    /// Calls `on_edge(key_number, edge)` for each dispatched event, in
    /// order. `key_number`s outside `0..N` are ignored.
    pub fn poll(
        &mut self,
        events: impl IntoIterator<Item = KeyEvent>,
        now: u32,
        mut on_edge: impl FnMut(usize, Edge),
    ) {
        for event in events {
            let Some(state) = self.keys.get_mut(event.key_number) else {
                continue;
            };
            if event.pressed {
                if state.down_since.is_none() {
                    // Genuinely fresh press -- start the timer.
                    state.down_since = Some(now);
                    state.hold_fired = false;
                }
                // else: a same-key re-press while already tracked -- bounce
                // during the hold, not a new press. Keep the original
                // down_since so held time keeps accumulating through it.
                state.released_at = None;
            } else if state.down_since.is_some() {
                state.released_at = Some(now);
            }
        }

        // Finalize releases that survived past the grace window -- a real
        // release, not bounce.
        for key in 0..N {
            let state = &mut self.keys[key];
            let Some(released_at) = state.released_at else {
                continue;
            };
            if now.wrapping_sub(released_at) < self.grace_ms {
                continue;
            }
            let since = state.down_since.take();
            state.released_at = None;
            if since.is_some() && !state.hold_fired {
                on_edge(key, Edge::Tap);
            }
            state.hold_fired = false;
        }

        for key in 0..N {
            let state = &mut self.keys[key];
            let Some(since) = state.down_since else {
                continue;
            };
            if !state.hold_fired && now.wrapping_sub(since) >= self.hold_ms {
                state.hold_fired = true;
                on_edge(key, Edge::Hold);
            }
        }
    }
}
