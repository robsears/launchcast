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
//! DIST needs *both* a rocket fix (`frame.fix_lat`/`fix_lon`, latched in
//! `link.rs`) and the handheld's own fix (`frame.my_lat`/`my_lon`, from
//! `gps.rs`) -- reads `--` until both are present, same as Python would
//! show before either GPS ever produced a fix.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use heapless::String;
use launchcast_common::{self as common, Sensor};
use launchcast_ground_logic::{battery_percent, haversine_m, nogo_reason, signal_percent, NogoReason, Units};

use crate::display_util::text;
use crate::frame::Frame;
use crate::icons;
use crate::{handheld_art, lowbatt_art, rocket_art};

// Shared with screen_missing.rs, which mirrors this whole layout (rocket
// glyph swapped for the magnifying glass, telemetry swapped for "??") so
// the two screens read as the same view, not a jarring context switch.
pub const ROCKET_X: i32 = 4;
pub const ROCKET_Y: i32 = 44;
pub const TABLE_X: i32 = ROCKET_X + rocket_art::ROCKET_ART_W as i32 + 6;
pub const STATUS_Y: i32 = 204;

const RX: i32 = 265;

// The CONTROLLER column is shifted up from the ROCKET column's baseline
// (44) -- unlike the general "screens don't draw above y=40" rule
// (screen_header.rs), the header draws nothing at all in this column's
// x-range (RX=265) above y=14: LAUNCHCAST/the screen name live at x=4,
// the rocket cluster ends by ~x=222, and the handheld's own battery
// cluster (x=315+) is well clear too -- so there's no header collision
// to worry about here. Shifted to y=22 (2026-08-19, real-hardware
// feedback -- was y=40, then y=40 again briefly with a version line
// squeezed in below instead of here) specifically so the fw-version line
// under "CONTROLLER" (MC_VERSION_Y) doesn't cost the command log any of
// its vertical room -- net effect vs. the original (pre-version-line)
// layout is everything below the glyph sits 8px *higher* now, not lower.
const HANDHELD_Y: i32 = 22;
const MC_TITLE_Y: i32 = HANDHELD_Y + handheld_art::HANDHELD_ART_H as i32 + 2;
const MC_VERSION_Y: i32 = MC_TITLE_Y + 18; // handheld fw version
const MC_ROW0: i32 = MC_VERSION_Y + 10; // GPS LOCK
const MC_ROW1: i32 = MC_ROW0 + 10; // BATTERY
const MC_ROW2: i32 = MC_ROW1 + 10; // DIST
const CMD_LOG_Y0: i32 = MC_ROW2 + 8;
const CMD_LOG_ROW: i32 = 10;

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
    let nogo = nogo_reason(tel);

    // The low-battery glyph pre-empts the normal idle/armed rocket art --
    // seeing "can't launch" at a glance shouldn't require reading the
    // NO-GO banner text at the bottom too. Charging doesn't get its own
    // glyph (no art for it) -- the header's bolt icon and the BATTERY
    // row below already cover that case, so the rocket illustration
    // stays as normal idle/armed art.
    if nogo == Some(NogoReason::LowBattery) {
        lowbatt_art::draw(display, ROCKET_X, ROCKET_Y, 1);
    } else {
        rocket_art::draw(display, ROCKET_X, ROCKET_Y, frame.armed(), 1);
    }

    // ----- ROCKET systems check ------------------------------------------
    text(display, TABLE_X, 44, "ROCKET", 2);

    let mut line: String<40> = String::new();
    let _ = core::fmt::write(&mut line, format_args!("SYSTEMS CHECK:   v{}", tel.fw_version));
    text(display, TABLE_X, 64, &line, 1);

    line.clear();
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
    // ARM captures the ground-pressure reference (see CLAUDE.md); before
    // that, altitude reads as a meaningless ~0 that never changes, which
    // looks like a frozen/broken reading rather than "not applicable
    // yet." BOOT/IDLE show N/A instead; ARMED onward shows the real
    // (now reference-relative) value as it's actually received.
    if tel.state <= common::State::IDLE {
        let _ = line.push_str("ALTITUDE:        N/A");
    } else {
        let _ = core::fmt::write(
            &mut line,
            format_args!(
                "ALTITUDE:        {:.1}{}",
                units.distance(tel.alt_baro_m as f32),
                units.distance_label()
            ),
        );
    }
    text(display, TABLE_X, 150, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("BATTERY:         {}%", battery_percent(Some(tel.batt_volts))));
    text(display, TABLE_X, 162, &line, 1);
    if tel.sensors & Sensor::CHG != 0 {
        // Icon, not " CHG" text -- matches the header's icon-only
        // convention, and the text form used to overflow this row past
        // RX at a 3-digit percentage (100% CHG). Placed immediately
        // after the text with no gap; still a few px into the
        // CONTROLLER column's margin in that same worst case, but far
        // less than the 30px the old text overflowed by.
        icons::draw_bolt(display, TABLE_X + line.len() as i32 * 8, 162, 1);
    }

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("SIGNAL STRENGTH: {}%", signal_percent(frame.rssi)));
    text(display, TABLE_X, 174, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("STATUS:          {}", tel.state_name()));
    text(display, TABLE_X, 186, &line, 1);

    // Command status (SENT ARM.../ARMED OK/etc) only ever renders once,
    // in the command log under CONTROLLER (drawn below) -- this bottom
    // line is reserved for the NO-GO alert alone, not a second copy of
    // the same status text.
    draw_controller_panel(display, frame);

    if let Some(reason) = nogo {
        text(display, 4, STATUS_Y, reason.message(), 1);
    }
}

