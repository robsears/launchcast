"""Stub out CircuitPython hardware modules so firmware imports under CPython.

The flight firmware imports `board`, `busio`, `digitalio`, `pwmio`, `analogio`,
`neopixel`, and `microcontroller` at module scope. None exist off-board. These
stubs let the pure-logic classes -- FlightState in particular -- be imported and
tested without hardware.

Nothing here simulates hardware behavior. Any test that would need a real
peripheral belongs on the bench, not in CI.
"""

import sys
import types
import os

# The board has a flat filesystem: code.py and packet.py sit side by side,
# so the firmware does `import packet`. Make that resolve in CI too, rather
# than rewriting imports at deploy time.
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "common"))


def _stub(name, **attrs):
    mod = types.ModuleType(name)
    for key, value in attrs.items():
        setattr(mod, key, value)
    sys.modules[name] = mod
    return mod


class _Pin:
    """Placeholder for a board pin object."""

    def __init__(self, name):
        self.name = name

    def __repr__(self):
        return "<Pin {}>".format(self.name)


class _AnyAttr(types.ModuleType):
    """Module that yields a _Pin for any attribute access.

    `board` exposes a different pin set on every port. Rather than enumerate
    them, hand back a plausible object for whatever the firmware asks for.
    """

    def __getattr__(self, name):
        pin = _Pin(name)
        setattr(self, name, pin)
        return pin


def _install():
    board = _AnyAttr("board")
    board.STEMMA_I2C = lambda: None
    board.SPI = lambda: None
    sys.modules["board"] = board

    _stub("busio", I2C=lambda *a, **k: None, SPI=lambda *a, **k: None)

    class _DigitalInOut:
        def __init__(self, pin):
            self.pin = pin
            self.value = True
            self.direction = None
            self.pull = None

    _stub(
        "digitalio",
        DigitalInOut=_DigitalInOut,
        Direction=types.SimpleNamespace(INPUT="in", OUTPUT="out"),
        Pull=types.SimpleNamespace(UP="up", DOWN="down"),
    )

    class _PWMOut:
        def __init__(self, pin, frequency=0, duty_cycle=0, variable_frequency=False):
            self.pin = pin
            self.frequency = frequency
            self.duty_cycle = duty_cycle

    _stub("pwmio", PWMOut=_PWMOut)

    class _AnalogIn:
        def __init__(self, pin):
            self.pin = pin
            self.value = 32768

    _stub("analogio", AnalogIn=_AnalogIn)

    class _NeoPixel:
        def __init__(self, pin, n, brightness=1.0, pixel_order=None):
            self._px = [(0, 0, 0)] * n

        def __setitem__(self, i, v):
            self._px[i] = v

        def __getitem__(self, i):
            return self._px[i]

    _stub("neopixel", NeoPixel=_NeoPixel, GRB="GRB", RGB="RGB")

    _stub("microcontroller", reset=lambda: None)


_install()