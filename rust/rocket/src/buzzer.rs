//! Piezo buzzer -- differential drive across D5/D6 (GPIO5/GPIO6). PWM
//! register math lives in `rocket-logic::buzzer` (hardware-free, host-
//! tested); this module owns the two PWM peripherals. See that module's
//! docs for why this is two independent PWM slices, not one synchronized
//! pair -- GPIO5/GPIO6 don't share a slice on this chip.

use embassy_rp::peripherals::{PWM_SLICE2, PWM_SLICE3};
use embassy_rp::pwm::{Config as PwmConfig, Pwm};
use embassy_rp::Peri;
use fixed::traits::ToFixed;
use launchcast_rocket_logic::buzzer::{pwm_top_and_half, BUZZER_HZ, SYS_CLK_HZ};

pub struct Buzzer<'d> {
    hi: Pwm<'d>,
    lo: Pwm<'d>,
    config: PwmConfig,
}

impl<'d> Buzzer<'d> {
    pub fn new(slice_hi: Peri<'d, PWM_SLICE3>, pin_hi: Peri<'d, embassy_rp::peripherals::PIN_6>, slice_lo: Peri<'d, PWM_SLICE2>, pin_lo: Peri<'d, embassy_rp::peripherals::PIN_5>) -> Self {
        let (top, compare) = pwm_top_and_half(SYS_CLK_HZ, BUZZER_HZ);
        let mut config = PwmConfig::default();
        config.top = top;
        config.divider = 1.to_fixed();
        config.compare_a = 0; // silent until buzz_on()
        config.compare_b = compare;

        // GPIO6 = PWM_SLICE3 channel A, GPIO5 = PWM_SLICE2 channel B --
        // see rocket-logic::buzzer's docs for the slice/channel mapping.
        let hi = Pwm::new_output_a(slice_hi, pin_hi, config.clone());
        let lo = Pwm::new_output_b(slice_lo, pin_lo, config.clone());

        Self { hi, lo, config }
    }

    pub fn on(&mut self) {
        let (_, compare) = pwm_top_and_half(SYS_CLK_HZ, BUZZER_HZ);
        self.config.compare_a = compare;
        self.config.compare_b = compare;
        self.hi.set_config(&self.config);
        self.lo.set_config(&self.config);
    }

    pub fn off(&mut self) {
        self.config.compare_a = 0;
        self.config.compare_b = 0;
        self.hi.set_config(&self.config);
        self.lo.set_config(&self.config);
    }
}
