//! LaunchCast shared packet definitions.
//!
//! Was a Rust port of `common/packet.py`; as of 2026-08-19 that
//! relationship is retired by user call -- `packet.py` (and the rest of
//! the CircuitPython implementation) is prototyping history that's
//! going away, not a format this crate needs to stay bit-for-bit
//! compatible with going forward. This crate is now the single source
//! of truth for the wire format on both boards, full stop.
//!
//! All multi-byte fields are little-endian with no padding, packed by hand
//! (no external crate) so this builds `no_std`, alloc-free, on
//! thumbv6m-none-eabi as well as the host.
#![cfg_attr(not(test), no_std)]

pub mod epoch;
pub mod fix_average;
pub mod nmea;

// --- Protocol identity -------------------------------------------------------

/// Binary 10100101; alternating bits and its own bit-reverse. Confirms a
/// received frame is meant for us.
pub const MAGIC: u8 = 0xA5;

/// Arbitrary non-default RFM95 sync word shared by both radios. A
/// hardware-level filter applied before a frame reaches software, so it
/// keeps other LoRa traffic on the same MAGIC byte from ever reaching us.
pub const SYNC_WORD: u8 = 0x2B;

pub const PKT_TELEMETRY: u8 = 0x01;
pub const PKT_COMMAND: u8 = 0x02;
/// Rocket -> handheld, sent in response to a `Command::GET_SUMMARY_BASE`
/// request. See [`pack_summary`]/[`unpack_summary`].
pub const PKT_SUMMARY: u8 = 0x03;
/// Rocket -> handheld, sent in response to a `Command::GET_FLIGHT_INDEX`
/// request. See [`pack_flight_index`]/[`unpack_flight_index`].
pub const PKT_FLIGHT_INDEX: u8 = 0x04;

pub const TELEMETRY_SIZE: usize = 40;
pub const COMMAND_SIZE: usize = 7;
pub const SUMMARY_SIZE: usize = 67;
/// magic + pkt_type + count -- see [`pack_flight_index`].
const FLIGHT_INDEX_HEADER_SIZE: usize = 3;
/// Largest a [`PKT_FLIGHT_INDEX`] frame can ever be, at
/// `MAX_STORED_FLIGHTS` entries -- the actual size on the wire is
/// smaller whenever fewer flights are stored (see that constant's docs
/// on why this packet is variable-length instead of always paying for
/// the max).
pub const FLIGHT_INDEX_MAX_SIZE: usize = FLIGHT_INDEX_HEADER_SIZE + MAX_STORED_FLIGHTS as usize * 4;

// --- Telemetry: rocket -> handheld -------------------------------------------
//
//  offset  field        type     units on wire
//  ------  -----------  -------  --------------------------------
//   0      magic        u8       0xA5
//   1      pkt_type     u8       0x01
//   2      counter      u16      packets since boot, wraps
//   4      uptime_ms    u32      ms since boot
//   8      state        u8       State::*
//   9      lat          f32      degrees
//  13      lon          f32      degrees
//  17      alt_baro     i16      meters AGL
//  19      speed        i16      cm/s, vertical, + is up
//  21      temp         i16      deci-degrees C
//  23      accel x,y,z  i16*3    milli-g
//  29      gyro x,y,z   i16*3    deci-degrees/s
//  35      batt         u8       (volts - 3.0) * 100
//  36      gps_flags    u8       bit0 = fix, bits 1-5 = sat count
//  37      flight_count u8       stored flights available via GET_SUMMARY (0 = none/unset)
//  38      sensors      u8       Sensor::* bitfield
//  39      fw_version   u8       rocket firmware build counter (0 = unset)
//                                                     total: 40 bytes

