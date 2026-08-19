//! Hardware-free half of the LIS3MDL magnetometer driver: just the
//! register constants needed for a presence check.
//!
//! Deliberately minimal, not a full driver: checked `rocket/code.py`'s
//! main loop and confirmed `hw.mag` is never actually read anywhere in
//! it (magnetometer setup only ever feeds the `Sensor::MAG` presence
//! bit) -- and the wire telemetry format itself (`common::pack_telemetry`)
//! has no field for magnetometer data at all, so there's nowhere for a
//! reading to go even if one were taken. `MAG` is also not in
//! `Sensor::REQUIRED`. So this port only implements what's actually used:
//! confirm the chip responds and is who it says it is. A continuous-mode
//! config + `magnetic()` reader can be added later if a real use for the
//! data shows up -- not built speculatively now.

/// Confirmed via CLAUDE.md.
pub const I2C_ADDR: u8 = 0x1C;

pub const REG_WHO_AM_I: u8 = 0x0F;
/// Expected `WHO_AM_I` value -- matches `adafruit_lis3mdl`'s `_LIS3MDL_CHIP_ID`.
pub const CHIP_ID: u8 = 0x3D;
