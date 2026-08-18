//! Rust port of `tests/test_nav.py`.

use launchcast_ground_logic::{bearing_deg, compass_point, haversine_m, relative_arrow};

const SF_LAT: f32 = 37.7749;
const SF_LON: f32 = -122.4194;
const LA_LAT: f32 = 34.0522;
const LA_LON: f32 = -118.2437;
const SF_TO_LA_KM: f32 = 559.0; // well-known great-circle distance, +/- a few km

// --- haversine_m ---------------------------------------------------------

#[test]
fn haversine_same_point_is_zero() {
    assert_eq!(haversine_m(SF_LAT, SF_LON, SF_LAT, SF_LON), 0.0);
}

#[test]
fn haversine_sf_to_la_matches_known_distance() {
    let d_km = haversine_m(SF_LAT, SF_LON, LA_LAT, LA_LON) / 1000.0;
    assert!((d_km - SF_TO_LA_KM).abs() < 5.0);
}

#[test]
fn haversine_is_symmetric() {
    let d1 = haversine_m(SF_LAT, SF_LON, LA_LAT, LA_LON);
    let d2 = haversine_m(LA_LAT, LA_LON, SF_LAT, SF_LON);
    assert!((d1 - d2).abs() < 1e-3); // f32, not Python's f64 -- looser than 1e-6
}

// --- bearing_deg -----------------------------------------------------------

#[test]
fn bearing_due_north() {
    assert!((bearing_deg(0.0, 0.0, 1.0, 0.0) - 0.0).abs() < 1e-4);
}

#[test]
fn bearing_due_east() {
    assert!((bearing_deg(0.0, 0.0, 0.0, 1.0) - 90.0).abs() < 1e-4);
}

#[test]
fn bearing_due_south() {
    assert!((bearing_deg(0.0, 0.0, -1.0, 0.0) - 180.0).abs() < 1e-4);
}

#[test]
fn bearing_due_west() {
    assert!((bearing_deg(0.0, 0.0, 0.0, -1.0) - 270.0).abs() < 1e-4);
}

#[test]
fn bearing_always_in_range() {
    for (lat2, lon2) in [(5.0, 5.0), (-5.0, -5.0), (5.0, -5.0), (-5.0, 5.0)] {
        let b = bearing_deg(0.0, 0.0, lat2, lon2);
        assert!((0.0..360.0).contains(&b));
    }
}

// --- compass_point -----------------------------------------------------------

#[test]
fn compass_point_cardinals() {
    assert_eq!(compass_point(0.0), "N");
    assert_eq!(compass_point(90.0), "E");
    assert_eq!(compass_point(180.0), "S");
    assert_eq!(compass_point(270.0), "W");
}

#[test]
fn compass_point_wraps_at_north() {
    // 360 - 11.25 rounds up into N, not NNW
    assert_eq!(compass_point(349.0), "N");
    assert_eq!(compass_point(0.0), "N");
    assert_eq!(compass_point(11.0), "N");
}

// --- relative_arrow ------------------------------------------------------

#[test]
fn relative_arrow_none_heading_is_none() {
    assert_eq!(relative_arrow(90.0, None), None);
}

#[test]
fn relative_arrow_ahead_when_aligned() {
    assert_eq!(relative_arrow(90.0, Some(90.0)), Some("^ AHEAD"));
}

#[test]
fn relative_arrow_turn_around_when_opposite() {
    assert_eq!(relative_arrow(90.0, Some(270.0)), Some("v TURN AROUND"));
}

#[test]
fn relative_arrow_right_when_target_is_clockwise() {
    assert_eq!(relative_arrow(90.0, Some(0.0)), Some(">> RIGHT"));
}

#[test]
fn relative_arrow_left_when_target_is_counterclockwise() {
    assert_eq!(relative_arrow(0.0, Some(90.0)), Some("<< LEFT"));
}

#[test]
fn relative_arrow_covers_full_circle_without_crashing() {
    let mut heading = 0;
    while heading < 360 {
        assert!(relative_arrow(180.0, Some(heading as f32)).is_some());
        heading += 5;
    }
}
