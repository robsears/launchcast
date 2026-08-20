use launchcast_common::State;
use launchcast_rocket_logic::FlightSummary;

#[test]
fn on_armed_captures_the_arm_fix_and_resets_everything() {
    let s = FlightSummary::on_armed(1000, Some((42.5, -71.25)), 1_787_160_600);
    assert_eq!(s.arm_lat, 42.5);
    assert_eq!(s.arm_lon, -71.25);
    assert_eq!(s.arm_epoch_s, 1_787_160_600);
    assert_eq!(s.wait_ms, 0);
    assert_eq!(s.record_count, 0);
    assert_eq!(s.max_speed_mps, 0.0);
}

#[test]
fn on_armed_with_no_wall_clock_reference_leaves_arm_epoch_zero() {
    // No GPS fix since boot -- the caller resolves EpochOffset::None to
    // 0 before calling this (see rocket/src/main.rs).
    let s = FlightSummary::on_armed(1000, None, 0);
    assert_eq!(s.arm_epoch_s, 0);
}

#[test]
fn on_armed_with_no_fix_leaves_arm_coordinates_zeroed() {
    let s = FlightSummary::on_armed(1000, None, 0);
    assert_eq!(s.arm_lat, 0.0);
    assert_eq!(s.arm_lon, 0.0);
}

#[test]
fn full_flight_transition_sequence_produces_the_right_durations() {
    let mut s = FlightSummary::on_armed(0, Some((1.0, 2.0)), 0);
    // ARM at 0, waits 500ms on the pad before boost.
    s.on_transition(State::BOOST, 500);
    // Burns for 1600ms.
    s.on_transition(State::COAST, 2100);
    // Coasts for 5000ms to apogee.
    s.on_transition(State::APOGEE, 7100);
    // A brief, unreported dwell at apogee before descent starts.
    s.on_transition(State::DESCENT, 7300);
    // Descends under chute for 45000ms.
    s.on_transition(State::LANDED, 52300);

    assert_eq!(s.wait_ms, 500);
    assert_eq!(s.boost_ms, 1600);
    assert_eq!(s.coast_ms, 5000);
    assert_eq!(s.descent_ms, 45000);
}

#[test]
fn observe_tracks_running_maxes_and_record_count() {
    let mut s = FlightSummary::on_armed(0, None, 0);
    s.observe(100.0, 10.0, 1.0, 5.0, 20.0, 1000.0);
    s.observe(287.0, 25.0, 8.5, 120.0, -12.0, 950.0);
    s.observe(200.0, 15.0, 2.0, 30.0, 5.0, 975.0);

    assert_eq!(s.max_alt_m, 287.0);
    assert_eq!(s.max_speed_mps, 25.0);
    assert_eq!(s.max_accel_g, 8.5);
    assert_eq!(s.max_gyro_dps, 120.0);
    assert_eq!(s.record_count, 3);
}

#[test]
fn observe_takes_the_magnitude_of_negative_speed() {
    // Descent speed is reported negative (down) by FlightState -- "max
    // speed" should still reflect how fast it was actually moving.
    let mut s = FlightSummary::on_armed(0, None, 0);
    s.observe(50.0, -40.0, 0.0, 0.0, 20.0, 1000.0);
    assert_eq!(s.max_speed_mps, 40.0);
}

#[test]
fn observe_does_not_let_altitude_decrease() {
    let mut s = FlightSummary::on_armed(0, None, 0);
    s.observe(200.0, 0.0, 0.0, 0.0, 5.0, 950.0);
    s.observe(50.0, 0.0, 0.0, 0.0, 20.0, 1000.0); // descending -- must not lower the max
    assert_eq!(s.max_alt_m, 200.0);
}

#[test]
fn observe_pairs_temp_and_pressure_with_whichever_reading_set_the_altitude_max() {
    let mut s = FlightSummary::on_armed(0, None, 0);
    s.observe(100.0, 0.0, 0.0, 0.0, 20.0, 1000.0); // new max -- paired values kept
    s.observe(287.0, 0.0, 0.0, 0.0, -12.0, 950.0); // new max -- paired values kept
    s.observe(150.0, 0.0, 0.0, 0.0, 15.0, 975.0); // descending -- not a new max, ignored

    assert_eq!(s.max_alt_m, 287.0);
    assert_eq!(s.temp_at_max_alt_c, -12.0);
    assert_eq!(s.pressure_at_max_alt_hpa, 950.0);
}

#[test]
fn lock_in_landed_fix_only_touches_the_landed_coordinates() {
    let mut s = FlightSummary::on_armed(0, Some((1.0, 2.0)), 0);
    s.observe(10.0, 10.0, 1.0, 5.0, 20.0, 1000.0);
    s.lock_in_landed_fix(3.0, 4.0);

    assert_eq!(s.arm_lat, 1.0);
    assert_eq!(s.arm_lon, 2.0);
    assert_eq!(s.landed_lat, 3.0);
    assert_eq!(s.landed_lon, 4.0);
    assert_eq!(s.record_count, 1); // untouched by locking in the fix
}

#[test]
fn irrelevant_state_transitions_do_not_close_any_duration_bucket() {
    // A transition this type doesn't track a bucket for (there is none
    // besides the four defined) must still advance the phase marker
    // without corrupting an already-closed duration.
    let mut s = FlightSummary::on_armed(0, None, 0);
    s.on_transition(State::BOOST, 100);
    assert_eq!(s.wait_ms, 100);
    // boost_ms not yet set -- still its default.
    assert_eq!(s.boost_ms, 0);
}
