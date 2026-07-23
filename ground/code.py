"""
LaunchCast ground station firmware -- handheld receiver.

Runs on an Adafruit Feather RP2040 RFM95 (PID 5714) with:
  Sharp Memory Display 2.7" 400x240 (PID 4694) over SPI
  PA1010D GPS (I2C via STEMMA QT)
  Three momentary buttons on D9/D10/D11, active-low with internal pull-ups
  2500 mAh LiPo through a slide switch to BAT

Design contract:
  - The display refreshes on a TIMER, not on packet arrival. This satisfies
    the Sharp panel's VCOM polarity-inversion requirement and, more usefully,
    distinguishes "no new data" from "firmware crashed."
  - The last valid GPS fix from the rocket is LATCHED. When the final packets
    drop out behind terrain, you walk toward the last known position. This is
    the single most valuable feature in the file.
  - Staleness is displayed explicitly. A frozen number with no age is a lie.

Copy packet.py to the board alongside this file.
"""

import time
import math
import board
import busio
import digitalio

import packet
from packet import State, Command, Sensor

# --- Tuning ------------------------------------------------------------------
GPS_HZ = 1              # Local GPS refresh rate

LINK_STALE_MS = 3000    # No packet for this long -> show as stale
LINK_LOST_MS = 15000    # No packet for this long -> show as LOST

HOLD_MS = 2000          # ARM/DISARM requires a deliberate hold
DEBOUNCE_MS = 50        # MNinimum time that must pass between accepted 
                        # state changes on a mechanical button. Needed because
                        # firmware loop runs so fast that without debouncing, a
                        # single physical press registers as five or six
                        # separate press/release events.

EARTH_R_M = 6371000.0   # Radius of the Earth in m. We probably won't need to change this.

# --- Display --------------------------------------------------------

DISPLAY_HZ = 2.0    # Display refresh rate; also services VCOM
DISP_W = 400        # Pixels wide
DISP_H = 240        # Pixels tall

# --- Hardware ----------------------------------------------------------------

class Hardware:
    def __init__(self):
        self.i2c = None
        self.gps = None
        self.radio = None
        self.display = None
        self.buttons = {}
        self.errors = []

    def init_all(self):
        self._init_i2c()
        self._init_gps()
        self._init_radio()
        self._init_display()
        self._init_buttons()
        return len(self.errors) == 0

    def _init_i2c(self):
        try:
            self.i2c = board.STEMMA_I2C()
        except Exception:
            try:
                self.i2c = busio.I2C(board.SCL, board.SDA)
            except Exception as e:
                self.errors.append("i2c: {}".format(e))

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
                pass
        except Exception as e:
            self.errors.append("radio: {}".format(e))

    def _init_display(self):
        try:
            import adafruit_sharpmemorydisplay

            spi = board.SPI()
            cs = digitalio.DigitalInOut(board.D12)  # verify against wiring
            self.display = adafruit_sharpmemorydisplay.SharpMemoryDisplay(
                spi, cs, DISP_W, DISP_H
            )
            self.display.fill(1)  # 1 = light on this panel
            self.display.show()
        except Exception as e:
            self.errors.append("display: {}".format(e))

    def _init_buttons(self):
        pins = {"chirp": board.D9, "arm": board.D10, "menu": board.D11}
        for name, pin in pins.items():
            try:
                btn = digitalio.DigitalInOut(pin)
                btn.direction = digitalio.Direction.INPUT
                btn.pull = digitalio.Pull.UP
                self.buttons[name] = btn
            except Exception as e:
                self.errors.append("btn {}: {}".format(name, e))


# --- Buttons -----------------------------------------------------------------


