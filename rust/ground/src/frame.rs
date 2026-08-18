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

    pub tx_status: &'a str,
    /// Most-recent-last scrolling log of sent commands and their
    /// resolution (see `cmdlog.rs`). Rendered on the FLIGHT screen under
    /// the CONTROLLER panel.
    pub cmd_log: &'a [heapless::String<{ crate::cmdlog::STATUS_LEN }>; crate::cmdlog::CMD_LOG_LINES],
    pub screen_name: &'a str,
    pub next_screen_name: &'a str,
    pub prev_screen_name: &'a str,
}

impl<'a> Frame<'a> {
    pub fn armed(&self) -> bool {
        self.tel.is_some_and(|t| t.state == common::State::ARMED)
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
