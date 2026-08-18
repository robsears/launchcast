//! Ground-station-side "is it safe to send ARM" gate.
//!
//! Independent of (and in addition to) the rocket's own
//! `Sensor.flight_ready` refusal in `rocket/code.py` -- that one only
//! covers missing/failed peripherals (BARO/IMU/LOG), and deliberately
//! excludes `Sensor::CHG` (a live power state, not a peripheral-health
//! flag -- see `common/src/lib.rs`). This gate is the other half: things
//! a person watching the handheld should see and be stopped from doing
//! *before* ever sending the command, not just after a refusal comes
//! back over the radio. Low battery and "currently charging" (why would
//! you launch mid-charge?) are exactly that kind of thing.

use launchcast_common::{Sensor, Telemetry};

/// Below this, arming is refused. Matches the payload battery threshold
/// already used for the FLIGHT screen's low-battery glyph/banner.
pub const NOGO_BATT_V: f32 = 3.80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NogoReason {
    LowBattery,
    Charging,
}

impl NogoReason {
    pub fn message(self) -> &'static str {
        match self {
            NogoReason::LowBattery => "*** PAYLOAD BATT LOW -- NO GO ***",
            NogoReason::Charging => "*** PAYLOAD CHARGING -- NO GO ***",
        }
    }
}

/// Whether the payload's own telemetry currently rules out sending ARM.
/// `None` means there's no reason to refuse. Battery takes priority over
/// charging when (implausibly) both apply at once -- a dead-flat battery
/// is the more urgent fact even if it happens to be on a charger.
pub fn nogo_reason(tel: &Telemetry) -> Option<NogoReason> {
    if tel.batt_volts < NOGO_BATT_V {
        Some(NogoReason::LowBattery)
    } else if tel.sensors & Sensor::CHG != 0 {
        Some(NogoReason::Charging)
    } else {
        None
    }
}
