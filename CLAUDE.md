# LaunchCast — Project Context for Software Thread

## What it is
A LoRa telemetry system for a model rocket: a **payload** that flies inside the rocket and broadcasts sensor data, and a **handheld ground station** that receives and displays it, with an uplink for commands. Repo: "LaunchCast," Apache-2.0. Language: **CircuitPython** on both boards. The hardware is **done and validated**; remaining work is firmware/software.

## Hardware Stack 1 — Payload (flies in the rocket)
- **Feather RP2040 RFM95** (915 MHz LoRa) — flight computer + radio
- **BMP580** barometer, **LSM6DSOX+LIS3MDL** 9-DoF IMU, **PA1010D** GPS (all I²C via STEMMA QT)
- **PS1240 piezo buzzer** on D5/D6 (differential drive), peak resonance **5250 Hz** (measured)
- Onboard NeoPixel (status), **500mAh LiPo** through a slide switch, external **2:1 voltage divider on A0** for battery sense
- Spring antenna soldered to ANT pad
- Housed in a **3D-printed PETG sled**, 54.7g fully wired, fits the BT-65 payload tube; flight motor Estes D12-5, 57g payload ceiling
- **I²C addresses:** GPS 0x10, LIS3MDL 0x1c, BMP580 0x47, LSM6DSOX 0x6a
- **Radio pins:** `board.RFM_CS`, `RFM_RST`, `RFM_IO0-5`. `board.STEMMA_I2C()` exists.

## Hardware Stack 2 — Handheld (ground station)
- **Feather RP2040 RFM95** (identical radio config)
- **Sharp Memory Display 2.7" 400×240** over SPI, **CS on D6** (needs `font5x8.bin` on board)
- Second **PA1010D GPS** (for computing distance to rocket)
- **2500mAh LiPo** + slide switch, USB-C extension for charge/program without disassembly
- **Three buttons**, active-low w/ internal pull-ups, wired GPIO→button→shared GND:
  - **D9 = ARM/DISARM** (2s hold), **D10 = CHIRP** (tap), **D11 = MENU** (tap)
- Spring antenna, 3D-printed enclosure with labeled buttons

## Firmware (current state)
- **`common/packet.py`** (shared, symlinked to both): 40-byte telemetry downlink, 7-byte command uplink, MAGIC=0xA5, SYNC_WORD=0x2B. Has a Sensor bitfield (BARO/IMU/MAG/GPS/LOG/BATT). 100 pytest tests passing in CI.
- **`rocket/code.py`**: flight state machine (BOOT→IDLE→ARMED→BOOST→COAST→APOGEE→DESCENT→LANDED). Apogee detected from **barometric velocity**, not accel. ARM via uplink only (captures ground-pressure reference). Logs to `flight.bin` (binary). Buzzer three-chirp beacon in LANDED. Radio SF7/BW125/CR4-5.
- **`ground/code.py`**: timer-driven display (services Sharp VCOM), three screens (FLIGHT/RECOVERY/DIAG cycled by MENU), latched last-known rocket GPS fix for recovery, ARM/DISARM with 1-2s confirmation against payload's reported state, CHIRP fire-and-forget.
- **Nix flake** dev environment; `nix run .#test/.#doctor/.#deploy-*`; Makefile deploys via `cp` to CircuitPython volumes (boards labeled LC-ROCKET / LC-GROUND).

## Confirmed working
Both radios talk (RSSI ~-42, zero rejects), all payload sensors read correct values, handheld displays live telemetry, GPS fixes on both, all three buttons register, buzzer chirps on uplink command, sled fits and transmits from inside the airframe.

## What's left (the software thread)
- Full end-to-end integration test on real hardware (flight state machine through a real/simulated flight, three-screen UI driven by buttons, uplink from real button presses)
- Validate achieved loop rate / logging rate (is IMU_HZ=100 realistic?)
- Replace synthetic D12 flight profile in tests with **real flight-log data** after first flight (regression tests)
- Any UI/screen refinement
- **Before flight (not software):** ground-test protocol — power-duration, shake test, ejection-charge test with sled installed, radio range walk; verify current NAR safety code, motor instructions, launch-site/airspace rules

## Known bugs
- Button presses do not register reliably. Buttons must be held down for up to several seconds before registering.
- Button presses crash the program loop.
- GPS overland range between handheld and payload is exceptionally large (estimate of 15-20m when the radios are right next to each other)

## Active bugs (as of 2026-08-13)

### Button press crash (partially fixed)
- Root cause: the button-handling block in ground/code.py was the
  only section in the main loop without try/except — any exception
  there killed the whole `while True` loop.
- Fix applied: wrapped it in try/except with `print(e)` on failure.
  This contains the crash but does NOT fix the underlying trigger.
- Still unknown: what exception was actually firing. Next debug
  step: capture the printed exception type/message when it recurs.

### Button press delay (not yet root-caused)
- Symptom: presses take multiple seconds to register.
- Ruled out: GPS update time — instrumented, consistently 52-54ms,
  not the bottleneck.
- Current hypothesis being tested: instrumenting BUTTON EVENT prints
  to check whether Button.update() is firing at all vs. firing but
  not propagating (see git log / this file's history for the
  isolation steps already tried, so you don't repeat them).

---

Your task is to help me develop the software further. Specifically addressing the bugs and helping to make the UI refinements and any other remaining software action items.