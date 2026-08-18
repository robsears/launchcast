//! Rust port of `tests/test_packet.py`. Same behavior, same cases -- this is
//! the spec the Python suite already validated against, translated rather
//! than re-derived. Keep the two files in sync when either changes.

use launchcast_common::*;

fn sample() -> TelemetryInput {
    TelemetryInput {
        counter: 42,
        uptime_ms: 123_456,
        state: State::COAST,
        lat: 41.2565,
        lon: -95.9345,
        alt_baro_m: 287.0,
        speed_mps: -14.2,
        temp_c: 23.5,
        accel_g: [0.02, -0.01, 0.98],
        gyro_dps: [1.5, -0.3, 12.0],
        batt_volts: 3.94,
        has_fix: true,
        satellites: 9,
        cam_rec: 0,
        sensors: Sensor::ALL,
        cam_disk: 0,
    }
}

// --- Sizes are contractual ---------------------------------------------------

#[test]
fn telemetry_is_40_bytes() {
    assert_eq!(TELEMETRY_SIZE, 40);
}

#[test]
fn command_is_7_bytes() {
    assert_eq!(COMMAND_SIZE, 7);
}

// --- Round trip ----------------------------------------------------------------

#[test]
fn telemetry_round_trip() {
    let out = unpack_telemetry(&pack_telemetry(&sample())).unwrap();
    assert_eq!(out.counter, 42);
    assert_eq!(out.uptime_ms, 123_456);
    assert_eq!(out.state, State::COAST);
    assert_eq!(out.state_name(), "COAST");
    assert_eq!(out.alt_baro_m, 287);
    assert_eq!(out.satellites, 9);
    assert!(out.has_fix);
}

#[test]
fn scalar_precision() {
    let out = unpack_telemetry(&pack_telemetry(&sample())).unwrap();
    assert!((out.lat - 41.2565).abs() < 1e-4);
    assert!((out.lon - (-95.9345)).abs() < 1e-4);
    assert!((out.speed_mps - (-14.2)).abs() < 0.01);
    assert!((out.temp_c - 23.5).abs() < 0.05);
    assert!((out.batt_volts - 3.94).abs() < 0.01);
}

#[test]
fn vector_round_trip() {
    let out = unpack_telemetry(&pack_telemetry(&sample())).unwrap();
    for (got, want) in out.accel_g.iter().zip([0.02, -0.01, 0.98]) {
        assert!((got - want).abs() < 0.002);
    }
    for (got, want) in out.gyro_dps.iter().zip([1.5, -0.3, 12.0]) {
        assert!((got - want).abs() < 0.1);
    }
}

// --- Rejection -----------------------------------------------------------------

#[test]
fn reject_empty() {
    assert!(unpack_telemetry(&[]).is_none());
}

#[test]
fn reject_wrong_length() {
    assert!(unpack_telemetry(&[0u8; 39]).is_none());
    assert!(unpack_telemetry(&[0u8; 41]).is_none());
}

#[test]
fn reject_bad_magic() {
    let mut frame = pack_telemetry(&sample());
    frame[0] = 0x00;
    assert!(unpack_telemetry(&frame).is_none());
}

#[test]
fn reject_wrong_packet_type() {
    // A command frame padded to 40 bytes must not decode as telemetry.
    let mut frame = pack_telemetry(&sample());
    frame[1] = PKT_COMMAND;
    assert!(unpack_telemetry(&frame).is_none());
}

#[test]
fn reject_all_zeros() {
    assert!(unpack_telemetry(&[0u8; 40]).is_none());
}

#[test]
fn reject_all_ones() {
    // Stuck-high line. 0xFF != MAGIC, so this must reject.
    assert!(unpack_telemetry(&[0xFFu8; 40]).is_none());
}

// --- Saturation, not overflow ---------------------------------------------------

#[test]
fn altitude_clamps_high() {
    let mut input = sample();
    input.alt_baro_m = 99999.0;
    let out = unpack_telemetry(&pack_telemetry(&input)).unwrap();
    assert_eq!(out.alt_baro_m, 32767);
}

#[test]
fn altitude_clamps_low() {
    let mut input = sample();
    input.alt_baro_m = -99999.0;
    let out = unpack_telemetry(&pack_telemetry(&input)).unwrap();
    assert_eq!(out.alt_baro_m, -32768);
}

