use launchcast_rocket_logic::buzzer::{pwm_top_and_half, BUZZER_HZ, SYS_CLK_HZ};

#[test]
fn matches_hand_computed_reference_at_buzzer_hz() {
    // 125MHz / 5250Hz -- hand-computed in Python (see docs/rust-rewrite.md's
    // rocket-port session log).
    let (top, compare) = pwm_top_and_half(SYS_CLK_HZ, BUZZER_HZ);
    assert_eq!(top, 23809);
    assert_eq!(compare, 11905);
}

#[test]
fn resulting_frequency_is_close_to_target() {
    let (top, _) = pwm_top_and_half(SYS_CLK_HZ, BUZZER_HZ);
    let actual_hz = SYS_CLK_HZ as f32 / (top as f32 + 1.0);
    assert!((actual_hz - BUZZER_HZ as f32).abs() < 1.0);
}

#[test]
fn compare_is_half_of_period_for_a_square_wave() {
    let (top, compare) = pwm_top_and_half(SYS_CLK_HZ, BUZZER_HZ);
    let period = top as u32 + 1;
    assert_eq!(compare as u32 * 2, period);
}

#[test]
fn does_not_panic_at_a_very_low_target() {
    let (top, compare) = pwm_top_and_half(SYS_CLK_HZ, 1);
    assert_eq!(top, u16::MAX);
    assert!(compare > 0);
}
