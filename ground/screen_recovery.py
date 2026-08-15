"""RECOVERY screen: distance/bearing/walking directions to the last known
rocket fix. The fix is latched on Link, so this keeps working after the
rocket goes silent."""

import units
from display_util import text
from nav import bearing_deg, compass_point, haversine_m, relative_arrow


def draw(display, frame):
    link = frame.link

    if link.fix_lat is None:
        text(display, 4, 90, "NO FIX LATCHED", size=2)
        text(display, 4, 120, "walk toward last seen bearing")
        return

    if frame.my_lat is None:
        text(display, 4, 60, "ROCKET", size=2)
        text(display, 4, 84, "{:.6f}".format(link.fix_lat), size=2)
        text(display, 4, 108, "{:.6f}".format(link.fix_lon), size=2)
        text(display, 4, 140, "waiting for own GPS fix")
        return

    d = haversine_m(frame.my_lat, frame.my_lon, link.fix_lat, link.fix_lon)
    b = bearing_deg(frame.my_lat, frame.my_lon, link.fix_lat, link.fix_lon)
    text(display, 4, 52, "{:.0f} {}".format(units.distance(d), units.distance_label()), size=3)
    text(display, 180, 52, "{:.0f} {}".format(b, compass_point(b)), size=3)

    arrow = relative_arrow(b, frame.my_heading)
    if arrow:
        text(display, 4, 100, arrow, size=3)
    else:
        text(display, 4, 100, "walk to get heading", size=2)

    text(display, 4, 150, "rocket {:.6f}".format(link.fix_lat))
    text(display, 4, 166, "       {:.6f}".format(link.fix_lon))
    fix_age = (frame.now - link.fix_age_ms) / 1000.0
    text(display, 4, 190, "fix age {:.0f}s".format(fix_age))
    if frame.status != "LIVE":
        text(display, 200, 190, "LATCHED -- rocket silent")