#[test]
fn accel_clamps_rather_than_wrapping() {
    // A clipped accel must not change sign -- that would look like the
    // rocket reversed direction.
    let mut input = sample();
    input.accel_g = [50.0, -50.0, 0.0];
    let out = unpack_telemetry(&pack_telemetry(&input)).unwrap();
    assert!(out.accel_g[0] > 0.0);
    assert!(out.accel_g[1] < 0.0);
}

// --- Battery encoding ------------------------------------------------------------

#[test]
fn battery_encoding_round_trip() {
    for volts in [3.00f32, 3.30, 3.70, 3.80, 4.20, 5.55] {
        assert!((decode_batt(encode_batt(volts)) - volts).abs() < 0.005);
    }
}

#[test]
fn battery_clamps_below_range() {
    assert_eq!(encode_batt(1.0), 0);
}

#[test]
fn battery_clamps_above_range() {
    assert_eq!(encode_batt(9.0), 255);
}

#[test]
fn battery_gate_threshold_is_representable() {
    // 3.80 V is the no-go gate; it must survive the round trip exactly.
    assert!((decode_batt(encode_batt(3.80)) - 3.80).abs() < 1e-6);
}

// --- GPS flags -----------------------------------------------------------------

#[test]
fn gps_flags_round_trip() {
    for sats in [0u8, 1, 9, 12, 31] {
        let raw = encode_gps_flags(true, sats);
        let (fix, got) = decode_gps_flags(raw);
        assert!(fix);
        assert_eq!(got, sats);
    }
}

#[test]
fn gps_flags_no_fix() {
    let (fix, sats) = decode_gps_flags(encode_gps_flags(false, 7));
    assert!(!fix);
    assert_eq!(sats, 7);
}

#[test]
fn gps_satellites_saturate_at_31() {
    // Five bits. More than 31 satellites must clamp, not wrap to a small
    // number that looks like a poor fix.
    let (_, sats) = decode_gps_flags(encode_gps_flags(true, 40));
    assert_eq!(sats, 31);
}

// --- Sensor bitfield -------------------------------------------------------------

#[test]
fn sensor_bits_are_distinct() {
    let bits: Vec<u8> = Sensor::NAMES.iter().map(|(bit, _)| *bit).collect();
    let mut unique = bits.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(bits.len(), unique.len());
    for bit in bits {
        assert_eq!(bit & (bit - 1), 0, "not a single bit: {:#x}", bit);
    }
}

#[test]
fn sensor_all_covers_every_named_bit() {
    for (bit, _) in Sensor::NAMES {
        assert!(Sensor::ALL & bit != 0);
    }
}

#[test]
fn flight_ready_requires_baro_imu_log() {
    assert!(Sensor::flight_ready(Sensor::REQUIRED));
    assert!(Sensor::flight_ready(Sensor::ALL));
}

#[test]
fn flight_ready_fails_without_each_required() {
    for missing in [Sensor::BARO, Sensor::IMU, Sensor::LOG] {
        assert!(!Sensor::flight_ready(Sensor::ALL & !missing));
    }
}

#[test]
fn flight_ready_tolerates_optional_loss() {
    for optional in [Sensor::MAG, Sensor::GPS, Sensor::BATT] {
        assert!(Sensor::flight_ready(Sensor::ALL & !optional));
    }
}

#[test]
fn sensor_decode_partitions() {
    let flags = Sensor::BARO | Sensor::IMU | Sensor::LOG;
    let present: std::collections::HashSet<_> = Sensor::present(flags).collect();
    let missing: std::collections::HashSet<_> = Sensor::missing(flags).collect();
    assert_eq!(present, ["BARO", "IMU", "LOG"].into_iter().collect());
    assert_eq!(missing, ["MAG", "GPS", "BATT"].into_iter().collect());
}

#[test]
fn sensor_flags_survive_the_wire() {
    let flags = Sensor::BARO | Sensor::IMU | Sensor::LOG;
    let mut input = sample();
    input.sensors = flags;
    let out = unpack_telemetry(&pack_telemetry(&input)).unwrap();
    assert_eq!(out.sensors, flags);
}

#[test]
fn chg_is_excluded_from_names_all_and_required() {
    // CHG is a live power state (USB present), not a peripheral health flag
    // -- it must not show up as a "missing sensor" mid-flight when USB is
    // (normally, correctly) unplugged.
    assert!(Sensor::NAMES.iter().all(|(bit, _)| *bit != Sensor::CHG));
    assert_eq!(Sensor::ALL & Sensor::CHG, 0);
    assert_eq!(Sensor::REQUIRED & Sensor::CHG, 0);
    assert!(Sensor::flight_ready(Sensor::ALL)); // unaffected by CHG being unset
}

