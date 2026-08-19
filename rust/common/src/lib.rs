//! LaunchCast shared packet definitions.
//!
//! Rust port of `common/packet.py`, the single source of truth for the wire
//! format on both boards. If you change layout, scaling, or a constant here,
//! check it against `common/packet.py` (and vice versa) — the Python and
//! Rust sides must stay bit-for-bit compatible on the wire while both boards
//! exist in mixed CircuitPython/Rust fleets during the rewrite. See
//! `docs/rust-rewrite.md`.
//!
//! All multi-byte fields are little-endian with no padding, packed by hand
//! (no external crate) so this builds `no_std`, alloc-free, on
//! thumbv6m-none-eabi as well as the host.
#![cfg_attr(not(test), no_std)]

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

pub const TELEMETRY_SIZE: usize = 40;
pub const COMMAND_SIZE: usize = 7;

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
//  37      cam_rec      u8       reserved, send 0
//  38      sensors      u8       Sensor::* bitfield
//  39      cam_disk     u8       reserved, send 0
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
    pub cam_rec: u8,
    pub sensors: u8,
    pub cam_disk: u8,
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
    pub cam_rec: bool,
    pub sensors: u8,
    pub cam_disk: u8,
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
    buf[37] = input.cam_rec;
    buf[38] = input.sensors;
    buf[39] = input.cam_disk;

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
    let cam_rec = data[37];
    let sensors = data[38];
    let cam_disk = data[39];

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
        cam_rec: cam_rec != 0,
        sensors,
        cam_disk,
    })
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

pub struct Command;

impl Command {
    /// Request nothing; presence test.
    pub const PING: u8 = 0x10;
    /// Sound the buzzer for a few seconds.
    pub const CHIRP: u8 = 0x01;
    /// IDLE -> ARMED.
    pub const ARM: u8 = 0x02;
    /// ARMED -> IDLE.
    pub const DISARM: u8 = 0x03;
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
