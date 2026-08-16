# Recovering a Corrupted Board Filesystem

What happened during a firmware update session on the rocket board, how we
diagnosed it, and the runbook for fixing it if it happens again (on either
board).

## What happened

While iterating on `rocket/boot.py`'s flight-mode/dev-mode logic, the rocket
board went through a lot of hard resets and power cycles in a short window --
BOOTSEL/RESET combos, unplug/replug to force a fresh `supervisor.runtime.
usb_connected` check, etc. At some point one of those resets landed mid-write,
and the board's internal FAT filesystem came out of it corrupted.

Symptom: the CIRCUITPY drive mounted with garbled, unreadable filenames (mojibake
instead of `code.py` / `packet.py`), and the volume showed up labeled as a raw
serial number (`C0E0-F50B`) instead of `LC-ROCKET`. That serial-number fallback
is the tell -- it's what a file manager shows when the volume label stored in
the filesystem's own root directory structure is unreadable. Practically, this
meant the board wasn't running any recognizable `code.py`, so it stopped
transmitting telemetry entirely (the ground station showed `NO TELEMETRY
rejects: 0` -- nothing arriving over the radio at all, as opposed to `rejects`
climbing, which would mean packets arriving but failing to decode).

## Why reflashing CircuitPython alone didn't fix it

The first fix attempt was to reflash CircuitPython from its `.uf2` (see
[Recovery runbook](#recovery-runbook) below). That did *not* fix it -- same
garbled files, same `C0E0-F50B`, right after the reflash.

Reason: flashing a CircuitPython `.uf2` replaces the interpreter/firmware only.
It does **not** reformat the internal filesystem partition unless CircuitPython
can't mount it at all. This board's corruption was bad enough to garble
filenames but not bad enough to make CircuitPython refuse to mount it, so the
same damaged filesystem just came right back after the reflash.

The actual fix has to be an explicit filesystem reformat, from the CircuitPython
REPL:

```python
import storage
storage.erase_filesystem()
```

This is CircuitPython's built-in "the filesystem is corrupt, rebuild it" tool.
It erases and reformats the internal flash filesystem and reboots.

## A red herring along the way: no `/dev/ttyACM*`

Partway through recovery, the serial port disappeared entirely (no
`/dev/ttyACM*` device at all). That's expected, not a new failure: the RP2040's
UF2 bootloader (`RPI-RP2`, seen in `lsusb` as `2e8a:0003 Raspberry Pi RP2
Boot`) is mass-storage-only and never presents a serial/CDC interface. If
you're mid-recovery and lose the serial port, check `lsusb` for that ID first
-- it just means the board is sitting in the bootloader waiting for a `.uf2`,
not that something new broke. A stale Nautilus/file-manager window can also
keep showing an old (corrupted) mount after the board has already re-entered
the bootloader -- don't trust a file manager's cached view here; re-check with
`lsusb` and `find /run/media/$USER -maxdepth 1`.

## Recovery runbook

1. **Note the CircuitPython version first**, if `boot_out.txt` is still
   readable (it survives corruption that garbles the other files, since it's
   regenerated fresh on every boot). It'll look like:
   ```
   Adafruit CircuitPython 10.2.1 on 2026-05-13; Adafruit Feather RP2040 RFM with rp2040
   Board ID:adafruit_feather_rp2040_rfm
   ```
   Matching the version avoids introducing a CircuitPython upgrade as an extra
   variable while you're debugging something else.

2. **Enter the UF2 bootloader**: hold **BOOT**, press and release **RESET**
   while still holding BOOT (or hold BOOT while plugging in USB). A drive
   named `RPI-RP2` should appear. Confirm with `lsusb` if unsure --
   `2e8a:0003 Raspberry Pi RP2 Boot` means you're in.

3. **Copy the matching `.uf2`** (this repo keeps a copy of the one used for
   both boards at
   `docs/downloads/adafruit-circuitpython-adafruit_feather_rp2040_rfm-en_US-10.2.1.uf2`)
   onto `RPI-RP2`, and force a `sync` before the board has a chance to reset
   itself mid-write:
   ```
   cp docs/downloads/adafruit-circuitpython-*.uf2 /run/media/$USER/RPI-RP2/ && sync
   ```
   The board reboots on its own once the copy is complete.

4. **Reformat the filesystem from the REPL** -- this is the step that
   actually fixes corruption, not step 3:
   ```
   make monitor          # or screen/minicom on /dev/ttyACM0
   ```
   At the `>>>` prompt (Ctrl-C first if something's running):
   ```python
   import storage
   storage.erase_filesystem()
   ```
   The board erases, reformats, and reboots automatically.

5. **Confirm it's clean**: `make volumes` should now show a plain `CIRCUITPY`
   volume (not yet relabeled).

6. **Re-provision from the repo**:
   ```
   make setup-rocket      # or setup-ground -- labels the volume, needs a reset after
   #  ... reset the board ...
   make volumes           # confirm it now shows LC-ROCKET / LC-GROUND
   make libs-rocket        # or libs-ground -- reinstalls CircuitPython libraries,
                            # wiped along with everything else by the reformat
   make deploy-rocket      # or deploy-ground
   ```

## Prevention

The root trigger was resetting/unplugging the board while a write could still
be in flight -- easy to do when iterating quickly on `boot.py` and wanting a
truly fresh boot each time. Where practical, let a write finish and the OS
finish flushing (`sync`, or just give the "copied" progress bar/notification a
moment to clear) before pulling power or hitting RESET, especially right after
`boot.py`'s own remount dance runs.
