use launchcast_rocket_logic::imu::{raw_to_accel_mps2, raw_to_gyro_dps};

#[test]
fn zero_counts_decode_to_zero() {
    assert_eq!(raw_to_accel_mps2([0, 0, 0]), [0.0, 0.0, 0.0]);
    assert_eq!(raw_to_gyro_dps([0, 0, 0]), [0.0, 0.0, 0.0]);
}

#[test]
fn accel_matches_hand_computed_reference() {
    // 1000 counts @ 0.122 mg/LSB * standard gravity -- hand-computed in
    // Python (see docs/rust-rewrite.md's rocket-port session log).
    let [x, _, _] = raw_to_accel_mps2([1000, 0, 0]);
    assert!((x - 1.196_411_3).abs() < 1e-4);
}

#[test]
fn accel_is_negative_for_negative_counts() {
    let [x, _, _] = raw_to_accel_mps2([-500, 0, 0]);
    assert!((x - (-0.598_205_6)).abs() < 1e-4);
}

#[test]
fn gyro_matches_hand_computed_reference() {
    // 1000 counts @ 8.75 mdps/LSB = 8.75 dps exactly.
    let [x, _, _] = raw_to_gyro_dps([1000, 0, 0]);
    assert!((x - 8.75).abs() < 1e-4);
}

#[test]
fn axes_are_independent() {
    let [x, y, z] = raw_to_accel_mps2([100, -200, 300]);
    let [x2, _, _] = raw_to_accel_mps2([100, 0, 0]);
    let [_, y2, _] = raw_to_accel_mps2([0, -200, 0]);
    let [_, _, z2] = raw_to_accel_mps2([0, 0, 300]);
    assert_eq!(x, x2);
    assert_eq!(y, y2);
    assert_eq!(z, z2);
}
