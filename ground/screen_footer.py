"""Footer rendered on every screen: what each button does right now.

Contextual per screen -- see the ARM/DISARM vs BACK split in code.py's
button dispatch, which this must stay in sync with:
  - FLIGHT: ARM/DISARM sends the command; MENU advances to the next screen.
  - anywhere else: ARM/DISARM goes back one screen instead (MENU still
    advances, so the screen list is a loop you can walk either direction
    a step at a time).
"""

from display_util import text

FOOTER_Y = 222


def draw(display, frame):
    if frame.is_flight:
        arm_label = "HOLD:DISARM" if frame.armed else "HOLD:ARM"
    else:
        arm_label = "HOLD:BACK>{}".format(frame.prev_screen_name)

    text(display, 10, FOOTER_Y, "MENU>{}".format(frame.next_screen_name))
    text(display, 170, FOOTER_Y, arm_label)
    text(display, 330, FOOTER_Y, "TAP:CHIRP")
