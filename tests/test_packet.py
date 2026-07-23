"""Tests for the shared packet wire format.

packet.py is pure Python with no hardware imports, so it runs unmodified
under CPython. A format-string mistake here breaks both boards silently,
which is exactly the failure worth catching before a flight.
"""

import struct

import pytest

from common import packet
from common.packet import Command, Sensor, State


# --- Sizes are contractual ---------------------------------------------------


def test_telemetry_is_40_bytes():
    assert packet.TELEMETRY_SIZE == 40


def test_command_is_7_bytes():
    assert packet.COMMAND_SIZE == 7


def test_format_strings_are_little_endian():
    # Native alignment would insert padding and silently change the size.
    assert packet.TELEMETRY_FMT.startswith("<")
    assert packet.COMMAND_FMT.startswith("<")


# --- Round trip --------------------------------------------------------------


def _sample(**overrides):
    args = dict(
        counter=42,
        uptime_ms=123456,
        state=State.COAST,
        lat=41.2565,
        lon=-95.9345,
        alt_baro_m=287,
        speed_mps=-14.2,
        temp_c=23.5,
        accel_g=(0.02, -0.01, 0.98),
        gyro_dps=(1.5, -0.3, 12.0),
        batt_volts=3.94,
        has_fix=True,
        satellites=9,
        sensors=Sensor.ALL,
    )
    args.update(overrides)
    return args


def test_telemetry_round_trip():
    out = packet.unpack_telemetry(packet.pack_telemetry(**_sample()))
    assert out is not None
    assert out["counter"] == 42
    assert out["uptime_ms"] == 123456
    assert out["state"] == State.COAST
    assert out["state_name"] == "COAST"
    assert out["alt_baro_m"] == 287
    assert out["satellites"] == 9
    assert out["has_fix"] is True


@pytest.mark.parametrize(
    "field,value,tol",
    [
        ("lat", 41.2565, 1e-4),
        ("lon", -95.9345, 1e-4),
        ("speed_mps", -14.2, 0.01),
        ("temp_c", 23.5, 0.05),
        ("batt_volts", 3.94, 0.01),
    ],
)
def test_scalar_precision(field, value, tol):
    out = packet.unpack_telemetry(packet.pack_telemetry(**_sample()))
    assert abs(out[field] - value) < tol


def test_vector_round_trip():
    out = packet.unpack_telemetry(packet.pack_telemetry(**_sample()))
    for got, want in zip(out["accel_g"], (0.02, -0.01, 0.98)):
        assert abs(got - want) < 0.002
    for got, want in zip(out["gyro_dps"], (1.5, -0.3, 12.0)):
        assert abs(got - want) < 0.1


# --- Rejection ---------------------------------------------------------------


def test_reject_empty():
    assert packet.unpack_telemetry(b"") is None


def test_reject_wrong_length():
    assert packet.unpack_telemetry(b"\x00" * 39) is None
    assert packet.unpack_telemetry(b"\x00" * 41) is None


def test_reject_bad_magic():
    frame = bytearray(packet.pack_telemetry(**_sample()))
    frame[0] = 0x00
    assert packet.unpack_telemetry(bytes(frame)) is None


def test_reject_wrong_packet_type():
    """A command frame padded to 40 bytes must not decode as telemetry."""
    frame = bytearray(packet.pack_telemetry(**_sample()))
    frame[1] = packet.PKT_COMMAND
    assert packet.unpack_telemetry(bytes(frame)) is None


def test_reject_all_zeros():
    assert packet.unpack_telemetry(b"\x00" * 40) is None


def test_reject_all_ones():
    """Stuck-high line. 0xFF != MAGIC, so this must reject."""
    assert packet.unpack_telemetry(b"\xff" * 40) is None


# --- Saturation, not overflow ------------------------------------------------


def test_altitude_clamps_high():
    out = packet.unpack_telemetry(packet.pack_telemetry(**_sample(alt_baro_m=99999)))
    assert out["alt_baro_m"] == 32767


def test_altitude_clamps_low():
    out = packet.unpack_telemetry(packet.pack_telemetry(**_sample(alt_baro_m=-99999)))
    assert out["alt_baro_m"] == -32768


def test_accel_clamps_rather_than_wrapping():
    """A clipped accel must not change sign -- that would look like the
    rocket reversed direction."""
    out = packet.unpack_telemetry(
        packet.pack_telemetry(**_sample(accel_g=(50.0, -50.0, 0.0)))
    )
    assert out["accel_g"][0] > 0
    assert out["accel_g"][1] < 0


# --- Battery encoding --------------------------------------------------------


@pytest.mark.parametrize("volts", [3.00, 3.30, 3.70, 3.80, 4.20, 5.55])
def test_battery_encoding_round_trip(volts):
    assert abs(packet.decode_batt(packet.encode_batt(volts)) - volts) < 0.005


def test_battery_clamps_below_range():
    assert packet.encode_batt(1.0) == 0


def test_battery_clamps_above_range():
    assert packet.encode_batt(9.0) == 255


def test_battery_gate_threshold_is_representable():
    """3.80 V is the no-go gate; it must survive the round trip exactly."""
    assert packet.decode_batt(packet.encode_batt(3.80)) == pytest.approx(3.80)


# --- GPS flags ---------------------------------------------------------------


@pytest.mark.parametrize("sats", [0, 1, 9, 12, 31])
def test_gps_flags_round_trip(sats):
    raw = packet.encode_gps_flags(True, sats)
    fix, got = packet.decode_gps_flags(raw)
    assert fix is True
    assert got == sats


