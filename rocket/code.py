"""
LaunchCast flight firmware -- rocket payload.

Runs on an Adafruit Feather RP2040 RFM95 (PID 5714) with:
  BMP580 barometer, LSM6DSOX+LIS3MDL IMU, PA1010D GPS  (I2C via STEMMA QT)
  PS1240 piezo across D5/D6 (differential drive)
  onboard NeoPixel as status indicator
  500 mAh LiPo through a slide switch to BAT

Design contract:
  - The downlink is a MONITOR. The flash log is the DATASET. Radio packets
    drop; flash does not. Never let a radio failure stall the log loop.
  - Loop order is: sense -> log -> state -> radio. Sensing and logging are
    never skipped. Radio work is best-effort and time-boxed.
  - Apogee is detected from BAROMETRIC VELOCITY, not acceleration.
    Acceleration is ~1 g through the entire coast/descent transition.

Copy packet.py to the board alongside this file.
"""

import time
import math
import struct
import board
import busio
import digitalio
import pwmio
import analogio
import microcontroller

import packet
from packet import State, Command, Sensor

# --- Tuning ------------------------------------------------------------------

LOG_PATH = "/flight.bin"    # Binary log format; CSV takes more overhead
IMU_HZ = 100                # Acclerometer/gyro: target sample rate
BARO_HZ = 25                # Pressure/temp: target sample rate
GPS_HZ = 1                  # PA1010D (GPS) default NMEA rate
TX_HZ_IDLE = 0.5            # Broadcast rate when not in flight
TX_HZ_FLIGHT = 2.0          # Broadcast rate when in flight
BOOST_THRESHOLD_G = 3.0     # Any acceleration > 3.0g means "liftoff!"
BOOST_MIN_MS = 150          # Acceleration must persist this long to avoid accidental bumps
COAST_THRESHOLD_G = 1.5     # Below 1.5g, we're done boosting and now coasting
APOGEE_VEL_MPS = 1.0        # *Vertical* velocity below this at apex
DESCENT_VEL_MPS = -2.0      # Sustained negative velocity -> under chute
LANDED_VEL_MPS = 0.5        # Landing detection: motion near-zero velocity...
LANDED_ALT_M = 15.0         # ...near ground level
LANDED_HOLD_MS = 3000       # ...for this long, means we've landed.

BUZZER_HZ = 4000            # Piezo max-vol freq. TODO: re-tune per part after a bench sweep
CHIRP_MS = 1000             # Duration of a commanded chirp

BATT_DIVIDER = 2.0          # Onboard voltage divider ratio; 1.0 if reading BAT directly
BATT_SAMPLES = 8            # Number of samples to average for estimating battery life

VEL_ALPHA = 0.3             # Vertical velocity is unreliable via GPS at high speed, short
                            # flights, so instead we derive it from barometric pressure.
                            # The BMP580 gives us this, but the readings a *noisy*, and
                            # measuring in 10^-1 Hz-scale increments amplifies this noise.
                            # So we use first-order IIR low-pass filtering, an exponentially
                            # weighted moving average: y = α·x + (1−α)·y_prev. Our `VEL_ALPHA`
                            # is the α value. α = 1 means no filtering. α → 0 means the value
                            # never changes. α = 0.3 behaves roughly like averaging the
                            # last 3–4 readings.

# --- Hardware ----------------------------------------------------------------


