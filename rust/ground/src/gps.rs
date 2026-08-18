//! Handheld's own GPS (PA1010D over I2C), for computing distance/bearing
//! to the rocket's last known fix on the RECOVERY screen. Port of the GPS
//! half of `Hardware`/`code.py`'s main loop.
//!
//! Sentence parsing (`NmeaLineReader`, `parse_rmc`) lives in
//! `ground-logic`, hardware-free and host-tested (see its module docs for
//! why sending `PMTK*` configuration commands is skipped for now) -- this
//! module is only the I2C transport loop wired to real hardware.

use embassy_rp::i2c::{Blocking, I2c};
use embassy_rp::peripherals::I2C1;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use launchcast_ground_logic::{parse_rmc, NmeaLineReader};

/// Confirmed via CLAUDE.md and `adafruit_gps`'s own default -- the
/// PA1010D's fixed I2C address.
const GPS_I2C_ADDR: u16 = 0x10;
/// Matches `adafruit_gps`'s I2C `_fill()` default chunk size.
const CHUNK_SIZE: usize = 32;
/// How often to poll I2C for new bytes -- well under the GPS's ~1Hz fix
/// rate, so a sentence is picked up promptly once it's actually ready.
const POLL_PERIOD_MS: u64 = 200;
/// Matches `code.py`'s "course over ground substitutes for a compass...
/// only valid while moving" gate.
const MIN_SPEED_FOR_HEADING_KNOTS: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
pub struct MyFix {
    pub lat: f32,
    pub lon: f32,
    pub heading: Option<f32>,
}

/// Latest own-GPS fix. `None` until the first valid `$..RMC` sentence
/// with status `A` is seen -- never overwritten by an invalid (`V`)
/// sentence, so a fix once acquired doesn't flicker away on a single bad
/// read, matching `code.py`'s own `if hw.gps.has_fix:` gate (which only
/// ever assigns `my_lat`/`my_lon`, never clears them).
pub static MY_GPS: Mutex<CriticalSectionRawMutex, Option<MyFix>> = Mutex::new(None);

#[embassy_executor::task]
pub async fn gps_task(mut i2c: I2c<'static, I2C1, Blocking>) {
    let mut reader: NmeaLineReader<96> = NmeaLineReader::new();
    let mut chunk = [0u8; CHUNK_SIZE];
    loop {
        if i2c.blocking_read(GPS_I2C_ADDR, &mut chunk).is_ok() {
            for &byte in &chunk {
                let Some(line) = reader.feed(byte) else {
                    continue;
                };
                let Some(fix) = parse_rmc(&line) else {
                    continue;
                };
                if !fix.valid {
                    continue;
                }
                let heading = if fix.speed_knots > MIN_SPEED_FOR_HEADING_KNOTS {
                    fix.track_deg
                } else {
                    None
                };
                *MY_GPS.lock().await = Some(MyFix {
                    lat: fix.lat,
                    lon: fix.lon,
                    heading,
                });
            }
        }
        Timer::after_millis(POLL_PERIOD_MS).await;
    }
}
