//! Handheld's own battery voltage, via the RP2040's onboard ADC. Port of
//! `Hardware.battery_volts()` in `ground/code.py`.
//!
//! CircuitPython's `analogio.AnalogIn.value` always normalizes readings to
//! a 16-bit range (0-65535) regardless of the chip's native ADC
//! resolution, for portability across boards -- Python's formula divides
//! by 65535.0 accordingly. The RP2040's ADC is natively 12-bit (0-4095),
//! and `embassy_rp::adc::Adc::blocking_read` returns that raw 12-bit
//! value directly, so this divides by 4095.0 instead; the rest of the
//! formula (3.3V reference, external divider ratio) is unchanged.
//!
//! `my_charging` (`code.py`'s `supervisor.runtime.usb_connected` check)
//! isn't ported -- detecting USB presence on bare-metal embassy-rp needs
//! either a real USB device stack or reading VBUS-sense hardware state
//! this firmware doesn't set up, so it's a real, separate piece of work,
//! not folded into this one. `my_charging` stays `false` for now.

use embassy_rp::adc::{Adc, Blocking, Channel, Error as AdcError};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;

/// Onboard voltage divider ratio -- matches `code.py`'s `BATT_DIVIDER`.
/// A0 is wired to BAT through an external 2:1 divider (see CLAUDE.md),
/// not the regulated 3.3V rail.
const BATT_DIVIDER: f32 = 2.0;
/// Matches `code.py`'s `BATT_SAMPLES`.
const BATT_SAMPLES: u32 = 8;
/// Matches `code.py`'s `vbat_period` (checked every 2s).
const POLL_PERIOD_MS: u64 = 2000;

/// Latest handheld battery voltage. `None` until the first successful
/// read (or if every sample in a round fails).
pub static MY_BATT: Mutex<CriticalSectionRawMutex, Option<f32>> = Mutex::new(None);

fn read_volts(adc: &mut Adc<'static, Blocking>, channel: &mut Channel<'static>) -> Result<f32, AdcError> {
    let mut total: u32 = 0;
    for _ in 0..BATT_SAMPLES {
        total += adc.blocking_read(channel)? as u32;
    }
    let avg = total as f32 / BATT_SAMPLES as f32;
    Ok((avg / 4095.0) * 3.3 * BATT_DIVIDER)
}

#[embassy_executor::task]
pub async fn battery_task(mut adc: Adc<'static, Blocking>, mut channel: Channel<'static>) {
    loop {
        if let Ok(volts) = read_volts(&mut adc, &mut channel) {
            *MY_BATT.lock().await = Some(volts);
        }
        Timer::after_millis(POLL_PERIOD_MS).await;
    }
}
