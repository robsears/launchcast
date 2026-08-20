//! Hardware-free NMEA 0183 handling for the PA1010D GPS over I2C -- both
//! boards have one (payload and handheld), and both run the same chip
//! with the same CircuitPython library today (`adafruit_gps`), so this
//! lives in `common` rather than being duplicated or owned by just one
//! board's `-logic` crate. Originally written for the ground station only
//! (see the bug-log entries in `docs/rust-rewrite.md` this module's docs
//! used to point to) and relocated here once the rocket port needed the
//! exact same parsing. Two pieces, both pure logic with no I2C dependency
//! so both are host-testable:
//!
//! - [`NmeaLineReader`]: assembles raw I2C bytes into complete sentence
//!   lines, replicating `adafruit_gps`'s exact padding-filter rule for
//!   the PA1010D's I2C "streaming" protocol: a bare `0x0A` that doesn't
//!   follow `0x0D` is filler (the module pads idle I2C reads with
//!   linefeeds when it has nothing new to send), not a real line
//!   terminator, and must be dropped rather than treated as an empty line.
//! - [`parse_rmc`]: decodes a `$..RMC` sentence (fix status, lat/lon,
//!   speed over ground, track angle) -- the one sentence type that alone
//!   covers everything either board's main loop reads off its own GPS,
//!   so nothing else is parsed.
//!
//! Neither board sends `PMTK314`/`PMTK220` (which sentences the chip
//! emits, and at what rate) from this Rust port: PA1010D/MTK3339-family
//! chips emit RMC by factory default, and MTK sentence-output
//! configuration is session-only (reset by a power cycle) unless
//! separately told to persist, so skipping these two just means relying
//! on the chip's own default output rather than the original Python's
//! explicit one.
//!
//! `PMTK313`/`PMTK301` (enable SBAS search / DGPS correction source =
//! WAAS) are a different matter -- those aren't sentence-output settings
//! at all, they're accuracy config. The ground station's Rust port
//! skipped them at first (mistaking them for sentence-output config too),
//! which left its GPS running uncorrected while the rocket's (still
//! `rocket/code.py` at the time, which does send both) had WAAS the whole
//! time -- that asymmetry showed up as a large (60-120ft) gap between two
//! at-rest fixes that should've read the same. [`checksum`] plus the
//! framing in each board's own GPS module sends both now, closing the gap
//! with the existing, tested Python behavior.

use heapless::String;

/// Assembles raw I2C bytes into complete NMEA sentence lines.
pub struct NmeaLineReader<const N: usize> {
    line: String<N>,
    last_byte: u8,
}

impl<const N: usize> Default for NmeaLineReader<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> NmeaLineReader<N> {
    pub fn new() -> Self {
        Self {
            line: String::new(),
            last_byte: 0,
        }
    }

    /// Feed one raw byte read from the GPS's I2C "streaming" interface.
    /// Returns `Some(line)` (sentence text, `$...*XX` with no trailing
    /// `\r\n`) once a complete sentence has been assembled.
    pub fn feed(&mut self, byte: u8) -> Option<String<N>> {
        if byte == 0x0A && self.last_byte != 0x0D {
            // Filler LF -- a real line feed only ever follows a CR (see
            // this module's docs). Not stored, and deliberately doesn't
            // update `last_byte` either, so a run of filler bytes can't
            // make a later, genuine `\r\n` look like it needs a second CR.
            return None;
        }
        self.last_byte = byte;
        if byte == b'\n' {
            if self.line.ends_with('\r') {
                self.line.pop();
            }
            let result = self.line.clone();
            self.line.clear();
            return if result.is_empty() { None } else { Some(result) };
        }
        if self.line.push(byte as char).is_err() {
            // Line longer than N (garbled data, or N too small for this
            // sentence) -- drop it and resync on the next terminator
            // rather than silently truncating.
            self.line.clear();
        }
        None
    }
}

/// XOR checksum of an NMEA sentence body (the bytes between `$` and `*`).
/// Used both to verify incoming sentences ([`parse_rmc`]) and to frame
/// outgoing `PMTK*` configuration commands ([`framed_command`]) -- the
/// same algorithm either direction.
pub fn checksum(body: &str) -> u8 {
    body.bytes().fold(0u8, |acc, b| acc ^ b)
}

