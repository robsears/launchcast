"""Tests for imperial/metric conversion.

units.py has no hardware imports, so it runs off-board. These lock down the
conversion math and the UNITS switch itself -- get either wrong and every
screen quietly shows the wrong number.
"""

import ground.units as units


def test_c_to_f_freezing():
    assert abs(units.c_to_f(0) - 32.0) < 1e-9


def test_c_to_f_boiling():
    assert abs(units.c_to_f(100) - 212.0) < 1e-9


def test_m_to_ft_known_value():
    # 1 meter is defined as exactly 3.28084 ft to 6 sig figs
    assert abs(units.m_to_ft(1) - 3.28084) < 1e-9


def test_m_to_ft_zero_is_zero():
    assert units.m_to_ft(0) == 0.0


def test_imperial_is_the_default():
    assert units.UNITS == "imperial"


def test_temperature_and_distance_follow_units_switch():
    original = units.UNITS
    try:
        units.UNITS = "imperial"
        assert abs(units.temperature(0) - 32.0) < 1e-9
        assert units.temperature_label() == "F"
        assert abs(units.distance(1) - 3.28084) < 1e-9
        assert units.distance_label() == "ft"

        units.UNITS = "metric"
        assert units.temperature(0) == 0
        assert units.temperature_label() == "C"
        assert units.distance(1) == 1
        assert units.distance_label() == "m"
    finally:
        units.UNITS = original
