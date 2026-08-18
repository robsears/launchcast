//! LaunchCast ground station hardware-free logic.
//!
//! Pure state machines with no GPIO/radio/display imports, ported here so
//! they're host-testable exactly like `launchcast-common` -- see
//! `docs/rust-rewrite.md`'s migration strategy. `launchcast-ground` (the
//! firmware crate) wires these to real hardware.
#![cfg_attr(not(test), no_std)]

pub mod hold_tracker;
pub mod icons;
pub mod imu;
pub mod link;
pub mod nav;
pub mod nmea;
pub mod units;

pub use hold_tracker::{Edge, HoldTracker, KeyEvent, DEFAULT_GRACE_MS, DEFAULT_HOLD_MS};
pub use icons::{battery_level, battery_percent, signal_level, signal_percent, BATT_CURVE};
pub use imu::accel_magnitude_g;
pub use link::{link_status, LinkStatus, LINK_LOST_MS, LINK_STALE_MS};
pub use nav::{bearing_deg, compass_point, haversine_m, relative_arrow, EARTH_R_M};
pub use nmea::{parse_rmc, NmeaLineReader, RmcFix};
pub use units::{c_to_f, m_to_ft, Units};
