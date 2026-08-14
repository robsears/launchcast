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
  - Buttons are captured by keypad.Keys, a background scan independent of
    the main loop, so a slow GPS/display call can't cause a press to be
    dropped. Hold-duration math still runs on the main loop's own clock
    (not the event's timestamp -- different clock base, do not mix).

Copy packet.py to the board alongside this file.
"""

import time
import math
import board
import busio
import digitalio
import analogio
import keypad

import packet
from packet import State, Command, Sensor

# --- Tuning ------------------------------------------------------------------
GPS_HZ = 1              # Local GPS refresh rate

LINK_STALE_MS = 3000    # No packet for this long -> show as stale
LINK_LOST_MS = 15000    # No packet for this long -> show as LOST

HOLD_MS = 2000          # ARM/DISARM requires a deliberate hold
DEBOUNCE_MS = 50        # keypad.Keys scan interval. Debounce happens in the
                        # background at this cadence regardless of what the
                        # main loop is doing, so a slow GPS/display call can't
                        # cause a press to be missed.

EARTH_R_M = 6371000.0   # Radius of the Earth in m. We probably won't need to change this.

CMD_CONFIRM_MS = 2000   # window to see the payload's state change after ARM/DISARM

BATT_DIVIDER = 2.0      # Onboard voltage divider ratio; 1.0 if reading BAT directly
BATT_SAMPLES = 8        # Number of samples to average for estimating battery life

# --- Display --------------------------------------------------------

DISPLAY_HZ = 2.0    # Display refresh rate; also services VCOM
DISP_W = 400        # Pixels wide
DISP_H = 240        # Pixels tall

# --- Hardware ----------------------------------------------------------------

# Order must match the pins tuple in Hardware._init_buttons -- keypad.Keys
# reports events by index into that tuple, not by name.
BUTTON_NAMES = ("arm", "chirp", "menu")


class Hardware:
    def __init__(self):
        self.i2c = None
        self.gps = None
        self.radio = None
        self.display = None
        self.keys = None
        self.errors = []
        self.vbat = None

    def init_all(self):
        self._init_i2c()
        self._init_gps()
        self._init_radio()
        self._init_display()
        self._init_buttons()
        self._init_vbat()
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
            # SBAS/WAAS narrows single-unit error from ~5-10m to ~1-3m under open
            # sky (No effect indoors/under tree cover). Two independent receivers
            # still won't agree to zero -- their errors are uncorrelated.
            self.gps.send_command(b"PMTK313,1")
            self.gps.send_command(b"PMTK301,2")
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
            cs = digitalio.DigitalInOut(board.D6)  # verify against wiring
            self.display = adafruit_sharpmemorydisplay.SharpMemoryDisplay(
                spi, cs, DISP_W, DISP_H
            )
            self.display.fill(1)  # 1 = light on this panel
            self.display.show()
        except Exception as e:
            self.errors.append("display: {}".format(e))

    def _init_buttons(self):
        try:
            pins = (board.D9, board.D10, board.D11)  # matches BUTTON_NAMES order
            self.keys = keypad.Keys(
                pins,
                value_when_pressed=False,  # active low
                pull=True,
                interval=DEBOUNCE_MS / 1000.0,
            )
        except Exception as e:
            self.errors.append("buttons: {}".format(e))

    def _init_vbat(self):
        try:
            pin = getattr(board, "VOLTAGE_MONITOR", None) or board.A0
            self.vbat = analogio.AnalogIn(pin)
        except Exception as e:
            self.errors.append("vbat: {}".format(e))

    def battery_volts(self):
        if not self.vbat:
            return None
        total = 0
        for _ in range(BATT_SAMPLES):
            total += self.vbat.value
        return (total / BATT_SAMPLES / 65535.0) * 3.3 * BATT_DIVIDER


# --- Buttons -----------------------------------------------------------------


class HoldTracker:
    """Turns keypad.Keys press/release edges into 'tap' / 'hold' events.

    keypad.Keys debounces and timestamps presses in the background (a
    supervisor-level scan, not the Python main loop), so an edge is never
    missed just because the loop is stuck in a slow GPS or display call. This
    class only adds the piece keypad.Keys doesn't have: "still held after
    HOLD_MS" isn't an edge, so it has to be checked every pass rather than
    read off the event queue.

    A tap fires on RELEASE, so a hold does not also register as a tap.
    """

    def __init__(self, names):
        self.names = names  # index (key_number) -> name
        self.down_since = {}     # name -> ms timestamp, present only while held
        self.hold_fired = set()

    def poll(self, keys, now):
        """Drain queued edges and check for newly-expired holds.

        Returns a list of (name, 'tap' | 'hold') pairs, in order.
        """
        out = []
        while True:
            event = keys.events.get()
            if event is None:
                break
            name = self.names[event.key_number]
            if event.pressed:
                self.down_since[name] = now
                self.hold_fired.discard(name)
            else:
                since = self.down_since.pop(name, None)
                if since is not None and name not in self.hold_fired:
                    out.append((name, "tap"))
                self.hold_fired.discard(name)

        for name, since in self.down_since.items():
            if name not in self.hold_fired and now - since >= HOLD_MS:
                self.hold_fired.add(name)
                out.append((name, "hold"))

        return out


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


def draw(display, link, my_lat, my_lon, my_heading, screen, now, tx_status, my_batt):
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
        # ----- PAYLOAD (left column) -----
        text(4, 50, tel["state_name"], 3)
        text(4, 88,  "ALT {:>7.1f}m".format(tel["alt_baro_m"]), 2)
        text(4, 110, "TMP {:>6.1f}C".format(tel["temp_c"]), 2)
        text(4, 132, "ACC {:.2f} {:.2f} {:.2f}".format(*tel["accel_g"]), 1)
        text(4, 148, "BAT {:.2f}V".format(tel["batt_volts"]), 2)

        present, missing = Sensor.decode(tel["sensors"])
        text(4, 172, "UP {}".format(" ".join(present)), 1)
        if missing:
            text(4, 186, "DN {}".format(" ".join(missing)), 1)

        if tel["has_fix"]:
            text(4, 204, "{:.5f} {:.5f}".format(tel["lat"], tel["lon"]), 1)
            text(4, 218, "SAT {}".format(tel["satellites"]), 1)
        else:
            text(4, 204, "payload GPS: no fix", 1)

        # ----- HANDHELD + LINK (right column) -----
        rx = 210
        text(rx, 50, status, 2)
        if age is not None:
            text(rx, 72, "AGE {:.1f}s".format(age / 1000.0), 1)
        text(rx, 88,  "RSSI {}".format(link.rssi if link.rssi is not None else "--"), 1)
        text(rx, 102, "SNR  {}".format(link.snr if link.snr is not None else "--"), 1)
        text(rx, 116, "PKT {} REJ {}".format(link.packets, link.rejects), 1)
        if my_batt is not None:
            text(rx, 134, "HH BAT {:.2f}V".format(my_batt), 1)

        # distance to rocket, if both have a fix
        if link.fix_lat is not None and my_lat is not None:
            d = haversine_m(my_lat, my_lon, link.fix_lat, link.fix_lon)
            b = bearing_deg(my_lat, my_lon, link.fix_lat, link.fix_lon)
            text(rx, 156, "DIST {:.0f}m".format(d), 2)
            text(rx, 178, "BRG {:.0f} {}".format(b, compass_point(b)), 1)
        elif link.fix_lat is not None:
            text(rx, 156, "rocket seen,", 1)
            text(rx, 170, "need own fix", 1)
        else:
            text(rx, 156, "no rocket fix", 1)

        # ----- alerts / command status (bottom, full width) -----
        if tel["batt_volts"] < 3.80:
            text(4, 232, "*** PAYLOAD BATT LOW -- NO GO ***", 1)
        else:
            text(rx, 200, tx_status, 1)

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
    held = HoldTracker(BUTTON_NAMES)

    screen = SCREEN_FLIGHT
    seq = 0
    tx_status = "ready"
    # Pending ARM/DISARM awaiting confirmation from the payload's reported state.
    pending = None          # dict: {"want": State.ARMED/IDLE, "sent_ms": t, "seq": n}

    my_lat = None
    my_lon = None
    my_heading = None
    my_batt = None

    next_draw = 0
    next_gps = 0
    next_vbat = 0
    draw_period = int(1000 / DISPLAY_HZ)
    gps_period = int(1000 / GPS_HZ)
    vbat_period = 2000 # check battery every 2s

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
                t0 = ms()
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
                dt = ms() - t0
                if dt > 20:
                    print("GPS UPDATE TOOK", dt, "ms")

        # -- own battery ----
        if now >= next_vbat:
            next_vbat = now + vbat_period
            try:
                my_batt = hw.battery_volts()
            except Exception:
                pass

        # -- buttons ----------------------------------------------------------
        # keypad.Keys captured and debounced any edges in the background --
        # this just drains them, so it can't be starved by a slow GPS/display
        # call earlier in this same iteration.
        try:
            events = held.poll(hw.keys, now) if hw.keys else []
        except Exception as e:
            events = []
            print("button poll exception:", e)

        for name, event in events:
            try:
                if name == "menu" and event == "tap":
                    screen = (screen + 1) % SCREEN_COUNT

                elif name == "chirp" and event == "tap":
                    seq += 1
                    _send(hw, packet.pack_command(seq, Command.CHIRP))
                    tx_status = "CHIRP sent"

                elif name == "arm" and event == "hold":
                    armed = link.tel is not None and link.tel["state"] == State.ARMED
                    seq += 1
                    if armed:
                        _send(hw, packet.pack_command(seq, Command.DISARM))
                        pending = {"want": State.IDLE, "sent_ms": now, "seq": seq}
                        tx_status = "DISARM sent..."
                    else:
                        _send(hw, packet.pack_command(seq, Command.ARM))
                        pending = {"want": State.ARMED, "sent_ms": now, "seq": seq}
                        tx_status = "ARM sent..."
            except Exception as e:
                tx_status = "button handler failed"
                print("button handler exception:", e)

        # -- confirm or fail a pending ARM/DISARM -----------------------------
        if pending is not None:
            cur = link.tel["state"] if link.tel is not None else None
            if cur == pending["want"]:
                tx_status = "ARMED OK" if pending["want"] == State.ARMED else "DISARMED OK"
                pending = None
            elif now - pending["sent_ms"] > CMD_CONFIRM_MS:
                tx_status = "!! COMMAND FAILED -- retry"
                pending = None

        # -- display, on a timer (also services VCOM) -------------------------
        if now >= next_draw:
            next_draw = now + draw_period
            t0 = ms()
            try:
                draw(hw.display, link, my_lat, my_lon, my_heading,
                     screen, now, tx_status, my_batt)
            except Exception as e:
                print("draw failed:", e)
            dt = ms() - t0
            if dt > 50:
                print("DRAW TOOK", dt, "ms")

        # -- loop-latency watchdog ---------------------------------------------
        # Buttons are only sampled once per pass through this loop, so any
        # single blocking call here (radio/GPS/display) delays every button
        # by the same amount. If this fires, that's why taps need a long hold.
        loop_dt = ms() - now
        if loop_dt > 150:
            print("SLOW LOOP:", loop_dt, "ms")


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