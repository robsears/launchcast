//! Rust port of the battery-bucketing cases in `tests/test_icons.py`. Only
//! `BATT_CURVE`/`battery_percent`/`battery_level` are ported so far -- see
//! `src/icons.rs` for why the drawing functions aren't here.

use launchcast_ground_logic::{battery_level, battery_percent, signal_level, signal_percent, BATT_CURVE};

#[test]
fn signal_level_none_is_zero_bars() {
    assert_eq!(signal_level(None), 0);
}

#[test]
fn signal_level_buckets() {
    assert_eq!(signal_level(Some(-30)), 4);
    assert_eq!(signal_level(Some(-50)), 4);
    assert_eq!(signal_level(Some(-51)), 3);
    assert_eq!(signal_level(Some(-70)), 3);
    assert_eq!(signal_level(Some(-71)), 2);
    assert_eq!(signal_level(Some(-90)), 2);
    assert_eq!(signal_level(Some(-91)), 1);
    assert_eq!(signal_level(Some(-110)), 1);
    assert_eq!(signal_level(Some(-111)), 0);
    assert_eq!(signal_level(Some(-140)), 0);
}

#[test]
fn signal_percent_matches_level_times_25() {
    for rssi in [None, Some(-30), Some(-55), Some(-75), Some(-95), Some(-140)] {
        assert_eq!(signal_percent(rssi), signal_level(rssi) as u16 * 25);
    }
}

#[test]
fn battery_percent_none_is_zero() {
    assert_eq!(battery_percent(None), 0);
}

#[test]
fn battery_percent_matches_curve_anchors_exactly() {
    for (volts, pct) in BATT_CURVE {
        assert_eq!(battery_percent(Some(volts)), pct);
    }
}

#[test]
fn battery_percent_clamps_outside_the_curve() {
    assert_eq!(battery_percent(Some(5.0)), 100); // above the top anchor
    assert_eq!(battery_percent(Some(3.0)), 0); // below the bottom anchor
}

#[test]
fn battery_percent_interpolates_between_anchors() {
    // Halfway between 3.78V/50% and 3.82V/60% -> 55%. This is exactly the
    // kind of reading the old 4-bucket scheme collapsed into one bucket.
    assert_eq!(battery_percent(Some(3.80)), 55);
}

#[test]
fn battery_percent_is_monotonic_with_voltage() {
    let mut volts: Vec<f32> = BATT_CURVE.iter().map(|(v, _)| *v).collect();
    volts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let percents: Vec<u8> = volts.iter().map(|v| battery_percent(Some(*v))).collect();
    let mut sorted_percents = percents.clone();
    sorted_percents.sort_unstable();
    assert_eq!(percents, sorted_percents);
}

#[test]
fn battery_level_none_is_zero() {
    assert_eq!(battery_level(None), 0);
}

#[test]
fn battery_level_caps_at_4() {
    assert_eq!(battery_level(Some(4.20)), 4);
    assert_eq!(battery_level(Some(100.0)), 4); // pathological input, still clamps
}

#[test]
fn battery_level_is_percent_over_25_capped() {
    for volts in [4.20, 4.00, 3.90, 3.80, 3.68, 3.50, 3.30] {
        assert_eq!(
            battery_level(Some(volts)),
            (battery_percent(Some(volts)) / 25).min(4)
        );
    }
}
