//! Rust port of `tests/test_flight_state.py`. Same synthesized D12-5 flight
//! profile, same state sequence and timing assertions -- see that file's
//! module docstring for why this particular piece gets the most thorough
//! test treatment of anything in the firmware.
//!
//! Test-file-only code (the profile synthesis, `alt_to_pressure`) uses
//! plain `f64`/`std` math (`.powf()`, no `libm`) since integration tests
//! always link against `std` regardless of the crate's own `no_std`-ness --
//! there's no reason to route through `libm` here.

use launchcast_common::State;
use launchcast_rocket_logic::{
    accel_magnitude, FlightState, APOGEE_VEL_MPS, BOOST_MIN_MS, BOOST_THRESHOLD_G,
    COAST_THRESHOLD_G, LANDED_HOLD_MS,
};

const GROUND_HPA: f32 = 1013.25;
const BARO_DT_MS: u32 = 40; // 25 Hz

/// Inverse of `FlightState::pressure_to_alt`.
fn alt_to_pressure(alt_m: f64, ground_hpa: f64) -> f32 {
    (ground_hpa * (1.0 - alt_m / 44330.0).powf(1.0 / 0.1903)) as f32
}

/// Altitude (m) and total acceleration (g) at time `t` seconds.
///
/// Loosely a D12-5 lofting ~190 g: 1.6 s burn to ~90 m, coast to ~300 m at
/// about 7 s, ejection, then ~5 m/s under the chute. Numbers are
/// illustrative, not a simulation -- the point is the SHAPE, which is what
/// the state machine keys on.
fn d12_profile(t: f64) -> (f32, f32) {
    let burn = 1.6_f64;
    let apogee_t = 7.0_f64;
    let g = 9.80665_f64;

    let (alt, a_g) = if t < burn {
        let a_g = 6.5;
        let alt = 0.5 * (a_g * g - g) * t * t;
        (alt, a_g)
    } else if t < apogee_t {
        let v0 = (6.5 * g - g) * burn;
        let alt0 = 0.5 * (6.5 * g - g) * burn * burn;
        let dt = t - burn;
        let alt = alt0 + v0 * dt - 0.5 * g * dt * dt;
        (alt, 1.0) // coasting: only gravity
    } else {
        let v0 = (6.5 * g - g) * burn;
        let alt0 = 0.5 * (6.5 * g - g) * burn * burn;
        let dtc = apogee_t - burn;
        let peak = alt0 + v0 * dtc - 0.5 * g * dtc * dtc;
        let alt = peak - 5.0 * (t - apogee_t); // 5 m/s descent
        (alt, 1.0)
    };

    (alt.max(0.0) as f32, a_g as f32)
}

/// Drive `FlightState` through a profile. Returns `[(t_ms, state), ...]`,
/// starting with the initial `(0, start_state)` entry.
fn run_profile(
    fs: &mut FlightState,
    duration_s: f64,
    dt_ms: u32,
    profile: impl Fn(f64) -> (f32, f32),
    start_state: u8,
) -> Vec<(u32, u8)> {
    fs.set_ground_reference(GROUND_HPA);
    fs.transition(start_state, 0);

    let mut history = vec![(0u32, fs.state)];
    let steps = (duration_s * 1000.0 / dt_ms as f64) as u32;

    for i in 1..=steps {
        let t_ms = i * dt_ms;
        let (alt, a_g) = profile(t_ms as f64 / 1000.0);
        fs.update_altitude(alt_to_pressure(alt as f64, GROUND_HPA as f64), t_ms);
        if fs.update(a_g, t_ms) {
            history.push((t_ms, fs.state));
        }
    }

    history
}

/// `run_profile` with the defaults `test_flight_state.py` uses for most
/// cases: 25 Hz sampling, the D12 profile, starting from ARMED.
fn run_default_profile(fs: &mut FlightState, duration_s: f64) -> Vec<(u32, u8)> {
    run_profile(fs, duration_s, BARO_DT_MS, d12_profile, State::ARMED)
}

fn history_times(history: &[(u32, u8)]) -> std::collections::HashMap<u8, u32> {
    history.iter().map(|&(t, s)| (s, t)).collect()
}

// --- Altitude and velocity -------------------------------------------------

#[test]
fn pressure_to_altitude_is_invertible() {
    let mut fs = FlightState::new();
    fs.set_ground_reference(GROUND_HPA);
    for alt in [0.0, 10.0, 100.0, 300.0, 1000.0] {
        let pressure = alt_to_pressure(alt, GROUND_HPA as f64);
        assert!((fs.pressure_to_alt(pressure) - alt as f32).abs() < 0.5);
    }
}

#[test]
fn altitude_is_zero_without_ground_reference() {
    // Before ARM there is no datum. Reporting a real altitude would be a lie.
    let fs = FlightState::new();
    assert_eq!(fs.pressure_to_alt(900.0), 0.0);
}

