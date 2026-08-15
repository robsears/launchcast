"""Great-circle navigation math shared by the RECOVERY and FLIGHT screens.

Pure computation, no hardware imports, so it runs and tests off-board.
"""

import math

EARTH_R_M = 6371000.0  # Radius of the Earth in m. We probably won't need to change this.


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
    pts = (
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE",
        "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW",
    )
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