/// The handheld's own status: GPS lock, battery, distance to the rocket,
/// and the scrolling command log. Doesn't depend on whether the rocket is
/// currently heard from, so it's shared verbatim between FLIGHT and
/// MISSING (see `screen_missing.rs`) -- only the left-hand ROCKET panel
/// differs between the two.
pub fn draw_controller_panel<D>(display: &mut D, frame: &Frame)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let units = Units::default();
    let mut line: String<40> = String::new();

    handheld_art::draw(display, RX, HANDHELD_Y, 1);

    text(display, RX, MC_TITLE_Y, "CONTROLLER", 2);

    let _ = core::fmt::write(&mut line, format_args!("FIRMWARE: v{}", crate::FIRMWARE_VERSION));
    text(display, RX, MC_VERSION_Y, &line, 1);

    line.clear();
    let _ = core::fmt::write(
        &mut line,
        format_args!("GPS LOCK: {}", if frame.my_lat.is_some() { "FIXED" } else { "SEARCH" }),
    );
    text(display, RX, MC_ROW0, &line, 1);

    line.clear();
    let _ = core::fmt::write(
        &mut line,
        format_args!(
            "BATTERY:  {}%{}",
            battery_percent(frame.my_batt),
            if frame.my_charging { " CHG" } else { "" }
        ),
    );
    text(display, RX, MC_ROW1, &line, 1);

    line.clear();
    match (frame.my_lat, frame.my_lon, frame.fix_lat, frame.fix_lon) {
        (Some(my_lat), Some(my_lon), Some(fix_lat), Some(fix_lon)) => {
            let d = haversine_m(my_lat, my_lon, fix_lat, fix_lon);
            let _ = core::fmt::write(
                &mut line,
                format_args!("DIST:     {:.0}{}", units.distance(d), units.distance_label()),
            );
        }
        // Either GPS hasn't produced a fix yet -- own (SEARCH above) or
        // the rocket's (never latched, see link.rs).
        _ => {
            let _ = line.push_str("DIST:     --");
        }
    }
    text(display, RX, MC_ROW2, &line, 1);

    // ----- command log (sent commands + their resolution) ----------------
    for (i, entry) in frame.cmd_log.iter().enumerate() {
        if !entry.is_empty() {
            text(display, RX, CMD_LOG_Y0 + i as i32 * CMD_LOG_ROW, entry, 1);
        }
    }
}
