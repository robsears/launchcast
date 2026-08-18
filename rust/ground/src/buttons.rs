//! Real GPIO wiring for the three handheld buttons, driving the
//! hardware-free `HoldTracker` state machine from
//! `launchcast_ground_logic`.
//!
//! Port of the button half of `ground/code.py`'s `Hardware._init_buttons`
//! and the main loop's button-poll block. Pins, active-low sense, and
//! names/order all match `ground/code.py` (`board.D9/D10/D11`, `pull=True`,
//! `value_when_pressed=False`, `BUTTON_NAMES = ("arm", "chirp", "menu")`) --
//! GPIO numbers (9/10/11) confirmed against `docs/images/feather
//! pinout.png`, not guessed.
//!
//! CircuitPython's `keypad.Keys` debounces in the background at
//! `interval=DEBOUNCE_MS / 1000.0` (`ground/code.py`'s `DEBOUNCE_MS = 50`).
//! There's no equivalent supervisor-level scan here, so [`Buttons::poll`]
//! is meant to be called on a fixed [`DEBOUNCE_MS`] timer: a raw level
//! change observed between two samples that far apart is treated as a real
//! edge. Mechanical bounce settles in single-digit milliseconds, far under
//! the 50 ms sample spacing, so this is standard poll-interval debounce,
//! not per-edge filtering -- the same tradeoff `keypad.Keys` makes, just
//! without a background scan doing the sampling.

use embassy_rp::gpio::{Input, Pull};
use embassy_rp::peripherals::{PIN_10, PIN_11, PIN_9};
use embassy_rp::Peri;
use embassy_time::Instant;
use launchcast_ground_logic::{Edge, HoldTracker, KeyEvent};

/// Matches `ground/code.py`'s `DEBOUNCE_MS` (`keypad.Keys`'s scan
/// interval).
pub const DEBOUNCE_MS: u64 = 50;

/// Matches `ground/code.py`'s `BUTTON_NAMES`, index-for-index -- key_number
/// 0/1/2 = arm/chirp/menu.
pub const BUTTON_NAMES: [&str; 3] = ["arm", "chirp", "menu"];

pub struct Buttons {
    pins: [Input<'static>; 3],
    raw_pressed: [bool; 3],
    tracker: HoldTracker<3>,
}

impl Buttons {
    pub fn new(
        pin_arm: Peri<'static, PIN_9>,
        pin_chirp: Peri<'static, PIN_10>,
        pin_menu: Peri<'static, PIN_11>,
    ) -> Self {
        Self {
            pins: [
                Input::new(pin_arm, Pull::Up),
                Input::new(pin_chirp, Pull::Up),
                Input::new(pin_menu, Pull::Up),
            ],
            raw_pressed: [false; 3],
            // HOLD_MS/GRACE_MS in ground/code.py are 2000/250 -- exactly
            // HoldTracker's defaults.
            tracker: HoldTracker::new(),
        }
    }

    /// Sample all three buttons and dispatch any tap/hold events via
    /// `on_edge(key_number, edge)`. Call on a fixed [`DEBOUNCE_MS`] timer.
    pub fn poll(&mut self, on_edge: impl FnMut(usize, Edge)) {
        let now_ms = Instant::now().as_millis() as u32;
        let mut events: [Option<KeyEvent>; 3] = [None; 3];

        for (key_number, pin) in self.pins.iter().enumerate() {
            let pressed = pin.is_low(); // active-low
            if pressed != self.raw_pressed[key_number] {
                self.raw_pressed[key_number] = pressed;
                events[key_number] = Some(KeyEvent {
                    key_number,
                    pressed,
                });
            }
        }

        self.tracker
            .poll(events.into_iter().flatten(), now_ms, on_edge);
    }
}

/// `"tap"` / `"hold"`, matching the strings `ground/code.py`'s
/// `HoldTracker` produces (used only for the `BUTTON EVENT` log line).
pub fn edge_name(edge: Edge) -> &'static str {
    match edge {
        Edge::Tap => "tap",
        Edge::Hold => "hold",
    }
}
