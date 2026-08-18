//! Status-icon bucketing logic. Partial port of `ground/icons.py`: just the
//! hardware-free number-crunching (`battery_percent`, `battery_level`,
//! `signal_level`, `signal_percent`), not the bitmap-drawing functions,
//! which depend on a `display`-shaped sink (`pixel()`/`fill_rect()`/
//! `rect()`) and live in the `ground` firmware crate instead, wired to
//! `embedded-graphics`'s `DrawTarget`.

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

/// 1S LiPo rest-voltage -> remaining-charge estimate. Empirical sigmoid
/// (not a physical model), calibrated around 3.7V nominal: `123 * (1 -
/// 1/((1 + (V/3.7)^80)^0.165))`. Steep dropoff below ~3.7V (matches LiPo
/// discharge sag), saturates to ~100% by ~4.2V. Replaced an earlier
/// piecewise-linear anchor table (`BATT_CURVE`) after real flight data
/// showed it disagreeing with an observed 4.1V->5.0V jump (battery
/// reaching full charge and the Feather switching over to USB power,
/// *not* a battery voltage the curve needed to represent at all) --
/// this formula's own saturation behavior handles that case correctly
/// without needing a special case: an above-range reading like 5.0V
/// still clamps to 100%, same as any other over-4.2V input.
pub fn battery_percent(volts: Option<f32>) -> u8 {
    let Some(volts) = volts else {
        return 0;
    };
    let ratio = volts / 3.7;
    let pct = 123.0 * (1.0 - 1.0 / libm::powf(1.0 + libm::powf(ratio, 80.0), 0.165));
    libm::roundf(pct.clamp(0.0, 100.0)) as u8
}

/// Bucket battery percent into a 0-4 fill level for the bar icon.
pub fn battery_level(volts: Option<f32>) -> u8 {
    (battery_percent(volts) / 25).min(4)
}