/// Frame a raw PMTK payload (e.g. `"PMTK301,2"`) into the full command
/// bytes the chip expects: `"$PAYLOAD*XX\r\n"`. Shared by both boards'
/// GPS init (`ground/src/gps.rs`, `rocket/src/gps.rs`) -- each board
/// still owns the actual I2C write, this just builds the bytes.
pub fn framed_command<const N: usize>(payload: &str) -> String<N> {
    let mut s: String<N> = String::new();
    let _ = s.push('$');
    let _ = s.push_str(payload);
    let _ = s.push('*');
    let _ = core::fmt::write(&mut s, format_args!("{:02X}", checksum(payload)));
    let _ = s.push_str("\r\n");
    s
}

/// A decoded `$..RMC` sentence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RmcFix {
    /// `false` when the receiver reports status `V` (data not valid,
    /// i.e. no fix) -- fields are still parsed (usually stale/zero) but
    /// shouldn't be trusted when this is `false`.
    pub valid: bool,
    pub lat: f32,
    pub lon: f32,
    pub speed_knots: f32,
    /// Track angle (course over ground), degrees true. NMEA leaves this
    /// field blank when the receiver isn't moving fast enough to compute
    /// a reliable course.
    pub track_deg: Option<f32>,
    /// UTC date+time, if both the sentence's time and date fields parsed
    /// cleanly -- `None` on a cold start, before the receiver has
    /// decoded enough of the satellite signal to know the time at all
    /// (empty fields), not tied to `valid`: a receiver can know the time
    /// before it has a full position fix. See [`UtcDateTime`]/[`unix_ms`]
    /// for turning this into a wall-clock reference -- callers decide
    /// for themselves whether to also require `valid` before trusting it
    /// for their own purposes.
    pub utc: Option<UtcDateTime>,
}

/// A UTC calendar date + time-of-day, parsed from an NMEA sentence's
/// `hhmmss.sss` and `ddmmyy` fields. Deliberately not a general-purpose
/// datetime type (no timezone, no arithmetic beyond [`unix_ms`]) -- this
/// project only ever needs "what UTC instant was this," once, to
/// establish a wall-clock reference against each board's own free-
/// running monotonic clock (see `rocket/src/gps.rs`/`ground/src/gps.rs`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UtcDateTime {
    /// Full year -- NMEA's `yy` is 2 digits, assumed `2000 + yy` (the
    /// standard NMEA convention; fine for any realistic lifetime of this
    /// hardware).
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millis: u16,
}

/// Days since the Unix epoch (1970-01-01) for a given proleptic
/// Gregorian civil date. Howard Hinnant's `days_from_civil` algorithm
/// (public domain, the same integer math used inside most datetime
/// libraries' own guts) -- hand-rolled here rather than pulling in a
/// datetime crate: the only thing this project ever needs is exactly
/// this one conversion, no timezones, no calendar arithmetic beyond it,
/// so a crate would be more dependency than the problem calls for.
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64; // [0, 399]
    let mp = (if m > 2 { m - 3 } else { m + 9 }) as i64; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era as i64 * 146097 + doe - 719468
}

/// Convert a UTC date+time to milliseconds since the Unix epoch.
pub fn unix_ms(dt: &UtcDateTime) -> i64 {
    let days = days_from_civil(dt.year as i32, dt.month as u32, dt.day as u32);
    days * 86_400_000
        + dt.hour as i64 * 3_600_000
        + dt.minute as i64 * 60_000
        + dt.second as i64 * 1000
        + dt.millis as i64
}

/// Inverse of `days_from_civil` -- days since the Unix epoch back to a
/// proleptic Gregorian civil date. Same Hinnant algorithm family, same
/// public-domain source.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Inverse of `unix_ms` -- milliseconds since the Unix epoch back to a
/// [`UtcDateTime`], for displaying `arm_epoch_s` on the SUMMARY screen.
pub fn from_unix_ms(ms: i64) -> UtcDateTime {
    let days = ms.div_euclid(86_400_000);
    let ms_of_day = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let hour = (ms_of_day / 3_600_000) as u8;
    let minute = ((ms_of_day / 60_000) % 60) as u8;
    let second = ((ms_of_day / 1000) % 60) as u8;
    let millis = (ms_of_day % 1000) as u16;
    UtcDateTime {
        year: year as u16,
        month: month as u8,
        day: day as u8,
        hour,
        minute,
        second,
        millis,
    }
}

