"""
LaunchCast shared packet definitions.

Single source of truth for the wire format. This file is copied verbatim to
both the rocket payload and the handheld ground station. If you change a
format string here, copy it to both boards before flying.

All multi-byte fields are little-endian with no padding ('<' prefix).

Units on the wire are chosen so every field fits its type with margin at the
expected flight envelope (~300 m AGL, D12-5 motor). Decoding restores physical
units.
"""

import struct

# --- Protocol identity -------------------------------------------------------

MAGIC = 0xA5            # Binary 10100101; alternating bits and it's own bit-reverse.
                        # Used to confirm that signal received is meant for us.

SYNC_WORD = 0x2B        # Arbitrary non-default hardware sync word for both RFM95 radios.
                        # Basically a shared secret between the radios. This ends up being
                        # a hardware-level filter applied before the packet even reaches
                        # the code. Prevents interference from any other LoRa signals
                        # using the same magic byte

PKT_TELEMETRY = 0x01    # Telemetry packets; 0000 0001 in binary
PKT_COMMAND = 0x02      # Command packets; 0000 0010 in binary

# TELEMETRY DATA:
# --- Downlink: rocket -> handheld -------------------------------------------
#
#  offset  field        type     units on wire
#  ------  -----------  -------  --------------------------------
#   0      magic        uint8    0xA5
#   1      pkt_type     uint8    0x01
#   2      counter      uint16   packets since boot, wraps
#   4      uptime_ms    uint32   ms since boot
#   8      state        uint8    State.*
#   9      lat          float32  degrees
#  13      lon          float32  degrees
#  17      alt_baro     int16    meters AGL
#  19      speed        int16    cm/s, vertical, + is up
#  21      temp         int16    deci-degrees C
#  23      accel x,y,z  int16*3  milli-g
#  29      gyro x,y,z   int16*3  deci-degrees/s
#  35      batt         uint8    (volts - 3.0) * 100
#  36      gps_flags    uint8    bit0 = fix, bits 1-5 = sat count
#  37      cam_rec      uint8    reserved, send 0
#  38      sensors      uint8    Sensor.* bitfield
#  39      cam_disk     uint8    reserved, send 0
#                                                     total: 40 bytes

TELEMETRY_FMT = "<BBHIBffhhhhhhhhhBBBBB"
TELEMETRY_SIZE = struct.calcsize(TELEMETRY_FMT)

def pack_telemetry(
    counter,
    uptime_ms,
    state,
    lat,
    lon,
    alt_baro_m,
    speed_mps,
    temp_c,
    accel_g,
    gyro_dps,
    batt_volts,
    has_fix,
    satellites,
    cam_rec=0,
    sensors=0,
    cam_disk=0,
):
    """Build a 40-byte telemetry frame from physical units.

    accel_g and gyro_dps are 3-element sequences in g and degrees/sec.
    """
    ax, ay, az = (_clamp_i16(v * 1000) for v in accel_g)
    gx, gy, gz = (_clamp_i16(v * 10) for v in gyro_dps)

    return struct.pack(
        TELEMETRY_FMT,
        MAGIC,
        PKT_TELEMETRY,
        counter & 0xFFFF,
        uptime_ms & 0xFFFFFFFF,
        state,
        lat,
        lon,
        _clamp_i16(alt_baro_m),
        _clamp_i16(speed_mps * 100),
        _clamp_i16(temp_c * 10),
        ax,
        ay,
        az,
        gx,
        gy,
        gz,
        encode_batt(batt_volts),
        encode_gps_flags(has_fix, satellites),
        cam_rec,
        sensors,
        cam_disk,
    )


