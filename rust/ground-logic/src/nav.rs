//! Great-circle navigation math shared by the RECOVERY and FLIGHT screens.
//!
//! Port of `ground/nav.py`. Pure computation, no hardware imports, so it
//! builds and tests exactly like the rest of `launchcast-ground-logic`.

/// Radius of the Earth in m. We probably won't need to change this.
pub const EARTH_R_M: f32 = 6371000.0;

/// Euclidean (always-nonnegative) remainder, matching Python's `%` on
/// floats. `f32::rem_euclid` needs `std` -- not available here -- so this
/// is the plain-arithmetic equivalent, no `libm` needed.
fn rem_euclid(a: f32, m: f32) -> f32 {
    let r = a % m;
    if r < 0.0 {
        r + m
    } else {
        r
    }
}

/// Great-circle distance in meters.
pub fn haversine_m(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let sin_dp_2 = libm::sinf(dp / 2.0);
    let sin_dl_2 = libm::sinf(dl / 2.0);
    let a = sin_dp_2 * sin_dp_2 + libm::cosf(p1) * libm::cosf(p2) * sin_dl_2 * sin_dl_2;
    2.0 * EARTH_R_M * libm::atan2f(libm::sqrtf(a), libm::sqrtf(1.0 - a))
}

/// Initial great-circle bearing, degrees true, 0-360.
pub fn bearing_deg(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dl = (lon2 - lon1).to_radians();
    let y = libm::sinf(dl) * libm::cosf(p2);
    let x = libm::cosf(p1) * libm::sinf(p2) - libm::sinf(p1) * libm::cosf(p2) * libm::cosf(dl);
    rem_euclid(libm::atan2f(y, x).to_degrees(), 360.0)
}

const COMPASS_POINTS: [&str; 16] = [
    "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW",
    "NNW",
];

pub fn compass_point(deg: f32) -> &'static str {
    let idx = (rem_euclid(deg + 11.25, 360.0) / 22.5) as usize;
    // `.min(15)` guards a float-rounding edge case at the exact 360 deg
    // boundary that `rem_euclid` doesn't fully rule out; Python's `int()`
    // truncation has the same theoretical edge but no equivalent guard.
    COMPASS_POINTS[idx.min(15)]
}

/// Turn instruction relative to the direction you are walking.
///
/// Only meaningful when moving -- GPS course over ground is undefined at a
/// standstill. Returns `None` if heading is unavailable.
pub fn relative_arrow(bearing: f32, heading: Option<f32>) -> Option<&'static str> {
    let heading = heading?;
    let rel = rem_euclid(bearing - heading, 360.0);
    Some(if !(22.5..337.5).contains(&rel) {
        "^ AHEAD"
    } else if rel < 67.5 {
        "> 45 RIGHT"
    } else if rel < 112.5 {
        ">> RIGHT"
    } else if rel < 157.5 {
        ">> BACK RIGHT"
    } else if rel < 202.5 {
        "v TURN AROUND"
    } else if rel < 247.5 {
        "<< BACK LEFT"
    } else if rel < 292.5 {
        "<< LEFT"
    } else {
        "< 45 LEFT"
    })
}
