# LaunchCast — Project Context for Software Thread

## What it is
A LoRa telemetry system for a model rocket: a **payload** that flies inside the rocket and broadcasts sensor data, and a **handheld ground station** that receives and displays it, with an uplink for commands. Repo: "LaunchCast," Apache-2.0. Language: **CircuitPython** on both boards. A **Rust rewrite is under active consideration** — see `docs/rust-rewrite.md` before starting that work; the CircuitPython implementation described below is the current, working reference behavior to port against. The hardware is **done and validated**; remaining work is firmware/software.

## Hardware Stack 1 — Payload (flies in the rocket)
- **Feather RP2040 RFM95** (915 MHz LoRa) — flight computer + radio
- **BMP580** barometer, **LSM6DSOX+LIS3MDL** 9-DoF IMU, **PA1010D** GPS (all I²C via STEMMA QT)
- **PS1240 piezo buzzer** on D5/D6 (differential drive), peak resonance **5250 Hz** (measured)
- Onboard NeoPixel (status), **500mAh LiPo** through a slide switch, external **2:1 voltage divider on A0** for battery sense (wired to BAT, not the regulated 3.3V rail — confirmed correct)
- Spring antenna soldered to ANT pad
- Housed in a **3D-printed PETG sled**, 54.7g fully wired, fits the BT-65 payload tube; flight motor Estes D12-5, 57g payload ceiling
- **I²C addresses:** GPS 0x10, LIS3MDL 0x1c, BMP580 0x47, LSM6DSOX 0x6a
- **Radio pins:** `board.RFM_CS`, `RFM_RST`, `RFM_IO0-5`. `board.STEMMA_I2C()` exists. No `board.VOLTAGE_MONITOR` on this board (confirmed via REPL) — the external A0 divider is the only battery sense path.

## Hardware Stack 2 — Handheld (ground station)
- **Feather RP2040 RFM95** (identical radio config)
- **Sharp Memory Display 2.7" 400×240** over SPI, **CS on D6** (needs `font5x8.bin` on board)
- Second **PA1010D GPS** (for computing distance to rocket)
- **2500mAh LiPo** + slide switch, USB-C extension for charge/program without disassembly
- **Three buttons**, active-low w/ internal pull-ups, wired GPIO→button→shared GND:
  - **D9 = ARM/DISARM** (2s hold), **D10 = CHIRP** (tap), **D11 = MENU** (tap)
- Spring antenna, 3D-printed enclosure with labeled buttons
- Both boards have an inline switch on the battery's positive JST lead. If it's left open while charging, the charge IC correctly reports a fault (rapid amber CHG blink, no battery detected) — this is expected charger behavior, not a hardware defect.

