//! Hardware-free half of the status NeoPixel: per-state color table and
//! brightness scaling. Port of `rocket/code.py`'s `PIXEL_FOR_STATE` dict
//! and `neopixel.NeoPixel(..., brightness=0.2)`.
//!
//! CircuitPython's `neopixel` library scales every channel by the
//! configured `brightness` internally, in the driver, *after* a color is
//! assigned -- so `PIXEL_FOR_STATE`'s values are the pre-scaled ("full")
//! colors, and the actual wire output is scaled down at send time. This
//! module keeps that same two-step shape: [`color_for_state`] returns the
//! unscaled table value, [`scale_brightness`] is applied separately
//! (`rocket/src/pixel.rs`, at the point of writing to the PIO driver,
//! since `embassy-rp`'s `PioWs2812` has no brightness concept of its
//! own).

use launchcast_common::State;

/// Matches `code.py`'s `neopixel.NeoPixel(..., brightness=0.2)`.
pub const PIXEL_BRIGHTNESS: f32 = 0.2;

/// Scale an RGB triple by `brightness` (0.0-1.0), rounding each channel
/// the same way CircuitPython's `_pixelbuf` does (linear scale + round,
/// no gamma correction).
pub fn scale_brightness(rgb: [u8; 3], brightness: f32) -> [u8; 3] {
    rgb.map(|c| libm::roundf(c as f32 * brightness).clamp(0.0, 255.0) as u8)
}

/// Unscaled ("full") color for a flight state -- matches `code.py`'s
/// `PIXEL_FOR_STATE`. Falls back to `(16, 16, 16)` (dim white) for any
/// value outside the known states, matching `code.py`'s own
/// `PIXEL_FOR_STATE.get(fs.state, (16, 16, 16))` fallback in its state-
/// change handler.
pub fn color_for_state(state: u8) -> [u8; 3] {
    match state {
        State::BOOT => [0, 0, 32],     // dim blue
        State::IDLE => [16, 16, 0],    // dim yellow
        State::ARMED => [0, 32, 0],    // green
        State::BOOST => [48, 16, 0],   // orange
        State::COAST => [0, 16, 32],   // cyan
        State::APOGEE => [32, 0, 32],  // magenta
        State::DESCENT => [0, 24, 24], // teal
        State::LANDED => [48, 0, 0],   // red (brightest)
        _ => [16, 16, 16],
    }
}
