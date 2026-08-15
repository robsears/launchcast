"""Header rendered on every screen: title, screen name, link status/age,
and the global status icons (link signal, handheld battery).

Screens should not draw above y=40 -- that band belongs to the header.
Screen size: 400x240
"""

import icons
from display_util import text

# display: screen obj
# frame: draw frame, w/ everything we may need to render
def draw(display, frame):

    text(display, 4, 4, "LAUNCHCAST", size=2) # 16x16, 160px wide
    text(display, 4, 26, frame.screen_name) # 8x8

    GAP = 4
    SIGNAL_SCALE = 2

    # rocket cluster: payload icon, link signal, PAYLOAD battery -- the
    # voltage the rocket radios down, not the handheld's own battery.
    rocket_x = 160
    rocket_signal = rocket_x + icons.ROCKET_W + GAP
    rocket_batt = rocket_signal + icons.SIGNAL_W * SIGNAL_SCALE + GAP

    icons.draw_rocket(display,  rocket_x,      4, scale=1)
    icons.draw_signal(display,  rocket_signal, 4, frame.link.rssi, scale=SIGNAL_SCALE)
    icons.draw_battery(display, rocket_batt,   4, frame.payload_batt)

    # handheld's own battery -- separate from the rocket cluster above, or
    # it silently overwrites the one reading we have for the ground unit.
    icons.draw_ground(display, 315,   4, scale=1)
    text(display, 330, 4, "HH")
    icons.draw_battery(display, 330, 14, frame.my_batt)

    # text(display, 230, 4, frame.status, size=2)
    # if frame.age is not None:
    #     text(display, 230, 26, "{:.1f}s ago".format(frame.age / 1000.0))
