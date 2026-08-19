//! Rocket's own GPS (PA1010D over I2C). Port of the GPS half of
//! `rocket/code.py`'s main loop.
//!
//! Sentence parsing (`NmeaLineReader`, `parse_rmc`, `framed_command`)
//! lives in `common::nmea`, shared with the ground station's own GPS
//! (`ground/src/gps.rs`) -- this module is only the I2C transport.
//!
//! Simplification vs `rocket/code.py`: satellite count always reports 0.
//! `adafruit_gps` tracks it from `$..GGA` sentences; this port only
//! parses `$..RMC` (same scope `common::nmea` already had for the ground
//! station), which has no satellite-count field at all. Not flight-
//! critical -- purely informational -- so a second sentence-type parser
//! for one diagnostic field wasn't judged worth the scope here. Can be
//! added later if that field turns out to matter.
//!
//! Unlike the ground station's GPS, this publishes the *latest* fix
//! directly, not a rolling average -- `rocket/code.py` doesn't average
//! either (`lat = hw.gps.latitude or 0.0`, straight off the just-parsed
//! sentence), and this GPS's job (position during/after an actual flight)
//! isn't the ground station's "settle a stationary reading" problem the
//! averaging exists for.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use embedded_hal::i2c::I2c;
use heapless::String;
use launchcast_common::nmea::{framed_command, parse_rmc, NmeaLineReader};

/// Confirmed via CLAUDE.md -- the PA1010D's fixed I2C address. Generic
/// over `I2c` (not the concrete `embassy_rp::i2c::I2c`) -- this GPS
/// shares the same physical bus as the BMP580/LSM6DSOX/LIS3MDL (all four
/// on `STEMMA_I2C`, per CLAUDE.md's address list), so it's handed a
/// shared-bus device wrapper (`embedded_hal_bus::i2c::RefCellDevice`),
/// same as those other drivers, not a dedicated peripheral instance.
const GPS_I2C_ADDR: u8 = 0x10;
/// Matches `adafruit_gps`'s I2C `_fill()` default chunk size.
const CHUNK_SIZE: usize = 32;
/// Matches `rocket/code.py`'s `GPS_HZ = 1` (via `PMTK220,1000` below) --
/// this GPS doesn't need the ground station's faster-sampling-for-
/// averaging treatment, so polls at a plain, unhurried cadence.
const POLL_PERIOD_MS: u64 = 200;

#[derive(Debug, Clone, Copy)]
pub struct GpsFix {
    pub has_fix: bool,
    pub lat: f32,
    pub lon: f32,
    // No `satellites` field -- see this module's docs on why it's a
    // fixed 0 in telemetry (no GGA parsing), so there'd be nothing to
    // ever assign here.
}

/// Latest GPS reading. `has_fix` flips to `false` on an invalid sentence
/// (unlike the ground station's latched last-known fix) -- matches
/// `rocket/code.py`'s `else: has_fix = False` exactly; `lat`/`lon` are
/// left at their last value either way, same as Python's locals only
/// ever being reassigned in the `if has_fix:` branch.
pub static GPS_FIX: Mutex<CriticalSectionRawMutex, GpsFix> = Mutex::new(GpsFix {
    has_fix: false,
    lat: 0.0,
    lon: 0.0,
});

fn send_command<I: I2c>(i2c: &mut I, payload: &str) {
    let cmd: String<48> = framed_command(payload);
    // Best-effort, matches ground/src/gps.rs -- if the chip doesn't ack,
    // it still runs on its own defaults.
    let _ = i2c.write(GPS_I2C_ADDR, cmd.as_bytes());
}

#[embassy_executor::task]
pub async fn gps_task(mut i2c: crate::i2c_bus::SharedI2cDevice) {
    // Matches rocket/code.py's Hardware._init_gps exactly (all four
    // commands, unlike the ground station which skips PMTK314/220).
    send_command(&mut i2c, "PMTK314,0,1,0,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0");
    send_command(&mut i2c, "PMTK220,1000");
    send_command(&mut i2c, "PMTK313,1"); // enable SBAS satellite search
    send_command(&mut i2c, "PMTK301,2"); // DGPS correction source = WAAS
    Timer::after_millis(POLL_PERIOD_MS).await;

    let mut reader: NmeaLineReader<96> = NmeaLineReader::new();
    let mut chunk = [0u8; CHUNK_SIZE];

    loop {
        if i2c.read(GPS_I2C_ADDR, &mut chunk).is_ok() {
            for &byte in &chunk {
                let Some(line) = reader.feed(byte) else {
                    continue;
                };
                let Some(fix) = parse_rmc(&line) else {
                    continue;
                };
                let mut g = GPS_FIX.lock().await;
                if fix.valid {
                    g.has_fix = true;
                    g.lat = fix.lat;
                    g.lon = fix.lon;
                } else {
                    g.has_fix = false;
                }
            }
        }
        Timer::after_millis(POLL_PERIOD_MS).await;
    }
}
