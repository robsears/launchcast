//! IMU-derived quantities for display. Port of `ground/imu.py`.
//!
//! `accel_g` in telemetry is already per-axis g (see `common/packet.py`),
//! so the magnitude below needs no unit conversion -- it's what an
//! accelerometer actually measures: an unmoving rocket reads ~1.0g
//! (gravity), not 0. That's also the convention `rocket/code.py`'s own
//! `accel_magnitude` (`launchcast_rocket_logic::accel_magnitude`) and its
//! `BOOST_THRESHOLD_G`/`COAST_THRESHOLD_G` are defined against, so
//! displaying the raw magnitude here (not magnitude - 1) keeps the ground
//! display consistent with what the flight computer is actually deciding
//! on.

pub fn accel_magnitude_g(accel_g: [f32; 3]) -> f32 {
    let [x, y, z] = accel_g;
    libm::sqrtf(x * x + y * y + z * z)
}
