//! SUMMARY: the decoded highlights of whichever flight was selected on
//! FLIGHTS -- pending/ready/failed, matching `summary_request.rs`'s
//! state machine. MENU here goes back to FLIGHTS specifically (see
//! `screen.rs`/`main.rs`'s button_task), not the normal screen rotation.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use heapless::String;
use launchcast_common::nmea::from_unix_ms;
use launchcast_ground_logic::{haversine_m, Units};

use crate::display_util::text;
use crate::frame::Frame;
use crate::summary_request::SummaryRequest;

// The "FLIGHT N" title is drawn at y=44 in size-2 text (embedded-graphics'
// FONT_10X20 -- see display_util.rs -- 20px tall), so it occupies
// y=44..64. ROW0_Y was originally 56 -- inside that band, causing a real
// collision on real hardware (2026-08-19). 68 clears it with a 4px gap.
const ROW0_Y: i32 = 68;
const ROW_STEP: i32 = 16;

/// `t_ms` -> `"12.3s"`, matching how every duration on this screen is
/// shown -- the wire format keeps millisecond precision (real for a
/// ~1.5s boost phase), the display doesn't need it.
fn seconds<const N: usize>(ms: u32) -> String<N> {
    let mut s = String::new();
    let _ = core::fmt::write(&mut s, format_args!("{:.1}s", ms as f32 / 1000.0));
    s
}

pub fn draw<D>(display: &mut D, frame: &Frame)
where
    D: DrawTarget<Color = BinaryColor>,
{
    match frame.summary_request {
        SummaryRequest::Idle => {
            text(display, 4, 70, "NO FLIGHT SELECTED", 1);
            text(display, 4, 94, "MENU to go back and pick one", 1);
        }
        SummaryRequest::Pending { index, .. } => {
            let mut line: String<32> = String::new();
            let _ = core::fmt::write(&mut line, format_args!("REQUESTING FLIGHT {}...", index + 1));
            text(display, 4, 70, &line, 1);
        }
        SummaryRequest::Failed { index } => {
            let mut line: String<32> = String::new();
            let _ = core::fmt::write(&mut line, format_args!("NO RESPONSE FOR FLIGHT {}", index + 1));
            text(display, 4, 70, &line, 1);
            text(display, 4, 94, "out of range, or link too weak", 1);
            text(display, 4, 112, "MENU, then try again", 1);
        }
        SummaryRequest::Ready(s) => {
            let units = Units::default();
            let mut line: String<40> = String::new();

            let _ = core::fmt::write(&mut line, format_args!("FLIGHT {}", s.flight_index + 1));
            text(display, 4, 44, &line, 2);

            let total_ms = s.wait_ms + s.boost_ms + s.coast_ms + s.descent_ms;
            line.clear();
            let _ = core::fmt::write(
                &mut line,
                format_args!(
                    "WAIT {} BOOST {} COAST {}",
                    seconds::<16>(s.wait_ms),
                    seconds::<16>(s.boost_ms),
                    seconds::<16>(s.coast_ms)
                ),
            );
            text(display, 4, ROW0_Y, &line, 1);

            line.clear();
            let _ = core::fmt::write(
                &mut line,
                format_args!("DESCENT {}   TOTAL {}", seconds::<16>(s.descent_ms), seconds::<16>(total_ms)),
            );
            text(display, 4, ROW0_Y + ROW_STEP, &line, 1);

            // MAX ALT and the at-apogee temp/pressure share a line --
            // the latter is a reading paired with the former, not an
            // independent extreme (see common::SummaryInput's docs on
            // temp_at_max_alt_c/pressure_at_max_alt_hpa), so keeping
            // them visually together also happens to save a row.
            line.clear();
            let _ = core::fmt::write(
                &mut line,
                format_args!(
                    "MAX ALT: {:.0}{}  ({:.0}{} {:.0}hPa)",
                    units.distance(s.max_alt_m),
                    units.distance_label(),
                    units.temperature(s.temp_at_max_alt_c),
                    units.temperature_label(),
                    s.pressure_at_max_alt_hpa
                ),
            );
            text(display, 4, ROW0_Y + ROW_STEP * 2, &line, 1);

            line.clear();
            let _ = core::fmt::write(&mut line, format_args!("MAX SPEED: {:.0} m/s", s.max_speed_mps));
            text(display, 4, ROW0_Y + ROW_STEP * 3, &line, 1);

            line.clear();
            let _ = core::fmt::write(&mut line, format_args!("MAX G: {:.1}", s.max_accel_g));
            text(display, 4, ROW0_Y + ROW_STEP * 4, &line, 1);

            line.clear();
            let _ = core::fmt::write(&mut line, format_args!("MAX ROTATION: {:.0} deg/s", s.max_gyro_dps));
            text(display, 4, ROW0_Y + ROW_STEP * 5, &line, 1);

            // Overland distance -- computed here, not transmitted: the
            // rocket sends the raw ARM/LANDED fixes, the ground already
            // has haversine_m (used identically for RECOVERY), so
            // there's no reason to duplicate that math on the rocket.
            line.clear();
            if (s.arm_lat, s.arm_lon) != (0.0, 0.0) && (s.landed_lat, s.landed_lon) != (0.0, 0.0) {
                let d = haversine_m(s.arm_lat, s.arm_lon, s.landed_lat, s.landed_lon);
                let _ = core::fmt::write(
                    &mut line,
                    format_args!("OVERLAND: {:.0}{}", units.distance(d), units.distance_label()),
                );
            } else {
                let _ = line.push_str("OVERLAND: -- (no GPS fix)");
            }
            text(display, 4, ROW0_Y + ROW_STEP * 6, &line, 1);

            // Both estimates, not measured: the ground never sees the raw
            // log unless it's physically pulled. Sample rate is
            // record_count over the full logged duration (ARM through
            // LANDED -- logging runs the whole time, including the WAIT
            // phase, so total_ms is the right denominator). Download size
            // assumes ~110 bytes/record for the *decoded CSV* row (not
            // the 48-byte raw flash/wire record) -- user-supplied figure,
            // matching what a real bench pull actually produced.
            line.clear();
            let _ = core::fmt::write(&mut line, format_args!("{} LOG RECORDS", s.record_count));
            if total_ms > 0 {
                let hz = s.record_count as f32 / (total_ms as f32 / 1000.0);
                let est_kb = s.record_count as f32 * 110.0 / 1024.0;
                let _ = core::fmt::write(&mut line, format_args!("  ~{hz:.0}Hz  ~{est_kb:.0}KB"));
            }
            text(display, 4, ROW0_Y + ROW_STEP * 7, &line, 1);

            // arm_epoch_s is 0 whenever the rocket armed before its GPS
            // ever got a UTC fix (see rocket-logic::flight_summary's
            // on_armed) -- nothing meaningful to show in that case.
            line.clear();
            if s.arm_epoch_s > 0 {
                let dt = from_unix_ms(s.arm_epoch_s as i64 * 1000);
                let _ = core::fmt::write(
                    &mut line,
                    format_args!(
                        "ARMED {:04}-{:02}-{:02} {:02}:{:02}:{:02}Z",
                        dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
                    ),
                );
            } else {
                let _ = line.push_str("ARMED: unknown (no GPS fix at arm)");
            }
            text(display, 4, ROW0_Y + ROW_STEP * 8, &line, 1);
        }
    }
}
