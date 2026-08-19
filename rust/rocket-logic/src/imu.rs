//! Hardware-free half of the LSM6DSOX IMU wiring: raw-counts-to-physical-
//! units conversion. The actual I2C driver comes from the `lsm6dsox`
//! crate (see `docs/rust-rewrite.md`'s ecosystem check) -- this module
//! exists because of a real surprise found while integrating it: its
//! `Accelerometer` trait implementation (`accel_norm()`) returns values
//! in **g**, not the m/s² the `accelerometer` crate's trait is
//! conventionally documented to return -- confirmed by checking the
//! crate's own sensitivity constant (`AccelerometerScale::Accel4g =>
//! 0.000122`, which is the sensor's datasheet mg/LSB spec, not an m/s²
//! figure). Trusting the trait's semantic label instead of that number
//! would have fed g-unit values into [`FlightState`](crate::FlightState),
//! which expects m/s² input (it internally divides by standard gravity
//! again) -- silently wrong by a factor of ~9.8, exactly during boost
//! detection.
//!
//! To avoid relying on either trait wrapper's implied units again, this
//! driver reads raw counts (`accel_raw()`/`angular_rate_raw()`, not the
//! "normalized" methods) and does its own scaling here, against the
//! sensor's documented per-LSB sensitivity at the range this project
//! configures -- the same range `adafruit_lsm6ds`'s default already uses
//! (±4g / ±250dps), so behavior matches the current production system.

const STANDARD_GRAVITY_MPS2: f32 = 9.80665;

/// LSM6DSOX accelerometer sensitivity at the `Accel4g` full-scale range:
/// 0.122 mg/LSB (ST datasheet spec; matches both `adafruit_lsm6ds`'s
/// default range and the `lsm6dsox` crate's own `AccelerometerScale::
/// Accel4g` factor).
const ACCEL_G_PER_LSB: f32 = 0.000122;

/// LSM6DSOX gyroscope sensitivity at the `Dps250` full-scale range: 8.75
/// mdps/LSB (ST datasheet spec; matches `adafruit_lsm6ds`'s default
/// range and the `lsm6dsox` crate's own `GyroscopeScale::Dps250` factor).
const GYRO_DPS_PER_LSB: f32 = 0.008750;

/// Raw accelerometer counts -> m/s² (not g -- see module docs).
pub fn raw_to_accel_mps2(raw: [i16; 3]) -> [f32; 3] {
    raw.map(|v| v as f32 * ACCEL_G_PER_LSB * STANDARD_GRAVITY_MPS2)
}

/// Raw gyroscope counts -> degrees/s.
pub fn raw_to_gyro_dps(raw: [i16; 3]) -> [f32; 3] {
    raw.map(|v| v as f32 * GYRO_DPS_PER_LSB)
}
