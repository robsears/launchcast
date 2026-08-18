//! Rust port of `tests/test_hold_tracker.py`. Same cases, same bounce-bridging
//! regression coverage -- see that file's module docstring for why the
//! bounce-bridging tests exist (a real bug, not a hypothetical).

use launchcast_ground_logic::{Edge, HoldTracker, KeyEvent, DEFAULT_GRACE_MS, DEFAULT_HOLD_MS};

const BTN: usize = 0;

fn press() -> KeyEvent {
    KeyEvent {
        key_number: BTN,
        pressed: true,
    }
}

fn release() -> KeyEvent {
    KeyEvent {
        key_number: BTN,
        pressed: false,
    }
}

/// Poll with the given events and return the dispatched (key_number, Edge)
/// pairs, mirroring the Python tests' `tracker.poll(...) == [...]`.
fn poll(tracker: &mut HoldTracker<1>, events: Vec<KeyEvent>, now: u32) -> Vec<(usize, Edge)> {
    let mut out = Vec::new();
    tracker.poll(events, now, |key, edge| out.push((key, edge)));
    out
}

#[test]
fn defaults_are_2s_hold_and_250ms_grace() {
    let tracker: HoldTracker<1> = HoldTracker::new();
    assert_eq!(tracker.hold_ms(), DEFAULT_HOLD_MS);
    assert_eq!(DEFAULT_HOLD_MS, 2000);
    assert_eq!(tracker.grace_ms(), DEFAULT_GRACE_MS);
    assert_eq!(DEFAULT_GRACE_MS, 250);
}

// --- clean tap -----------------------------------------------------------

#[test]
fn clean_tap_fires_after_grace_window_elapses() {
    let mut tracker: HoldTracker<1> = HoldTracker::with_timing(2000, 250);
    assert_eq!(poll(&mut tracker, vec![press()], 0), vec![]);
    assert_eq!(poll(&mut tracker, vec![release()], 100), vec![]);
    assert_eq!(poll(&mut tracker, vec![], 200), vec![]); // still in grace
    assert_eq!(poll(&mut tracker, vec![], 351), vec![(BTN, Edge::Tap)]);
}

#[test]
fn tap_not_reported_twice() {
    let mut tracker: HoldTracker<1> = HoldTracker::with_timing(2000, 250);
    poll(&mut tracker, vec![press()], 0);
    poll(&mut tracker, vec![release()], 100);
    assert_eq!(poll(&mut tracker, vec![], 351), vec![(BTN, Edge::Tap)]);
    assert_eq!(poll(&mut tracker, vec![], 500), vec![]);
}

// --- clean hold ------------------------------------------------------------

#[test]
fn hold_fires_once_hold_ms_elapses_while_pressed() {
    let mut tracker: HoldTracker<1> = HoldTracker::with_timing(2000, 250);
    poll(&mut tracker, vec![press()], 0);
    assert_eq!(poll(&mut tracker, vec![], 1999), vec![]);
    assert_eq!(poll(&mut tracker, vec![], 2000), vec![(BTN, Edge::Hold)]);
}

#[test]
fn hold_does_not_fire_twice_while_still_held() {
    let mut tracker: HoldTracker<1> = HoldTracker::with_timing(2000, 250);
    poll(&mut tracker, vec![press()], 0);
    assert_eq!(poll(&mut tracker, vec![], 2000), vec![(BTN, Edge::Hold)]);
    assert_eq!(poll(&mut tracker, vec![], 2500), vec![]);
}

#[test]
fn release_after_hold_fired_does_not_also_emit_a_tap() {
    let mut tracker: HoldTracker<1> = HoldTracker::with_timing(2000, 250);
    poll(&mut tracker, vec![press()], 0);
    poll(&mut tracker, vec![], 2000); // hold fires
    poll(&mut tracker, vec![release()], 2100);
    assert_eq!(poll(&mut tracker, vec![], 2400), vec![]);
}

// --- bounce bridging (the actual bug fix) -----------------------------------

#[test]
fn bounce_mid_hold_does_not_reset_the_timer() {
    // A glitch at t=100 (released, then re-pressed at t=150, well inside the
    // 250ms grace window) must not push the hold deadline out from the
    // original t=0 press.
    let mut tracker: HoldTracker<1> = HoldTracker::with_timing(2000, 250);
    poll(&mut tracker, vec![press()], 0);
    poll(&mut tracker, vec![release()], 100);
    poll(&mut tracker, vec![press()], 150); // bounce re-press cancels the release
                                            // Elapsed from the ORIGINAL press (t=0), not the bounce (t=150).
    assert_eq!(poll(&mut tracker, vec![], 1999), vec![]);
    assert_eq!(poll(&mut tracker, vec![], 2000), vec![(BTN, Edge::Hold)]);
}

#[test]
fn bounce_mid_hold_does_not_emit_a_spurious_tap() {
    let mut tracker: HoldTracker<1> = HoldTracker::with_timing(2000, 250);
    poll(&mut tracker, vec![press()], 0);
    poll(&mut tracker, vec![release()], 100);
    poll(&mut tracker, vec![press()], 150);
    // If the bounce were mistaken for a real release, a tap would have fired
    // somewhere around now=350 (100 + grace_ms). Confirm it never does.
    for now in [200, 350, 500, 1000] {
        assert_eq!(poll(&mut tracker, vec![], now), vec![]);
    }
}

#[test]
fn repeated_bounces_still_bridge_to_one_hold() {
    let mut tracker: HoldTracker<1> = HoldTracker::with_timing(2000, 250);
    poll(&mut tracker, vec![press()], 0);
    for t in [200, 600, 1200, 1800] {
        poll(&mut tracker, vec![release()], t);
        poll(&mut tracker, vec![press()], t + 10);
    }
    assert_eq!(poll(&mut tracker, vec![], 2000), vec![(BTN, Edge::Hold)]);
}

#[test]
fn release_that_survives_past_grace_is_a_real_release_not_a_bounce() {
    // A release with no re-press within grace_ms is genuine, even if the
    // total held time so far is well under hold_ms -- it must finalize as a
    // tap rather than being held open forever.
    let mut tracker: HoldTracker<1> = HoldTracker::with_timing(2000, 250);
    poll(&mut tracker, vec![press()], 0);
    poll(&mut tracker, vec![release()], 100);
    assert_eq!(poll(&mut tracker, vec![], 351), vec![(BTN, Edge::Tap)]);
}