def unpack_telemetry(data):
    """Return a dict in physical units, or None if the frame isn't ours.

    Rejects on length, magic byte, and packet type. Never raises on bad
    input -- a malformed frame is a routine event on a shared ISM band.
    """
    if len(data) != TELEMETRY_SIZE:
        return None
    if data[0] != MAGIC or data[1] != PKT_TELEMETRY:
        return None

    try:
        fields = struct.unpack(TELEMETRY_FMT, data)
    except (ValueError, struct.error):
        return None

    (
        _magic,
        _ptype,
        counter,
        uptime_ms,
        state,
        lat,
        lon,
        alt_baro,
        speed,
        temp,
        ax,
        ay,
        az,
        gx,
        gy,
        gz,
        batt,
        gps_flags,
        cam_rec,
        sensors,
        cam_disk,
    ) = fields

    has_fix, satellites = decode_gps_flags(gps_flags)

    return {
        "counter": counter,
        "uptime_ms": uptime_ms,
        "state": state,
        "state_name": State.name(state),
        "lat": lat,
        "lon": lon,
        "alt_baro_m": alt_baro,
        "speed_mps": speed / 100.0,
        "temp_c": temp / 10.0,
        "accel_g": (ax / 1000.0, ay / 1000.0, az / 1000.0),
        "gyro_dps": (gx / 10.0, gy / 10.0, gz / 10.0),
        "batt_volts": decode_batt(batt),
        "has_fix": has_fix,
        "satellites": satellites,
        "cam_rec": bool(cam_rec),
        "sensors": sensors,
        "cam_disk": cam_disk,
    }

# COMMAND DATA:
# --- Uplink: handheld -> rocket ---------------------------------------------
#
#  offset  field      type    notes
#  ------  ---------  ------  ----------------------------------
#   0      magic      uint8   0xA5
#   1      pkt_type   uint8   0x02
#   2      seq        uint16  increments; rocket rejects replays
#   4      cmd        uint8   Command.*
#   5      checksum   uint16  sum of bytes 0..4, mod 65536
#                                              total: 7 bytes

COMMAND_FMT = "<BBHBH"
COMMAND_SIZE = struct.calcsize(COMMAND_FMT)

def _checksum(payload):
    return sum(payload) & 0xFFFF


def pack_command(seq, cmd):
    body = struct.pack("<BBHB", MAGIC, PKT_COMMAND, seq & 0xFFFF, cmd)
    return body + struct.pack("<H", _checksum(body))


def unpack_command(data):
    """Return (seq, cmd) or None. Checksum failures return None.

    The rocket should additionally reject any seq it has already seen, to
    keep a repeated or reflected frame from re-triggering a command.
    """
    if len(data) != COMMAND_SIZE:
        return None
    if data[0] != MAGIC or data[1] != PKT_COMMAND:
        return None

    try:
        _magic, _ptype, seq, cmd, checksum = struct.unpack(COMMAND_FMT, data)
    except (ValueError, struct.error):
        return None

    if checksum != _checksum(data[:5]):
        return None

    return seq, cmd

class Command:
    PING = 0x10     # request nothing; presence test
    CHIRP = 0x01    # sound the buzzer for a few seconds
    ARM = 0x02      # IDLE -> ARMED
    DISARM = 0x03   # ARMED -> IDLE


# --- Flight state machine ------------------------------------------------------------

class State:
    BOOT = 0    # Initial state when switched on. Move on when all sensors initialized.
    IDLE = 1    # Alive and waiting to hear from mission control.
                # TODO: Pre-flight state? Maybe run through a secquence of chirps and flashes?
                # Trigging pre-flight checks happen when it first connects to handheld, and
                # the status checks could be a signal returned to the handheld. 
    ARMED = 2   # Handheld sends the "ARM" signal. Payload is logging, waiting for sudden acceleration (boost)
    BOOST = 3   # Sudden acceleration detected -- motor burning
    COAST = 4   # Acceleration no longer detected or below threshold - we're coasting
    APOGEE = 5  # Acceleration + velcity are zero or below threshold - we're at our peak
    DESCENT = 6 # Acceleration/velocity are >0 or above threshold - we're going down!
    LANDED = 7  # Movement has stopped - we've landed.

    NAMES = (
        "BOOT",
        "IDLE",
        "ARMED",
        "BOOST",
        "COAST",
        "APOGEE",
        "DESCENT",
        "LANDED",
    )

    @classmethod
    def name(cls, value):
        if 0 <= value < len(cls.NAMES):
            return cls.NAMES[value]
        return "UNKNOWN({})".format(value)


