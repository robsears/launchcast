use launchcast_common::{Sensor, Telemetry};
use launchcast_ground_logic::{nogo_reason, NogoReason, NOGO_BATT_V};

fn telemetry(batt_volts: f32, sensors: u8) -> Telemetry {
    Telemetry {
        counter: 0,
        uptime_ms: 0,
        state: 0,
        lat: 0.0,
        lon: 0.0,
        alt_baro_m: 0,
        speed_mps: 0.0,
        temp_c: 0.0,
        accel_g: [0.0; 3],
        gyro_dps: [0.0; 3],
        batt_volts,
        has_fix: false,
        satellites: 0,
        cam_rec: false,
        sensors,
        cam_disk: 0,
    }
}

#[test]
fn healthy_telemetry_is_not_nogo() {
    let tel = telemetry(4.0, 0);
    assert_eq!(nogo_reason(&tel), None);
}

#[test]
fn low_battery_is_nogo() {
    let tel = telemetry(NOGO_BATT_V - 0.01, 0);
    assert_eq!(nogo_reason(&tel), Some(NogoReason::LowBattery));
}

#[test]
fn battery_at_the_threshold_is_not_nogo() {
    let tel = telemetry(NOGO_BATT_V, 0);
    assert_eq!(nogo_reason(&tel), None);
}

#[test]
fn charging_is_nogo() {
    let tel = telemetry(4.0, Sensor::CHG);
    assert_eq!(nogo_reason(&tel), Some(NogoReason::Charging));
}

#[test]
fn low_battery_takes_priority_over_charging() {
    let tel = telemetry(NOGO_BATT_V - 0.01, Sensor::CHG);
    assert_eq!(nogo_reason(&tel), Some(NogoReason::LowBattery));
}

#[test]
fn messages_are_distinct_and_labeled_no_go() {
    assert!(NogoReason::LowBattery.message().contains("NO GO"));
    assert!(NogoReason::Charging.message().contains("NO GO"));
    assert_ne!(NogoReason::LowBattery.message(), NogoReason::Charging.message());
}
