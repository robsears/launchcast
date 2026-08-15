"""Unit conversion for the ground station's display.

UNITS picks feet/Fahrenheit vs meters/Celsius for every screen. Screens
should call distance()/temperature() rather than format the raw telemetry
field directly, and there's a single place (this module) to flip when a
settings screen eventually makes it a runtime toggle instead of a constant.
"""

UNITS = "imperial"  # "imperial" or "metric"


def c_to_f(c):
    return c * 9.0 / 5.0 + 32.0


def m_to_ft(m):
    return m * 3.28084


def temperature(c):
    """Convert a Celsius reading (the wire format's unit) to the display unit."""
    return c_to_f(c) if UNITS == "imperial" else c


def temperature_label():
    return "F" if UNITS == "imperial" else "C"


def distance(m):
    """Convert a meters reading (the wire format's unit) to the display unit."""
    return m_to_ft(m) if UNITS == "imperial" else m


def distance_label():
    return "ft" if UNITS == "imperial" else "m"