class Hardware:
    """Owns every peripheral. Any sensor may be None if it failed to init."""

    def __init__(self):
        self.i2c = None
        self.baro = None
        self.imu = None
        self.mag = None
        self.gps = None
        self.radio = None
        self.pixel = None
        self.buzz_hi = None
        self.buzz_lo = None
        self.vbat = None
        self.errors = []

    def init_all(self):
        self._init_pixel()
        self._init_buzzer()
        self._init_battery()
        self._init_i2c()
        self._init_baro()
        self._init_imu()
        self._init_gps()
        self._init_radio()
        return len(self.errors) == 0

    # -- individual peripherals, each failing independently -------------------

    def _init_pixel(self):
        try:
            import neopixel

            self.pixel = neopixel.NeoPixel(board.NEOPIXEL, 1, brightness=0.2)
            self.pixel[0] = (0, 0, 32)
        except Exception as e:
            self.errors.append("pixel: {}".format(e))

    def _init_buzzer(self):
        try:
            self.buzz_hi = pwmio.PWMOut(
                board.D5, frequency=BUZZER_HZ, duty_cycle=0, variable_frequency=True
            )
            self.buzz_lo = pwmio.PWMOut(
                board.D6, frequency=BUZZER_HZ, duty_cycle=0, variable_frequency=True
            )
        except Exception as e:
            self.errors.append("buzzer: {}".format(e))

    def _init_battery(self):
        try:
            pin = getattr(board, "VOLTAGE_MONITOR", None) or board.A0
            self.vbat = analogio.AnalogIn(pin)
        except Exception as e:
            self.errors.append("vbat: {}".format(e))

    def _init_i2c(self):
        try:
            self.i2c = board.STEMMA_I2C()
        except Exception:
            try:
                self.i2c = busio.I2C(board.SCL, board.SDA)
            except Exception as e:
                self.errors.append("i2c: {}".format(e))

    def _init_baro(self):
        if not self.i2c:
            return
        try:
            import adafruit_bmp5xx

            self.baro = adafruit_bmp5xx.BMP5XX_I2C(self.i2c)
        except Exception as e:
            self.errors.append("baro: {}".format(e))

    def _init_imu(self):
        if not self.i2c:
            return
        try:
            from adafruit_lsm6ds.lsm6dsox import LSM6DSOX

            self.imu = LSM6DSOX(self.i2c)
        except Exception as e:
            self.errors.append("imu: {}".format(e))
        try:
            import adafruit_lis3mdl

            self.mag = adafruit_lis3mdl.LIS3MDL(self.i2c)
        except Exception as e:
            self.errors.append("mag: {}".format(e))

    def _init_gps(self):
        if not self.i2c:
            return
        try:
            import adafruit_gps

            self.gps = adafruit_gps.GPS_GtopI2C(self.i2c, debug=False)
            self.gps.send_command(b"PMTK314,0,1,0,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0")
            self.gps.send_command(b"PMTK220,1000")
        except Exception as e:
            self.errors.append("gps: {}".format(e))

    def _init_radio(self):
        try:
            import adafruit_rfm9x

            spi = board.SPI()
            cs = digitalio.DigitalInOut(board.RFM_CS)
            rst = digitalio.DigitalInOut(board.RFM_RST)
            self.radio = adafruit_rfm9x.RFM9x(spi, cs, rst, 915.0)
            self.radio.tx_power = 20
            self.radio.spreading_factor = 7
            self.radio.signal_bandwidth = 125000
            self.radio.coding_rate = 5
            self.radio.enable_crc = True
            try:
                self.radio.sync_word = packet.SYNC_WORD
            except Exception:
                pass  # some library versions expose this differently
        except Exception as e:
            self.errors.append("radio: {}".format(e))

    # -- helpers --------------------------------------------------------------

    def battery_volts(self):
        if not self.vbat:
            return 0.0
        total = 0
        for _ in range(BATT_SAMPLES):
            total += self.vbat.value
        avg = total / BATT_SAMPLES
        return (avg / 65535.0) * 3.3 * BATT_DIVIDER

    def buzz_on(self, freq=BUZZER_HZ):
        """Differential drive: two pins, opposite phase, ~6.6 Vp-p."""
        if not (self.buzz_hi and self.buzz_lo):
            return
        self.buzz_hi.frequency = freq
        self.buzz_lo.frequency = freq
        self.buzz_hi.duty_cycle = 32768
        self.buzz_lo.duty_cycle = 32768  # see note in loop(): phase is approximate

    def buzz_off(self):
        if self.buzz_hi:
            self.buzz_hi.duty_cycle = 0
        if self.buzz_lo:
            self.buzz_lo.duty_cycle = 0

    def set_pixel(self, rgb):
        if self.pixel:
            try:
                self.pixel[0] = rgb
            except Exception:
                pass

    def sensor_flags(self, log_ok=False):
        """Bitfield of peripherals that came up. Called once after init and
        again whenever something is known to have failed mid-flight."""
        from packet import Sensor
        flags = 0
        if self.baro:
            flags |= Sensor.BARO
        if self.imu:
            flags |= Sensor.IMU
        if self.mag:
            flags |= Sensor.MAG
        if self.gps:
            flags |= Sensor.GPS
        if self.vbat:
            flags |= Sensor.BATT
        if log_ok:
            flags |= Sensor.LOG
        return flags

