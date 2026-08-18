//! Rust port of `tests/test_units.py`.
//!
//! `test_temperature_and_distance_follow_units_switch` and
//! `test_imperial_is_the_default` become `Units::default()` /
//! per-variant checks here rather than mutating a module-level global --
//! see `src/units.rs` for why `Units` is an explicit enum, not a mutable
//! `static`.

use launchcast_ground_logic::{c_to_f, m_to_ft, Units};

#[test]
fn c_to_f_freezing() {
    assert!((c_to_f(0.0) - 32.0).abs() < 1e-6);
}

#[test]
fn c_to_f_boiling() {
    assert!((c_to_f(100.0) - 212.0).abs() < 1e-6);
}

#[test]
fn m_to_ft_known_value() {
    // 1 meter is defined as exactly 3.28084 ft to 6 sig figs
    assert!((m_to_ft(1.0) - 3.28084).abs() < 1e-6);
}

#[test]
fn m_to_ft_zero_is_zero() {
    assert_eq!(m_to_ft(0.0), 0.0);
}

#[test]
fn imperial_is_the_default() {
    assert_eq!(Units::default(), Units::Imperial);
}

#[test]
fn temperature_and_distance_follow_units_switch() {
    assert!((Units::Imperial.temperature(0.0) - 32.0).abs() < 1e-6);
    assert_eq!(Units::Imperial.temperature_label(), "F");
    assert!((Units::Imperial.distance(1.0) - 3.28084).abs() < 1e-6);
    assert_eq!(Units::Imperial.distance_label(), "ft");

    assert_eq!(Units::Metric.temperature(0.0), 0.0);
    assert_eq!(Units::Metric.temperature_label(), "C");
    assert_eq!(Units::Metric.distance(1.0), 1.0);
    assert_eq!(Units::Metric.distance_label(), "m");
}
