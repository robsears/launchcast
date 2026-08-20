"""Shared display helper for the ground station's screen modules."""


def text(display, x, y, s, size=1, color=0):
    try:
        display.text(s, x, y, color, size=size)
    except TypeError:
        display.text(s, x, y, color)
