//! Everything a screen/header/footer module might want to render, bundled
//! once per draw call. Port of `code.py`'s `Frame` class -- computed once
//! in `main.rs`'s display task from `link::LINK`/`gps::MY_GPS`/
//! `battery::MY_BATT`, not recomputed per screen.

use launchcast_common as common;
use launchcast_ground_logic::{nogo_reason, LinkStatus, NogoReason};

pub struct Frame<'a> {
    pub tel: Option<&'a common::Telemetry>,
    pub rssi: Option<i16>,
    pub snr: Option<i16>,
    pub packets: u32,
    pub rejects: u32,
    pub status: LinkStatus,

    /// The rocket's latched last-known GPS fix -- survives the rocket
    /// going silent (see `link.rs`). Not the same as `tel`'s own
    /// lat/lon, which reflects only the *current* frame.
    pub fix_lat: Option<f32>,
    pub fix_lon: Option<f32>,
    pub fix_age_ms: Option<u32>,

    /// The ground station's own GPS/battery. See `gps.rs`/`battery.rs`.
    pub my_lat: Option<f32>,
    pub my_lon: Option<f32>,
    pub my_heading: Option<f32>,
    pub my_batt: Option<f32>,
    pub my_charging: bool,
    /// The ground station's own current wall-clock time (Unix ms),
    /// derived from its own independently-captured `gps::EPOCH_OFFSET`
    /// -- `None` until this board's GPS has produced a UTC fix. No
    /// rocket/wire involvement -- see `screen_header.rs`.
    pub my_wall_clock_ms: Option<i64>,

    pub tx_status: &'a str,
    /// Most-recent-last scrolling log of sent commands and their
    /// resolution (see `cmdlog.rs`). Rendered on the FLIGHT screen under
    /// the CONTROLLER panel.
    pub cmd_log: &'a [heapless::String<{ crate::cmdlog::STATUS_LEN }>; crate::cmdlog::CMD_LOG_LINES],
    pub screen_name: &'a str,
    pub next_screen_name: &'a str,
    pub prev_screen_name: &'a str,

    /// Currently selected row on FLIGHTS (`screen::selected()`) -- read
    /// here rather than called directly by `screen_flights.rs` only for
    /// consistency with everything else being assembled once per draw.
    pub selected_flight: u8,
    /// Pending/Ready/Failed state of the last-requested flight summary
    /// -- see `summary_request.rs`. Rendered on SUMMARY.
    pub summary_request: crate::summary_request::SummaryRequest,
    /// Idle/Pending/Ready/Empty/Failed state of the cached flight-index
    /// fetch -- see `flight_index.rs`. Rendered on FLIGHTS in place of
    /// the old live `Telemetry::flight_count` byte, which couldn't tell
    /// "rocket really has N flights" apart from "rocket power-cycled and
    /// lost its RAM-only flights since we last heard from it."
    pub flight_index_state: crate::flight_index::IndexState,
    /// `(cached, total)` while the background per-flight summary
    /// prefetch still has work left to do, `None` once it's done -- see
    /// `flight_index.rs`. Rendered on FLIGHTS as a "FETCHING..." banner.
    pub prefetch_progress: Option<(u8, u8)>,
}

impl<'a> Frame<'a> {
    pub fn armed(&self) -> bool {
        self.tel.is_some_and(|t| t.state == common::State::ARMED)
    }

    /// True for any state RECOVER is valid from -- everything past
    /// ARMED, not just LANDED. Broadened 2026-08-19 after finding on
    /// real hardware that a flight can get stuck mid-state-machine
    /// (e.g. APOGEE never transitioning to DESCENT) with no way to
    /// recover it when RECOVER only accepted LANDED specifically --
    /// must stay in sync with the identical match in
    /// `rocket/src/main.rs`'s DISARM handling.
    pub fn recoverable(&self) -> bool {
        self.tel.is_some_and(|t| {
            matches!(
                t.state,
                common::State::BOOST | common::State::COAST | common::State::APOGEE | common::State::DESCENT | common::State::LANDED
            )
        })
    }

    pub fn payload_batt(&self) -> Option<f32> {
        self.tel.map(|t| t.batt_volts)
    }

    pub fn payload_charging(&self) -> bool {
        self.tel.is_some_and(|t| t.sensors & common::Sensor::CHG != 0)
    }

    /// Whether the payload's telemetry currently rules out sending ARM
    /// (see `ground-logic::nogo`). `None` both when there's no reason to
    /// refuse *and* when there's no telemetry to judge at all -- callers
    /// that need to tell those apart should check `self.tel` directly.
    pub fn nogo(&self) -> Option<NogoReason> {
        self.tel.and_then(nogo_reason)
    }
}
