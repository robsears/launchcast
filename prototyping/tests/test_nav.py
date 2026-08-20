"""Tests for the great-circle navigation math used by RECOVERY and FLIGHT.

nav.py has no hardware imports, so it's cheap to check off-board -- and it's
exactly the kind of math (angle wraparound, latched-fix bearings) that's easy
to get subtly backwards and only notice in the field.
"""

from ground.nav import bearing_deg, compass_point, haversine_m, relative_arrow

SF_LAT, SF_LON = 37.7749, -122.4194
LA_LAT, LA_LON = 34.0522, -118.2437
SF_TO_LA_KM = 559  # well-known great-circle distance, +/- a few km


# --- haversine_m -------------------------------------------------------------


def test_haversine_same_point_is_zero():
    assert haversine_m(SF_LAT, SF_LON, SF_LAT, SF_LON) == 0.0


def test_haversine_sf_to_la_matches_known_distance():
    d_km = haversine_m(SF_LAT, SF_LON, LA_LAT, LA_LON) / 1000.0
    assert abs(d_km - SF_TO_LA_KM) < 5


def test_haversine_is_symmetric():
    d1 = haversine_m(SF_LAT, SF_LON, LA_LAT, LA_LON)
    d2 = haversine_m(LA_LAT, LA_LON, SF_LAT, SF_LON)
    assert abs(d1 - d2) < 1e-6


# --- bearing_deg ---------------------------------------------------------


def test_bearing_due_north():
    assert abs(bearing_deg(0.0, 0.0, 1.0, 0.0) - 0.0) < 1e-6


def test_bearing_due_east():
    assert abs(bearing_deg(0.0, 0.0, 0.0, 1.0) - 90.0) < 1e-6


def test_bearing_due_south():
    assert abs(bearing_deg(0.0, 0.0, -1.0, 0.0) - 180.0) < 1e-6


def test_bearing_due_west():
    assert abs(bearing_deg(0.0, 0.0, 0.0, -1.0) - 270.0) < 1e-6


def test_bearing_always_in_range():
    for lat2, lon2 in ((5, 5), (-5, -5), (5, -5), (-5, 5)):
        b = bearing_deg(0.0, 0.0, lat2, lon2)
        assert 0.0 <= b < 360.0


# --- compass_point -------------------------------------------------------


def test_compass_point_cardinals():
    assert compass_point(0) == "N"
    assert compass_point(90) == "E"
    assert compass_point(180) == "S"
    assert compass_point(270) == "W"


def test_compass_point_wraps_at_north():
    # 360 - 11.25 rounds up into N, not NNW
    assert compass_point(349) == "N"
    assert compass_point(0) == "N"
    assert compass_point(11) == "N"


# --- relative_arrow ------------------------------------------------------


def test_relative_arrow_none_heading_is_none():
    assert relative_arrow(90, None) is None


def test_relative_arrow_ahead_when_aligned():
    assert relative_arrow(90, 90) == "^ AHEAD"


def test_relative_arrow_turn_around_when_opposite():
    assert relative_arrow(90, 270) == "v TURN AROUND"


def test_relative_arrow_right_when_target_is_clockwise():
    assert relative_arrow(90, 0) == ">> RIGHT"


def test_relative_arrow_left_when_target_is_counterclockwise():
    assert relative_arrow(0, 90) == "<< LEFT"


def test_relative_arrow_covers_full_circle_without_crashing():
    for heading in range(0, 360, 5):
        result = relative_arrow(180, heading)
        assert isinstance(result, str)
