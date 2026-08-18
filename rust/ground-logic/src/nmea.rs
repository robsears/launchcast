//! Hardware-free NMEA 0183 handling for the ground station's own GPS
//! (PA1010D over I2C). Two pieces, both pure logic with no I2C dependency
//! so both are host-testable:
//!
//! - [`NmeaLineReader`]: assembles raw I2C bytes into complete sentence
//!   lines, replicating `adafruit_gps`'s (the CircuitPython library
//!   `ground/code.py` uses) exact padding-filter rule for the PA1010D's
//!   I2C "streaming" protocol: a bare `0x0A` that doesn't follow `0x0D`
//!   is filler (the module pads idle I2C reads with linefeeds when it has
//!   nothing new to send), not a real line terminator, and must be
//!   dropped rather than treated as an empty line.
//! - [`parse_rmc`]: decodes a `$..RMC` sentence (fix status, lat/lon,
//!   speed over ground, track angle) -- the one sentence type that alone
//!   covers everything `ground/code.py`'s main loop reads off its own
//!   GPS (`latitude`, `longitude`, `speed_knots`, `track_angle_deg`), so
//!   nothing else is parsed.
//!
//! `ground/code.py` also sends `PMTK314`/`PMTK220` to configure which
//! sentences the chip emits and at what rate -- not sent by this port:
//! PA1010D/MTK3339-family chips emit RMC by factory default, and MTK
//! sentence-output configuration is session-only (reset by a power
//! cycle) unless separately told to persist, so skipping these two just
//! means relying on the chip's own default output rather than Python's
//! explicit one.
//!
//! `PMTK313`/`PMTK301` (enable SBAS search / DGPS correction source =
//! WAAS) are a different matter -- those aren't sentence-output settings
//! at all, they're accuracy config, and skipping them left this GPS
//! running uncorrected while the rocket's (which still runs
//! `rocket/code.py`, unchanged) sends both at boot. That asymmetry showed
//! up as a large (60-120ft) gap between two at-rest fixes that should've
//! read the same. [`checksum`] plus the framing in `ground/src/gps.rs`
//! sends both, closing the gap with the existing, tested Python behavior.

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
/// outgoing `PMTK*` configuration commands (see `ground/src/gps.rs`) --
/// the same algorithm either direction.
pub fn checksum(body: &str) -> u8 {
    body.bytes().fold(0u8, |acc, b| acc ^ b)
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
    let _utc = fields.next()?;
    let status = fields.next()?;
    let lat_raw = fields.next()?;
    let lat_hemi = fields.next()?;
    let lon_raw = fields.next()?;
    let lon_hemi = fields.next()?;
    let speed_raw = fields.next()?;
    let track_raw = fields.next()?;

    let lat = coord_to_decimal(lat_raw, lat_hemi)?;
    let lon = coord_to_decimal(lon_raw, lon_hemi)?;
    let speed_knots = speed_raw.parse().unwrap_or(0.0);
    let track_deg = if track_raw.is_empty() { None } else { track_raw.parse().ok() };

    Some(RmcFix {
        valid: status == "A",
        lat,
        lon,
        speed_knots,
        track_deg,
    })
}