/// Physical-unit inputs for [`pack_telemetry`]. Grouped into a struct rather
/// than passed positionally — `pack_telemetry` mirrors 16 Python keyword
/// arguments, which would just be an easy-to-misorder wall of `f32`s here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TelemetryInput {
    pub counter: u16,
    pub uptime_ms: u32,
    pub state: u8,
    pub lat: f32,
    pub lon: f32,
    pub alt_baro_m: f32,
    pub speed_mps: f32,
    pub temp_c: f32,
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
    pub batt_volts: f32,
    pub has_fix: bool,
    pub satellites: u8,
    /// Former reserved field (`cam_rec`, always 0, never populated by
    /// anything -- from the same never-built camera concept `fw_version`'s
    /// byte came from). Repurposed 2026-08-19: how many completed flight
    /// summaries are currently available via `Command::GET_SUMMARY_BASE`
    /// (see `flight_summary.rs`) -- the ground station uses "nonzero" as
    /// the gate for whether the FLIGHTS screen even appears in the menu
    /// rotation.
    pub flight_count: u8,
    pub sensors: u8,
    /// Rocket firmware build counter -- byte 39 of the wire format, a
    /// former reserved field (`cam_disk`, always 0, never populated by
    /// anything). Renamed and repurposed 2026-08-18 so real telemetry
    /// can confirm a deploy actually took -- see docs/rust-rewrite.md.
    /// `0` means "not set" (an older firmware, or ground station's own
    /// telemetry which has no such field to report).
    pub fw_version: u8,
}

/// A decoded telemetry frame, in physical units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Telemetry {
    pub counter: u16,
    pub uptime_ms: u32,
    pub state: u8,
    pub lat: f32,
    pub lon: f32,
    pub alt_baro_m: i16,
    pub speed_mps: f32,
    pub temp_c: f32,
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
    pub batt_volts: f32,
    pub has_fix: bool,
    pub satellites: u8,
    pub flight_count: u8,
    pub sensors: u8,
    pub fw_version: u8,
}

impl Telemetry {
    pub fn state_name(&self) -> &'static str {
        State::name(self.state)
    }
}

/// Build a 40-byte telemetry frame from physical units.
pub fn pack_telemetry(input: &TelemetryInput) -> [u8; TELEMETRY_SIZE] {
    let mut buf = [0u8; TELEMETRY_SIZE];

    buf[0] = MAGIC;
    buf[1] = PKT_TELEMETRY;
    buf[2..4].copy_from_slice(&input.counter.to_le_bytes());
    buf[4..8].copy_from_slice(&input.uptime_ms.to_le_bytes());
    buf[8] = input.state;
    buf[9..13].copy_from_slice(&input.lat.to_le_bytes());
    buf[13..17].copy_from_slice(&input.lon.to_le_bytes());
    buf[17..19].copy_from_slice(&clamp_i16(input.alt_baro_m).to_le_bytes());
    buf[19..21].copy_from_slice(&clamp_i16(input.speed_mps * 100.0).to_le_bytes());
    buf[21..23].copy_from_slice(&clamp_i16(input.temp_c * 10.0).to_le_bytes());

    let [ax, ay, az] = input.accel_g;
    buf[23..25].copy_from_slice(&clamp_i16(ax * 1000.0).to_le_bytes());
    buf[25..27].copy_from_slice(&clamp_i16(ay * 1000.0).to_le_bytes());
    buf[27..29].copy_from_slice(&clamp_i16(az * 1000.0).to_le_bytes());

    let [gx, gy, gz] = input.gyro_dps;
    buf[29..31].copy_from_slice(&clamp_i16(gx * 10.0).to_le_bytes());
    buf[31..33].copy_from_slice(&clamp_i16(gy * 10.0).to_le_bytes());
    buf[33..35].copy_from_slice(&clamp_i16(gz * 10.0).to_le_bytes());

    buf[35] = encode_batt(input.batt_volts);
    buf[36] = encode_gps_flags(input.has_fix, input.satellites);
    buf[37] = input.flight_count;
    buf[38] = input.sensors;
    buf[39] = input.fw_version;

    buf
}