def test_gps_flags_no_fix():
    fix, sats = packet.decode_gps_flags(packet.encode_gps_flags(False, 7))
    assert fix is False
    assert sats == 7


def test_gps_satellites_saturate_at_31():
    """Five bits. More than 31 satellites must clamp, not wrap to a small
    number that looks like a poor fix."""
    _, sats = packet.decode_gps_flags(packet.encode_gps_flags(True, 40))
    assert sats == 31


# --- Sensor bitfield ---------------------------------------------------------


def test_sensor_bits_are_distinct():
    bits = [bit for bit, _ in Sensor.NAMES]
    assert len(bits) == len(set(bits))
    for bit in bits:
        assert bit & (bit - 1) == 0, "not a single bit: {:#x}".format(bit)


def test_sensor_all_covers_every_named_bit():
    for bit, _ in Sensor.NAMES:
        assert Sensor.ALL & bit


def test_flight_ready_requires_baro_imu_log():
    assert Sensor.flight_ready(Sensor.REQUIRED)
    assert Sensor.flight_ready(Sensor.ALL)


@pytest.mark.parametrize("missing", [Sensor.BARO, Sensor.IMU, Sensor.LOG])
def test_flight_ready_fails_without_each_required(missing):
    assert not Sensor.flight_ready(Sensor.ALL & ~missing)


@pytest.mark.parametrize("optional", [Sensor.MAG, Sensor.GPS, Sensor.BATT])
def test_flight_ready_tolerates_optional_loss(optional):
    assert Sensor.flight_ready(Sensor.ALL & ~optional)


def test_sensor_decode_partitions():
    flags = Sensor.BARO | Sensor.IMU | Sensor.LOG
    present, missing = Sensor.decode(flags)
    assert set(present) == {"BARO", "IMU", "LOG"}
    assert set(missing) == {"MAG", "GPS", "BATT"}


def test_sensor_flags_survive_the_wire():
    flags = Sensor.BARO | Sensor.IMU | Sensor.LOG
    out = packet.unpack_telemetry(packet.pack_telemetry(**_sample(sensors=flags)))
    assert out["sensors"] == flags


# --- State names -------------------------------------------------------------


def test_every_state_has_a_name():
    for i in range(len(State.NAMES)):
        assert State.name(i) == State.NAMES[i]


def test_unknown_state_does_not_raise():
    assert "UNKNOWN" in State.name(99)
    assert "UNKNOWN" in State.name(-1)


def test_state_values_are_sequential():
    """The name lookup indexes NAMES directly, so values must match order."""
    ordered = [
        State.BOOT, State.IDLE, State.ARMED, State.BOOST,
        State.COAST, State.APOGEE, State.DESCENT, State.LANDED,
    ]
    assert ordered == list(range(len(State.NAMES)))


# --- Commands ----------------------------------------------------------------


@pytest.mark.parametrize(
    "cmd", [Command.PING, Command.CHIRP, Command.ARM, Command.DISARM]
)
def test_command_round_trip(cmd):
    assert packet.unpack_command(packet.pack_command(1234, cmd)) == (1234, cmd)


def test_command_rejects_corrupted_byte():
    frame = bytearray(packet.pack_command(7, Command.CHIRP))
    frame[4] ^= 0xFF
    assert packet.unpack_command(bytes(frame)) is None


@pytest.mark.parametrize("index", range(7))
def test_command_checksum_catches_any_single_byte_flip(index):
    """Every byte is covered -- a flip anywhere must fail the check."""
    frame = bytearray(packet.pack_command(99, Command.ARM))
    frame[index] ^= 0x01
    assert packet.unpack_command(bytes(frame)) is None


def test_command_rejects_wrong_length():
    assert packet.unpack_command(b"\x00" * 6) is None
    assert packet.unpack_command(b"\x00" * 8) is None


def test_command_rejects_telemetry_frame():
    assert packet.unpack_command(packet.pack_telemetry(**_sample())) is None


def test_command_seq_wraps_cleanly():
    assert packet.unpack_command(packet.pack_command(65535, Command.PING))[0] == 65535
    assert packet.unpack_command(packet.pack_command(65536, Command.PING))[0] == 0


def test_command_values_are_distinct():
    values = [Command.PING, Command.CHIRP, Command.ARM, Command.DISARM]
    assert len(values) == len(set(values))


# --- Cross-cutting -----------------------------------------------------------


def test_packet_types_are_distinct_and_nonzero():
    assert packet.PKT_TELEMETRY != packet.PKT_COMMAND
    assert packet.PKT_TELEMETRY != 0
    assert packet.PKT_COMMAND != 0


def test_magic_is_not_a_degenerate_byte():
    """0x00 and 0xFF are what stuck lines produce."""
    assert packet.MAGIC not in (0x00, 0xFF)


def test_sync_word_avoids_lora_defaults():
    """0x12 is the private-LoRa default, 0x34 is LoRaWAN. Using either
    means hearing traffic that isn't ours."""
    assert packet.SYNC_WORD not in (0x12, 0x34, 0x00)


def test_telemetry_fmt_matches_documented_field_count():
    # 21 fields: magic, type, counter, uptime, state, lat, lon, alt, speed,
    # temp, 3 accel, 3 gyro, batt, gps_flags, cam_rec, sensors, cam_disk
    assert len(struct.unpack(packet.TELEMETRY_FMT, b"\x00" * 40)) == 21