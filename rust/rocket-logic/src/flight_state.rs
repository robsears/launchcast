//! Owns flight state transitions and the barometric velocity estimate.
//!
//! Port of `FlightState` and `accel_magnitude` in `rocket/code.py` -- the
//! only part of the rocket firmware that is pure computation, and (per that
//! file's design contract) the logic that gets exercised exactly once per
//! flight, irreversibly, ~300 m up. `FlightLog`, radio TX, and sensor
//! drivers are hardware-dependent and are not part of this port.
//!
//! Altitude is measured above ground level (AGL), referenced to a ground
//! datum captured at ARM time.

use launchcast_common::State;

// --- Tuning --------------------------------------------------------------
// Same values and meaning as the constants of the same name in
// `rocket/code.py`. Keep the two in sync.

/// Any acceleration above this means "liftoff!"
pub const BOOST_THRESHOLD_G: f32 = 3.0;
/// Acceleration must persist this long to avoid accidental bumps.
pub const BOOST_MIN_MS: u32 = 150;
/// Below this, we're done boosting and now coasting.
pub const COAST_THRESHOLD_G: f32 = 1.5;
/// *Vertical* velocity below this at apex.
pub const APOGEE_VEL_MPS: f32 = 1.0;
/// Sustained negative velocity -> under chute.
pub const DESCENT_VEL_MPS: f32 = -2.0;
/// Landing detection: motion near-zero velocity...
pub const LANDED_VEL_MPS: f32 = 0.5;
/// ...near ground level...
pub const LANDED_ALT_M: f32 = 15.0;
/// ...for this long, means we've landed.
pub const LANDED_HOLD_MS: u32 = 3000;

/// Vertical velocity is unreliable via GPS at high speed, short EMA smooths
/// the barometric derivative instead.
const VEL_ALPHA: f32 = 0.3;

const STANDARD_GRAVITY_MPS2: f32 = 9.80665;

/// Magnitude of a 3-axis accelerometer reading (m/s^2), in g.
pub fn accel_magnitude(accel_mps2: [f32; 3]) -> f32 {
    let [x, y, z] = accel_mps2;
    libm::sqrtf(x * x + y * y + z * z) / STANDARD_GRAVITY_MPS2
}

/// Magnitude of a 3-axis gyroscope reading, in deg/s. Used for
/// `FlightSummary`'s "max rotation" highlight -- a cheap instability
/// sanity check, not a stability/center-of-pressure measurement (that
/// needs the full time series analyzed offline, plus known mass
/// properties this system doesn't have -- see `flight_summary.rs`'s
/// docs on scope).
pub fn gyro_magnitude(gyro_dps: [f32; 3]) -> f32 {
    let [x, y, z] = gyro_dps;
    libm::sqrtf(x * x + y * y + z * z)
}

pub struct FlightState {
    pub state: u8,
    pub ground_pressure: Option<f32>,
    pub alt_m: f32,
    pub vel_mps: f32,
    pub max_alt_m: f32,
    pub entered_ms: u32,
    last_alt: Option<f32>,
    last_t: Option<u32>,
    boost_start: Option<u32>,
    landed_start: Option<u32>,
}

impl Default for FlightState {
    fn default() -> Self {
        Self::new()
    }
}

impl FlightState {
    pub fn new() -> Self {
        Self {
            state: State::BOOT,
            ground_pressure: None,
            alt_m: 0.0,
            vel_mps: 0.0,
            max_alt_m: 0.0,
            entered_ms: 0,
            last_alt: None,
            last_t: None,
            boost_start: None,
            landed_start: None,
        }
    }

    pub fn set_ground_reference(&mut self, pressure_hpa: f32) {
        self.ground_pressure = Some(pressure_hpa);
    }

    /// Standard barometric formula, referenced to the ground datum.
    ///
    /// Mainly an internal step of [`FlightState::update_altitude`]; public
    /// because `tests/flight_state.rs` exercises it directly, mirroring
    /// `test_flight_state.py`'s direct calls to `fs._pressure_to_alt(...)`.
    pub fn pressure_to_alt(&self, pressure_hpa: f32) -> f32 {
        let Some(ground) = self.ground_pressure else {
            return 0.0;
        };
        if ground == 0.0 || pressure_hpa <= 0.0 {
            return 0.0;
        }
        let ratio = pressure_hpa / ground;
        44330.0 * (1.0 - libm::powf(ratio, 0.1903))
    }

    pub fn update_altitude(&mut self, pressure_hpa: f32, now_ms: u32) {
        self.alt_m = self.pressure_to_alt(pressure_hpa);
        if self.alt_m > self.max_alt_m {
            self.max_alt_m = self.alt_m;
        }

        if let (Some(last_alt), Some(last_t)) = (self.last_alt, self.last_t) {
            // Signed subtraction, not wrapping: this mirrors the Python
            // version's plain `now_ms - self._last_t`, which the `dt >
            // 0.001` guard below relies on going negative (not wrapping to
            // a huge positive) if a caller ever passes a non-monotonic
            // `now_ms`.
            let dt = (now_ms as f32 - last_t as f32) / 1000.0;
            if dt > 0.001 {
                let raw = (self.alt_m - last_alt) / dt;
                self.vel_mps += VEL_ALPHA * (raw - self.vel_mps);
            }
        }

        self.last_alt = Some(self.alt_m);
        self.last_t = Some(now_ms);
    }

    pub fn transition(&mut self, new_state: u8, now_ms: u32) -> bool {
        if new_state != self.state {
            self.state = new_state;
            self.entered_ms = now_ms;
            true
        } else {
            false
        }
    }

    /// Advance the state machine. Returns `true` if the state changed.
    ///
    /// BOOT and IDLE and ARMED transitions are driven externally (sensor
    /// init, uplink commands). Everything from BOOST onward is autonomous
    /// and one-way -- there is no path back to ARMED in flight.
    pub fn update(&mut self, accel_mag_g: f32, now_ms: u32) -> bool {
        match self.state {
            State::ARMED => {
                if accel_mag_g >= BOOST_THRESHOLD_G {
                    match self.boost_start {
                        None => self.boost_start = Some(now_ms),
                        Some(start) if now_ms.wrapping_sub(start) >= BOOST_MIN_MS => {
                            return self.transition(State::BOOST, now_ms);
                        }
                        Some(_) => {}
                    }
                } else {
                    self.boost_start = None;
                }
            }
            State::BOOST => {
                if accel_mag_g < COAST_THRESHOLD_G {
                    return self.transition(State::COAST, now_ms);
                }
            }
            State::COAST => {
                // Apogee is a VELOCITY event, not an acceleration event.
                if libm::fabsf(self.vel_mps) <= APOGEE_VEL_MPS || self.vel_mps < 0.0 {
                    return self.transition(State::APOGEE, now_ms);
                }
            }
            State::APOGEE => {
                if self.vel_mps <= DESCENT_VEL_MPS {
                    return self.transition(State::DESCENT, now_ms);
                }
            }
            State::DESCENT => {
                let settled =
                    libm::fabsf(self.vel_mps) <= LANDED_VEL_MPS && self.alt_m <= LANDED_ALT_M;
                if settled {
                    match self.landed_start {
                        None => self.landed_start = Some(now_ms),
                        Some(start) if now_ms.wrapping_sub(start) >= LANDED_HOLD_MS => {
                            return self.transition(State::LANDED, now_ms);
                        }
                        Some(_) => {}
                    }
                } else {
                    self.landed_start = None;
                }
            }
            _ => {}
        }
        false
    }
}
