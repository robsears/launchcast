//! Small monochrome status icons for the ground station's Sharp Memory
//! Display. Port of `ground/icons.py`'s drawing functions -- the
//! hardware-free bucketing logic (`battery_percent`/`battery_level`/
//! `signal_level`/`signal_percent`) already lives in
//! `launchcast_ground_logic`, ported separately so it stays host-testable.
//! Bitmap data lives in `icon_bitmaps.rs` (generated from `icons.py`'s
//! ASCII art, see that module's docs).
//!
//! `draw_bitmap` mirrors `icons.py`'s stencil semantics exactly: a `0` bit
//! means "leave whatever's already there alone" (no draw call at all),
//! not "clear to background" -- matching Python's `draw_bitmap`, which
//! only ever calls `display.pixel()`/`fill_rect()` for `'1'` characters.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use launchcast_ground_logic::{battery_level, signal_level};

use crate::icon_bitmaps::{
    BOLT_BITS, BOLT_H, BOLT_W, GROUND_BITS, ROCKET_ICON_BITS, SIGNAL_BARS_0, SIGNAL_BARS_1, SIGNAL_BARS_2,
    SIGNAL_BARS_3, SIGNAL_BARS_4,
};
// Re-exported under the same short names `screen_header.py` uses
// (`icons.ROCKET_W`, ...) for the small header icon -- unambiguous within
// this module even though `rocket_art.rs` separately has its own, much
// larger `ROCKET_ART_W`/`_H` for the big FLIGHT-screen illustration.
pub use crate::icon_bitmaps::{
    ROCKET_ICON_H as ROCKET_H, ROCKET_ICON_W as ROCKET_W, GROUND_H, GROUND_W, SIGNAL_H, SIGNAL_W,
};
pub const BATT_W: i32 = 20;
pub const BATT_H: i32 = 10;

/// Draw a monochrome bitmap. `rows`/`bits` is bit-packed, MSB-first per
/// row (see `icon_bitmaps.rs`) -- the Rust equivalent of `icons.py`'s
/// `draw_bitmap(display, x, y, rows, color=0, scale=1)`, which took rows
/// of `'0'`/`'1'` strings instead.
#[allow(clippy::too_many_arguments)] // mirrors icons.py's own parameter list closely; a bundling struct would just be indirection for a single, stable-shape internal helper
pub fn draw_bitmap<D>(display: &mut D, x: i32, y: i32, w: usize, h: usize, bits: &[u8], color: BinaryColor, scale: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let stride = w.div_ceil(8);
    for row in 0..h {
        for col in 0..w {
            let byte = bits[row * stride + col / 8];
            let bit = 7 - (col % 8);
            if (byte >> bit) & 1 == 0 {
                continue;
            }
            if scale <= 1 {
                let _ = Pixel(Point::new(x + col as i32, y + row as i32), color).draw(display);
            } else {
                let _ = Rectangle::new(
                    Point::new(x + col as i32 * scale, y + row as i32 * scale),
                    Size::new(scale as u32, scale as u32),
                )
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(display);
            }
        }
    }
}

/// Handheld (ground) glyph -- matches `icons.py`'s `draw_ground`.
pub fn draw_ground<D>(display: &mut D, x: i32, y: i32, scale: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    draw_bitmap(display, x, y, GROUND_W, GROUND_H, &GROUND_BITS, BinaryColor::On, scale);
}

/// Small rocket (payload) header glyph -- matches `icons.py`'s
/// `draw_rocket`. Not the big FLIGHT-screen illustration; see
/// `rocket_art::draw` for that.
pub fn draw_rocket<D>(display: &mut D, x: i32, y: i32, scale: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    draw_bitmap(display, x, y, ROCKET_W, ROCKET_H, &ROCKET_ICON_BITS, BinaryColor::On, scale);
}

/// Cell-phone-style signal bars, bucketed from RSSI -- matches `icons.py`'s
/// `draw_signal`.
pub fn draw_signal<D>(display: &mut D, x: i32, y: i32, rssi: Option<i16>, scale: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let bits: &[u8] = match signal_level(rssi) {
        4 => &SIGNAL_BARS_4,
        3 => &SIGNAL_BARS_3,
        2 => &SIGNAL_BARS_2,
        1 => &SIGNAL_BARS_1,
        _ => &SIGNAL_BARS_0,
    };
    draw_bitmap(display, x, y, SIGNAL_W, SIGNAL_H, bits, BinaryColor::On, scale);
}

/// Battery outline + fill segments -- matches `icons.py`'s `draw_battery`
/// (`display.rect` -> stroke-only rectangle, `display.fill_rect` -> a
/// filled one, both 1:1 with `embedded-graphics` primitives).
pub fn draw_battery<D>(display: &mut D, x: i32, y: i32, volts: Option<f32>, color: BinaryColor)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let _ = Rectangle::new(Point::new(x, y), Size::new(BATT_W as u32, BATT_H as u32))
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(display);
    // nub
    let _ = Rectangle::new(Point::new(x + BATT_W, y + 3), Size::new(2, (BATT_H - 6) as u32))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(display);

    let level = battery_level(volts) as i32;
    let seg_w = (BATT_W - 4) / 4;
    for i in 0..level {
        let _ = Rectangle::new(
            Point::new(x + 2 + i * seg_w, y + 2),
            Size::new((seg_w - 1) as u32, (BATT_H - 4) as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(display);
    }
}

/// Charging bolt -- matches `icons.py`'s `draw_bolt`.
pub fn draw_bolt<D>(display: &mut D, x: i32, y: i32, scale: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    draw_bitmap(display, x, y, BOLT_W, BOLT_H, &BOLT_BITS, BinaryColor::On, scale);
}
