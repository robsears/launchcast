use launchcast_rocket_logic::bmp580::decode_temp_press;

#[test]
fn all_zero_bytes_decode_to_zero() {
    let (temp_c, pressure_hpa) = decode_temp_press([0; 6]);
    assert_eq!(temp_c, 0.0);
    assert_eq!(pressure_hpa, 0.0);
}

#[test]
fn decodes_a_known_positive_reading() {
    // 25.0 C, 1000.0 hPa -- hand-computed reference values (see
    // docs/rust-rewrite.md's rocket-port session log for the derivation).
    let bytes = [0, 0, 25, 0, 168, 97];
    let (temp_c, pressure_hpa) = decode_temp_press(bytes);
    assert!((temp_c - 25.0).abs() < 1e-6);
    assert!((pressure_hpa - 1000.0).abs() < 1e-6);
}

#[test]
fn sign_extends_a_negative_temperature() {
    // -10.5 C, 1000.0 hPa.
    let bytes = [0, 128, 245, 0, 168, 97];
    let (temp_c, pressure_hpa) = decode_temp_press(bytes);
    assert!((temp_c - (-10.5)).abs() < 1e-6);
    assert!((pressure_hpa - 1000.0).abs() < 1e-6);
}

#[test]
fn temp_and_pressure_are_independent() {
    // Changing only the pressure bytes shouldn't perturb the decoded
    // temperature, and vice versa -- catches an accidental bit-overlap
    // bug in the 24-bit field split.
    let (t1, _) = decode_temp_press([0, 0, 25, 0, 0, 0]);
    let (t2, _) = decode_temp_press([0, 0, 25, 255, 255, 127]);
    assert_eq!(t1, t2);

    let (_, p1) = decode_temp_press([0, 0, 0, 0, 168, 97]);
    let (_, p2) = decode_temp_press([255, 255, 127, 0, 168, 97]);
    assert_eq!(p1, p2);
}
