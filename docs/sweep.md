# Buzzer Frequency Sweep 

The payload Feather has a little piezo buzzer soldered across D5/D6, which it uses to "chirp". Piezoelectric buzzers like this use [piezoelectric materials](https://en.wikipedia.org/wiki/Piezoelectricity), which essentially convert electricity to sound. More specifically, they use electric _frequencies_; simply applying a voltage across the material doesn't cause it to make a sound. It's the oscillation that does it.

Because of this, piezoelectric buzzers have a couple of [resonance frequencies](https://en.wikipedia.org/wiki/Resonance), at which they produce higher auditory volumes. The piezo buzzer I am using is a [PS1240](https://www.adafruit.com/product/160) ([specs](PS1240-piezo.pdf)), which indicates that peak volumes with an input frequency around 4500 Hz. But that's just from a random sample, and also it's based on a 3V input.

The reason for putting the buzzer across D5/D6 is I can oscillate the voltage of the pins from -3.3 to +3.3, giving me an actual range of anbout 6.6V across the leads, and we have no data on how loud we can get the buzzer with a 6V square-wave voltage.

To ensure we get the loudest frequency, we'll perform a sweep test with a cell phone SPL meter app. We'll do some coarse measurements to get an idea where the peaks are, and then another pass to refine it.

This is the code:

```python
"""
Finds the loudest drive frequency for THIS specific PS1240 in its mounting.
Piezo resonance is sharp and part-specific; the datasheet nominal (4 kHz) is
a starting point, not the answer.

Wiring: buzzer across D5 and D6 (differential drive, ~6.6 Vp-p).

Method:
  1. Run this with an SPL meter app on your phone at a FIXED distance
     (10 cm to match the datasheet, or 30 cm for convenience -- just keep
     it constant).
  2. Quiet room. Note the dB reading for each frequency as it's announced
     on the serial console.
  3. Find the peak, then narrow with FINE_SWEEP around it.

Differential drive note: two PWM channels, same frequency. CircuitPython
does not guarantee they run 180 deg out of phase, so you may see less than
the theoretical +6 dB over single-ended. Still the right way to wire it.
"""

import time
import board
import pwmio

# --- sweep configuration -----------------------------------------------------

COARSE_START = 2000
COARSE_STOP = 10000
COARSE_STEP = 250

HOLD_S = 3.0     # time to hold each tone (read the meter during this)
GAP_S = 1.0      # silence between tones
DUTY = 32768     # 50% square

# After the coarse sweep finds a peak, set this and re-run to narrow.
FINE = False
FINE_CENTER = 4000
FINE_SPAN = 400
FINE_STEP = 25

# --- drive -------------------------------------------------------------------

hi = pwmio.PWMOut(board.D5, frequency=COARSE_START, duty_cycle=0,
                  variable_frequency=True)
lo = pwmio.PWMOut(board.D6, frequency=COARSE_START, duty_cycle=0,
                  variable_frequency=True)


def tone(freq):
    hi.frequency = freq
    lo.frequency = freq
    hi.duty_cycle = DUTY
    lo.duty_cycle = DUTY


def silence():
    hi.duty_cycle = 0
    lo.duty_cycle = 0


def sweep(start, stop, step):
    print("--- sweep {}-{} Hz, step {} ---".format(start, stop, step))
    print("hold each tone for {:.0f}s, note the dB reading".format(HOLD_S))
    print()
    for freq in range(start, stop + 1, step):
        print(">>> {} Hz".format(freq))
        tone(freq)
        time.sleep(HOLD_S)
        silence()
        time.sleep(GAP_S)
    print()
    print("--- sweep complete ---")


# --- run ---------------------------------------------------------------------

silence()
time.sleep(1.0)

if FINE:
    sweep(FINE_CENTER - FINE_SPAN, FINE_CENTER + FINE_SPAN, FINE_STEP)
else:
    sweep(COARSE_START, COARSE_STOP, COARSE_STEP)

silence()
print("done. Peak frequency goes in BUZZER_HZ.")

while True:
    time.sleep(1)
```

For me, this ended up being about 5250 Hz, which I measured to be around 103 dB a few inches away.

In terms of sound attenuation, the volume of the buzzer at increasing distance  would roughly be:

| Distance (ft) | Decibels | Sounds like   |
|---------------|----------|---------------|
| 0             | 103      | Motorcycle    |
| 10            | 63       | Normal speech |
| 25            | 55       | Light traffic |
| 50            | 49       | Light traffic |
| 100           | 43       | Soft music    |
| 250           | 35       | Whisper       |