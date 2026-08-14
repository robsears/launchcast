"""
LaunchCast boot.py -- handheld ground station.

Runs ONCE at power-on, before code.py. Editing this file means unplug/replug
(or press RESET) to test it.

The handheld writes nothing to flash -- no flight log -- so it stays in the
default host-writable mode and never needs a flight-mode remount. This file
exists only to LABEL the volume LC-GROUND, so that `make deploy-ground` cannot
accidentally target the rocket board when both are plugged in.

Setting a label requires a board-writable filesystem, which by default is not
the case over USB. So we briefly take write access just for the label, then
hand it back to the host. The label persists, so this only does real work on
the first boot after deploy; later boots see it is already set and skip.
"""

import storage

LABEL = "LC-GROUND"

fs = storage.getmount("/")

if fs.label != LABEL:
    try:
        storage.remount("/", readonly=False)   # board-writable, briefly
        fs.label = LABEL
        print("boot: labeled", LABEL)
    except Exception as e:
        print("boot: label unchanged ({})".format(e))
    finally:
        storage.remount("/", readonly=True)    # always hand back to the host
else:
    print("boot: label OK ({})".format(LABEL))