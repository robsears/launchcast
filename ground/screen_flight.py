"""FLIGHT screen: whimsy over density -- a rocket illustration plus two
compact "systems check" tables (the rocket's, then the handheld's own),
rather than a wall of raw numbers. Raw per-axis/link numbers still live on
DIAG for anyone who wants them.

The default screen -- ARM/DISARM only sends its command here (see
screen_footer.py and code.py's button dispatch), so the payload's battery
voltage stays as an exact number, not just a percentage: it gates the
NO-GO banner below.
"""

import handheld_art
import icons
import rocket_art
import units
from display_util import text
from nav import haversine_m
from packet import Sensor

ROCKET_X = 4
ROCKET_Y = 44
TABLE_X = ROCKET_X + rocket_art.ROCKET_ART_W + 6
RX = 265  # MISSION CONTROL column start -- clears "ACCELEROMETER: OFFLINE" (worst case) with room to spare
HANDHELD_Y = 44
MC_TABLE_Y = HANDHELD_Y + handheld_art.HANDHELD_ART_H + 4
STATUS_Y = 204  # alert / command-status band, above the footer


def _online(tel, bit):
    return "ONLINE" if tel["sensors"] & bit else "OFFLINE"


def draw(display, frame):
    tel = frame.tel
    link = frame.link

    rocket_art.draw(display, ROCKET_X, ROCKET_Y, armed=frame.armed, scale=1)

    # ----- ROCKET systems check ------------------------------------------
    text(display, TABLE_X, 44, "ROCKET", size=2)
    text(display, TABLE_X, 64, "SYSTEMS CHECK:")
    text(display, TABLE_X, 78,  "ATMOSPHERE:      {}".format(_online(tel, Sensor.BARO)))
    text(display, TABLE_X, 90,  "ACCELEROMETER:   {}".format(_online(tel, Sensor.IMU)))
    text(display, TABLE_X, 102, "MAGNETOMETER:    {}".format(_online(tel, Sensor.MAG)))
    text(display, TABLE_X, 114, "FILESYSTEM:      {}".format(_online(tel, Sensor.LOG)))
    text(display, TABLE_X, 126, "GPS LOCK:        {}".format("FIXED" if tel["has_fix"] else "SEARCH"))
    text(display, TABLE_X, 138, "TEMPERATURE:     {:.1f}{}".format(
        units.temperature(tel["temp_c"]), units.temperature_label()))
    text(display, TABLE_X, 150, "ALTITUDE:        {:.1f}{}".format(
        units.distance(tel["alt_baro_m"]), units.distance_label()))
    text(display, TABLE_X, 162, "BATTERY:         {}%".format(icons.battery_percent(tel["batt_volts"])))
    text(display, TABLE_X, 174, "SIGNAL STRENGTH: {}%".format(icons.signal_percent(link.rssi)))
    text(display, TABLE_X, 186, "STATUS:          {}".format(tel["state_name"]))

    # ----- MISSION CONTROL (handheld) systems check ----------------------
    handheld_art.draw(display, RX, HANDHELD_Y, scale=1)

    text(display, RX, MC_TABLE_Y, "CONTROLLER", size=2)
    text(display, RX, MC_TABLE_Y + 21, "GPS LOCK: {}".format(
        "FIXED" if frame.my_lat is not None else "SEARCH"))
    text(display, RX, MC_TABLE_Y + 33, "BATTERY:  {}%".format(icons.battery_percent(frame.my_batt)))

    if link.fix_lat is not None and frame.my_lat is not None:
        d = haversine_m(frame.my_lat, frame.my_lon, link.fix_lat, link.fix_lon)
        text(display, RX, MC_TABLE_Y + 45, "DIST:     {:.1f}{}".format(
            units.distance(d), units.distance_label()))
    else:
        text(display, RX, MC_TABLE_Y + 45, "DIST:     --")

    # ----- alerts / command status (bottom, full width) -------------------
    if tel["batt_volts"] < 3.80:
        text(display, 4, STATUS_Y, "*** PAYLOAD BATT LOW -- NO GO ***")
    else:
        text(display, 4, STATUS_Y, frame.tx_status)