## Firmware (current state)
- **`common/packet.py`** (shared, symlinked to both): 40-byte telemetry downlink, 7-byte command uplink, MAGIC=0xA5, SYNC_WORD=0x2B. Sensor bitfield (BARO/IMU/MAG/GPS/LOG/BATT/**CHG**). `CHG` (USB power present) is deliberately excluded from `NAMES`/`ALL`/`REQUIRED`/`decode()` — it's a live power state, not a peripheral-health flag, and is normally 0 for an entire real flight. 155 pytest tests passing in CI.
- **`rocket/code.py`**: flight state machine (BOOT→IDLE→ARMED→BOOST→COAST→APOGEE→DESCENT→LANDED). Apogee detected from **barometric velocity**, not accel. ARM via uplink only (captures ground-pressure reference). Logs to `flight.bin` (binary) **from ARMED onward only** — BOOT/IDLE can sit open-ended across bench sessions and would otherwise fill the flash with nothing useful. An ARM that's DISARMed without ever reaching BOOST clears the log (`FlightLog.clear()`); a real flight can never take that code path back through DISARM, so this can't touch real flight data. Buzzer three-chirp beacon in LANDED. Radio SF7/BW125/CR4-5. Reports `Sensor.CHG` live at TX time from `supervisor.runtime.usb_connected`.
- **`ground/code.py`**: timer-driven display (services Sharp VCOM), three screens (FLIGHT/RECOVERY/DIAG cycled by MENU), latched last-known rocket GPS fix for recovery, CHIRP fire-and-forget. ARM/DISARM is confirmed or failed by **counting payload telemetry frames** since the command was sent (`CMD_CONFIRM_FRAMES`, via `Link.packets`) rather than a wall-clock timeout, so the window tracks the payload's actual TX rate; a link-lost fallback still fires if no frames arrive at all. Prints `BUTTON EVENT: name event` on every tap/hold dispatch. Runs `gc.collect()` after every draw cycle plus a low-memory watchdog print — CircuitPython's non-compacting GC can fail a large allocation purely from heap fragmentation, independent of total free memory (hit this directly, see history below).
- **`ground/hold_tracker.py`**: `HoldTracker`, the tap/hold button state machine, extracted into its own hardware-free module specifically so it's unit tested (`tests/test_hold_tracker.py`) — importing `ground/code.py` itself in tests is unsafe (its filename `code.py` shadows Python's stdlib `code` module, used by `pdb`, the moment `ground/` is added to `sys.path`). Bridges a release-then-re-press of the same key within `GRACE_MS` as one continuous hold rather than restarting the `HOLD_MS` countdown — a single mechanical/contact bounce mid-hold was otherwise forcing several retries (many real seconds) before a hold ever registered, even though tap (needs only one clean edge) was unaffected.
- **`ground/icons.py`**: battery percent is a 15-point interpolated discharge curve (`BATT_CURVE`), replacing the old 4-bucket linear scheme that put a 95%-charged and a 75%-charged pack in the same bucket.
- **Nix flake** dev environment; `nix run .#test/.#doctor/.#deploy-*`; Makefile deploys via `cp` to CircuitPython volumes (boards labeled LC-ROCKET / LC-GROUND). Any new `ground/*.py` module must be added to `GROUND_FILES` in the Makefile or it silently won't deploy (bit us once with `hold_tracker.py`).

## Confirmed working
Both radios talk (RSSI ~-42, zero rejects), all payload sensors read correct values, handheld displays live telemetry, GPS fixes on both, all three buttons register, buzzer chirps on uplink command, sled fits and transmits from inside the airframe, and **ARM/DISARM confirmed working end-to-end over the real radio link** (hold-to-arm from the handheld, frame-counted confirmation, observed payload state change).

## What's left (the software thread)
- Full end-to-end integration test through an actual/simulated flight (BOOST→COAST→APOGEE→DESCENT→LANDED) — only ARM/DISARM has been exercised on real hardware so far.
- **Residual ARM/DISARM latency**: after the deliberate 2s hold completes, there's still a several-second delay before the command visibly sends. Not yet root-caused — the hold-timer bounce bug (see history) is fixed and shouldn't cause this; `BUTTON EVENT` prints in `ground/code.py` are in place to help isolate it next.
- GPS overland range between handheld and payload is exceptionally large (15-20m even with radios right next to each other) — open, not investigated yet.
- Validate achieved loop rate / logging rate (is IMU_HZ=100 realistic?)
- Replace synthetic D12 flight profile in tests with **real flight-log data** after first flight (regression tests)
- Any UI/screen refinement
- **Before flight (not software):** ground-test protocol — power-duration, shake test, ejection-charge test with sled installed, radio range walk; verify current NAR safety code, motor instructions, launch-site/airspace rules
- **Parallel workstream under consideration, not started:** a full Rust rewrite — primarily to get real RP2040 dual-core support (CircuitPython has none, confirmed via `adafruit/circuitpython#4106`) and move the responsiveness/GC issues above onto a platform that doesn't structurally have them. See `docs/rust-rewrite.md`.

## Known bugs / operational gotchas
- GPS overland range between handheld and payload is exceptionally large (15-20m even with radios right next to each other) — open.
- The residual ARM/DISARM send-latency issue described above — open.
- Button-crash containment: an exception in the button-handling block of `ground/code.py` is caught (try/except, `print(e)`) so it can't kill the main loop, but the original trigger was never confirmed. Plausibly related to the same contact-bounce behavior that caused the hold-timer bug (now fixed) — not verified.
- **Board filesystem corruption**: a board put through many hard resets/power-cycles in a short window (e.g. iterating on `boot.py`) can end up with a corrupted internal FAT filesystem — garbled filenames, volume label falls back to a raw serial number (e.g. `C0E0-F50B`) instead of `LC-ROCKET`/`LC-GROUND`. Reflashing CircuitPython's `.uf2` alone does **not** fix this — it only replaces the interpreter, and won't reformat a filesystem that's damaged but still mountable. Full recovery runbook: `docs/filesystem-recovery.md`.

## Bug investigation log

### 2026-08-13 — button crash + delay, first pass
- Crash: root cause was the button-handling block in `ground/code.py` being the only section of the main loop without try/except — any exception there killed the whole loop. Contained (not root-caused) with try/except + `print(e)`.
- Delay: GPS update time ruled out as the cause (instrumented at a consistent 52-54ms). Hypothesis at the time was that button events weren't firing/propagating at all.

### 2026-08-15/16 — battery charging, hold-timer bug fixed, memory, filesystem corruption
- Battery "won't charge" was initially suspected to be a software voltage-divider bug; traced instead to an inline switch on the battery's positive JST lead being left open during charging — the charge IC's rapid-amber "fault / no battery" blink is correct behavior, not a defect. No code was at fault.
- Root-caused the button-hold delay: `HoldTracker` reset its hold timer on every press edge, including a bounce mid-hold, which could force several retries before a 2s hold ever completed. Fixed with a grace-period bridge (`ground/hold_tracker.py`, `GRACE_MS`) and extracted the class to its own module for unit testing.
- Added `Sensor.CHG` (USB-present bit) and charging indicators on both screens; replaced the ARM/DISARM wall-clock confirm timeout with a payload-frame count (`CMD_CONFIRM_FRAMES`).
- Rocket's `flight.bin` had grown to ~7MB from unconditional per-sample logging during BOOT/IDLE across many bench sessions, eventually causing `Error splicing file: No space left on device` on deploy. Fixed by gating logging to ARMED-onward and clearing the log on an aborted (never-boosted) arm cycle.
- Hit a real `MemoryError` (`memory allocation failed, allocating 9516 bytes`) after the above deploy, from CircuitPython's non-compacting GC fragmenting under the added per-draw allocation churn. Mitigated with periodic `gc.collect()` + a low-memory watchdog print.
- The rocket board's filesystem got corrupted from the repeated hard resets during `boot.py` iteration. A plain CircuitPython `.uf2` reflash did **not** fix it (same garbled files afterward) — recovery required `storage.erase_filesystem()` from the REPL. Runbook: `docs/filesystem-recovery.md`.
- New residual bug surfaced after all of the above: ARM/DISARM works, but there's a multi-second delay between the hold completing and the command actually sending. Not yet root-caused (see "What's left").

---

Your task is to help me develop the software further. Specifically addressing the bugs and helping to make the UI refinements and any other remaining software action items. If asked to work on the Rust rewrite specifically, start from `docs/rust-rewrite.md` rather than re-deriving the rationale here.
