//! Footer rendered on every screen: what each button does right now. Port
//! of `screen_footer.py`, extended 2026-08-19 for FLIGHTS/SUMMARY.
//!
//! Contextual per screen -- must stay in sync with `main.rs`'s button
//! dispatch (`button_task` and `core0_task`'s forwarded-event handler):
//!   - FLIGHT: a 2s **hold** on the left button sends ARM/DISARM/RECOVER;
//!     a **tap** on the right sends CHIRP; MENU advances.
//!   - FLIGHTS: a **tap** on the left cycles the list cursor (no hold --
//!     nothing here needs a mis-press guard); a **tap** on the right
//!     selects the highlighted flight and requests its summary, or, if
//!     the cache came back genuinely empty, re-checks with the rocket
//!     instead (`TAP:REFRESH`, see `main.rs`'s FLIGHTS-refresh branch);
//!     MENU advances (leaving FLIGHTS via the normal rotation, same as
//!     any other screen).
//!   - SUMMARY: both the left and right buttons are inert by design --
//!     nothing to hold or tap here, just read the result. MENU goes back
//!     to FLIGHTS specifically, not the next screen in the rotation.
//!   - RECOVERY: a mere **tap** on the left goes back one screen (no
//!     hold needed -- no command to guard against a mis-press here);
//!     right button still sends CHIRP; MENU still advances.
//!   - DIAG: left/MENU same as RECOVERY, but the right button is
//!     repurposed to manually invalidate the flight-index cache
//!     (`TAP:CLR CACHE`, see `flight_index.rs`/`main.rs`'s DIAG-specific
//!     forwarded-event branch) instead of sending CHIRP. Its label sits
//!     10px left of the usual right-column x (`"TAP:RELOAD FLIGHTS"` ran
//!     off the right edge of the 400px display at the normal position;
//!     even the shorter replacement text keeps the nudge, for margin).

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use heapless::String;

use crate::display_util::text;
use crate::flight_index::IndexState;
use crate::frame::Frame;
use crate::screen;

const FOOTER_Y: i32 = 222;

pub fn draw<D>(display: &mut D, frame: &Frame, current_screen: u8, prev_screen_name: &str)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mut left_label: String<24> = String::new();
    let mut right_label: String<24> = String::new();
    let mut right_x: i32 = 330;

    match current_screen {
        screen::FLIGHT => {
            if frame.armed() {
                // Always allowed regardless of NOGO -- the gate is about
                // preventing a *new* arm, never about trapping the user
                // out of disarming one that's already active.
                let _ = left_label.push_str("HOLD:DISARM");
            } else if frame.recoverable() {
                // Same hold, same DISARM wire command -- the rocket
                // treats DISARM-from-any-post-ARMED-state as "acknowledge
                // recovery" (silence the beacon if it's sounding, back to
                // IDLE) rather than the abort-and-rewind meaning DISARM
                // has from ARMED (rocket/src/main.rs). Valid past LANDED
                // too (BOOST/COAST/APOGEE/DESCENT) so a flight stuck
                // mid-state-machine isn't unrecoverable. Not a NOGO check
                // below -- silencing the beacon isn't a launch-safety
                // decision.
                let _ = left_label.push_str("HOLD:RECOVER");
            } else if frame.nogo().is_none() {
                let _ = left_label.push_str("HOLD:ARM");
            }
            // else: leave blank -- a NOGO condition (low battery/
            // charging, see ground-logic::nogo) means holding this
            // button does nothing right now (see main.rs's core0_task),
            // so nothing should invite the press. The NO-GO banner on
            // FLIGHT itself explains why.
            let _ = right_label.push_str("TAP:CHIRP");
        }
        screen::FLIGHTS => {
            let _ = left_label.push_str("TAP:CYCLE");
            if matches!(frame.flight_index_state, IndexState::Ready { count } if count > 0) {
                let _ = right_label.push_str("TAP:SELECT");
            } else if frame.flight_index_state == IndexState::Empty {
                // Nothing to select, but a successful "no flights" answer
                // is exactly the case worth offering a manual re-check
                // for (e.g. the user just RECOVERed on the rocket itself
                // without a wire ACK reaching the handheld) -- see
                // main.rs's FLIGHTS-refresh branch.
                let _ = right_label.push_str("TAP:REFRESH");
            }
            // else (Idle/Pending/Failed): leave blank -- nothing useful
            // to do here yet; Idle/Pending resolve on their own via
            // auto-fetch, Failed's message already says to try again via
            // navigating away and back.
        }
        screen::SUMMARY => {
            // Both inert by design -- see module docs. Left blank
            // rather than any label at all.
        }
        screen::DIAG => {
            // A tap, not a hold, navigates back -- see main.rs's
            // button_task -- so this label must say so too, or it
            // implies a 2s hold is still required here.
            let _ = core::fmt::write(&mut left_label, format_args!("TAP:BACK>{prev_screen_name}"));
            let _ = right_label.push_str("TAP:CLR CACHE");
            right_x -= 10;
        }
        _ => {
            // A tap, not a hold, navigates back -- see main.rs's
            // button_task -- so this label must say so too, or it
            // implies a 2s hold is still required here.
            let _ = core::fmt::write(&mut left_label, format_args!("TAP:BACK>{prev_screen_name}"));
            let _ = right_label.push_str("TAP:CHIRP");
        }
    }

    let mut menu_label: String<24> = String::new();
    let _ = core::fmt::write(&mut menu_label, format_args!("MENU>{}", frame.next_screen_name));

    text(display, 10, FOOTER_Y, &menu_label, 1);
    text(display, 170, FOOTER_Y, &left_label, 1);
    text(display, right_x, FOOTER_Y, &right_label, 1);
}