# --- Flight state machine ----------------------------------------------------
# NOTE: NeoPixels on Feathers are often GRB order, not RGB. If colors come out wrong,
# that's why. Set `neopixel.NeoPixel(..., pixel_order=neopixel.GRB)`
PIXEL_FOR_STATE = {
    State.BOOT: (0, 0, 32),     # Dim blue
    State.IDLE: (16, 16, 0),    # Dim yellow
    State.ARMED: (0, 32, 0),    # Green
    State.BOOST: (48, 16, 0),   # Orange
    State.COAST: (0, 16, 32),   # Cyan
    State.APOGEE: (32, 0, 32),  # Magenta
    State.DESCENT: (0, 24, 24), # Teal
    State.LANDED: (48, 0, 0),   # Red (brightest)
}


class FlightState:
    """Owns state transitions and the barometric velocity estimate.

    Altitude is measured above ground level (AGL), referenced to a ground datum captured at ARM time.
    """

    def __init__(self):
        self.state = State.BOOT
        self.ground_pressure = None
        self.alt_m = 0.0
        self.vel_mps = 0.0
        self.max_alt_m = 0.0
        self._last_alt = None
        self._last_t = None
        self._boost_start = None
        self._landed_start = None
        self.entered_ms = 0

    def set_ground_reference(self, pressure_hpa):
        self.ground_pressure = pressure_hpa

    def _pressure_to_alt(self, pressure_hpa):
        """Standard barometric formula, referenced to the ground datum."""
        if not self.ground_pressure or pressure_hpa <= 0:
            return 0.0
        ratio = pressure_hpa / self.ground_pressure
        return 44330.0 * (1.0 - math.pow(ratio, 0.1903))

    def update_altitude(self, pressure_hpa, now_ms):
        self.alt_m = self._pressure_to_alt(pressure_hpa)
        if self.alt_m > self.max_alt_m:
            self.max_alt_m = self.alt_m

        if self._last_alt is not None and self._last_t is not None:
            dt = (now_ms - self._last_t) / 1000.0
            if dt > 0.001:
                raw = (self.alt_m - self._last_alt) / dt
                self.vel_mps += VEL_ALPHA * (raw - self.vel_mps)

        self._last_alt = self.alt_m
        self._last_t = now_ms

    def transition(self, new_state, now_ms):
        if new_state != self.state:
            self.state = new_state
            self.entered_ms = now_ms
            return True
        return False

    def update(self, accel_mag_g, now_ms):
        """Advance the state machine. Returns True if the state changed.

        BOOT and IDLE and ARMED transitions are driven externally (sensor
        init, uplink commands). Everything from BOOST onward is autonomous
        and one-way -- there is no path back to ARMED in flight.
        """
        s = self.state

        if s == State.ARMED:
            if accel_mag_g >= BOOST_THRESHOLD_G:
                if self._boost_start is None:
                    self._boost_start = now_ms
                elif now_ms - self._boost_start >= BOOST_MIN_MS:
                    return self.transition(State.BOOST, now_ms)
            else:
                self._boost_start = None

        elif s == State.BOOST:
            if accel_mag_g < COAST_THRESHOLD_G:
                return self.transition(State.COAST, now_ms)

        elif s == State.COAST:
            # Apogee is a VELOCITY event, not an acceleration event.
            if abs(self.vel_mps) <= APOGEE_VEL_MPS or self.vel_mps < 0:
                return self.transition(State.APOGEE, now_ms)

        elif s == State.APOGEE:
            if self.vel_mps <= DESCENT_VEL_MPS:
                return self.transition(State.DESCENT, now_ms)

        elif s == State.DESCENT:
            settled = abs(self.vel_mps) <= LANDED_VEL_MPS and self.alt_m <= LANDED_ALT_M
            if settled:
                if self._landed_start is None:
                    self._landed_start = now_ms
                elif now_ms - self._landed_start >= LANDED_HOLD_MS:
                    return self.transition(State.LANDED, now_ms)
            else:
                self._landed_start = None

        return False


# --- Flight log --------------------------------------------------------------

