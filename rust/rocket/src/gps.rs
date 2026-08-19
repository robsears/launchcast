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
//! Unlike `rocket/code.py` (`lat = hw.gps.latitude or 0.0`, straight off
//! the just-parsed sentence, always), this averages fixes the same way
//! the ground station does ([`FixAverage`], `common::fix_average`'s
//! fixed-capacity ring buffer, published continuously as new samples
//! arrive) -- but only while the rocket is stationary and something
//! might actually read the result: BOOT/IDLE waiting on the pad, and
//! LANDED during recovery. The instant flight is detected (ARMED through
//! DESCENT -- see [`should_average`]) this publishes the latest raw
//! sample directly instead, both because averaging a moving GPS just
//! adds lag to a position nothing reads until recovery anyway
//! (`gps_period_ms` in `main.rs` already pauses GPS *polling* entirely
//! for those states) and because letting the ring buffer keep filling
//! through a flight would delay how quickly the first post-landing
//! average sheds stale airborne samples -- [`should_average`]'s
//! transition edge resets it instead of waiting that out.

use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use embedded_hal::i2c::I2c;
use heapless::String;
use launchcast_common::fix_average::FixAverage;
use launchcast_common::nmea::{framed_command, parse_rmc, NmeaLineReader};
use launchcast_common::State;

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

/// Current flight phase, published by `flight_task` (`main.rs`) every
/// loop tick so this task can decide whether to average incoming fixes
/// or pass them through raw -- see module docs and [`should_average`].
/// `Relaxed`: `gps_task` and `flight_task` both run on the same
/// cooperative, single-threaded core1 executor, so there's no real
/// concurrent-write race here, just a "read the latest published value"
/// need.
pub static FLIGHT_STATE: AtomicU8 = AtomicU8::new(State::BOOT);

/// Average incoming fixes while the rocket is stationary and something
/// will eventually read the result (BOOT/IDLE on the pad, LANDED during
/// recovery); publish raw the instant it's plausibly moving (ARMED --
/// the countdown to boost -- through DESCENT). See module docs for why
/// ARMED is grouped with "stationary" here even though the rocket could
/// in principle be handled at that moment: it's still sitting on the pad
/// pre-launch, no different from IDLE, and `gps_period_ms` doesn't even
/// poll GPS again until BOOST clears anyway.
fn should_average(state: u8) -> bool {
    !matches!(state, State::BOOST | State::COAST | State::APOGEE | State::DESCENT)
}

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
    let mut avg = FixAverage::new();
    let mut averaging = should_average(FLIGHT_STATE.load(Ordering::Relaxed));

    loop {
        // -- flight-phase edge: start the next average clean rather than
        // carrying over samples from whichever period (airborne or
        // stationary) just ended -- see module docs. The ring buffer
        // would eventually evict a stale run on its own (see
        // common::fix_average's docs), but not until WINDOW_SAMPLES more
        // samples arrive -- an explicit reset here is instant instead. --
        let now_averaging = should_average(FLIGHT_STATE.load(Ordering::Relaxed));
        if now_averaging != averaging {
            avg.reset();
            averaging = now_averaging;
        }

        if i2c.read(GPS_I2C_ADDR, &mut chunk).is_ok() {
            for &byte in &chunk {
                let Some(line) = reader.feed(byte) else {
                    continue;
                };
                let Some(fix) = parse_rmc(&line) else {
                    continue;
                };
                // has_fix flips immediately either way -- matches this
                // module's original (and still deliberate) behavior of
                // not latching a stale fix through a lost signal, unlike
                // the ground station. Only lat/lon go through the
                // average.
                let mut g = GPS_FIX.lock().await;
                g.has_fix = fix.valid;
                if fix.valid {
                    if averaging {
                        avg.add(fix.lat, fix.lon);
                        if let Some((lat, lon)) = avg.mean() {
                            g.lat = lat;
                            g.lon = lon;
                        }
                    } else {
                        g.lat = fix.lat;
                        g.lon = fix.lon;
                    }
                }
            }
        }

        Timer::after_millis(POLL_PERIOD_MS).await;
    }
}