class Button:
    """Active-low button with debounce and hold detection.

    A tap fires on RELEASE, so a hold does not also register as a tap.

    - A press longer than `DEBOUNCE_MS` is a "tap". 
    - A press longer than `HOLD_MS` is a "hold".
    """

    def __init__(self, pin_obj):
        self.pin = pin_obj
        self.pressed = False
        self.since = 0
        self.hold_fired = False
        self._last_change = 0

    def update(self, now):
        """Return 'tap', 'hold', or None."""
        if self.pin is None:
            return None
        down = not self.pin.value  # active low

        if down != self.pressed:
            if now - self._last_change < DEBOUNCE_MS:
                return None
            self._last_change = now
            self.pressed = down
            if down:
                self.since = now
                self.hold_fired = False
            else:
                if not self.hold_fired:
                    return "tap"
            return None

        if down and not self.hold_fired and now - self.since >= HOLD_MS:
            self.hold_fired = True
            return "hold"

        return None


# --- Navigation --------------------------------------------------------------


def haversine_m(lat1, lon1, lat2, lon2):
    """Great-circle distance in meters."""
    p1 = math.radians(lat1)
    p2 = math.radians(lat2)
    dp = math.radians(lat2 - lat1)
    dl = math.radians(lon2 - lon1)
    a = math.sin(dp / 2) ** 2 + math.cos(p1) * math.cos(p2) * math.sin(dl / 2) ** 2
    return 2 * EARTH_R_M * math.atan2(math.sqrt(a), math.sqrt(1 - a))


def bearing_deg(lat1, lon1, lat2, lon2):
    """Initial great-circle bearing, degrees true, 0-360."""
    p1 = math.radians(lat1)
    p2 = math.radians(lat2)
    dl = math.radians(lon2 - lon1)
    y = math.sin(dl) * math.cos(p2)
    x = math.cos(p1) * math.sin(p2) - math.sin(p1) * math.cos(p2) * math.cos(dl)
    return (math.degrees(math.atan2(y, x)) + 360.0) % 360.0


def compass_point(deg):
    pts = ("N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE",
           "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW")
    return pts[int((deg + 11.25) % 360 / 22.5)]


def relative_arrow(bearing, heading):
    """Turn instruction relative to the direction you are walking.

    Only meaningful when moving -- GPS course over ground is undefined at
    a standstill. Returns None if heading is unavailable.
    """
    if heading is None:
        return None
    rel = (bearing - heading + 360.0) % 360.0
    if rel < 22.5 or rel >= 337.5:
        return "^ AHEAD"
    if rel < 67.5:
        return "> 45 RIGHT"
    if rel < 112.5:
        return ">> RIGHT"
    if rel < 157.5:
        return ">> BACK RIGHT"
    if rel < 202.5:
        return "v TURN AROUND"
    if rel < 247.5:
        return "<< BACK LEFT"
    if rel < 292.5:
        return "<< LEFT"
    return "< 45 LEFT"


# --- Link state --------------------------------------------------------------


class Link:
    """Everything known about the rocket, plus how stale it is.

    The last valid fix is latched separately from the live telemetry so it
    survives the rocket going silent.
    """

    def __init__(self):
        self.tel = None  # last decoded telemetry dict
        self.last_rx_ms = 0
        self.rssi = None
        self.snr = None
        self.packets = 0
        self.rejects = 0

        # Latched -- never cleared by packet loss
        self.fix_lat = None
        self.fix_lon = None
        self.fix_alt = None
        self.fix_age_ms = 0

        self.max_alt = 0.0
        self.max_vel = 0.0

    def ingest(self, data, now, rssi=None, snr=None):
        tel = packet.unpack_telemetry(data)
        if tel is None:
            self.rejects += 1
            return False

        self.tel = tel
        self.last_rx_ms = now
        self.rssi = rssi
        self.snr = snr
        self.packets += 1

        if tel["has_fix"] and tel["lat"] != 0.0:
            self.fix_lat = tel["lat"]
            self.fix_lon = tel["lon"]
            self.fix_alt = tel["alt_baro_m"]
            self.fix_age_ms = now

        if tel["alt_baro_m"] > self.max_alt:
            self.max_alt = tel["alt_baro_m"]
        if abs(tel["speed_mps"]) > abs(self.max_vel):
            self.max_vel = tel["speed_mps"]

        return True

    def age_ms(self, now):
        if self.last_rx_ms == 0:
            return None
        return now - self.last_rx_ms

    def status(self, now):
        age = self.age_ms(now)
        if age is None:
            return "WAITING"
        if age > LINK_LOST_MS:
            return "LOST"
        if age > LINK_STALE_MS:
            return "STALE"
        return "LIVE"


