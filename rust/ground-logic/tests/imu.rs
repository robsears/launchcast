//! Rust port of `tests/test_imu.py`.

use launchcast_ground_logic::accel_magnitude_g;

#[test]
fn at_rest_reads_about_one_g() {
    assert!((accel_magnitude_g([0.0, 0.0, 1.0]) - 1.0).abs() < 1e-6);
}

#[test]
fn zero_input_is_zero() {
    assert_eq!(accel_magnitude_g([0.0, 0.0, 0.0]), 0.0);
}

#[test]
fn pythagorean_quadruple() {
    // 2^2 + 3^2 + 6^2 == 7^2
    assert!((accel_magnitude_g([2.0, 3.0, 6.0]) - 7.0).abs() < 1e-6);
}

#[test]
fn negative_axes_dont_cancel() {
    assert!((accel_magnitude_g([-3.0, -4.0, 0.0]) - 5.0).abs() < 1e-6);
}