/// Decode a telemetry frame, or `None` if it isn't ours.
///
/// Rejects on length, magic byte, and packet type. Never panics on bad
/// input — a malformed frame is a routine event on a shared ISM band.
pub fn unpack_telemetry(data: &[u8]) -> Option<Telemetry> {
    if data.len() != TELEMETRY_SIZE {
        return None;
    }
    if data[0] != MAGIC || data[1] != PKT_TELEMETRY {
        return None;
    }

    let counter = u16::from_le_bytes(data[2..4].try_into().unwrap());
    let uptime_ms = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let state = data[8];
    let lat = f32::from_le_bytes(data[9..13].try_into().unwrap());
    let lon = f32::from_le_bytes(data[13..17].try_into().unwrap());
    let alt_baro_m = i16::from_le_bytes(data[17..19].try_into().unwrap());
    let speed = i16::from_le_bytes(data[19..21].try_into().unwrap());
    let temp = i16::from_le_bytes(data[21..23].try_into().unwrap());
    let ax = i16::from_le_bytes(data[23..25].try_into().unwrap());
    let ay = i16::from_le_bytes(data[25..27].try_into().unwrap());
    let az = i16::from_le_bytes(data[27..29].try_into().unwrap());
    let gx = i16::from_le_bytes(data[29..31].try_into().unwrap());
    let gy = i16::from_le_bytes(data[31..33].try_into().unwrap());
    let gz = i16::from_le_bytes(data[33..35].try_into().unwrap());
    let batt = data[35];
    let gps_flags = data[36];
    let flight_count = data[37];
    let sensors = data[38];
    let fw_version = data[39];

    let (has_fix, satellites) = decode_gps_flags(gps_flags);

    Some(Telemetry {
        counter,
        uptime_ms,
        state,
        lat,
        lon,
        alt_baro_m,
        speed_mps: speed as f32 / 100.0,
        temp_c: temp as f32 / 10.0,
        accel_g: [ax as f32 / 1000.0, ay as f32 / 1000.0, az as f32 / 1000.0],
        gyro_dps: [gx as f32 / 10.0, gy as f32 / 10.0, gz as f32 / 10.0],
        batt_volts: decode_batt(batt),
        has_fix,
        satellites,
        flight_count,
        sensors,
        fw_version,
    })
}

// --- Summary: rocket -> handheld (one flight's highlights) -------------------
//
// Sent in response to a Command::GET_SUMMARY_BASE request -- see
// rocket-logic::flight_summary for what these fields mean operationally
// and why the two GPS fixes are captured when they are.
//
//  offset  field          type   units on wire
//  ------  -------------  -----  --------------------------------
//   0      magic          u8     0xA5
//   1      pkt_type       u8     0x03
//   2      flight_index   u8     which stored flight this answers
//   3      wait_ms        u32    ARM -> BOOST
//   7      boost_ms       u32    BOOST -> COAST
//  11      coast_ms       u32    COAST -> APOGEE
//  15      descent_ms     u32    DESCENT -> LANDED
//  19      arm_lat        f32    degrees, averaged fix at ARM
//  23      arm_lon        f32    degrees
//  27      landed_lat     f32    degrees, averaged fix locked in at RECOVER
//  31      landed_lon     f32    degrees
//  35      max_speed_mps  f32
//  39      max_alt_m      f32
//  43      temp_at_max_alt_c        f32
//  47      pressure_at_max_alt_hpa  f32
//  51      max_accel_g    f32
//  55      max_gyro_dps   f32
//  59      record_count   u32    log entries written this flight
//  63      arm_epoch_s    u32    unix seconds at ARM; 0 = no wall clock yet
//                                                     total: 67 bytes

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SummaryInput {
    pub flight_index: u8,
    pub wait_ms: u32,
    pub boost_ms: u32,
    pub coast_ms: u32,
    pub descent_ms: u32,
    pub arm_lat: f32,
    pub arm_lon: f32,
    pub landed_lat: f32,
    pub landed_lon: f32,
    pub max_speed_mps: f32,
    pub max_alt_m: f32,
    /// Temperature/pressure at the moment `max_alt_m` was recorded, not
    /// independent extremes of their own -- see
    /// `rocket-logic::flight_summary::FlightSummary::observe`'s docs.
    pub temp_at_max_alt_c: f32,
    pub pressure_at_max_alt_hpa: f32,
    pub max_accel_g: f32,
    pub max_gyro_dps: f32,
    pub record_count: u32,
    /// Unix seconds at ARM, from the rocket's own `EpochOffset` (see
    /// `common::epoch`) -- `0` if the rocket had no wall-clock reference
    /// yet at ARM time (no GPS fix since boot).
    pub arm_epoch_s: u32,
}

