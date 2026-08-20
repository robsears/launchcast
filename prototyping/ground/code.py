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
import gc
import board
import busio
import digitalio
import analogio
import keypad
import supervisor

import packet
from packet import State, Command, Sensor
from display_util import text
from hold_tracker import HoldTracker
import screen_header
import screen_footer
import screen_flight
import screen_recovery
import screen_diagnostics

# --- Tuning ------------------------------------------------------------------
GPS_HZ = 1              # Local GPS refresh rate

LINK_STALE_MS = 3000    # No packet for this long -> show as stale
LINK_LOST_MS = 15000    # No packet for this long -> show as LOST

HOLD_MS = 2000          # ARM/DISARM requires a deliberate hold
DEBOUNCE_MS = 50        # keypad.Keys scan interval. Debounce happens in the
                        # background at this cadence regardless of what the
                        # main loop is doing, so a slow GPS/display call can't
                        # cause a press to be missed.
GRACE_MS = 250          # HoldTracker bridges a release-then-re-press of the
                        # same key within this window, so a brief mechanical
                        # bounce mid-hold doesn't restart HOLD_MS or misfire
                        # as a tap. Adds the same delay to genuine tap
                        # dispatch (it now finalizes GRACE_MS after release
                        # instead of immediately) -- should be imperceptible
                        # for a physical button, but lower it if CHIRP starts
                        # feeling laggy.

CMD_CONFIRM_FRAMES = 3  # payload telemetry frames to see before declaring an
                        # ARM/DISARM failed. Counting frames rather than
                        # elapsed time follows the payload's own TX rate
                        # (TX_HZ_IDLE/TX_HZ_FLIGHT), so the window is neither
                        # too short at low rate nor needlessly long at high
                        # rate. A link-lost fallback below still applies when
                        # no frames are arriving at all to count.

BATT_DIVIDER = 2.0      # Onboard voltage divider ratio; 1.0 if reading BAT directly
BATT_SAMPLES = 8        # Number of samples to average for estimating battery life

# Below this, treat "USB connected" as USB-power-only, not charging -- the
# BAT rail reads low/unstable with no cell attached (e.g. slide switch left
# open), and we don't want that showing up as a false CHARGING indicator.
# Verify empirically on real hardware (switch open + USB in) and adjust.
BATT_PRESENT_MIN_V = 2.5

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
# HoldTracker itself lives in hold_tracker.py -- it has no hardware imports,
# so it can be unit tested off-board (see tests/test_hold_tracker.py).


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
        self.batt_volts = 0.0

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

        if tel["batt_volts"]:
            self.batt_volts = tel["batt_volts"]

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
SCREEN_NAMES = ("FLIGHT", "RECOVERY", "DIAG")
SCREEN_MODULES = (screen_flight, screen_recovery, screen_diagnostics)


class Frame:
    """Everything a screen/header/footer module might want to render,
    computed once per draw() call so nobody recomputes link.status() etc.

    screen/prev/next names are precomputed here, not looked up by the
    header/footer modules, so MENU (screen+1) and ARM-as-back (screen-1)
    stay in lockstep with the button dispatch in main() below -- add a
    screen, and this is the one place that has to know about it.
    """

    def __init__(self, link, my_lat, my_lon, my_heading, my_batt, my_charging,
                 screen, now, tx_status):
        self.link = link
        self.my_lat = my_lat
        self.my_lon = my_lon
        self.my_heading = my_heading
        self.my_batt = my_batt
        self.my_charging = my_charging
        self.screen = screen
        self.now = now
        self.tx_status = tx_status

        self.tel = link.tel
        self.status = link.status(now)
        self.age = link.age_ms(now)
        self.armed = self.tel is not None and self.tel["state"] == State.ARMED
        self.payload_batt = self.tel["batt_volts"] if self.tel is not None else None
        self.payload_charging = (
            self.tel is not None and bool(self.tel["sensors"] & Sensor.CHG)
        )

        self.is_flight = screen == SCREEN_FLIGHT
        self.screen_name = SCREEN_NAMES[screen]
        self.next_screen_name = SCREEN_NAMES[(screen + 1) % SCREEN_COUNT]
        self.prev_screen_name = SCREEN_NAMES[(screen - 1) % SCREEN_COUNT]


