//! Footer rendered on every screen: what each button does right now. Port
//! of `screen_footer.py`.
//!
//! Contextual per screen -- see the ARM/DISARM vs BACK split in
//! `main.rs`'s button dispatch, which this must stay in sync with:
//!   - FLIGHT: ARM/DISARM sends the command; MENU advances to the next
//!     screen.
//!   - anywhere else: ARM/DISARM goes back one screen instead (MENU still
//!     advances, so the screen list is a loop you can walk either
//!     direction a step at a time).
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
        let _ = arm_label.push_str(if frame.armed() { "HOLD:DISARM" } else { "HOLD:ARM" });
    } else {
        let _ = core::fmt::write(&mut arm_label, format_args!("HOLD:BACK>{prev_screen_name}"));
    }

    let mut menu_label: String<24> = String::new();
    let _ = core::fmt::write(&mut menu_label, format_args!("MENU>{}", frame.next_screen_name));

    text(display, 10, FOOTER_Y, &menu_label, 1);
    text(display, 170, FOOTER_Y, &arm_label, 1);
    text(display, 330, FOOTER_Y, "TAP:CHIRP", 1);
}