# t_ms, state, alt, vel, pressure, temp, ax, ay, az, gx, gy, gz
LOG_FMT = "<IBffffffffff"
LOG_SIZE = struct.calcsize(LOG_FMT)  # 45 bytes


class FlightLog:
    """Append-only binary log. Buffers in RAM, flushes on a size threshold.

    If the filesystem is read-only (USB connected without boot.py remount),
    every method degrades to a no-op and the flight continues.
    """

    def __init__(self, path=LOG_PATH, flush_bytes=1024):
        self.path = path
        self.flush_bytes = flush_bytes
        self.buf = bytearray()
        self.enabled = False
        self.records = 0
        self.dropped = 0
        try:
            with open(self.path, "ab") as f:
                f.write(b"")
            self.enabled = True
        except (OSError, RuntimeError):
            self.enabled = False

    def write(self, t_ms, state, alt, vel, pressure, temp, accel, gyro):
        if not self.enabled:
            self.dropped += 1
            return
        try:
            self.buf += struct.pack(
                LOG_FMT,
                t_ms & 0xFFFFFFFF,
                state,
                alt,
                vel,
                pressure,
                temp,
                accel[0],
                accel[1],
                accel[2],
                gyro[0],
                gyro[1],
                gyro[2],
            )
            self.records += 1
            if len(self.buf) >= self.flush_bytes:
                self.flush()
        except Exception:
            self.dropped += 1

    def flush(self):
        if not self.enabled or not self.buf:
            return
        try:
            with open(self.path, "ab") as f:
                f.write(self.buf)
            self.buf = bytearray()
        except Exception:
            self.enabled = False


# --- Main --------------------------------------------------------------------


def ms():
    return time.monotonic_ns() // 1_000_000


def accel_magnitude(a):
    return math.sqrt(a[0] * a[0] + a[1] * a[1] + a[2] * a[2]) / 9.80665


