//! Header rendered on every screen: title, screen name, and the global
//! status icons (rocket cluster: payload icon/link signal/payload
//! battery; handheld's own battery). Port of `screen_header.py`.
//!
//! Screens should not draw above y=40 -- that band belongs to the header.
//! Screen size: 400x240 (matches `screen_header.py`'s own comment).

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use heapless::String;
use launchcast_common::nmea::from_unix_ms;

use crate::display_util::text;
use crate::frame::Frame;
use crate::icons;

pub fn draw<D>(display: &mut D, frame: &Frame)
where
    D: DrawTarget<Color = BinaryColor>,
{
    text(display, 4, 4, "LAUNCHCAST", 2); // 16x16, 160px wide (Python's own sizing)
    text(display, 4, 26, frame.screen_name, 1); // 8x8

    const GAP: i32 = 4;
    const SIGNAL_SCALE: i32 = 2;

    // rocket cluster: payload icon, link signal, PAYLOAD battery -- the
    // voltage the rocket radios down, not the handheld's own battery.
    // Pushed from x=160 to x=240 (2026-08-19) to leave room for the
    // wall-clock string between the title and this cluster -- see below.
    let rocket_x: i32 = 240;
    let rocket_signal = rocket_x + icons::ROCKET_W as i32 + GAP;
    let rocket_batt = rocket_signal + icons::SIGNAL_W as i32 * SIGNAL_SCALE + GAP;

    icons::draw_rocket(display, rocket_x, 4, 1);
    icons::draw_signal(display, rocket_signal, 4, frame.rssi, SIGNAL_SCALE);
    icons::draw_battery(display, rocket_batt, 4, frame.payload_batt(), BinaryColor::On);
    if frame.payload_charging() {
        icons::draw_bolt(display, rocket_batt + icons::BATT_W + 2, 4, 1);
    }

    // handheld's own battery -- separate from the rocket cluster above, or
    // it silently overwrites the one reading we have for the ground unit.
    // No "HH" label (removed 2026-08-19) -- the ground glyph immediately
    // to its left already identifies this as the handheld's own battery,
    // same as the rocket icon does for the cluster above; the battery
    // icon moved up into the row that label used to occupy. Pushed from
    // x=315 to x=355 (2026-08-19) alongside the rocket cluster's shift.
    let ground_x: i32 = 355;
    let ground_batt = ground_x + icons::GROUND_W as i32 + GAP;
    icons::draw_ground(display, ground_x, 4, 1);
    icons::draw_battery(display, ground_batt, 4, frame.my_batt, BinaryColor::On);
    if frame.my_charging {
        icons::draw_bolt(display, ground_batt + icons::BATT_W + 2, 4, 1);
    }

    // The handheld's own wall-clock time -- own GPS fix, own
    // EpochOffset, no rocket/wire involvement (see frame.rs's
    // my_wall_clock_ms docs). Same row as "LAUNCHCAST" (not the
    // screen-name row below it), in the gap between the title and the
    // rocket cluster -- roomier there than squeezed under the screen
    // name. Fixed-width format, so a fixed x lines up every draw; blank
    // until this board's GPS has a UTC fix.
    if let Some(ms) = frame.my_wall_clock_ms {
        let dt = from_unix_ms(ms);
        let mut line: String<24> = String::new();
        let _ = core::fmt::write(
            &mut line,
            format_args!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z",
                dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
            ),
        );
        text(display, 112, 4, &line, 1);
    }
}