# --- Sensor health bitfield --------------------------------------------------
# One bit per peripheral, reported in every telemetry frame at zero extra
# airtime cost. Lets the handheld show what actually came up before launch.

class Sensor:
    BARO  = 0x01   # BMP580 pressure/temp
    IMU   = 0x02   # LSM6DSOX accel/gyro
    MAG   = 0x04   # LIS3MDL magnetometer
    GPS   = 0x08   # PA1010D
    LOG   = 0x10   # filesystem writable; flight log is recording
    BATT  = 0x20   # battery ADC readable

    NAMES = (
        (BARO, "BARO"),
        (IMU,  "IMU"),
        (MAG,  "MAG"),
        (GPS,  "GPS"),
        (LOG,  "LOG"),
        (BATT, "BATT"),
    )

    ALL = BARO | IMU | MAG | GPS | LOG | BATT

    # Flight-critical subset. MAG and GPS are nice to have; a missing
    # barometer means no apogee detection and a missing log means no dataset.
    REQUIRED = BARO | IMU | LOG

    @classmethod
    def decode(cls, raw):
        """Return ([present names], [missing names])."""
        present = [n for bit, n in cls.NAMES if raw & bit]
        missing = [n for bit, n in cls.NAMES if not raw & bit]
        return present, missing

    @classmethod
    def flight_ready(cls, raw):
        return (raw & cls.REQUIRED) == cls.REQUIRED

# --- Scaling helpers ---------------------------------------------------------

def encode_batt(volts):
    """3.00-5.55 V into one byte at 10 mV resolution."""
    v = int(round((volts - 3.0) * 100))
    return max(0, min(255, v))


def decode_batt(raw):
    return 3.0 + raw / 100.0


def encode_gps_flags(has_fix, satellites):
    return (1 if has_fix else 0) | (min(31, satellites) << 1)


def decode_gps_flags(raw):
    return bool(raw & 0x01), (raw >> 1) & 0x1F


def _clamp_i16(value):
    return max(-32768, min(32767, int(round(value))))


# --- Self-check --------------------------------------------------------------

if __name__ == "__main__":
    assert TELEMETRY_SIZE == 40, TELEMETRY_SIZE
    assert COMMAND_SIZE == 7, COMMAND_SIZE

    frame = pack_telemetry(
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
    )
    assert len(frame) == 40

    out = unpack_telemetry(frame)
    assert out is not None
    assert out["counter"] == 42
    assert out["state_name"] == "COAST"
    assert abs(out["lat"] - 41.2565) < 1e-4
    assert out["alt_baro_m"] == 287
    assert abs(out["speed_mps"] + 14.2) < 0.01
    assert abs(out["batt_volts"] - 3.94) < 0.01
    assert out["satellites"] == 9

    assert unpack_telemetry(b"\x00" * 40) is None
    assert unpack_telemetry(b"") is None

    cmd_frame = pack_command(7, Command.CHIRP)
    assert len(cmd_frame) == 7
    assert unpack_command(cmd_frame) == (7, Command.CHIRP)

    corrupted = bytearray(cmd_frame)
    corrupted[4] ^= 0xFF
    assert unpack_command(bytes(corrupted)) is None

    flags = Sensor.BARO | Sensor.IMU | Sensor.LOG | Sensor.GPS
    assert Sensor.flight_ready(flags)
    assert not Sensor.flight_ready(Sensor.IMU | Sensor.GPS)
    present, missing = Sensor.decode(flags)
    assert "BARO" in present and "MAG" in missing

    print("packet.py self-check passed")
    print("  telemetry: {} bytes".format(TELEMETRY_SIZE))
    print("  command:   {} bytes".format(COMMAND_SIZE))
