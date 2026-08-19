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
}

/// core1 -> core0. `None` until core1's first loop iteration.
pub static TELEMETRY: Mutex<CriticalSectionRawMutex, Option<LatestTelemetry>> = Mutex::new(None);

/// core0 -> core1: a received, replay-checked command byte (see
/// `launchcast_common::Command`). Replay rejection (`code.py`'s `if seq
/// != last_seq`) happens on core0, which is the side that actually sees
/// the raw `(seq, cmd)` pair off the radio -- core1 only ever sees
/// commands already known to be new.
pub static COMMANDS: Channel<CriticalSectionRawMutex, u8, 4> = Channel::new();
