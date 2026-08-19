//! Shared-bus type alias. BMP580, LSM6DSOX, LIS3MDL, and the GPS are all
//! on the same physical I2C bus (`STEMMA_I2C`, confirmed via CLAUDE.md's
//! address list) -- each driver gets its own lightweight
//! `RefCellDevice` handle onto the one real peripheral, constructed in
//! `main.rs` from a `StaticCell`-allocated `RefCell` (needs `'static`
//! for the embassy tasks that use it).

use embassy_rp::i2c::{Blocking, I2c};
use embassy_rp::peripherals::I2C1;
use embedded_hal_bus::i2c::RefCellDevice;

pub type SharedI2cDevice = RefCellDevice<'static, I2c<'static, I2C1, Blocking>>;