def main():
    hw = Hardware()
    hw.init_all()

    fs = FlightState()
    log = FlightLog()
    sensors = hw.sensor_flags(log_ok=log.enabled)
    present, missing = Sensor.decode(sensors)
    print("sensors up:", " ".join(present) or "NONE")
    if missing:
        print("sensors MISSING:", " ".join(missing))
    if not Sensor.flight_ready(sensors):
        print("*** NOT FLIGHT READY -- barometer, IMU, and log are required")

    # Everything below runs even if some sensors failed. A missing GPS
    # should not prevent a flight; a missing barometer degrades the state
    # machine to accel-only and is worth knowing about on the ground.
    for err in hw.errors:
        print("INIT FAIL:", err)

    if hw.baro is None:
        print("WARNING: no barometer -- altitude and apogee unavailable")

    # Sensor settling. The BMP580 needs a few reads before it is trustworthy.
    t0 = ms()
    while ms() - t0 < 2000:
        if hw.baro:
            try:
                _ = hw.baro.pressure
            except Exception:
                pass
        time.sleep(0.05)

    fs.transition(State.IDLE, ms())
    hw.set_pixel(PIXEL_FOR_STATE[State.IDLE])
    print("IDLE -- waiting for ARM from handheld")

    # Cadence tracking
    next_imu = 0
    next_baro = 0
    next_gps = 0
    next_tx = 0
    next_rx = 0

    counter = 0
    last_seq = None
    chirp_until = 0

    accel = (0.0, 0.0, 0.0)
    gyro = (0.0, 0.0, 0.0)
    pressure = 0.0
    temp_c = 0.0
    accel_g = 0.0
    lat = 0.0
    lon = 0.0
    has_fix = False
    sats = 0
    batt = 0.0
    batt_checked = 0

    imu_period = 1000 // IMU_HZ
    baro_period = 1000 // BARO_HZ
    gps_period = 1000 // GPS_HZ

    while True:
        now = ms()

        # -- sense: IMU (highest rate) ---------------------------------------
        if now >= next_imu:
            next_imu = now + imu_period
            if hw.imu:
                try:
                    accel = hw.imu.acceleration  # m/s^2
                    gyro = hw.imu.gyro  # rad/s
                    accel_g = accel_magnitude(accel)
                except Exception:
                    pass

        # -- sense: barometer -------------------------------------------------
        if now >= next_baro:
            next_baro = now + baro_period
            if hw.baro:
                try:
                    pressure = hw.baro.pressure
                    temp_c = hw.baro.temperature
                    fs.update_altitude(pressure, now)
                except Exception:
                    pass

        # -- log: every IMU sample, regardless of anything else ---------------
        log.write(now, fs.state, fs.alt_m, fs.vel_mps, pressure, temp_c, accel, gyro)

        # -- state machine ----------------------------------------------------
        if fs.update(accel_g, now):
            hw.set_pixel(PIXEL_FOR_STATE.get(fs.state, (16, 16, 16)))
            print("-> {}  alt={:.1f}m vel={:.1f}m/s".format(
                State.name(fs.state), fs.alt_m, fs.vel_mps))
            if fs.state == State.LANDED:
                log.flush()

        # -- sense: GPS (slow, and never allowed to block) --------------------
        if now >= next_gps:
            next_gps = now + gps_period
            if hw.gps:
                try:
                    hw.gps.update()
                    if hw.gps.has_fix:
                        has_fix = True
                        lat = hw.gps.latitude or 0.0
                        lon = hw.gps.longitude or 0.0
                        sats = hw.gps.satellites or 0
                    else:
                        has_fix = False
                except Exception:
                    pass

        # -- battery: slow, and only when not in powered flight ---------------
        if now - batt_checked > 5000 and fs.state not in (State.BOOST, State.COAST):
            batt_checked = now
            batt = hw.battery_volts()

        # -- radio: receive commands -----------------------------------------
        if hw.radio and now >= next_rx:
            next_rx = now + 100
            try:
                pkt = hw.radio.receive(timeout=0.0)
            except Exception:
                pkt = None
            if pkt:
                parsed = packet.unpack_command(bytes(pkt))
                if parsed:
                    seq, cmd = parsed
                    if seq != last_seq:  # reject replays
                        last_seq = seq
                        if cmd == Command.ARM and fs.state == State.IDLE:
                            if not Sensor.flight_ready(sensors):
                                print("ARM REFUSED -- not flight ready")
                                # three fast chirps = refusal
                                chirp_until = now + 600
                            else:
                                fs.set_ground_reference(pressure)
                                fs.transition(State.ARMED, now)
                                hw.set_pixel(PIXEL_FOR_STATE[State.ARMED])
                                print("ARMED  ground_p={:.2f}".format(pressure))
                        elif cmd == Command.DISARM and fs.state == State.ARMED:
                            fs.transition(State.IDLE, now)
                            hw.set_pixel(PIXEL_FOR_STATE[State.IDLE])
                            print("DISARMED")
                        elif cmd == Command.CHIRP:
                            chirp_until = now + CHIRP_MS

        # -- buzzer -----------------------------------------------------------
        if fs.state == State.LANDED:
            # Pulsed, not steady: far easier to localize by ear.
            if (now // 500) % 2 == 0:
                hw.buzz_on()
            else:
                hw.buzz_off()
        elif now < chirp_until:
            hw.buzz_on()
        else:
            hw.buzz_off()

        # -- radio: transmit telemetry ---------------------------------------
        if hw.radio and now >= next_tx:
            in_flight = fs.state in (
                State.ARMED,
                State.BOOST,
                State.COAST,
                State.APOGEE,
                State.DESCENT,
            )
            rate = TX_HZ_FLIGHT if in_flight else TX_HZ_IDLE
            next_tx = now + int(1000 / rate)

            frame = packet.pack_telemetry(
                counter=counter,
                uptime_ms=now,
                state=fs.state,
                lat=lat,
                lon=lon,
                alt_baro_m=fs.alt_m,
                speed_mps=fs.vel_mps,
                temp_c=temp_c,
                accel_g=(a / 9.80665 for a in accel),
                gyro_dps=(g * 57.2958 for g in gyro),
                batt_volts=batt,
                has_fix=has_fix,
                satellites=sats,
                sensors=sensors,
            )
            counter += 1
            try:
                hw.radio.send(frame)
            except Exception:
                pass  # a failed send must never stall the log loop


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        # Last resort: log the traceback where it survives a power cycle,
        # then reset rather than sitting dead on the pad.
        try:
            with open("/crash.txt", "a") as f:
                f.write("{}\n".format(e))
        except Exception:
            pass
        time.sleep(2)
        microcontroller.reset()