pub type Summary = SummaryInput;

/// Build a 67-byte flight-summary frame.
pub fn pack_summary(input: &SummaryInput) -> [u8; SUMMARY_SIZE] {
    let mut buf = [0u8; SUMMARY_SIZE];

    buf[0] = MAGIC;
    buf[1] = PKT_SUMMARY;
    buf[2] = input.flight_index;
    buf[3..7].copy_from_slice(&input.wait_ms.to_le_bytes());
    buf[7..11].copy_from_slice(&input.boost_ms.to_le_bytes());
    buf[11..15].copy_from_slice(&input.coast_ms.to_le_bytes());
    buf[15..19].copy_from_slice(&input.descent_ms.to_le_bytes());
    buf[19..23].copy_from_slice(&input.arm_lat.to_le_bytes());
    buf[23..27].copy_from_slice(&input.arm_lon.to_le_bytes());
    buf[27..31].copy_from_slice(&input.landed_lat.to_le_bytes());
    buf[31..35].copy_from_slice(&input.landed_lon.to_le_bytes());
    buf[35..39].copy_from_slice(&input.max_speed_mps.to_le_bytes());
    buf[39..43].copy_from_slice(&input.max_alt_m.to_le_bytes());
    buf[43..47].copy_from_slice(&input.temp_at_max_alt_c.to_le_bytes());
    buf[47..51].copy_from_slice(&input.pressure_at_max_alt_hpa.to_le_bytes());
    buf[51..55].copy_from_slice(&input.max_accel_g.to_le_bytes());
    buf[55..59].copy_from_slice(&input.max_gyro_dps.to_le_bytes());
    buf[59..63].copy_from_slice(&input.record_count.to_le_bytes());
    buf[63..67].copy_from_slice(&input.arm_epoch_s.to_le_bytes());

    buf
}

/// Decode a flight-summary frame, or `None` if it isn't ours.
pub fn unpack_summary(data: &[u8]) -> Option<Summary> {
    if data.len() != SUMMARY_SIZE {
        return None;
    }
    if data[0] != MAGIC || data[1] != PKT_SUMMARY {
        return None;
    }

    let f32_at = |off: usize| f32::from_le_bytes(data[off..off + 4].try_into().unwrap());
    let u32_at = |off: usize| u32::from_le_bytes(data[off..off + 4].try_into().unwrap());

    Some(Summary {
        flight_index: data[2],
        wait_ms: u32_at(3),
        boost_ms: u32_at(7),
        coast_ms: u32_at(11),
        descent_ms: u32_at(15),
        arm_lat: f32_at(19),
        arm_lon: f32_at(23),
        landed_lat: f32_at(27),
        landed_lon: f32_at(31),
        max_speed_mps: f32_at(35),
        max_alt_m: f32_at(39),
        temp_at_max_alt_c: f32_at(43),
        pressure_at_max_alt_hpa: f32_at(47),
        max_accel_g: f32_at(51),
        max_gyro_dps: f32_at(55),
        record_count: u32_at(59),
        arm_epoch_s: u32_at(63),
    })
}

