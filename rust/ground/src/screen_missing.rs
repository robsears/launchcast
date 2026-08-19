//! Themed replacement for the plain "NO TELEMETRY" text fallback. Shown
//! whenever `launchcast_ground_logic::telemetry_missing` says so: either
//! no frame has ever arrived, or the last one is older than
//! `TELEMETRY_MISSING_MS` (60s) -- so a payload that goes quiet mid-flight
//! reverts to this same screen instead of leaving a stale FLIGHT/RECOVERY/
//! DIAG view on screen looking current. Distinct from `LinkStatus::Lost`
//! (15s, used elsewhere as a live-screen "link degraded" indicator, e.g.
//! `screen_recovery.rs`) -- this is the coarser "nothing current to show
//! at all" threshold.
//!
//! Deliberately mirrors `screen_flight.rs`'s layout rather than being its
//! own design: same rocket-glyph position (magnifying glass instead),
//! same "SYSTEMS CHECK" table (every value "??" instead of a real
//! reading, header "SEARCHING" instead of "ROCKET"), and the *same*
//! `draw_controller_panel` on the right -- so the handheld's own GPS
//! lock/battery/command log stay visible even while the rocket itself is
//! unheard from, and flipping between this screen and FLIGHT once
//! telemetry resumes doesn't feel like a context switch.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;

use crate::display_util::text;
use crate::frame::Frame;
use crate::missing_art;
use crate::screen_flight::{draw_controller_panel, ROCKET_X, ROCKET_Y, TABLE_X};

/// Every ROCKET-side reading, in order, exactly matching
/// `screen_flight.rs`'s label column and row spacing (78..=186 in steps
/// of 12) so the two screens align pixel-for-pixel when flipped between.
const ROWS: [&str; 10] = [
    "ATMOSPHERE:      ??",
    "ACCELEROMETER:   ??",
    "MAGNETOMETER:    ??",
    "FILESYSTEM:      ??",
    "GPS LOCK:        ??",
    "TEMPERATURE:     ??",
    "ALTITUDE:        ??",
    "BATTERY:         ??",
    "SIGNAL STRENGTH: ??",
    "STATUS:          ??",
];
const ROW0_Y: i32 = 78;
const ROW_STEP: i32 = 12;

pub fn draw<D>(display: &mut D, frame: &Frame)
where
    D: DrawTarget<Color = BinaryColor>,
{
    missing_art::draw(display, ROCKET_X, ROCKET_Y, 1);

    text(display, TABLE_X, 44, "SEARCHING", 2);
    // No telemetry means no rocket fw_version to show either -- "??"
    // matches every other value on this screen (see ROWS below).
    text(display, TABLE_X, 64, "SYSTEMS CHECK:   v??", 1);

    for (i, row) in ROWS.iter().enumerate() {
        text(display, TABLE_X, ROW0_Y + i as i32 * ROW_STEP, row, 1);
    }

    // Command status renders once, in the command log under CONTROLLER
    // (drawn above) -- no live telemetry to gate a NO-GO alert on here,
    // so unlike FLIGHT, nothing else belongs at the bottom of this
    // screen.
    draw_controller_panel(display, frame);
}
