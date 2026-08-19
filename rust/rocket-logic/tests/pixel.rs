use launchcast_common::State;
use launchcast_rocket_logic::pixel::{color_for_state, scale_brightness, PIXEL_BRIGHTNESS};

#[test]
fn colors_match_the_python_table() {
    assert_eq!(color_for_state(State::BOOT), [0, 0, 32]);
    assert_eq!(color_for_state(State::IDLE), [16, 16, 0]);
    assert_eq!(color_for_state(State::ARMED), [0, 32, 0]);
    assert_eq!(color_for_state(State::BOOST), [48, 16, 0]);
    assert_eq!(color_for_state(State::COAST), [0, 16, 32]);
    assert_eq!(color_for_state(State::APOGEE), [32, 0, 32]);
    assert_eq!(color_for_state(State::DESCENT), [0, 24, 24]);
    assert_eq!(color_for_state(State::LANDED), [48, 0, 0]);
}

#[test]
fn unknown_state_falls_back_to_dim_white() {
    assert_eq!(color_for_state(200), [16, 16, 16]);
}

#[test]
fn scale_brightness_at_full_is_identity() {
    assert_eq!(scale_brightness([48, 16, 0], 1.0), [48, 16, 0]);
}

#[test]
fn scale_brightness_at_zero_is_black() {
    assert_eq!(scale_brightness([48, 16, 0], 0.0), [0, 0, 0]);
}

#[test]
fn scale_brightness_matches_the_configured_default() {
    // 48 * 0.2 = 9.6 -> rounds to 10.
    assert_eq!(scale_brightness([48, 0, 0], PIXEL_BRIGHTNESS), [10, 0, 0]);
}

#[test]
fn scale_brightness_never_exceeds_255() {
    assert_eq!(scale_brightness([255, 255, 255], 2.0), [255, 255, 255]);
}
