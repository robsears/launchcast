//! Sharp Memory LCD driver (LS027B7DH01, 400x240), hand-rolled.
//!
//! No usable off-the-shelf driver exists on stable Rust for this display
//! -- see `docs/rust-rewrite.md`'s ecosystem table (`sharp-memory-display`
//! is GPL-3.0+, incompatible with this project's license; `memory-lcd-spi`
//! needs a nightly-only `#![feature]`). The wire protocol below is ported
//! directly from Adafruit's CircuitPython driver
//! (`adafruit_sharpmemorydisplay`, the library `ground/code.py` currently
//! uses), read from its actual source rather than assumed, since getting
//! the command-byte/line-address/VCOM framing wrong produces a garbled or
//! dead display:
//!
//! ```text
//! [cmd] [addr(1) data(W/8) 0x00] * H  [0x00]
//! ```
//!
//! - `cmd` = `0x80` (WRITECMD) `| 0x40` (VCOM) if this frame should flip
//!   the VCOM polarity bit.
//! - `addr` is the 1-indexed line number, **bit-reversed** (the panel is
//!   natively LSB-first; the MCU's SPI hardware shifts MSB-first, so the
//!   address byte is pre-reversed to compensate -- pixel data is NOT
//!   reversed, it's sent as-is from the framebuffer).
//! - VCOM must toggle periodically or the panel takes DC-bias damage over
//!   time. `ground/code.py` doesn't do a separate lightweight VCOM-only
//!   update -- it just redraws the full frame at `DISPLAY_HZ = 2.0` and
//!   relies on that cadence to "also service VCOM" (its own comment).
//!   [`SharpMemoryDisplay::show`] mirrors that: call it periodically even
//!   when content hasn't changed.
//! - Chip select is **active HIGH** -- confirmed directly from the
//!   CircuitPython driver's `SPIDevice(..., cs_active_value=True)`, not
//!   the more common active-low assumption.
//!
//! Confirmed working on real hardware (2026-08-17), first via the RP2040's
//! hardware SPI1 -- but SPI1's SCK/MOSI/MISO pins (GPIO14/15/8) are
//! physically the *same* pins the onboard RFM95 radio uses (confirmed via
//! CircuitPython's board definition for this exact board, see
//! `docs/rust-rewrite.md` bug log). Sharing a single hardware SPI
//! peripheral across the two cores this firmware splits buttons+display
//! (core1) and radio+GPS (core0) across would mean one core's SPI
//! transaction blocks the other's -- reintroducing exactly the
//! "button press, no idea if it registered" latency this whole rewrite is
//! trying to eliminate. So the display now runs on a PIO-backed bus
//! instead (`embassy_rp::pio_programs::spi`) on different, otherwise-free
//! GPIOs (CLK=D5/GPIO5, MOSI=D12/GPIO12; CS stays on D6/GPIO6, since it's
//! just a manually-toggled GPIO, not part of the SPI peripheral) -- a real
//! independent hardware state machine, not bit-banging in the main loop,
//! so it has zero contention with the radio's SPI1 at all. The display is
//! write-only, but `pio_programs::spi::Spi`'s program is inherently
//! full-duplex (SPI's shift register clocks in and out simultaneously), so
//! it still requires a MISO pin argument -- GPIO1 (D0/UART RX, unused by
//! this firmware) is wired to nothing and its rx data is always discarded.

use embassy_rp::gpio::Output;
use embassy_rp::pio::Instance;
use embassy_rp::pio_programs::spi::Spi;
use embassy_rp::spi::Blocking;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

pub const WIDTH: usize = 400;
pub const HEIGHT: usize = 240;
const LINE_BYTES: usize = WIDTH / 8;
const FRAME_BYTES: usize = LINE_BYTES * HEIGHT;

const CMD_WRITE: u8 = 0x80;
const CMD_VCOM: u8 = 0x40;
// CMD_CLEAR (0x20), the panel's dedicated all-clear command, isn't used --
// the boot-time `fill(1)` in ground/code.py goes through the normal frame
// path instead, which this port matches (buffer starts all-`1`/light).

fn reverse_bits(mut n: u8) -> u8 {
    let mut result = 0u8;
    for _ in 0..8 {
        result <<= 1;
        result |= n & 1;
        n >>= 1;
    }
    result
}

pub struct SharpMemoryDisplay<'d, PIO: Instance, const SM: usize> {
    spi: Spi<'d, PIO, SM, Blocking>,
    cs: Output<'d>,
    vcom: bool,
    /// Bit-packed framebuffer: MSB-first per byte, left to right, one bit
    /// per pixel. `1` = light (matches `ground/code.py`'s boot-time
    /// `display.fill(1)  # 1 = light on this panel`), `0` = ink -- also
    /// matching `ground/icons.py`'s `draw_bitmap` default `color=0` for a
    /// drawn ("on") pixel.
    buffer: [u8; FRAME_BYTES],
}

impl<'d, PIO: Instance, const SM: usize> SharpMemoryDisplay<'d, PIO, SM> {
    pub fn new(spi: Spi<'d, PIO, SM, Blocking>, cs: Output<'d>) -> Self {
        Self {
            spi,
            cs,
            vcom: false,
            buffer: [0xFF; FRAME_BYTES],
        }
    }

    /// Send the full frame and flip the VCOM bit for next time. Must be
    /// called periodically (`ground/code.py` uses 2 Hz) even with no
    /// content changes -- see the module docs on VCOM.
    pub fn show(&mut self) {
        let mut cmd = CMD_WRITE;
        if self.vcom {
            cmd |= CMD_VCOM;
        }
        self.vcom = !self.vcom;

        self.cs.set_high(); // active-high CS
        let _ = self.spi.blocking_write(&[cmd]);
        for line in 0..HEIGHT {
            let addr = reverse_bits((line + 1) as u8);
            let start = line * LINE_BYTES;
            let _ = self.spi.blocking_write(&[addr]);
            let _ = self
                .spi
                .blocking_write(&self.buffer[start..start + LINE_BYTES]);
            let _ = self.spi.blocking_write(&[0x00]); // per-line trailing byte
        }
        let _ = self.spi.blocking_write(&[0x00]); // final trailing byte
        self.cs.set_low();
    }
}

impl<'d, PIO: Instance, const SM: usize> DrawTarget for SharpMemoryDisplay<'d, PIO, SM> {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            if point.x < 0 || point.y < 0 || point.x as usize >= WIDTH || point.y as usize >= HEIGHT
            {
                continue;
            }
            let x = point.x as usize;
            let y = point.y as usize;
            let byte = y * LINE_BYTES + x / 8;
            let bit = 7 - (x % 8);
            match color {
                BinaryColor::On => self.buffer[byte] &= !(1 << bit), // ink
                BinaryColor::Off => self.buffer[byte] |= 1 << bit,   // light
            }
        }
        Ok(())
    }
}

impl<'d, PIO: Instance, const SM: usize> OriginDimensions for SharpMemoryDisplay<'d, PIO, SM> {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}