// --- Flight index: rocket -> handheld (which flights are stored, and when) ---
//
// Sent in response to a Command::GET_FLIGHT_INDEX request -- the ground
// station's actual source of truth for what's available to request via
// GET_SUMMARY_BASE, replacing any assumption based on a possibly-stale
// telemetry byte. Variable length, unlike every other packet in this
// file -- see FLIGHT_INDEX_MAX_SIZE's docs on why.
//
//  offset  field        type   notes
//  ------  -----------  -----  --------------------------------
//   0      magic        u8     0xA5
//   1      pkt_type     u8     0x04
//   2      count        u8     0..=MAX_STORED_FLIGHTS
//   3..    epoch_s[i]   u32    unix seconds at ARM, oldest first, `count` of them
//                                     total: 3 + count*4 bytes

/// Build a flight-index frame from an ordered (oldest-first) list of
/// ARM unix-second timestamps -- one per currently-stored flight, same
/// index convention `Command::GET_SUMMARY_BASE` uses. Silently caps at
/// `MAX_STORED_FLIGHTS` entries if handed more (should never happen --
/// the rocket's own storage is capped at that size already).
pub fn pack_flight_index(timestamps: &[u32]) -> heapless::Vec<u8, FLIGHT_INDEX_MAX_SIZE> {
    let mut buf: heapless::Vec<u8, FLIGHT_INDEX_MAX_SIZE> = heapless::Vec::new();
    let count = timestamps.len().min(MAX_STORED_FLIGHTS as usize);
    let _ = buf.push(MAGIC);
    let _ = buf.push(PKT_FLIGHT_INDEX);
    let _ = buf.push(count as u8);
    for &ts in &timestamps[..count] {
        let _ = buf.extend_from_slice(&ts.to_le_bytes());
    }
    buf
}

/// Decode a flight-index frame, or `None` if it isn't ours or its
/// declared `count` doesn't match the actual payload length.
pub fn unpack_flight_index(data: &[u8]) -> Option<heapless::Vec<u32, { MAX_STORED_FLIGHTS as usize }>> {
    if data.len() < FLIGHT_INDEX_HEADER_SIZE {
        return None;
    }
    if data[0] != MAGIC || data[1] != PKT_FLIGHT_INDEX {
        return None;
    }
    let count = data[2] as usize;
    if count > MAX_STORED_FLIGHTS as usize || data.len() != FLIGHT_INDEX_HEADER_SIZE + count * 4 {
        return None;
    }

    let mut out = heapless::Vec::new();
    for i in 0..count {
        let off = FLIGHT_INDEX_HEADER_SIZE + i * 4;
        let ts = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        // Capacity is MAX_STORED_FLIGHTS and count was just checked
        // against it, so this can never fail.
        let _ = out.push(ts);
    }
    Some(out)
}

// --- Command: handheld -> rocket ---------------------------------------------
//
//  offset  field      type    notes
//  ------  ---------  ------  ----------------------------------
//   0      magic      u8      0xA5
//   1      pkt_type   u8      0x02
//   2      seq        u16     increments; rocket rejects replays
//   4      cmd        u8      Command::*
//   5      checksum   u16     sum of bytes 0..4, mod 65536
//                                              total: 7 bytes

fn checksum(payload: &[u8]) -> u16 {
    payload.iter().fold(0u32, |acc, &b| acc + b as u32) as u16
}

/// Build a 7-byte command frame. `seq` wrapping is enforced by its `u16`
/// type rather than an explicit mask, unlike the Python `seq & 0xFFFF`.
pub fn pack_command(seq: u16, cmd: u8) -> [u8; COMMAND_SIZE] {
    let mut buf = [0u8; COMMAND_SIZE];
    buf[0] = MAGIC;
    buf[1] = PKT_COMMAND;
    buf[2..4].copy_from_slice(&seq.to_le_bytes());
    buf[4] = cmd;
    let sum = checksum(&buf[..5]);
    buf[5..7].copy_from_slice(&sum.to_le_bytes());
    buf
}

