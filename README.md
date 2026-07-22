# LaunchCast

**Live telemetry from launchpad to landing.**

A LoRa telemetry payload for model rockets, with a handheld ground station. Built around the Adafruit Feather RP2040 RFM95.

## Hardware

### Rocket payload (~44 g)

| Role | Part | PID |
|---|---|---|
| Flight computer + LoRa | Feather RP2040 RFM95 | 5714 |
| Barometer | BMP580 | 6411 |
| 9-DoF IMU | LSM6DSOX + LIS3MDL | 4517 |
| GPS | Mini GPS PA1010D | 4415 |
| Power | 500 mAh LiPo | 1578 |
| Recovery aid | PS1240 piezo | 160 |
| Antenna | 915 MHz spring | 4269 |
| Structure | 3D-printed PETG sled | — |

### Handheld ground station

| Role | Part | PID |
|---|---|---|
| Receiver | Feather RP2040 RFM95 | 5714 |
| Display | Sharp Memory 2.7" 400×240 | 4694 |
| GPS | Mini GPS PA1010D | 4415 |
| Power | 2500 mAh LiPo | 328 |

All sensors chain on a single I²C bus via STEMMA QT. Only the Feathers require soldering.

---

## Development

**Language:** CircuitPython.

## License

Apache License 2.0. See `LICENSE`.
