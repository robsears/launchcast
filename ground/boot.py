"""LaunchCast boot.py -- handheld ground station.

The handheld writes nothing to flash, so it stays in the default
host-writable mode. This file exists only to label the volume, so that
`make deploy-ground` cannot accidentally target the rocket board when
both are plugged in.

setting the label requires the filesystem to be board-writable at that
moment, which by default it isn't. So this either needs a one-time
storage.remount("/", readonly=False) before the relabel, or you set
the label once manually from the REPL and then this file is a no-op
that just confirms it.

Simplest path when hardware arrives: plug in the handheld board, open
the REPL, and run:

```
import storage
storage.remount("/", readonly=False)
storage.getmount("/").label = "LC-GROUND"
```
"""

import storage

try:
    fs = storage.getmount("/")
    if fs.label != "LC-GROUND":
        fs.label = "LC-GROUND"
        print("boot: relabeled volume to LC-GROUND")
except Exception as e:
    print("boot: label unchanged ({})".format(e))