/// Return `(seq, cmd)`, or `None` on wrong length, bad magic/type, or a
/// checksum mismatch.
///
/// The rocket should additionally reject any `seq` it has already seen, to
/// keep a repeated or reflected frame from re-triggering a command.
pub fn unpack_command(data: &[u8]) -> Option<(u16, u8)> {
    if data.len() != COMMAND_SIZE {
        return None;
    }
    if data[0] != MAGIC || data[1] != PKT_COMMAND {
        return None;
    }

    let seq = u16::from_le_bytes(data[2..4].try_into().unwrap());
    let cmd = data[4];
    let received = u16::from_le_bytes(data[5..7].try_into().unwrap());

    if received != checksum(&data[..5]) {
        return None;
    }

    Some((seq, cmd))
}

/// How many completed flights the rocket keeps in RAM, and (since
/// `Command::GET_SUMMARY_BASE`'s value directly encodes the flight index
/// requested rather than carrying a separate parameter byte) the number
/// of command-byte values reserved for that purpose. Oldest evicted once
/// full; cleared on power cycle -- see `flight_summary.rs`'s docs on why
/// that's an acceptable tradeoff (the raw log partition remains the
/// durable source of truth regardless).
pub const MAX_STORED_FLIGHTS: u8 = 32;

pub struct Command;

impl Command {
    /// Request nothing; presence test.
    pub const PING: u8 = 0x10;
    /// Sound the buzzer for a few seconds.
    pub const CHIRP: u8 = 0x01;
    /// IDLE -> ARMED.
    pub const ARM: u8 = 0x02;
    /// ARMED -> IDLE (from ARMED: abort, rewinds the log; from LANDED:
    /// RECOVER, silences the beacon -- rocket tells the two apart by its
    /// own current state, see `rocket/src/main.rs`).
    pub const DISARM: u8 = 0x03;
    /// Request flight N's summary: send
    /// `GET_SUMMARY_BASE + N` (`N` in `0..MAX_STORED_FLIGHTS`) as the
    /// command byte -- the value itself *is* the flight index, there's
    /// no separate parameter field on the 7-byte command packet.
    /// Answered with a [`PKT_SUMMARY`] frame carrying the same index, or
    /// not answered at all if that index has nothing stored (the ground
    /// station's existing pending-command timeout already covers "no
    /// response arrived").
    pub const GET_SUMMARY_BASE: u8 = 0x20;
    /// Request the ordered list of ARM timestamps for every flight
    /// currently stored -- see [`pack_flight_index`]/
    /// [`unpack_flight_index`]. Answered with a [`PKT_FLIGHT_INDEX`]
    /// frame. This is the ground station's actual source of truth for
    /// "what flights exist and at what index" -- not a cached count
    /// from telemetry, which can't distinguish a real answer from a
    /// rocket that's since power-cycled and lost its RAM-only stored
    /// flights.
    pub const GET_FLIGHT_INDEX: u8 = 0x05;
}

// --- Flight state machine ----------------------------------------------------

pub struct State;

impl State {
    /// Initial state when switched on. Move on when all sensors initialized.
    pub const BOOT: u8 = 0;
    /// Alive and waiting to hear from mission control.
    pub const IDLE: u8 = 1;
    /// Handheld sent ARM. Payload is logging, waiting for boost.
    pub const ARMED: u8 = 2;
    /// Sudden acceleration detected -- motor burning.
    pub const BOOST: u8 = 3;
    /// Acceleration no longer detected or below threshold -- coasting.
    pub const COAST: u8 = 4;
    /// Acceleration + velocity are at/below threshold -- at peak.
    pub const APOGEE: u8 = 5;
    /// Acceleration/velocity above threshold again -- descending.
    pub const DESCENT: u8 = 6;
    /// Movement has stopped -- landed.
    pub const LANDED: u8 = 7;

    pub const NAMES: [&'static str; 8] = [
        "BOOT", "IDLE", "ARMED", "BOOST", "COAST", "APOGEE", "DESCENT", "LANDED",
    ];

