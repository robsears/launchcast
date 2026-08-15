"""Tests for the ground station's accel magnitude display.

Locks down the "don't subtract 1" convention: an unmoving rocket reads
~1.0g, matching what rocket/code.py's own accel_magnitude() (and the
BOOST_THRESHOLD_G/COAST_THRESHOLD_G thresholds it's compared against)
already assume.
"""

from ground.imu import accel_magnitude_g


def test_at_rest_reads_about_one_g():
    assert abs(accel_magnitude_g((0.0, 0.0, 1.0)) - 1.0) < 1e-9


def test_zero_input_is_zero():
    assert accel_magnitude_g((0.0, 0.0, 0.0)) == 0.0


def test_pythagorean_quadruple():
    # 2^2 + 3^2 + 6^2 == 7^2
    assert abs(accel_magnitude_g((2.0, 3.0, 6.0)) - 7.0) < 1e-9


def test_negative_axes_dont_cancel():
    assert abs(accel_magnitude_g((-3.0, -4.0, 0.0)) - 5.0) < 1e-9
