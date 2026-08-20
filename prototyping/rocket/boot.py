"""
LaunchCast boot.py -- rocket payload.

Runs ONCE at power-on, before code.py, and cannot be re-run without a hard
reset. Editing this file means unplug/replug (or press RESET) to test it.

Two jobs:

  1. FLIGHT-MODE REMOUNT. CircuitPython mounts its filesystem read-only to the
     BOARD whenever a USB host has it mounted read-write. Only one side can
     write at a time, and the host wins by default -- which means code.py
     cannot append to flight.bin and FlightLog silently disables itself.

     We decide by USB data presence: no host connection -> flight mode ->
     board writable. On battery at the pad, USB is absent, so flight mode is
     automatic. On the bench over USB, the host keeps write access and you
     edit normally (no logging, which is what you want while editing).

     supervisor.runtime.usb_connected reports the DATA link, not just 5 V, so
     a wall charger on the pad still counts as "no host" -> flight mode.

  2. VOLUME LABEL. Two identical Feathers both enumerate as CIRCUITPY, and it
     is genuinely easy to deploy flight firmware to the handheld by mistake.
     Labeling the volume LC-ROCKET lets `make deploy-rocket` target it
     unambiguously. Setting a label requires a board-writable filesystem; in
     flight mode we already have it, and in dev mode we grab it briefly just
     for the label operation, then hand write access back to the host.
"""

import storage
import supervisor

LABEL = "LC-ROCKET"

# --- flight mode -------------------------------------------------------------

flight = not supervisor.runtime.usb_connected

if flight:
    # Board may write; host sees a read-only drive. This is what lets
    # FlightLog append to /flight.bin.
    storage.remount("/", readonly=False)

# --- volume label ------------------------------------------------------------
# The label persists in the filesystem, so this only does real work on the
# first boot after deploy. Later boots see it is already set and skip.

fs = storage.getmount("/")

if fs.label != LABEL:
    try:
        if not flight:
            # Dev mode: briefly take write access just to set the label.
            storage.remount("/", readonly=False)
        fs.label = LABEL
        print("boot: labeled", LABEL)
    except Exception as e:
        print("boot: label unchanged ({})".format(e))
    finally:
        if not flight:
            # Always return write access to the host, even if labeling failed.
            storage.remount("/", readonly=True)
else:
    print("boot: label OK ({})".format(LABEL))

# --- report ------------------------------------------------------------------

print("boot:", "FLIGHT MODE (board writes, host read-only)"
      if flight else "DEV MODE (host writes, no flight logging)")