    /// Unlike Python's `State.name()`, an out-of-range value maps to the
    /// static string `"UNKNOWN"` rather than `"UNKNOWN({value})"` -- this
    /// crate is alloc-free, so it can't format the value into a owned
    /// string. Both are unambiguous as "not a real state".
    pub fn name(value: u8) -> &'static str {
        match Self::NAMES.get(value as usize) {
            Some(name) => name,
            None => "UNKNOWN",
        }
    }
}

// --- Sensor health bitfield --------------------------------------------------
// One bit per peripheral, reported in every telemetry frame at zero extra
// airtime cost. Lets the handheld show what actually came up before launch.

pub struct Sensor;

impl Sensor {
    pub const BARO: u8 = 0x01; // BMP580 pressure/temp
    pub const IMU: u8 = 0x02; // LSM6DSOX accel/gyro
    pub const MAG: u8 = 0x04; // LIS3MDL magnetometer
    pub const GPS: u8 = 0x08; // PA1010D
    pub const LOG: u8 = 0x10; // filesystem writable; flight log is recording
    pub const BATT: u8 = 0x20; // battery ADC readable
    /// USB power present (charging or charged). Deliberately excluded from
    /// `NAMES`/`ALL`/`REQUIRED`/`decode()`: it's a live power state, not a
    /// peripheral health flag, and it's normally 0 for the entire flight
    /// (USB unplugged). Mixing it into present/missing would make every
    /// flight show a "missing sensor".
    pub const CHG: u8 = 0x40;

    pub const NAMES: [(u8, &'static str); 6] = [
        (Self::BARO, "BARO"),
        (Self::IMU, "IMU"),
        (Self::MAG, "MAG"),
        (Self::GPS, "GPS"),
        (Self::LOG, "LOG"),
        (Self::BATT, "BATT"),
    ];

    pub const ALL: u8 = Self::BARO | Self::IMU | Self::MAG | Self::GPS | Self::LOG | Self::BATT;

    /// Flight-critical subset. MAG and GPS are nice to have; a missing
    /// barometer means no apogee detection and a missing log means no
    /// dataset.
    pub const REQUIRED: u8 = Self::BARO | Self::IMU | Self::LOG;

    pub fn flight_ready(raw: u8) -> bool {
        raw & Self::REQUIRED == Self::REQUIRED
    }

    /// Names of set bits in `raw`. Returns an iterator rather than
    /// Python's `list` since this crate has no allocator.
    pub fn present(raw: u8) -> impl Iterator<Item = &'static str> {
        Self::NAMES
            .iter()
            .filter(move |(bit, _)| raw & bit != 0)
            .map(|(_, name)| *name)
    }

    /// Names of unset bits in `raw`. See [`Sensor::present`].
    pub fn missing(raw: u8) -> impl Iterator<Item = &'static str> {
        Self::NAMES
            .iter()
            .filter(move |(bit, _)| raw & bit == 0)
            .map(|(_, name)| *name)
    }
}

// --- Scaling helpers ----------------------------------------------------------

/// 3.00-5.55 V into one byte at 10 mV resolution.
pub fn encode_batt(volts: f32) -> u8 {
    let v = libm::roundf((volts - 3.0) * 100.0);
    if v < 0.0 {
        0
    } else if v > 255.0 {
        255
    } else {
        v as u8
    }
}

pub fn decode_batt(raw: u8) -> f32 {
    3.0 + raw as f32 / 100.0
}

pub fn encode_gps_flags(has_fix: bool, satellites: u8) -> u8 {
    (if has_fix { 1 } else { 0 }) | (satellites.min(31) << 1)
}

pub fn decode_gps_flags(raw: u8) -> (bool, u8) {
    (raw & 0x01 != 0, (raw >> 1) & 0x1F)
}

fn clamp_i16(value: f32) -> i16 {
    let v = libm::roundf(value);
    if v < i16::MIN as f32 {
        i16::MIN
    } else if v > i16::MAX as f32 {
        i16::MAX
    } else {
        v as i16
    }
}