#[test]
fn ground_reference_makes_pad_altitude_zero() {
    let mut fs = FlightState::new();
    fs.set_ground_reference(987.6); // not sea level -- Omaha is ~330 m
    assert!(fs.pressure_to_alt(987.6).abs() < 0.01);
}

#[test]
fn velocity_converges_on_a_steady_climb() {
    // EMA has lag by design; after enough samples it must track truth.
    let mut fs = FlightState::new();
    fs.set_ground_reference(GROUND_HPA);
    for i in 0..200u32 {
        let t = i * BARO_DT_MS;
        fs.update_altitude(
            alt_to_pressure(10.0 * t as f64 / 1000.0, GROUND_HPA as f64),
            t,
        );
    }
    assert!((fs.vel_mps - 10.0).abs() < 0.5);
}

#[test]
fn velocity_is_negative_while_descending() {
    let mut fs = FlightState::new();
    fs.set_ground_reference(GROUND_HPA);
    for i in 0..200u32 {
        let t = i * BARO_DT_MS;
        fs.update_altitude(
            alt_to_pressure(300.0 - 5.0 * t as f64 / 1000.0, GROUND_HPA as f64),
            t,
        );
    }
    assert!(fs.vel_mps < -4.0);
}

#[test]
fn max_altitude_is_latched() {
    let mut fs = FlightState::new();
    run_default_profile(&mut fs, 120.0);
    assert!(fs.max_alt_m > fs.alt_m);
}

// --- Full flight -------------------------------------------------------------

#[test]
fn full_flight_visits_every_state_in_order() {
    let mut fs = FlightState::new();
    let history = run_default_profile(&mut fs, 120.0);
    let states: Vec<u8> = history.iter().map(|&(_, s)| s).collect();
    assert_eq!(
        states,
        vec![
            State::ARMED,
            State::BOOST,
            State::COAST,
            State::APOGEE,
            State::DESCENT,
            State::LANDED,
        ]
    );
}

#[test]
fn boost_fires_shortly_after_ignition() {
    let mut fs = FlightState::new();
    let history = history_times(&run_default_profile(&mut fs, 120.0));
    // Must wait out BOOST_MIN_MS, but not much longer.
    let t = history[&State::BOOST];
    assert!((BOOST_MIN_MS..=BOOST_MIN_MS + 200).contains(&t));
}

#[test]
fn burnout_detected_near_end_of_burn() {
    let mut fs = FlightState::new();
    let history = history_times(&run_default_profile(&mut fs, 120.0));
    let t = history[&State::COAST];
    assert!((1500..=1900).contains(&t)); // 1.6 s burn
}

#[test]
fn apogee_detected_near_the_actual_peak() {
    let mut fs = FlightState::new();
    let history = history_times(&run_default_profile(&mut fs, 120.0));
    let t = history[&State::APOGEE];
    assert!((6300..=7600).contains(&t)); // true apogee ~7.0 s
}

#[test]
fn landed_requires_the_hold_period() {
    let mut fs = FlightState::new();
    let history = history_times(&run_default_profile(&mut fs, 120.0));
    assert!(history[&State::LANDED] - history[&State::DESCENT] >= LANDED_HOLD_MS);
}

#[test]
fn apogee_altitude_is_plausible() {
    let mut fs = FlightState::new();
    run_default_profile(&mut fs, 120.0);
    assert!(200.0 < fs.max_alt_m && fs.max_alt_m < 400.0);
}

// --- Transitions are one-way -------------------------------------------------

#[test]
fn no_path_back_to_armed_after_boost() {
    // A rocket that has left the pad must not re-arm mid-flight.
    let mut fs = FlightState::new();
    let history = run_default_profile(&mut fs, 120.0);
    let seen: Vec<u8> = history.iter().map(|&(_, s)| s).collect();
    let boost_idx = seen.iter().position(|&s| s == State::BOOST).unwrap();
    let coast_idx = seen.iter().position(|&s| s == State::COAST).unwrap();
    assert!(boost_idx < coast_idx);
    assert!(!seen[1..].contains(&State::ARMED));
}

#[test]
fn state_never_regresses() {
    let mut fs = FlightState::new();
    let history = run_default_profile(&mut fs, 120.0);
    let values: Vec<u8> = history.iter().map(|&(_, s)| s).collect();
    let mut sorted = values.clone();
    sorted.sort_unstable();
    assert_eq!(values, sorted);
}

// --- Rejection of false triggers ---------------------------------------------

#[test]
fn a_brief_bump_does_not_trigger_boost() {
    // Handling the rocket on the pad must not launch the state machine.
    let mut fs = FlightState::new();
    let bump = |t: f64| -> (f32, f32) {
        // 5 g spike lasting 80 ms -- shorter than BOOST_MIN_MS
        let a = if (1.0..1.08).contains(&t) { 8.0 } else { 1.0 };
        (0.0, a)
    };
    let history = run_profile(&mut fs, 5.0, 10, bump, State::ARMED);
    let states: Vec<u8> = history.iter().map(|&(_, s)| s).collect();
    assert_eq!(states, vec![State::ARMED]);
}