# --- Screens -----------------------------------------------------------------

SCREEN_FLIGHT = 0
SCREEN_RECOVERY = 1
SCREEN_DIAG = 2
SCREEN_COUNT = 3


def draw(display, link, my_lat, my_lon, my_heading, screen, now, tx_status):
    """Render one frame. Sharp Memory: 0 = dark pixel, 1 = light."""
    if display is None:
        return

    display.fill(1)

    def text(x, y, s, size=1):
        try:
            display.text(s, x, y, 0, size=size)
        except TypeError:
            display.text(s, x, y, 0)

    status = link.status(now)
    age = link.age_ms(now)
    tel = link.tel

    # -- header, on every screen ---------------------------------------------
    text(4, 4, "LAUNCHCAST", 2)
    text(250, 4, status, 2)
    if age is not None:
        text(250, 26, "{:.1f}s ago".format(age / 1000.0))
    text(4, 26, ("FLIGHT", "RECOVERY", "DIAG")[screen])

    if tel is None:
        text(4, 90, "NO TELEMETRY", 3)
        text(4, 130, "rejects: {}".format(link.rejects))
        display.show()
        return

    if screen == SCREEN_FLIGHT:
        text(4, 52, tel["state_name"], 3)
        text(4, 92, "ALT  {:>7.1f} m".format(tel["alt_baro_m"]), 2)
        text(4, 116, "VEL  {:>7.1f} m/s".format(tel["speed_mps"]), 2)
        text(4, 140, "MAX  {:>7.1f} m".format(link.max_alt), 2)
        text(4, 168, "BATT {:.2f}V".format(tel["batt_volts"]))
        text(120, 168, "SAT {}".format(tel["satellites"]))
        text(200, 168, "PKT {}".format(link.packets))
        if link.rssi is not None:
            text(280, 168, "RSSI {}".format(link.rssi))

        ok = Sensor.flight_ready(tel["sensors"])
        _, missing = Sensor.decode(tel["sensors"])
        if ok:
            text(4, 190, "SENSORS OK")
        else:
            text(4, 190, "NOT READY: {}".format(" ".join(missing)))

        if tel["batt_volts"] < 3.80:
            text(4, 210, "*** BATTERY LOW -- NO GO ***", 2)

    elif screen == SCREEN_RECOVERY:
        if link.fix_lat is None:
            text(4, 90, "NO FIX LATCHED", 2)
            text(4, 120, "walk toward last seen bearing")
        elif my_lat is None:
            text(4, 60, "ROCKET", 2)
            text(4, 84, "{:.6f}".format(link.fix_lat), 2)
            text(4, 108, "{:.6f}".format(link.fix_lon), 2)
            text(4, 140, "waiting for own GPS fix")
        else:
            d = haversine_m(my_lat, my_lon, link.fix_lat, link.fix_lon)
            b = bearing_deg(my_lat, my_lon, link.fix_lat, link.fix_lon)
            text(4, 52, "{:.0f} m".format(d), 3)
            text(180, 52, "{:.0f} {}".format(b, compass_point(b)), 3)

            arrow = relative_arrow(b, my_heading)
            if arrow:
                text(4, 100, arrow, 3)
            else:
                text(4, 100, "walk to get heading", 2)

            text(4, 150, "rocket {:.6f}".format(link.fix_lat))
            text(4, 166, "       {:.6f}".format(link.fix_lon))
            fix_age = (now - link.fix_age_ms) / 1000.0
            text(4, 190, "fix age {:.0f}s".format(fix_age))
            if status != "LIVE":
                text(200, 190, "LATCHED -- rocket silent")

    else:  # SCREEN_DIAG
        text(4, 52, "pkts {}  rej {}".format(link.packets, link.rejects))
        text(4, 72, "rssi {}  snr {}".format(link.rssi, link.snr))
        text(4, 92, "state {}".format(tel["state_name"]))
        text(4, 112, "uptime {:.1f}s".format(tel["uptime_ms"] / 1000.0))
        text(4, 132, "counter {}".format(tel["counter"]))
        present, missing = Sensor.decode(tel["sensors"])
        text(4, 152, "up: {}".format(" ".join(present)))
        text(4, 168, "down: {}".format(" ".join(missing) or "none"))
        text(4, 192, "accel {:.2f} {:.2f} {:.2f}".format(*tel["accel_g"]))
        text(4, 208, tx_status)

    display.show()


