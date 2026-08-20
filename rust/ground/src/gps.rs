//! Handheld's own GPS (PA1010D over I2C), for computing distance/bearing
//! to the rocket's last known fix on the RECOVERY screen. Port of the GPS
//! half of `Hardware`/`code.py`'s main loop.
//!
//! Sentence parsing (`NmeaLineReader`, `parse_rmc`, `checksum`,
//! `framed_command`) lives in `common::nmea`, shared with the rocket's
//! own GPS (`rocket/src/gps.rs`) and hardware-free/host-tested there --
//! this module is only the I2C transport, both the read loop and the
//! `PMTK*` init writes that enable SBAS/WAAS correction on this GPS to
//! match `rocket/code.py`'s (see `common::nmea`'s docs for why the
//! accuracy gap that motivated this existed at all).
//!
//! Published fixes are a rolling [`FixAverage`] over the most recent
//! `common::fix_average::WINDOW_SAMPLES` fixes, not the single latest
//! sentence: this GPS's whole job is rangefinding a *stationary* handheld
//! (see CLAUDE.md/the session that added the PMTK313/301/397 init
//! above), and a raw instantaneous fix visibly wanders several meters
//! sample-to-sample even at rest. Averaging settles that out, at the cost
//! of the displayed position lagging real motion slightly -- an
//! explicit, acceptable trade for a rangefinder, not something a
//! moving-vehicle tracker could get away with. Published every loop
//! tick, continuously, not on a periodic window -- see
//! `common::fix_average`'s docs for why a fixed-capacity ring buffer
//! replaced an earlier snapshot-and-reset-every-N-seconds design
//! (2026-08-19, after real-hardware feedback that a full-flight distance
//! reading took a few real minutes to settle after cold start).

use embassy_rp::i2c::{Blocking, I2c};
use embassy_rp::peripherals::I2C1;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Instant, Timer};
use heapless::String;
use launchcast_ground_logic::{parse_rmc, FixAverage, NmeaLineReader};
use launchcast_common::epoch::EpochOffset;
use launchcast_common::nmea::framed_command;

/// Confirmed via CLAUDE.md and `adafruit_gps`'s own default -- the
/// PA1010D's fixed I2C address.
const GPS_I2C_ADDR: u16 = 0x10;
/// Matches `adafruit_gps`'s I2C `_fill()` default chunk size.
const CHUNK_SIZE: usize = 32;
/// How often to poll I2C for new bytes. Independent of the GPS's own fix
/// rate (see `PMTK220,100` below) -- this just needs to be frequent
/// enough that a `CHUNK_SIZE`-byte read doesn't fall behind whatever the
/// chip is actually producing.
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

/// Wall-clock reference, captured once on this board's first valid fix
/// with a decoded UTC time -- see `common::epoch`'s docs. Independent
/// of the rocket's own `EPOCH_OFFSET` (`rocket/src/gps.rs`): each board
/// has its own GPS and its own monotonic clock, so there's no cross-
/// board synchronization here, just the same math done twice. Read by
/// `screen_header.rs` for the header's wall-clock display.
pub static EPOCH_OFFSET: Mutex<CriticalSectionRawMutex, Option<EpochOffset>> = Mutex::new(None);

fn send_command(i2c: &mut I2c<'static, I2C1, Blocking>, payload: &str) {
    let cmd: String<32> = framed_command(payload);
    // Best-effort -- if the chip doesn't ack, the GPS still works on
    // whatever its own defaults are, same fallback posture as skipping
    // PMTK314/PMTK220 entirely (see module docs).
    let _ = i2c.blocking_write(GPS_I2C_ADDR, cmd.as_bytes());
}

#[embassy_executor::task]
pub async fn gps_task(mut i2c: I2c<'static, I2C1, Blocking>) {
    // Session-only (reset by a power cycle), matches rocket/code.py and
    // the original ground/code.py's own GPS init -- see module docs.
    send_command(&mut i2c, "PMTK313,1"); // enable SBAS satellite search
    send_command(&mut i2c, "PMTK301,2"); // DGPS correction source = WAAS
    // Beyond parity with the Python reference: this GPS's only job is
    // rangefinding a stationary handheld against a stationary (or
    // landed) rocket -- it's never used for in-motion tracking. MTK's
    // static-navigation threshold holds the reported fix steady below a
    // speed cutoff instead of showing normal GPS "wander" at rest, which
    // is exactly the accuracy trade this GPS should make. 0.2 m/s is
    // comfortably below walking pace (~0.5-1.4 m/s), so carrying the
    // handheld to search still updates position normally.
    send_command(&mut i2c, "PMTK397,0.2");
    // Also beyond parity: rocket/code.py and the original ground/code.py
    // both request PMTK220,1000 (1Hz). The PA1010D datasheet documents
    // up to 10Hz, and more raw fixes per averaging window (see module
    // docs) directly means a better-settled average -- this can only
    // help, never hurt: POLL_PERIOD_MS/CHUNK_SIZE below are unchanged, so
    // if the chip produces more data than this loop's read budget can
    // drain, the excess is simply not captured this cycle (the PA1010D's
    // I2C "streaming" interface is designed around exactly that -- see
    // NmeaLineReader's filler-byte handling), not lost/corrupted.
    send_command(&mut i2c, "PMTK220,100");
    // Give the chip a moment to digest all four writes before the read
    // loop starts hammering it -- not required by the protocol, just
    // cheap insurance against racing the chip's own ack.
    Timer::after_millis(POLL_PERIOD_MS).await;

    let mut reader: NmeaLineReader<96> = NmeaLineReader::new();
    let mut chunk = [0u8; CHUNK_SIZE];
    let mut avg = FixAverage::new();

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
                avg.add(fix.lat, fix.lon);
                // Not averaged -- course-over-ground is a circular
                // quantity (allowed to average through a "wrap" like
                // 359 deg/1 deg to something meaningless), and only
                // valid while moving anyway, which is the one case this
                // whole averaging scheme deliberately doesn't optimize
                // for. Just track the most recent reading.
                let heading = if fix.speed_knots > MIN_SPEED_FOR_HEADING_KNOTS {
                    fix.track_deg
                } else {
                    None
                };

                if let Some((lat, lon)) = avg.mean() {
                    *MY_GPS.lock().await = Some(MyFix { lat, lon, heading });
                }

                // Capture the wall-clock reference on the first valid
                // fix that has one -- see EPOCH_OFFSET's docs and
                // common::epoch's module docs on why this never gets
                // recomputed after.
                if let Some(utc) = fix.utc {
                    let mut offset = EPOCH_OFFSET.lock().await;
                    if offset.is_none() {
                        *offset = Some(EpochOffset::capture(&utc, Instant::now().as_millis() as u32));
                    }
                }
            }
        }

        Timer::after_millis(POLL_PERIOD_MS).await;
    }
}
