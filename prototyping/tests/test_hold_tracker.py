"""Tests for HoldTracker's tap/hold state machine.

hold_tracker.py has no hardware imports -- it only needs an object shaped
like keypad.Keys (an `events` queue with `.get()`), so it's driven here with
a small fake queue instead of real hardware.

The bounce-bridging behavior (press -> release -> re-press within grace_ms
counts as one continuous hold, not a reset) is the actual regression test:
before it, a single brief contact glitch mid-hold -- plausible on a cheap
tactile switch or marginal solder joint -- would silently restart the whole
hold_ms countdown, and a genuine hold could take several retries (many real
seconds) to ever register, even though tap (only needs one clean edge) was
unaffected.
"""

from ground.hold_tracker import DEFAULT_GRACE_MS, DEFAULT_HOLD_MS, HoldTracker


class FakeEvent:
    def __init__(self, key_number, pressed):
        self.key_number = key_number
        self.pressed = pressed


class FakeKeys:
    """events: list of FakeEvent, drained in order by .events.get()."""

    def __init__(self, events=()):
        self._events = list(events)
        self.events = self  # so `keys.events.get()` resolves to our .get()

    def get(self):
        if self._events:
            return self._events.pop(0)
        return None


def press(key_number=0):
    return FakeEvent(key_number, True)


def release(key_number=0):
    return FakeEvent(key_number, False)


NAMES = ("btn",)


def test_defaults_are_2s_hold_and_250ms_grace():
    tracker = HoldTracker(NAMES)
    assert tracker.hold_ms == DEFAULT_HOLD_MS == 2000
    assert tracker.grace_ms == DEFAULT_GRACE_MS == 250


# --- clean tap -----------------------------------------------------------


def test_clean_tap_fires_after_grace_window_elapses():
    tracker = HoldTracker(NAMES, hold_ms=2000, grace_ms=250)
    assert tracker.poll(FakeKeys([press()]), now=0) == []
    assert tracker.poll(FakeKeys([release()]), now=100) == []
    assert tracker.poll(FakeKeys([]), now=200) == []          # still in grace
    assert tracker.poll(FakeKeys([]), now=351) == [("btn", "tap")]


def test_tap_not_reported_twice():
    tracker = HoldTracker(NAMES, hold_ms=2000, grace_ms=250)
    tracker.poll(FakeKeys([press()]), now=0)
    tracker.poll(FakeKeys([release()]), now=100)
    assert tracker.poll(FakeKeys([]), now=351) == [("btn", "tap")]
    assert tracker.poll(FakeKeys([]), now=500) == []


# --- clean hold ------------------------------------------------------------


def test_hold_fires_once_hold_ms_elapses_while_pressed():
    tracker = HoldTracker(NAMES, hold_ms=2000, grace_ms=250)
    tracker.poll(FakeKeys([press()]), now=0)
    assert tracker.poll(FakeKeys([]), now=1999) == []
    assert tracker.poll(FakeKeys([]), now=2000) == [("btn", "hold")]


def test_hold_does_not_fire_twice_while_still_held():
    tracker = HoldTracker(NAMES, hold_ms=2000, grace_ms=250)
    tracker.poll(FakeKeys([press()]), now=0)
    assert tracker.poll(FakeKeys([]), now=2000) == [("btn", "hold")]
    assert tracker.poll(FakeKeys([]), now=2500) == []


def test_release_after_hold_fired_does_not_also_emit_a_tap():
    tracker = HoldTracker(NAMES, hold_ms=2000, grace_ms=250)
    tracker.poll(FakeKeys([press()]), now=0)
    tracker.poll(FakeKeys([]), now=2000)  # hold fires
    tracker.poll(FakeKeys([release()]), now=2100)
    assert tracker.poll(FakeKeys([]), now=2400) == []


# --- bounce bridging (the actual bug fix) -----------------------------------


def test_bounce_mid_hold_does_not_reset_the_timer():
    # A glitch at t=100 (released, then re-pressed at t=150, well inside the
    # 250ms grace window) must not push the hold deadline out from the
    # original t=0 press.
    tracker = HoldTracker(NAMES, hold_ms=2000, grace_ms=250)
    tracker.poll(FakeKeys([press()]), now=0)
    tracker.poll(FakeKeys([release()]), now=100)
    tracker.poll(FakeKeys([press()]), now=150)  # bounce re-press cancels the release
    # Elapsed from the ORIGINAL press (t=0), not the bounce (t=150).
    assert tracker.poll(FakeKeys([]), now=1999) == []
    assert tracker.poll(FakeKeys([]), now=2000) == [("btn", "hold")]


def test_bounce_mid_hold_does_not_emit_a_spurious_tap():
    tracker = HoldTracker(NAMES, hold_ms=2000, grace_ms=250)
    tracker.poll(FakeKeys([press()]), now=0)
    tracker.poll(FakeKeys([release()]), now=100)
    tracker.poll(FakeKeys([press()]), now=150)
    # If the bounce were mistaken for a real release, a tap would have fired
    # somewhere around now=350 (100 + grace_ms). Confirm it never does.
    for now in (200, 350, 500, 1000):
        assert tracker.poll(FakeKeys([]), now=now) == []


def test_repeated_bounces_still_bridge_to_one_hold():
    tracker = HoldTracker(NAMES, hold_ms=2000, grace_ms=250)
    tracker.poll(FakeKeys([press()]), now=0)
    for t in (200, 600, 1200, 1800):
        tracker.poll(FakeKeys([release()]), now=t)
        tracker.poll(FakeKeys([press()]), now=t + 10)
    assert tracker.poll(FakeKeys([]), now=2000) == [("btn", "hold")]


def test_release_that_survives_past_grace_is_a_real_release_not_a_bounce():
    # A release with no re-press within grace_ms is genuine, even if the
    # total held time so far is well under hold_ms -- it must finalize as a
    # tap rather than being held open forever.
    tracker = HoldTracker(NAMES, hold_ms=2000, grace_ms=250)
    tracker.poll(FakeKeys([press()]), now=0)
    tracker.poll(FakeKeys([release()]), now=100)
    assert tracker.poll(FakeKeys([]), now=351) == [("btn", "tap")]
