//! FLIGHT screen: a rocket illustration plus two compact "systems check"
//! tables (the rocket's, then the handheld's own), rather than a wall of
//! raw numbers. Port of `screen_flight.py`.
//!
//! The default screen -- ARM/DISARM only sends its command here (see
//! `screen_footer.rs` and `main.rs`'s button dispatch), so the payload's
//! battery voltage stays as an exact number, not just a percentage: it
//! gates the NO-GO banner below.
//!
//! Only called with a real `Telemetry` (matching `screen_flight.py`'s own
//! contract -- `code.py`'s top-level `draw()` handles the "no telemetry
//! yet" fallback before ever reaching a screen module); see `main.rs`'s
//! display task for that split.
//!
//! `frame.my_lat`/`my_batt` are always `None` for now -- the ground
//! station's own GPS/battery ADC aren't wired up in this port yet (see
//! docs/rust-rewrite.md) -- so the CONTROLLER table's GPS lock always
//! reads SEARCH and DIST always reads `--`, same as Python would show
//! before its own GPS ever produced a fix.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use heapless::String;
use launchcast_common::{self as common, Sensor};
use launchcast_ground_logic::{battery_percent, signal_percent, Units};

use crate::display_util::text;
use crate::frame::Frame;
use crate::{handheld_art, rocket_art};

const ROCKET_X: i32 = 4;
const ROCKET_Y: i32 = 44;
const TABLE_X: i32 = ROCKET_X + rocket_art::ROCKET_ART_W as i32 + 6;
const RX: i32 = 265;
const HANDHELD_Y: i32 = 44;
const MC_TABLE_Y: i32 = HANDHELD_Y + handheld_art::HANDHELD_ART_H as i32 + 4;
const STATUS_Y: i32 = 204;

fn online(sensors: u8, bit: u8) -> &'static str {
    if sensors & bit != 0 {
        "ONLINE"
    } else {
        "OFFLINE"
    }
}

pub fn draw<D>(display: &mut D, frame: &Frame, tel: &common::Telemetry)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let units = Units::default(); // imperial, matching units.py's UNITS default

    rocket_art::draw(display, ROCKET_X, ROCKET_Y, frame.armed(), 1);

    // ----- ROCKET systems check ------------------------------------------
    text(display, TABLE_X, 44, "ROCKET", 2);
    text(display, TABLE_X, 64, "SYSTEMS CHECK:", 1);

    let mut line: String<40> = String::new();
    let _ = core::fmt::write(&mut line, format_args!("ATMOSPHERE:      {}", online(tel.sensors, Sensor::BARO)));
    text(display, TABLE_X, 78, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("ACCELEROMETER:   {}", online(tel.sensors, Sensor::IMU)));
    text(display, TABLE_X, 90, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("MAGNETOMETER:    {}", online(tel.sensors, Sensor::MAG)));
    text(display, TABLE_X, 102, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("FILESYSTEM:      {}", online(tel.sensors, Sensor::LOG)));
    text(display, TABLE_X, 114, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("GPS LOCK:        {}", if tel.has_fix { "FIXED" } else { "SEARCH" }));
    text(display, TABLE_X, 126, &line, 1);

    line.clear();
    let _ = core::fmt::write(
        &mut line,
        format_args!("TEMPERATURE:     {:.1}{}", units.temperature(tel.temp_c), units.temperature_label()),
    );
    text(display, TABLE_X, 138, &line, 1);

    line.clear();
    let _ = core::fmt::write(
        &mut line,
        format_args!(
            "ALTITUDE:        {:.1}{}",
            units.distance(tel.alt_baro_m as f32),
            units.distance_label()
        ),
    );
    text(display, TABLE_X, 150, &line, 1);

    line.clear();
    let _ = core::fmt::write(
        &mut line,
        format_args!(
            "BATTERY:         {}%{}",
            battery_percent(Some(tel.batt_volts)),
            if tel.sensors & Sensor::CHG != 0 { " CHG" } else { "" }
        ),
    );
    text(display, TABLE_X, 162, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("SIGNAL STRENGTH: {}%", signal_percent(frame.rssi)));
    text(display, TABLE_X, 174, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("STATUS:          {}", tel.state_name()));
    text(display, TABLE_X, 186, &line, 1);

    // ----- MISSION CONTROL (handheld) systems check ----------------------
    handheld_art::draw(display, RX, HANDHELD_Y, 1);

    text(display, RX, MC_TABLE_Y, "CONTROLLER", 2);

    line.clear();
    let _ = core::fmt::write(
        &mut line,
        format_args!("GPS LOCK: {}", if frame.my_lat.is_some() { "FIXED" } else { "SEARCH" }),
    );
    text(display, RX, MC_TABLE_Y + 21, &line, 1);

    line.clear();
    let _ = core::fmt::write(
        &mut line,
        format_args!(
            "BATTERY:  {}%{}",
            battery_percent(frame.my_batt),
            if frame.my_charging { " CHG" } else { "" }
        ),
    );
    text(display, RX, MC_TABLE_Y + 33, &line, 1);

    // Latched last-known rocket GPS fix isn't tracked yet (Link's
    // fix_lat/fix_lon in code.py), and frame.my_lat is always None for
    // now regardless -- so this always takes the "--" branch today, same
    // as Python would show before either GPS ever produced a fix.
    text(display, RX, MC_TABLE_Y + 45, "DIST:     --", 1);

    // ----- alerts / command status (bottom, full width) -------------------
    if tel.batt_volts < 3.80 {
        text(display, 4, STATUS_Y, "*** PAYLOAD BATT LOW -- NO GO ***", 1);
    } else {
        text(display, 4, STATUS_Y, frame.tx_status, 1);
    }
}