#[test]
fn chg_bit_survives_the_wire_alongside_others() {
    let flags = Sensor::BARO | Sensor::IMU | Sensor::LOG | Sensor::CHG;
    let mut input = sample();
    input.sensors = flags;
    let out = unpack_telemetry(&pack_telemetry(&input)).unwrap();
    assert!(out.sensors & Sensor::CHG != 0);
    assert_eq!(out.sensors, flags);
}

// --- State names -----------------------------------------------------------------

#[test]
fn every_state_has_a_name() {
    for (i, name) in State::NAMES.iter().enumerate() {
        assert_eq!(State::name(i as u8), *name);
    }
}

#[test]
fn unknown_state_does_not_raise() {
    assert_eq!(State::name(99), "UNKNOWN");
    assert_eq!(State::name(255), "UNKNOWN"); // u8 has no negative; 255 stands in for Python's -1
}

#[test]
fn state_values_are_sequential() {
    // The name lookup indexes NAMES directly, so values must match order.
    let ordered = [
        State::BOOT,
        State::IDLE,
        State::ARMED,
        State::BOOST,
        State::COAST,
        State::APOGEE,
        State::DESCENT,
        State::LANDED,
    ];
    let expected: Vec<u8> = (0..State::NAMES.len() as u8).collect();
    assert_eq!(ordered.to_vec(), expected);
}

// --- Commands ----------------------------------------------------------------------

#[test]
fn command_round_trip() {
    for cmd in [Command::PING, Command::CHIRP, Command::ARM, Command::DISARM] {
        assert_eq!(unpack_command(&pack_command(1234, cmd)), Some((1234, cmd)));
    }
}

#[test]
fn command_rejects_corrupted_byte() {
    let mut frame = pack_command(7, Command::CHIRP);
    frame[4] ^= 0xFF;
    assert!(unpack_command(&frame).is_none());
}

#[test]
fn command_checksum_catches_any_single_byte_flip() {
    // Every byte is covered -- a flip anywhere must fail the check.
    for index in 0..7 {
        let mut frame = pack_command(99, Command::ARM);
        frame[index] ^= 0x01;
        assert!(
            unpack_command(&frame).is_none(),
            "index {} not covered",
            index
        );
    }
}

#[test]
fn command_rejects_wrong_length() {
    assert!(unpack_command(&[0u8; 6]).is_none());
    assert!(unpack_command(&[0u8; 8]).is_none());
}

#[test]
fn command_rejects_telemetry_frame() {
    assert!(unpack_command(&pack_telemetry(&sample())).is_none());
}

#[test]
fn command_seq_wraps_cleanly() {
    // Python masks `seq & 0xFFFF` at pack time; here the u16 type makes
    // "wraps at 65536" a static guarantee instead of a runtime mask, so the
    // wrap is exercised via `wrapping_add` rather than passing an
    // out-of-range literal.
    assert_eq!(
        unpack_command(&pack_command(65535, Command::PING))
            .unwrap()
            .0,
        65535
    );
    assert_eq!(
        unpack_command(&pack_command(65535u16.wrapping_add(1), Command::PING))
            .unwrap()
            .0,
        0
    );
}

#[test]
fn command_values_are_distinct() {
    let values = [Command::PING, Command::CHIRP, Command::ARM, Command::DISARM];
    let mut unique = values.to_vec();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(values.len(), unique.len());
}

// --- Cross-cutting -----------------------------------------------------------------

#[test]
fn packet_types_are_distinct_and_nonzero() {
    assert_ne!(PKT_TELEMETRY, PKT_COMMAND);
    assert_ne!(PKT_TELEMETRY, 0);
    assert_ne!(PKT_COMMAND, 0);
}

#[test]
fn magic_is_not_a_degenerate_byte() {
    // 0x00 and 0xFF are what stuck lines produce.
    assert_ne!(MAGIC, 0x00);
    assert_ne!(MAGIC, 0xFF);
}

#[test]
fn sync_word_avoids_lora_defaults() {
    // 0x12 is the private-LoRa default, 0x34 is LoRaWAN. Using either means
    // hearing traffic that isn't ours.
    assert!(![0x12u8, 0x34, 0x00].contains(&SYNC_WORD));
}
