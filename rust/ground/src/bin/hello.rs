//! Minimal boot/flash smoke test -- blinks the onboard red LED (D13/GPIO13,
//! confirmed via Adafruit's Feather RP2040 RFM95 pinout docs) with no SPI,
//! no display, no multicore, no async executor. If this doesn't blink after
//! flashing, the bug is in the build/link/flash pipeline itself, not in
//! `main.rs`'s display driver -- narrows the search before touching SPI.
#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use embassy_rp::config::Config;
use embassy_rp::gpio::{Level, Output};
use panic_probe as _;

/// embassy_rp::init's default clock config runs the core at 125MHz;
/// cortex_m::asm::delay burns exactly this many core cycles, so this is
/// ~0.5s on Config::default() specifically -- not a portable constant.
const HALF_SECOND_CYCLES: u32 = 62_500_000;

#[entry]
fn main() -> ! {
    let p = embassy_rp::init(Config::default());
    let mut led = Output::new(p.PIN_13, Level::Low);

    loop {
        led.set_high();
        cortex_m::asm::delay(HALF_SECOND_CYCLES);
        led.set_low();
        cortex_m::asm::delay(HALF_SECOND_CYCLES);
    }
}
