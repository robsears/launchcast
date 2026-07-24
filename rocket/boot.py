"""
LaunchCast boot.py -- rocket payload.

Runs ONCE at power-on, before code.py, and cannot be re-run without a hard
reset. Changing this file means unplug/replug to test it.

The problem it solves: CircuitPython mounts its filesystem read-only to the
BOARD whenever USB has it mounted read-write to the HOST. Only one side can
write. Default is host-writable, which means code.py cannot append to
flight.bin -- FlightLog silently disables itself and you fly with no dataset.

The switch decides:

    switch ON  (pin reads LOW)  -> FLIGHT MODE
                                   board can write, host filesystem read-only
    switch OFF (pin reads HIGH) -> DEV MODE
                                   host can write, board cannot log

This maps to the physical build: the slide switch already gates battery power
to BAT. On battery, USB is absent and the switch is on, so flight mode is
automatic. On the bench with USB and the switch off, you edit code normally.

NOTE: the switch is on the BATTERY line, not a GPIO. If you want boot.py to
sense it you must run a second wire from the switched side to ARM_PIN. If you
would rather not, set USE_SWITCH = False below and the board decides by
whether USB data is connected -- which is right most of the time and is one
fewer wire.
"""

import board
import digitalio
import storage
import supervisor

# --- Configuration -----------------------------------------------------------

USE_SWITCH = False  # True = read ARM_PIN; False = decide from USB presence
ARM_PIN = board.D13  # only used when USE_SWITCH is True

# --- Mode selection ----------------------------------------------------------


def _switch_says_flight():
    """True when the arming switch is closed (pin pulled to GND)."""
    pin = digitalio.DigitalInOut(ARM_PIN)
    pin.direction = digitalio.Direction.INPUT
    pin.pull = digitalio.Pull.UP
    closed = not pin.value  # active low
    pin.deinit()  # release it so code.py can claim the pin
    return closed


def _usb_says_flight():
    """True when no USB data connection is present.

    supervisor.runtime.usb_connected reports the DATA link, not merely
    5 V. A USB wall charger reads False here, which is the behavior we
    want -- charging on the pad should not disable logging.
    """
    try:
        return not supervisor.runtime.usb_connected
    except AttributeError:
        # Older CircuitPython. Fall back to serial connection state, which
        # is a weaker signal but better than assuming dev mode.
        return not supervisor.runtime.serial_connected


flight_mode = _switch_says_flight() if USE_SWITCH else _usb_says_flight()

# --- Apply -------------------------------------------------------------------

if flight_mode:
    # readonly=False means the BOARD may write. The host sees a read-only
    # drive. This is what lets FlightLog append to flight.bin.
    storage.remount("/", readonly=False)
    print("boot: FLIGHT MODE -- board can write, host read-only")
else:
    print("boot: DEV MODE -- host can write, no flight logging")

# --- Optional: distinct volume labels ----------------------------------------
# Two identical Feathers both mount as CIRCUITPY, and it is genuinely easy to
# deploy flight firmware to the handheld. Relabeling costs nothing and makes
# the Makefile targets unambiguous.
#
# Uncomment ONE of these, per board. Takes effect after the next hard reset.
#
# storage.remount("/", readonly=False)
# import microcontroller
# microcontroller.nvm[0] = 1
#
# Simpler: use storage.getmount("/").label -- but note that changing the
# label requires the filesystem to be board-writable at the time.

try:
    fs = storage.getmount("/")
    if fs.label != "LC-ROCKET" and flight_mode:
        fs.label = "LC-ROCKET"
        print("boot: relabeled volume to LC-ROCKET")
except Exception as e:
    print("boot: label unchanged ({})".format(e))