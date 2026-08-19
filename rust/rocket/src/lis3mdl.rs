//! LIS3MDL magnetometer -- presence check only. See
//! `rocket-logic::lis3mdl`'s module docs for why a full driver isn't
//! built: nothing in this system ever reads magnetometer data (not
//! `rocket/code.py`'s main loop, not the wire telemetry format), so
//! there's nothing beyond "is it there" to port.

use embedded_hal::i2c::I2c;
use launchcast_rocket_logic::lis3mdl::{CHIP_ID, I2C_ADDR, REG_WHO_AM_I};

/// `Ok(())` if the chip responds and reports the expected `WHO_AM_I` --
/// matches `adafruit_lis3mdl.LIS3MDL.__init__`'s own presence check
/// (`if self._chip_id != _LIS3MDL_CHIP_ID: raise RuntimeError(...)`).
pub fn probe<I: I2c>(i2c: &mut I) -> bool {
    let mut buf = [0u8; 1];
    i2c.write_read(I2C_ADDR, &[REG_WHO_AM_I], &mut buf).is_ok() && buf[0] == CHIP_ID
}
