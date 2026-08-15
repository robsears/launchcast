"""DIAG screen: raw counters and link stats, for debugging on the bench."""

from display_util import text
from packet import Sensor


def draw(display, frame):
    link = frame.link
    tel = frame.tel

    text(display, 4, 52, "pkts {}  rej {}".format(link.packets, link.rejects))
    text(display, 4, 72, "rssi {}  snr {}".format(link.rssi, link.snr))
    text(display, 4, 92, "state {}".format(tel["state_name"]))
    text(display, 4, 112, "uptime {:.1f}s".format(tel["uptime_ms"] / 1000.0))
    text(display, 4, 132, "counter {}".format(tel["counter"]))
    present, missing = Sensor.decode(tel["sensors"])
    text(display, 4, 152, "up: {}".format(" ".join(present)))
    text(display, 4, 168, "down: {}".format(" ".join(missing) or "none"))
    text(display, 4, 188, "accel {:.2f} {:.2f} {:.2f}".format(*tel["accel_g"]))
    text(display, 4, 204, frame.tx_status)
