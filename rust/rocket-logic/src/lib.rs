//! LaunchCast rocket payload hardware-free logic.
//!
//! Pure state machines with no sensor/radio/flash imports, ported here so
//! they're host-testable exactly like `launchcast-common` and
//! `launchcast-ground-logic` -- see `docs/rust-rewrite.md`'s migration
//! strategy. The eventual `launchcast-rocket` firmware crate wires this to
//! real hardware.
#![cfg_attr(not(test), no_std)]

pub mod bmp580;
pub mod buzzer;
pub mod flash_log;
pub mod flight_state;
pub mod imu;
pub mod lis3mdl;
pub mod pixel;

pub use flight_state::{
    accel_magnitude, FlightState, APOGEE_VEL_MPS, BOOST_MIN_MS, BOOST_THRESHOLD_G,
    COAST_THRESHOLD_G, DESCENT_VEL_MPS, LANDED_ALT_M, LANDED_HOLD_MS, LANDED_VEL_MPS,
};
