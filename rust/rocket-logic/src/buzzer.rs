//! Hardware-free half of the buzzer driver: PWM register math. The
//! actual PWM peripheral setup lives in `rocket/src/buzzer.rs`.
//!
//! Port of `rocket/code.py`'s `buzz_on`/`buzz_off` -- differential drive
//! across D5/D6 (GPIO5/GPIO6, confirmed via this board's CircuitPython
//! `pins.c`), each its own `pwmio.PWMOut` at the buzzer's resonance
//! (`BUZZER_HZ`), both driven to the same 50% duty cycle. Not a true
//! hardware-synchronized differential pair -- GPIO5 and GPIO6 fall on
//! *different* RP2040 PWM slices (`slice = gpio/2 % 8`: GPIO5 -> slice 2
//! channel B, GPIO6 -> slice 3 channel A), so there's no single PWM
//! instance that could drive both in lockstep even if wanted. This
//! matches `code.py`'s own docstring on the same point ("phase is
//! approximate") -- not a limitation this port introduces.

/// RP2040 default system clock post-`embassy_rp::init(Config::default())`
/// -- both boards' firmware use this default, unmodified.
pub const SYS_CLK_HZ: u32 = 125_000_000;

/// Piezo resonance / max-volume frequency -- matches `code.py`'s `BUZZER_HZ`.
pub const BUZZER_HZ: u32 = 5250;

/// `(top, compare_50pct)` for a PWM slice at clock divider 1 targeting
/// `target_hz` from `sys_clk_hz`. `top` sets the counter wrap point
/// (period = `top + 1` cycles); `compare_50pct` is the compare value for
/// a 50% duty square wave at that period. Saturates to `u16::MAX` if the
/// requested frequency would need a larger period than a 16-bit counter
/// (at divider 1) can represent -- not a real concern at this buzzer's
/// frequency (top comes out to ~23810 at 125MHz/5250Hz), but a division
/// by a near-zero target shouldn't panic.
pub fn pwm_top_and_half(sys_clk_hz: u32, target_hz: u32) -> (u16, u16) {
    let period = (sys_clk_hz + target_hz / 2) / target_hz; // rounding division
    let top = period.saturating_sub(1).min(u16::MAX as u32) as u16;
    let compare = (top as u32).div_ceil(2) as u16;
    (top, compare)
}
