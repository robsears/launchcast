//! Footer rendered on every screen: what each button does right now. Port
//! of `screen_footer.py`.
//!
//! Contextual per screen -- see the ARM/DISARM vs BACK split in
//! `main.rs`'s button dispatch, which this must stay in sync with:
//!   - FLIGHT: a 2s **hold** sends the ARM/DISARM command; MENU advances
//!     to the next screen.
//!   - anywhere else: a mere **tap** on the same button goes back one
//!     screen instead (no hold needed -- there's no command to guard
//!     against a mis-press on these screens). MENU still advances, so the
//!     screen list is a loop you can walk either direction a step at a
//!     time.
//!
//! Only FLIGHT exists so far, so `is_flight` is always `true` at every
//! call site today -- kept as a real parameter (not hardcoded) since
//! `screen_footer.py` itself is shared across all three screens and this
//! is meant to stay that way once RECOVERY/DIAG exist.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use heapless::String;

use crate::display_util::text;
use crate::frame::Frame;

const FOOTER_Y: i32 = 222;

pub fn draw<D>(display: &mut D, frame: &Frame, is_flight: bool, prev_screen_name: &str)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mut arm_label: String<24> = String::new();
    if is_flight {
        if frame.armed() {
            // Always allowed regardless of NOGO -- the gate is about
            // preventing a *new* arm, never about trapping the user out
            // of disarming one that's already active.
            let _ = arm_label.push_str("HOLD:DISARM");
        } else if frame.landed() {
            // Same hold, same DISARM wire command -- the rocket treats
            // DISARM-from-LANDED as "acknowledge recovery" (silence the
            // beacon, back to IDLE) rather than the abort-and-rewind
            // meaning DISARM has from ARMED (rocket/src/main.rs). Added
            // 2026-08-19 after discovering there was previously no way
            // out of LANDED short of a power cycle. Not a NOGO check
            // below -- silencing the beacon isn't a launch-safety
            // decision, so a low-battery/charging NOGO must never block
            // it (not to be confused with the planned RECOVERY *screen*,
            // an unrelated distance-tracking view -- this is the FLIGHT
            // footer's button label).
            let _ = arm_label.push_str("HOLD:RECOVER");
        } else if frame.nogo().is_none() {
            let _ = arm_label.push_str("HOLD:ARM");
        }
        // else: leave blank -- a NOGO condition (low battery/charging,
        // see ground-logic::nogo) means holding this button does
        // nothing right now (see main.rs's core0_task), so nothing
        // should invite the press. The NO-GO banner on FLIGHT itself
        // explains why.
    } else {
        // A tap, not a hold, navigates back off FLIGHT -- see main.rs's
        // button_task -- so this label must say so too, or it implies a
        // 2s hold is still required here.
        let _ = core::fmt::write(&mut arm_label, format_args!("TAP:BACK>{prev_screen_name}"));
    }

    let mut menu_label: String<24> = String::new();
    let _ = core::fmt::write(&mut menu_label, format_args!("MENU>{}", frame.next_screen_name));

    text(display, 10, FOOTER_Y, &menu_label, 1);
    text(display, 170, FOOTER_Y, &arm_label, 1);
    text(display, 330, FOOTER_Y, "TAP:CHIRP", 1);
}
