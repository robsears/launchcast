//! Payload battery voltage, via the RP2040's onboard ADC. Port of
//! `Hardware.battery_volts()` in `rocket/code.py`. Same formula as the
//! ground station's own battery reading (`ground/src/battery.rs`) --
//! `BATT_DIVIDER`/`BATT_SAMPLES` match `rocket/code.py`'s constants of
//! the same name, and both boards wire A0 to BAT through an external 2:1
//! divider (see CLAUDE.md).
//!
//! Deliberately just a function, not an autonomous polling task like the
//! ground station's `battery_task`: `rocket/code.py`'s own main loop
//! gates *when* to check the battery on flight phase (at most every 5s,
//! and never during BOOST/COAST -- powered flight) -- that scheduling
//! belongs to the main flight loop, not to this module.

use embassy_rp::adc::{Adc, Blocking, Channel, Error as AdcError};

const BATT_DIVIDER: f32 = 2.0;
const BATT_SAMPLES: u32 = 8;

pub fn read_volts(adc: &mut Adc<'static, Blocking>, channel: &mut Channel<'static>) -> Result<f32, AdcError> {
    let mut total: u32 = 0;
    for _ in 0..BATT_SAMPLES {
        total += adc.blocking_read(channel)? as u32;
    }
    let avg = total as f32 / BATT_SAMPLES as f32;
    Ok((avg / 4095.0) * 3.3 * BATT_DIVIDER)
}
