# LaunchCast

**Live telemetry from launchpad to landing.**

A LoRa telemetry payload for model rockets, with a handheld ground station. Built around the Adafruit Feather RP2040 RFM95.

## Hardware

### Rocket payload (~44 g)

| Role | Part | Price (USD) |
|---|---|---|
| Flight computer + LoRa | [Feather RP2040 RFM95](https://www.adafruit.com/product/5714) |  $29.95 |
| GPS | [Mini GPS PA1010D](https://www.adafruit.com/product/4415) | $29.95 |
| 9-DoF IMU | [LSM6DSOX + LIS3MDL](https://www.adafruit.com/product/4517) | $19.95 |
| Barometer | [BMP580](https://www.adafruit.com/product/6411) | $7.95 |
| Power | [3.7v 500 mAh LiPo](https://www.adafruit.com/product/1578) | $7.95 |
| Recovery aid | [PS1240 piezo](https://www.adafruit.com/product/160) | $1.50 |
| Antenna | [915 MHz spring](https://www.adafruit.com/product/4269) | $0.95 |
| Total | - | **$97.75** |

### Handheld ground station

| Role | Part | Price (USD) |
|---|---|---|
| Display | [Sharp Memory 2.7" 400×240](https://www.adafruit.com/product/4694) | $44.95 |
| Flight computer + LoRa | [Feather RP2040 RFM95](https://www.adafruit.com/product/5714) |  $29.95 |
| GPS | [Mini GPS PA1010D](https://www.adafruit.com/product/4415) | $29.95 |
| Power | [3.7v 2500 mAh LiPo](https://www.adafruit.com/product/5714) | $14.95 |
| Tactile Inputs | [Universal Proto-board PCBs 2cm x 8cm - 3 Pack](https://www.adafruit.com/product/4783)<br />[Colorful Round Tactile Button Switch Assortment - 15 pack](https://www.adafruit.com/product/1009) | $8.20|
| Antenna | [SMA to uFL/u.FL/IPX/IPEX RF Adapter Cable](https://www.adafruit.com/product/851)<br />[Mini GSM/Cellular Quad-Band Antenna - 2dBi SMA Plug](https://www.adafruit.com/product/1859) | $9.90 |
| Total | - | **$137.90** |

All sensors chain on a single I²C bus via STEMMA QT. Only the Feathers require soldering.

---

## Software

I run on NixOS and make heavy use of Nix flakes. This, plus direnv, ensures that any Nix user who clones this repo will have an identical workspace with all necessary tools. This is especially nice because you won't need to deal with Python or Conda, or polluting other environments. Everything you need is symlinked in .

Go set up Nix, and then you can get a local dev environment by running:

```
nix develop
```

Nix is also great because we can run tests and `make` scripts through it as well, ensuring the build environment also has everything it needs to do the work:

```
nix run .#test  
python -m pytest tests/ -q
....................................................................................................                                                                                                                                                                    [100%]
100 passed in 0.05s
```

Check out the `apps` section of `nix flake show` to see other `nix run .#` actions:

```
├───apps
│   ├───aarch64-darwin omitted (use '--all-systems' to show)
│   ├───aarch64-linux omitted (use '--all-systems' to show)
│   └───x86_64-linux
│       ├───check: app
│       ├───default: app
│       ├───deploy-ground: app
│       ├───deploy-rocket: app
│       ├───doctor: app
│       ├───libs-ground: app
│       ├───libs-rocket: app
│       ├───lint: app
│       ├───monitor: app
│       ├───pull-log: app
│       ├───test: app
│       └───volumes: app
```


**Language:** CircuitPython.

## Repository layout

```
launchcast/
├── common/
│   └── packet.py       # wire format -- single source of truth
├── rocket/
│   ├── code.py         # flight firmware
│   └── boot.py         # filesystem remount + volume label
├── ground/
│   ├── code.py         # handheld firmware
│   └── boot.py         # volume label
├── tests/
│   ├── test_packet.py
│   └── test_flight_state.py
├── conftest.py         # CircuitPython hardware stubs
├── docs/wiring/        # Fritzing sketches
├── nix/                # devshell, apps, shared package set
└── Makefile            # test, lint, deploy, pull flight logs
```

`common/packet.py` is copied to both boards at deploy time rather than
imported as a package. CircuitPython has a flat filesystem — `code.py` and
`packet.py` sit side by side on the device — so the firmware does a bare
`import packet`. `conftest.py` puts `common/` on `sys.path` so the same
import resolves in CI. Change the format string once; both sides stay in
sync because there is only one file.

The two boards are identical hardware running different firmware. `boot.py`
labels each volume (`LC-ROCKET` / `LC-GROUND`) so `make deploy-rocket`
cannot target the handheld when both are plugged in.

---

## Tests

```
nix run .#test          # or: make test
nix run .#check         # test + lint
```

100 tests, ~0.15 s. Two files, testing very different things.

**`test_packet.py`** covers the wire format. Sizes are contractual — 40 bytes
downlink, 7 up — and a change that breaks a round trip breaks both boards
silently. It also checks the things that only matter when something has gone
wrong: frames from stuck-high or stuck-low lines, wrong packet types, values
that saturate rather than wrapping. A clipped accelerometer reading must not
change sign, because that would look like the rocket reversed direction.

**`test_flight_state.py`** covers the state machine, which is the logic that
runs exactly once per flight, irreversibly, a few hundred meters up. It
synthesizes a pressure and acceleration profile for a plausible D12-5 flight,
feeds it through `FlightState` at the real sample rate, and asserts both the
state sequence and the transition timings. Boost must fire shortly after
ignition but not before `BOOST_MIN_MS`; apogee must land near the true peak;
states must never regress.

The pair worth understanding is `test_a_brief_bump_does_not_trigger_boost`
and `test_a_sustained_bump_does_trigger_boost`. They bracket `BOOST_MIN_MS`
from both sides — too lax and handling the rocket on the pad starts the
flight, too aggressive and a real launch is missed.

The firmware imports `board`, `busio`, `digitalio` and friends at module
scope, none of which exist off-device. `conftest.py` stubs them. Nothing
there simulates hardware behavior — anything needing a real peripheral is a
bench test, not a CI test.

`d12_profile` is a shape, not a simulation: 1.6 s burn, ballistic coast,
5 m/s descent. It exists so the state machine sees realistic transitions.
After the first flight it should be replaced with real log data, at which
point these become regression tests against measured behavior.

## License

Apache License 2.0. See `LICENSE`.
