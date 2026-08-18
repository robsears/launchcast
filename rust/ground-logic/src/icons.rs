//! Status-icon bucketing logic. Partial port of `ground/icons.py`: just the
//! hardware-free number-crunching (`BATT_CURVE`, `battery_percent`,
//! `battery_level`, `signal_level`, `signal_percent`), not the
//! bitmap-drawing functions, which depend on a `display`-shaped sink
//! (`pixel()`/`fill_rect()`/`rect()`) and live in the `ground` firmware
//! crate instead, wired to `embedded-graphics`'s `DrawTarget`.

/// Bucket an RSSI reading (dBm) into a 0-4 bar count. `None` -> 0.
pub fn signal_level(rssi: Option<i16>) -> u8 {
    let Some(rssi) = rssi else {
        return 0;
    };
    if rssi >= -50 {
        4
    } else if rssi >= -70 {
        3
    } else if rssi >= -90 {
        2
    } else if rssi >= -110 {
        1
    } else {
        0
    }
}

/// Same 0-4 bucket as the bar icon, just as a percentage for tables/text.
pub fn signal_percent(rssi: Option<i16>) -> u16 {
    signal_level(rssi) as u16 * 25
}

/// 1S LiPo rest-voltage discharge curve, highest voltage first. LiPo
/// voltage sags fast in the last ~20% and stays nearly flat across the top
/// ~30%, so a straight-line (or 4-bucket) volts-to-percent mapping put a
/// 95%-charged pack and a 75%-charged pack in the same bucket. Interpolated
/// linearly between these anchor points instead. The 3.30 V/0% anchor is an
/// assumed empty-pack cutoff (not one of the measured points) -- adjust if
/// flight data disagrees.
pub const BATT_CURVE: [(f32, u8); 15] = [
    (4.20, 100),
    (4.18, 95),
    (4.10, 90),
    (4.05, 85),
    (4.00, 80),
    (3.95, 75),
    (3.90, 70),
    (3.85, 65),
    (3.82, 60),
    (3.78, 50),
    (3.72, 40),
    (3.68, 30),
    (3.60, 20),
    (3.50, 10),
    (3.30, 0),
];

/// Interpolate a rest voltage onto the discharge curve. `None` -> 0.
///
/// Rounds half away from zero (`libm::roundf`), not Python's round-half-
/// to-even -- the two agree everywhere `BATT_CURVE`'s anchors actually
/// produce a tie (e.g. the 3.80 V / 55% midpoint), so this is a
/// no_std-driven implementation difference, not a behavior change.
pub fn battery_percent(volts: Option<f32>) -> u8 {
    let Some(volts) = volts else {
        return 0;
    };
    let (top_v, top_p) = BATT_CURVE[0];
    if volts >= top_v {
        return top_p;
    }
    let (bottom_v, bottom_p) = BATT_CURVE[BATT_CURVE.len() - 1];
    if volts <= bottom_v {
        return bottom_p;
    }
    for pair in BATT_CURVE.windows(2) {
        let (v_hi, p_hi) = pair[0];
        let (v_lo, p_lo) = pair[1];
        if volts >= v_lo {
            let frac = (volts - v_lo) / (v_hi - v_lo);
            return libm::roundf(p_lo as f32 + frac * (p_hi as f32 - p_lo as f32)) as u8;
        }
    }
    0 // unreachable -- range is fully covered by the checks above
}

/// Bucket battery percent into a 0-4 fill level for the bar icon.
pub fn battery_level(volts: Option<f32>) -> u8 {
    (battery_percent(volts) / 25).min(4)
}