def draw(display, frame):
    """Render one frame. Sharp Memory: 0 = dark pixel, 1 = light."""
    if display is None:
        return

    display.fill(1)
    screen_header.draw(display, frame)

    if frame.tel is None:
        text(display, 4, 90, "NO TELEMETRY", size=3)
        text(display, 4, 130, "rejects: {}".format(frame.link.rejects))
    else:
        SCREEN_MODULES[frame.screen].draw(display, frame)

    screen_footer.draw(display, frame)
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
    held = HoldTracker(BUTTON_NAMES, hold_ms=HOLD_MS, grace_ms=GRACE_MS)

    screen = SCREEN_FLIGHT
    seq = 0
    tx_status = "ready"
    # Pending ARM/DISARM awaiting confirmation from the payload's reported state.
    pending = None          # dict: {"want": State.ARMED/IDLE, "packets_at_send": n, "seq": n}

    my_lat = None
    my_lon = None
    my_heading = None
    my_batt = None
    my_charging = False

    next_draw = 0
    next_gps = 0
    next_vbat = 0
    next_memcheck = 0
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

        try:
            my_charging = (
                supervisor.runtime.usb_connected
                and my_batt is not None
                and my_batt >= BATT_PRESENT_MIN_V
            )
        except Exception:
            my_charging = False

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
            print("BUTTON EVENT:", name, event)
            try:
                if name == "menu" and event == "tap":
                    screen = (screen + 1) % SCREEN_COUNT

                elif name == "chirp" and event == "tap":
                    seq += 1
                    _send(hw, packet.pack_command(seq, Command.CHIRP))
                    tx_status = "CHIRP sent"

                elif name == "arm" and event == "hold":
                    if screen != SCREEN_FLIGHT:
                        # Off the FLIGHT screen, ARM/DISARM is repurposed as
                        # BACK -- the command can only be sent from FLIGHT.
                        screen = (screen - 1) % SCREEN_COUNT
                    else:
                        armed = link.tel is not None and link.tel["state"] == State.ARMED
                        seq += 1
                        if armed:
                            _send(hw, packet.pack_command(seq, Command.DISARM))
                            pending = {"want": State.IDLE, "packets_at_send": link.packets, "seq": seq}
                            tx_status = "DISARM sent..."
                        else:
                            _send(hw, packet.pack_command(seq, Command.ARM))
                            pending = {"want": State.ARMED, "packets_at_send": link.packets, "seq": seq}
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
            elif link.packets - pending["packets_at_send"] >= CMD_CONFIRM_FRAMES:
                tx_status = "!! COMMAND FAILED -- retry"
                pending = None
            elif link.status(now) == "LOST":
                # The check above can't fire if no frames are arriving at all
                # to count -- don't leave a stale "...sent" status up forever
                # if the link itself has dropped.
                tx_status = "!! COMMAND FAILED -- link lost"
                pending = None

        # -- display, on a timer (also services VCOM) -------------------------
        if now >= next_draw:
            next_draw = now + draw_period
            t0 = ms()
            try:
                frame = Frame(link, my_lat, my_lon, my_heading, my_batt, my_charging,
                               screen, now, tx_status)
                draw(hw.display, frame)
            except Exception as e:
                print("draw failed:", e)
            # Frame/draw() churn out several short-lived strings and dicts a
            # cycle. CircuitPython's collector doesn't compact the heap, so
            # that garbage fragments it over time until even a small
            # contiguous allocation (the display's own buffer, a formatted
            # string) can fail despite plenty of free memory in aggregate.
            # Collecting right after the allocation-heavy part of the loop
            # coalesces adjacent free blocks before they have a chance to
            # fragment further.
            gc.collect()
            dt = ms() - t0
            if dt > 50:
                print("DRAW TOOK", dt, "ms")

        # -- memory watchdog ----------------------------------------------------
        if now >= next_memcheck:
            next_memcheck = now + 5000
            free = gc.mem_free()
            if free < 20000:
                print("LOW MEMORY:", free, "bytes free")

        # -- loop-latency watchdog ---------------------------------------------
        # Buttons are only sampled once per pass through this loop, so any
        # single blocking call here (radio/GPS/display) delays every button
        # by the same amount. If this fires, that's why taps need a long hold.
        loop_dt = ms() - now
        if loop_dt > 150:
            print("SLOW LOOP:", loop_dt, "ms")


def _send(hw, pkt):
    """Transmit and return to receive. Half-duplex: TX blocks RX briefly."""
    if not hw.radio:
        return
    try:
        hw.radio.send(pkt)
    except Exception as e:
        print("send failed:", e)


if __name__ == "__main__":
    main()