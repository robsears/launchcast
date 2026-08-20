//! Cross-core state between core1 (sensors + flight-state machine) and
//! core0 (radio). Mirrors the ground station's own cross-core pattern
//! (`ground/src/link.rs`, `BUTTON_EVENTS`), roles reversed: here core1 is
//! the producer of telemetry and the consumer of commands, since this
//! board *sends* telemetry and *receives* commands.
//!
//! `counter`/`uptime_ms` are deliberately not part of [`LatestTelemetry`]
//! -- those are TX-time concepts ("packets sent so far", "uptime at the
//! moment of this transmission"), which belong to core0 (the sender),
//! not to whatever core1's sensor loop last observed. Physical-unit
//! fields are stored pre-converted to the units `common::TelemetryInput`
//! expects (`accel_g` already divided by standard gravity, `gyro_dps`
//! already in degrees/s), so core0 can copy them straight across at TX
//! time with no further conversion.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use launchcast_common as common;

#[derive(Clone, Copy)]
pub struct LatestTelemetry {
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
    pub sensors: u8,
    /// How many completed flight summaries core1 currently has stored --
    /// see `flight_summary.rs`. Copied straight into
    /// `common::TelemetryInput::flight_count` at TX time.
    pub flight_count: u8,
}

/// core1 -> core0. `None` until core1's first loop iteration.
pub static TELEMETRY: Mutex<CriticalSectionRawMutex, Option<LatestTelemetry>> = Mutex::new(None);

/// core0 -> core1: a received, replay-checked command byte (see
/// `launchcast_common::Command`). Replay rejection (`code.py`'s `if seq
/// != last_seq`) happens on core0, which is the side that actually sees
/// the raw `(seq, cmd)` pair off the radio -- core1 only ever sees
/// commands already known to be new.
pub static COMMANDS: Channel<CriticalSectionRawMutex, u8, 4> = Channel::new();

/// core1 -> core0: a summary response to radio out. Carries the
/// unpacked `SummaryInput`, not pre-packed bytes -- matches `TELEMETRY`'s
/// own shape (core0's `Radio::send_summary`, like `send_telemetry`, does
/// the actual `common::pack_summary` call), since only core0 owns the
/// radio (see `main.rs`'s module docs on the core split) but core1 owns
/// the stored-flights list this is built from. Capacity 1: the ground
/// station only ever has one summary request pending at a time (it waits
/// for a response or a timeout before sending the next), so there's
/// never a reason for more than one of these in flight.
pub static SUMMARY_RESPONSE: Channel<CriticalSectionRawMutex, common::SummaryInput, 1> = Channel::new();

/// core1 -> core0: an ordered (oldest-first) list of ARM timestamps to
/// radio out in response to `Command::GET_FLIGHT_INDEX` -- same
/// reasoning as `SUMMARY_RESPONSE` (core0 owns the radio, core1 owns
/// the stored-flights list this is built from), same capacity-1
/// justification (one request outstanding at a time).
pub static FLIGHT_INDEX_RESPONSE: Channel<CriticalSectionRawMutex, heapless::Vec<u32, { common::MAX_STORED_FLIGHTS as usize }>, 1> =
    Channel::new();
