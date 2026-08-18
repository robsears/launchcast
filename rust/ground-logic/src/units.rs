//! Unit conversion for the ground station's display.
//!
//! Port of `ground/units.py`. `Units` picks feet/Fahrenheit vs
//! meters/Celsius for every screen; screens should call
//! `Units::distance`/`Units::temperature` rather than format the raw
//! telemetry field directly.
//!
//! Unlike the Python version -- a module-level `UNITS` string mutated
//! in-place by a future settings screen -- this is an explicit `Copy` enum
//! passed to the conversion calls. A mutable global is awkward in
//! `no_std` (no implicit interior mutability without pulling in a
//! synchronization primitive), and passing the current unit system
//! explicitly is the more idiomatic Rust shape regardless; the firmware
//! layer that eventually reads a settings screen's choice just needs to
//! hold one `Units` value and pass it down.

/// Imperial is the default, matching `ground/units.py`'s `UNITS =
/// "imperial"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Units {
    #[default]
    Imperial,
    Metric,
}

pub fn c_to_f(c: f32) -> f32 {
    c * 9.0 / 5.0 + 32.0
}

pub fn m_to_ft(m: f32) -> f32 {
    m * 3.28084
}

impl Units {
    /// Convert a Celsius reading (the wire format's unit) to the display
    /// unit.
    pub fn temperature(self, c: f32) -> f32 {
        match self {
            Units::Imperial => c_to_f(c),
            Units::Metric => c,
        }
    }

    pub fn temperature_label(self) -> &'static str {
        match self {
            Units::Imperial => "F",
            Units::Metric => "C",
        }
    }

    /// Convert a meters reading (the wire format's unit) to the display
    /// unit.
    pub fn distance(self, m: f32) -> f32 {
        match self {
            Units::Imperial => m_to_ft(m),
            Units::Metric => m,
        }
    }

    pub fn distance_label(self) -> &'static str {
        match self {
            Units::Imperial => "ft",
            Units::Metric => "m",
        }
    }
}
