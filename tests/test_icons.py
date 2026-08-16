"""Tests for the ground station's status icons.

icons.py has no hardware imports -- it either buckets a number into a level
(signal_level, battery_level) or draws by calling pixel()/fill_rect() on
whatever `display`-shaped object it's handed. That means the bucketing logic
and the bitmap-walking logic can both be checked off-board with a fake
display that just records the calls it received.
"""

from ground.icons import (
    BATT_CURVE,
    BOLT_BITMAP,
    SIGNAL_BARS,
    battery_level,
    battery_percent,
    draw_battery,
    draw_bitmap,
    draw_bolt,
    signal_level,
    signal_percent,
)


class FakeDisplay:
    def __init__(self):
        self.pixels = set()
        self.rects = []

    def pixel(self, x, y, color):
        self.pixels.add((x, y, color))

    def rect(self, x, y, w, h, color):
        self.rects.append(("rect", x, y, w, h, color))

    def fill_rect(self, x, y, w, h, color):
        self.rects.append(("fill_rect", x, y, w, h, color))


# --- signal_level --------------------------------------------------------


def test_signal_level_none_is_zero_bars():
    assert signal_level(None) == 0


def test_signal_level_buckets():
    assert signal_level(-30) == 4
    assert signal_level(-50) == 4
    assert signal_level(-51) == 3
    assert signal_level(-70) == 3
    assert signal_level(-71) == 2
    assert signal_level(-90) == 2
    assert signal_level(-91) == 1
    assert signal_level(-110) == 1
    assert signal_level(-111) == 0
    assert signal_level(-140) == 0


def test_signal_bars_defined_for_every_level():
    for level in range(5):
        assert level in SIGNAL_BARS
        rows = SIGNAL_BARS[level]
        assert len(rows) == 5
        assert all(len(row) == 8 for row in rows)


# --- battery_percent / battery_level ----------------------------------------


def test_battery_percent_none_is_zero():
    assert battery_percent(None) == 0


def test_battery_percent_matches_curve_anchors_exactly():
    for volts, pct in BATT_CURVE:
        assert battery_percent(volts) == pct


def test_battery_percent_clamps_outside_the_curve():
    assert battery_percent(5.0) == 100   # above the top anchor
    assert battery_percent(3.0) == 0     # below the bottom anchor


def test_battery_percent_interpolates_between_anchors():
    # Halfway between 3.78V/50% and 3.82V/60% -> 55%. This is exactly the
    # kind of reading the old 4-bucket scheme collapsed into one bucket.
    assert battery_percent(3.80) == 55


def test_battery_percent_is_monotonic_with_voltage():
    volts = sorted(v for v, _ in BATT_CURVE)
    percents = [battery_percent(v) for v in volts]
    assert percents == sorted(percents)


def test_battery_level_none_is_zero():
    assert battery_level(None) == 0


def test_battery_level_caps_at_4():
    assert battery_level(4.20) == 4
    assert battery_level(100.0) == 4  # pathological input, still clamps


def test_battery_level_is_percent_over_25_capped():
    for volts in (4.20, 4.00, 3.90, 3.80, 3.68, 3.50, 3.30):
        assert battery_level(volts) == min(4, battery_percent(volts) // 25)


def test_signal_percent_matches_level_times_25():
    for rssi in (None, -30, -55, -75, -95, -140):
        assert signal_percent(rssi) == signal_level(rssi) * 25


# --- draw_bolt -----------------------------------------------------------


def test_bolt_bitmap_is_rectangular():
    width = len(BOLT_BITMAP[0])
    assert all(len(row) == width for row in BOLT_BITMAP)


def test_draw_bolt_places_pixels():
    display = FakeDisplay()
    draw_bolt(display, 0, 0)
    assert len(display.pixels) > 0


# --- draw_bitmap -------------------------------------------------------------


def test_draw_bitmap_places_pixels_at_set_bits():
    display = FakeDisplay()
    draw_bitmap(display, 10, 20, ("01", "10"), color=0)
    assert display.pixels == {(11, 20, 0), (10, 21, 0)}


def test_draw_bitmap_scale_draws_blocks_not_pixels():
    display = FakeDisplay()
    draw_bitmap(display, 0, 0, ("1",), color=0, scale=3)
    assert display.rects == [("fill_rect", 0, 0, 3, 3, 0)]


# --- draw_battery ------------------------------------------------------------


def test_draw_battery_fill_segment_count_tracks_level():
    for volts in (4.20, 3.98, 3.85, 3.70, 3.30):
        display = FakeDisplay()
        draw_battery(display, 0, 0, volts)
        fills = [r for r in display.rects if r[0] == "fill_rect"]
        # one fill_rect for the nub, plus one per filled segment
        assert len(fills) == 1 + battery_level(volts)
