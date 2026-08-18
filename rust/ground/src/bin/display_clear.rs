//! Second-stage smoke test: only the Sharp Memory LCD's dedicated "all
//! clear" command (M1 bit, 0x20) -- no line addressing, no framebuffer, no
//! multicore. Per the LS027B7DH01 datasheet this is [cmd=0x20] [0x00] with
//! CS held high across the transfer, the simplest possible real interaction
//! with the panel. If the panel visibly changes (goes/stays uniform blank)
//! in step with this loop, SPI wiring/timing/power/CS polarity are all
//! confirmed good and the bug is confined to `display.rs`'s line-write
//! loop. If nothing changes at all, the bug is upstream of that -- wiring,
//! power, or CS polarity.
//!
//! Updated 2026-08-17 to use the PIO-backed bus (CLK=D5, MOSI=D12) instead
//! of hardware SPI1, matching the real driver after the SPI1-vs-radio bus
//! conflict was found -- see `display.rs`'s module docs.
#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use embassy_rp::bind_interrupts;
use embassy_rp::config::Config;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler as PioInterruptHandler, Pio};
use embassy_rp::pio_programs::spi::Spi as PioSpi;
use embassy_rp::spi::Config as SpiConfig;
use panic_probe as _;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => PioInterruptHandler<PIO0>;
});

const CMD_CLEAR: u8 = 0x20;
const HALF_SECOND_CYCLES: u32 = 62_500_000;

#[entry]
fn main() -> ! {
    let p = embassy_rp::init(Config::default());
    let mut led = Output::new(p.PIN_13, Level::Low);

    let Pio {
        mut common, sm0, ..
    } = Pio::new(p.PIO0, Irqs);
    let mut spi_config = SpiConfig::default();
    spi_config.frequency = 2_000_000;
    let mut spi = PioSpi::new_blocking(&mut common, sm0, p.PIN_5, p.PIN_12, p.PIN_1, spi_config);
    let mut cs = Output::new(p.PIN_6, Level::Low);

    loop {
        cs.set_high();
        let _ = spi.blocking_write(&[CMD_CLEAR]);
        let _ = spi.blocking_write(&[0x00]);
        cs.set_low();

        // Onboard LED double-blinks each clear pulse -- a heartbeat visible
        // even if the panel itself shows nothing, so we can tell "loop is
        // still running" apart from "panel isn't responding."
        led.set_high();
        cortex_m::asm::delay(HALF_SECOND_CYCLES / 4);
        led.set_low();
        cortex_m::asm::delay(HALF_SECOND_CYCLES);
    }
}
