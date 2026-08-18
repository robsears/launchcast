//! Shared display helper for the ground station's screen modules. Port of
//! `display_util.py`.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_6X10};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text, TextStyleBuilder};

/// Port of `display_util.py`'s `text(display, x, y, s, size=1, color=0)`.
///
/// Two things don't map 1:1 onto `embedded-graphics` and are handled here
/// rather than at every call site:
/// - CircuitPython's built-in framebuf font is an 8x8 glyph scaled by an
///   integer `size` factor; `embedded-graphics` has no equivalent scalable
///   bitmap font, so `size` picks the closest fixed preset instead --
///   `FONT_6X10` for 1, `FONT_10X20` (the largest built into
///   `embedded-graphics`'s ascii set) for 2 and 3 alike. Anchor
///   coordinates are still ported 1:1 from the Python screens, so layouts
///   match structurally even though exact glyph metrics don't.
/// - CircuitPython's `display.text()` anchors `(x, y)` at the top-left of
///   the glyph box; `embedded-graphics`'s `Text` defaults to baseline
///   anchoring instead. Explicit `Baseline::Top` here matches the Python
///   convention the ported coordinates assume.
pub fn text<D>(display: &mut D, x: i32, y: i32, s: &str, size: u8)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let ink = BinaryColor::On;
    let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();
    if size <= 1 {
        let char_style = MonoTextStyle::new(&FONT_6X10, ink);
        let _ = Text::with_text_style(s, Point::new(x, y), char_style, text_style).draw(display);
    } else {
        let char_style = MonoTextStyle::new(&FONT_10X20, ink);
        let _ = Text::with_text_style(s, Point::new(x, y), char_style, text_style).draw(display);
    }
}
