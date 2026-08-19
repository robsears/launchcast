//! Status NeoPixel -- thin wrapper over `embassy-rp`'s first-party PIO
//! WS2812 driver (`embassy_rp::pio_programs::ws2812`, not a third-party
//! NeoPixel crate). Per-state color table and brightness scaling live in
//! `rocket-logic::pixel` (hardware-free, host-tested) -- this module just
//! applies them and writes. PIO/DMA construction itself happens in
//! `main.rs` (needs real hardware peripherals + interrupt bindings),
//! matching how `ground`'s display PIO bus is assembled in its `main.rs`
//! rather than inside `display.rs`.

use embassy_rp::peripherals::PIO0;
use embassy_rp::pio_programs::ws2812::{Grb, PioWs2812};
use launchcast_rocket_logic::pixel::{color_for_state, scale_brightness, PIXEL_BRIGHTNESS};
use smart_leds::RGB8;

pub struct StatusPixel<'d> {
    driver: PioWs2812<'d, PIO0, 0, 1, Grb>,
}

impl<'d> StatusPixel<'d> {
    pub fn new(driver: PioWs2812<'d, PIO0, 0, 1, Grb>) -> Self {
        Self { driver }
    }

    /// Set the pixel to the color for a flight state -- matches
    /// `code.py`'s `hw.set_pixel(PIXEL_FOR_STATE[...])`.
    pub async fn set_state(&mut self, state: u8) {
        let [r, g, b] = scale_brightness(color_for_state(state), PIXEL_BRIGHTNESS);
        self.driver.write(&[RGB8::new(r, g, b)]).await;
    }
}
