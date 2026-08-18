//! Rust port of the battery-bucketing cases in `tests/test_icons.py`. Only
//! `battery_percent`/`battery_level` are ported so far -- see
//! `src/icons.rs` for why the drawing functions aren't here.

use launchcast_ground_logic::{battery_level, battery_percent, signal_level, signal_percent};

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
fn battery_percent_matches_the_formula_at_reference_points() {
    // 123 * (1 - 1/((1 + (V/3.7)^80)^0.165)), hand-computed and verified
    // in Python before implementing -- see docs/rust-rewrite.md.
    for (volts, expected_pct) in [
        (4.20, 100),
        (4.10, 91),
        (4.00, 79),
        (3.90, 62),
        (3.80, 38),
        (3.70, 13),
        (3.60, 2),
        (3.50, 0),
    ] {
        assert_eq!(battery_percent(Some(volts)), expected_pct, "at {volts}V");
    }
}

#[test]
fn battery_percent_clamps_above_full_charge() {
    // A LiPo reporting full charge switches the Feather over to USB
    // power (~5.0V) -- an above-range reading like this must still
    // clamp to 100%, not the formula's raw (>100) output.
    assert_eq!(battery_percent(Some(5.0)), 100);
    assert_eq!(battery_percent(Some(4.5)), 100);
}

#[test]
fn battery_percent_floors_near_and_below_cutoff() {
    assert_eq!(battery_percent(Some(3.0)), 0);
    assert_eq!(battery_percent(Some(2.8)), 0); // the protection-circuit cutout point
}

#[test]
fn battery_percent_is_monotonic_with_voltage() {
    let volts = [2.8, 3.0, 3.3, 3.5, 3.6, 3.7, 3.8, 3.9, 4.0, 4.1, 4.2, 4.5, 5.0];
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
