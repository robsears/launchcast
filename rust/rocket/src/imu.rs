//! LSM6DSOX IMU -- the I2C transaction layer, thin over the `lsm6dsox`
//! crate. Unit conversion (raw counts -> m/s²/deg-s) lives in
//! `rocket-logic::imu` -- see its module docs for why this driver
//! deliberately reads raw counts (`accel_raw()`/`angular_rate_raw()`)
//! rather than the crate's "normalized" `accel_norm()`/`angular_rate()`
//! methods.

use embassy_time::Delay;
use embedded_hal::i2c::I2c;
use launchcast_rocket_logic::imu::{raw_to_accel_mps2, raw_to_gyro_dps};
use lsm6dsox::accelerometer::RawAccelerometer;
use lsm6dsox::{AccelerometerScale, DataRate, GyroscopeScale, Lsm6dsox, SlaveAddress};

#[derive(Debug, defmt::Format)]
pub enum ImuError {
    SetupFailed,
}

pub struct Imu<I: I2c> {
    dev: Lsm6dsox<I, Delay>,
}

impl<I: I2c> Imu<I> {
    /// Reset and configure -- accel ±4g / gyro ±250dps at ~104Hz, matching
    /// `adafruit_lsm6ds`'s own defaults (see `rocket-logic::imu`'s docs on
    /// why matching the range specifically matters: BOOST_THRESHOLD_G=3.0
    /// needs headroom a narrower range wouldn't have).
    pub fn new(i2c: I) -> Result<Self, ImuError> {
        let mut dev = Lsm6dsox::new(i2c, SlaveAddress::Low, Delay);
        dev.setup().map_err(|_| ImuError::SetupFailed)?;
        dev.set_accel_sample_rate(DataRate::Freq104Hz).map_err(|_| ImuError::SetupFailed)?;
        dev.set_accel_scale(AccelerometerScale::Accel4g).map_err(|_| ImuError::SetupFailed)?;
        dev.set_gyro_sample_rate(DataRate::Freq104Hz).map_err(|_| ImuError::SetupFailed)?;
        dev.set_gyro_scale(GyroscopeScale::Dps250).map_err(|_| ImuError::SetupFailed)?;
        Ok(Self { dev })
    }

    /// `(accel_mps2, gyro_dps)` if a fresh sample is ready, `None`
    /// otherwise -- matches `rocket/code.py`'s own forgiving pattern
    /// (`try: ... except: pass`, keep the previous reading on failure)
    /// rather than treating "not ready yet" (normal at this poll rate)
    /// as an error.
    pub fn read(&mut self) -> Option<([f32; 3], [f32; 3])> {
        let accel_raw = self.dev.accel_raw().ok()?;
        let gyro_raw = self.dev.angular_rate_raw().ok()?;
        let accel = raw_to_accel_mps2([accel_raw.x, accel_raw.y, accel_raw.z]);
        let gyro = raw_to_gyro_dps([gyro_raw.x, gyro_raw.y, gyro_raw.z]);
        Some((accel, gyro))
    }
}