# --- Main --------------------------------------------------------------------


def ms():
    return time.monotonic_ns() // 1_000_000


def main():
    hw = Hardware()
    hw.init_all()
    for err in hw.errors:
        print("INIT FAIL:", err)

    link = Link()
    buttons = {name: Button(obj) for name, obj in hw.buttons.items()}

    screen = SCREEN_FLIGHT
    seq = 0
    tx_status = "ready"

    my_lat = None
    my_lon = None
    my_heading = None

    next_draw = 0
    next_gps = 0
    draw_period = int(1000 / DISPLAY_HZ)
    gps_period = int(1000 / GPS_HZ)

    print("ground station up -- listening")

    while True:
        now = ms()

        # -- radio receive ----------------------------------------------------
        if hw.radio:
            try:
                data = hw.radio.receive(timeout=0.05)
            except Exception:
                data = None
            if data:
                rssi = None
                snr = None
                try:
                    rssi = hw.radio.last_rssi
                    snr = hw.radio.last_snr
                except Exception:
                    pass
                link.ingest(bytes(data), now, rssi, snr)

        # -- own GPS ----------------------------------------------------------
        if now >= next_gps:
            next_gps = now + gps_period
            if hw.gps:
                try:
                    hw.gps.update()
                    if hw.gps.has_fix:
                        my_lat = hw.gps.latitude
                        my_lon = hw.gps.longitude
                        # Course over ground substitutes for a compass and
                        # needs no calibration. Only valid while moving.
                        spd = hw.gps.speed_knots
                        if spd is not None and spd > 1.0:
                            my_heading = hw.gps.track_angle_deg
                except Exception:
                    pass

        # -- buttons ----------------------------------------------------------
        for name, btn in buttons.items():
            event = btn.update(now)
            if not event:
                continue

            if name == "menu" and event == "tap":
                screen = (screen + 1) % SCREEN_COUNT

            elif name == "chirp" and event == "tap":
                seq += 1
                frame = packet.pack_command(seq, Command.CHIRP)
                tx_status = "sent CHIRP"
                _send(hw, frame)

            elif name == "arm" and event == "hold":
                # Hold, not tap. A bumped button must not change rocket state.
                seq += 1
                armed = link.tel and link.tel["state"] == State.ARMED
                cmd = Command.DISARM if armed else Command.ARM
                frame = packet.pack_command(seq, cmd)
                tx_status = "sent {}".format("DISARM" if armed else "ARM")
                _send(hw, frame)

        # -- display, on a timer (also services VCOM) -------------------------
        if now >= next_draw:
            next_draw = now + draw_period
            try:
                draw(hw.display, link, my_lat, my_lon, my_heading,
                     screen, now, tx_status)
            except Exception as e:
                print("draw failed:", e)


def _send(hw, frame):
    """Transmit and return to receive. Half-duplex: TX blocks RX briefly."""
    if not hw.radio:
        return
    try:
        hw.radio.send(frame)
    except Exception as e:
        print("send failed:", e)


if __name__ == "__main__":
    main()