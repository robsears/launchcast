//! Third-stage smoke test: alternate the whole panel between all-ink and
//! all-light using the real `display.rs` driver's line-write path (the one
//! `display_clear.rs`'s dedicated hardware clear-mode command doesn't
//! exercise), single core, no multicore/executor -- same isolation
//! rationale as `display_clear.rs`. Reuses the actual driver file via
//! `#[path]` rather than reimplementing it, so this tests the real code,
//! not a second copy of it that could hide a different bug.
//!
//! If `display_clear.rs` produced no visible change at all, and this also
//! produces none, that points at wiring/power/CS rather than protocol
//! logic -- both tests go through identical CS-toggle-then-clock-bytes
//! mechanics, so if the panel were reachable at all, at least one should
//! show something.
//!
//! Updated 2026-08-17 to use the PIO-backed bus (CLK=D5, MOSI=D12) instead
//! of hardware SPI1, matching the real driver after the SPI1-vs-radio bus
//! conflict was found -- see `display.rs`'s module docs.
#![no_std]
#![no_main]

#[path = "../display.rs"]
mod display;

use cortex_m_rt::entry;
use defmt_rtt as _;
use embassy_rp::bind_interrupts;
use embassy_rp::config::Config;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler as PioInterruptHandler, Pio};
use embassy_rp::pio_programs::spi::Spi as PioSpi;
use embassy_rp::spi::Config as SpiConfig;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use panic_probe as _;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => PioInterruptHandler<PIO0>;
});

const ONE_SECOND_CYCLES: u32 = 125_000_000;

#[entry]
fn main() -> ! {
    let p = embassy_rp::init(Config::default());
    let mut led = Output::new(p.PIN_13, Level::Low);

    let Pio {
        mut common, sm0, ..
    } = Pio::new(p.PIO0, Irqs);
    let mut spi_config = SpiConfig::default();
    spi_config.frequency = 2_000_000;
    let spi = PioSpi::new_blocking(&mut common, sm0, p.PIN_5, p.PIN_12, p.PIN_1, spi_config);
    let cs = Output::new(p.PIN_6, Level::Low);
    let mut disp = display::SharpMemoryDisplay::new(spi, cs);

    loop {
        let _ = disp.clear(BinaryColor::On); // full ink/black
        disp.show();
        led.set_high();
        cortex_m::asm::delay(ONE_SECOND_CYCLES);

        let _ = disp.clear(BinaryColor::Off); // full light/white
        disp.show();
        led.set_low();
        cortex_m::asm::delay(ONE_SECOND_CYCLES);
    }
}