/// Parse NMEA's `hhmmss.sss` time field and `ddmmyy` date field into a
/// [`UtcDateTime`]. `None` if either field is missing/too short to
/// parse (a receiver that hasn't decoded the time yet leaves the time
/// field blank, same "nothing usable" convention as the rest of this
/// module) -- not a hard 6-vs-9-character length check on the time
/// field, since the fractional-seconds part's precision isn't
/// guaranteed across chip firmware versions.
fn parse_utc_datetime(time_field: &str, date_field: &str) -> Option<UtcDateTime> {
    if time_field.len() < 6 || date_field.len() != 6 {
        return None;
    }
    let hour: u8 = time_field.get(0..2)?.parse().ok()?;
    let minute: u8 = time_field.get(2..4)?.parse().ok()?;
    let second: u8 = time_field.get(4..6)?.parse().ok()?;
    let millis: u16 = match time_field.get(6..) {
        Some(frac) if frac.starts_with('.') && frac.len() > 1 => {
            let digits = &frac[1..];
            let value: u32 = digits.parse().ok()?;
            match digits.len() {
                1 => (value * 100) as u16,
                2 => (value * 10) as u16,
                3 => value as u16,
                n => (value / 10u32.pow((n - 3) as u32)) as u16,
            }
        }
        _ => 0,
    };

    let day: u8 = date_field.get(0..2)?.parse().ok()?;
    let month: u8 = date_field.get(2..4)?.parse().ok()?;
    let year_2d: u16 = date_field.get(4..6)?.parse().ok()?;

    Some(UtcDateTime {
        year: 2000 + year_2d,
        month,
        day,
        hour,
        minute,
        second,
        millis,
    })
}

/// Convert an NMEA `ddmm.mmmm` (or `dddmm.mmmm`) coordinate plus
/// hemisphere letter into signed decimal degrees.
fn coord_to_decimal(raw: &str, hemisphere: &str) -> Option<f32> {
    if raw.is_empty() {
        return None;
    }
    let value: f32 = raw.parse().ok()?;
    let degrees = libm::truncf(value / 100.0);
    let minutes = value - degrees * 100.0;
    let decimal = degrees + minutes / 60.0;
    match hemisphere {
        "N" | "E" => Some(decimal),
        "S" | "W" => Some(-decimal),
        _ => None,
    }
}

/// Parse one NMEA sentence. Returns `None` for anything that isn't a
/// `$..RMC` sentence, fails checksum validation, or is missing a field
/// this needs -- all treated as "nothing usable this line", same as a
/// timed-out radio receive elsewhere in this codebase.
pub fn parse_rmc(sentence: &str) -> Option<RmcFix> {
    let sentence = sentence.trim();
    let (body, checksum_hex) = sentence.strip_prefix('$')?.split_once('*')?;
    let expected = u8::from_str_radix(checksum_hex.trim(), 16).ok()?;
    let actual = checksum(body);
    if actual != expected {
        return None;
    }

    let mut fields = body.split(',');
    let talker_type = fields.next()?; // e.g. "GPRMC" / "GNRMC"
    if talker_type.len() < 3 || &talker_type[talker_type.len() - 3..] != "RMC" {
        return None;
    }
    let utc_time_raw = fields.next()?;
    let status = fields.next()?;
    let lat_raw = fields.next()?;
    let lat_hemi = fields.next()?;
    let lon_raw = fields.next()?;
    let lon_hemi = fields.next()?;
    let speed_raw = fields.next()?;
    let track_raw = fields.next()?;
    let utc_date_raw = fields.next()?;

    let lat = coord_to_decimal(lat_raw, lat_hemi)?;
    let lon = coord_to_decimal(lon_raw, lon_hemi)?;
    let speed_knots = speed_raw.parse().unwrap_or(0.0);
    let track_deg = if track_raw.is_empty() { None } else { track_raw.parse().ok() };
    let utc = parse_utc_datetime(utc_time_raw, utc_date_raw);

    Some(RmcFix {
        valid: status == "A",
        lat,
        lon,
        speed_knots,
        track_deg,
        utc,
    })
}
