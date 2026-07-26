# Wiring Guide

The Feather is based on the "32-bit dual-core ARM Cortex-M0+ microcontroller designed by Raspberry Pi Ltd. In January 2021, it was released as part of the Raspberry Pi Pico board." [[1](https://en.wikipedia.org/wiki/RP2040)]. The pinout is this:

![image](./images/feather%20pinout.png)

Use this as a reference for wiring the payload and handheld units.

## Payload

Wiring the payload is pretty simple. The modules for the GPS, motion and environment daisy-chain via 4-pin JST SH. The only thing I put on here was a small piezo buzzer across D5/D6, and a spring antenna on the ANT pad.

I also made up a quick and dirty voltage divider with two 100 kΩ resistors:

![image](./images/voltage%20divider.png)

I measured the resistor to be 99.9 kΩ, right is 97.2 kΩ, it’s 198.4 kΩ across both. The joined leads were soldered to A0, and the other ends are soldered to BAT and GND, respectively.

The daisy-chain is Feather -> BMP580 -> LSM6DSOX/LIS3MDL -> PA1010D GPS. It's easy enough to put together. In the REPL, you can confirm the I²C addresses of the components:

```
import board
i2c = board.STEMMA_I2C()
i2c.try_lock()
print([hex(a) for a in i2c.scan()])
['0x10', '0x1c', '0x47', '0x6a']
i2c.unlock()
```

## Handheld

Wiring the handheld is a bit more complicated due to the screen. I soldered headers to this one because space isn't really a concern. 

PA1010D GPS is connected to the Feather via Qwiic JST SH 4-pin, same as the payload. I also soldered a small spring antenna to the ANT pad.

Also like the payload, I made a voltage divider (measured at 99.9 kΩ and 99.6 kΩ, and 198.6 kΩ across both), and cut some female-female jumper wires in half and soldered them to the ends. I also used some heat shrink tubes. That way I could plug it into the headers:

![image](./images/voltage%20divider%20-%20handheld.png)

Just like the payload, the ends connect to BAT and GND, and the joined end connects to A0.

Wiring the screen takes 5 wires:

| Adafruit SHARP Memory Display Breakout | Wire Color | Adafruit Feather RP2040 Pin |
|----------------------------------------|------------|-----------------------------|
| 3x3                                    | Red        | 3.3V                        |
| GND                                    | Black      | GND                         |
| CLK                                    | Green      | SCK                         |
| DI                                     | Orange     | MOSI                        |
| CS                                     | Yellow     | D6                          |