#[test]
fn a_sustained_bump_does_trigger_boost() {
    // The rejection above must not be so aggressive it misses a real launch.
    let mut fs = FlightState::new();
    let sustained = |t: f64| -> (f32, f32) {
        let a = if t >= 1.0 { 8.0 } else { 1.0 };
        let alt = if t < 1.0 {
            0.0
        } else {
            30.0 * (t - 1.0) * (t - 1.0)
        };
        (alt as f32, a)
    };
    let history = run_profile(&mut fs, 5.0, 10, sustained, State::ARMED);
    assert!(history.iter().any(|&(_, s)| s == State::BOOST));
}

#[test]
fn idle_does_not_advance_on_acceleration() {
    // Only an uplink ARM leaves IDLE. Shaking the payload must do nothing.
    let mut fs = FlightState::new();
    let history = run_profile(&mut fs, 5.0, BARO_DT_MS, d12_profile, State::IDLE);
    let states: Vec<u8> = history.iter().map(|&(_, s)| s).collect();
    assert_eq!(states, vec![State::IDLE]);
}

#[test]
fn boot_does_not_advance_on_acceleration() {
    let mut fs = FlightState::new();
    let history = run_profile(&mut fs, 5.0, BARO_DT_MS, d12_profile, State::BOOT);
    let states: Vec<u8> = history.iter().map(|&(_, s)| s).collect();
    assert_eq!(states, vec![State::BOOT]);
}

// --- Thresholds are self-consistent ------------------------------------------

// These compare compile-time constants, so clippy sees an always-true
// assertion -- but they exist as *tests*, not `const { assert!(..) }`,
// specifically so a future change to one of these thresholds that breaks
// the invariant shows up as a `cargo test` failure, not a silent logic
// bug. The lint's suggested const-block form wouldn't run as a test at
// all.
#[test]
#[allow(clippy::assertions_on_constants)]
fn coast_threshold_below_boost_threshold() {
    // Otherwise BOOST and COAST could both be true, or neither.
    assert!(COAST_THRESHOLD_G < BOOST_THRESHOLD_G);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn boost_threshold_above_one_g() {
    // A rocket sitting on the pad reads 1 g. Anything at or below that
    // would fire BOOST the instant it is armed.
    assert!(BOOST_THRESHOLD_G > 1.0);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn coast_threshold_above_free_fall() {
    // Coast reads ~1 g from gravity, so the burnout threshold must sit
    // above it or COAST fires during the burn.
    assert!(COAST_THRESHOLD_G > 1.0);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn apogee_velocity_window_is_tight() {
    // Too wide and apogee fires during coast.
    assert!(0.0 < APOGEE_VEL_MPS && APOGEE_VEL_MPS <= 3.0);
}

// --- accel_magnitude ---------------------------------------------------------

#[test]
fn accel_magnitude_of_rest_is_one_g() {
    assert!((accel_magnitude([0.0, 0.0, 9.80665]) - 1.0).abs() < 1e-6);
}

#[test]
fn accel_magnitude_is_orientation_independent() {
    // The payload sits in the tube at an unknown roll angle. Magnitude
    // must not depend on which axis gravity lands on.
    let g = 9.80665;
    for vec in [[g, 0.0, 0.0], [0.0, g, 0.0], [0.0, 0.0, g], [0.0, 0.0, -g]] {
        assert!((accel_magnitude(vec) - 1.0).abs() < 1e-6);
    }
}

#[test]
fn accel_magnitude_combines_axes() {
    let g = 9.80665;
    assert!((accel_magnitude([3.0 * g, 4.0 * g, 0.0]) - 5.0).abs() < 1e-6);
}

#[test]
fn accel_magnitude_of_free_fall_is_zero() {
    assert_eq!(accel_magnitude([0.0, 0.0, 0.0]), 0.0);
}

// --- Robustness --------------------------------------------------------------

#[test]
fn zero_pressure_does_not_raise() {
    // A failed barometer read can return garbage. It must not panic.
    let mut fs = FlightState::new();
    fs.set_ground_reference(GROUND_HPA);
    fs.update_altitude(0.0, 100);
    assert_eq!(fs.alt_m, 0.0);
}

#[test]
fn repeated_timestamps_do_not_divide_by_zero() {
    let mut fs = FlightState::new();
    fs.set_ground_reference(GROUND_HPA);
    fs.update_altitude(alt_to_pressure(10.0, GROUND_HPA as f64), 1000);
    fs.update_altitude(alt_to_pressure(20.0, GROUND_HPA as f64), 1000); // same t
}

#[test]
fn apogee_detection_survives_rate_changes() {
    // If the loop runs slower than intended, apogee must still be found.
    for dt_ms in [20, 40, 100] {
        let mut fs = FlightState::new();
        let history = run_profile(&mut fs, 120.0, dt_ms, d12_profile, State::ARMED);
        assert!(
            history.iter().any(|&(_, s)| s == State::APOGEE),
            "apogee not detected at dt_ms={}",
            dt_ms
        );
    }
}
