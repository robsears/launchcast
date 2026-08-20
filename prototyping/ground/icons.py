"""Small monochrome status icons for the ground station's Sharp Memory Display.

adafruit_framebuf's blit() is an unimplemented stub, so there is no sprite
compositing to lean on -- icons are drawn a pixel (or a scaled block) at a
time via display.pixel()/fill_rect(). Bucketing logic (signal_level,
battery_level) takes plain numbers, not a display, so it can be unit tested
off-board.
"""
PIXELS_X, PIXELS_Y = 400, 240

def draw_bitmap(display, x, y, rows, color=0, scale=1):
    """Draw a monochrome bitmap.

    rows: sequence of equal-length '0'/'1' strings, one per pixel row.
    The leftmost character of each string is the leftmost pixel (MSB-first).
    """
    for j, row in enumerate(rows):
        for i, bit in enumerate(row):
            if bit == "1":
                if scale == 1:
                    display.pixel(x + i, y + j, color)
                else:
                    display.fill_rect(x + i * scale, y + j * scale, scale, scale, color)


# --- Handheld (ground) glyph ------------------------------------------------
GROUND_W, GROUND_H = 10, 10
GROUND_BITMAP = (
    "0000110000",
    "0001001000",
    "0010000100",
    "0101111010",
    "1010000101",
    "1010000101",
    "0101111010",
    "0010000100",
    "0001001000",
    "0000110000",
)

def draw_ground(display, x, y, scale=1):
    draw_bitmap(display, x, y, GROUND_BITMAP, scale=scale)

# --- Rocket (payload) glyph -------------------------------------------------

ROCKET_W, ROCKET_H = 8, 10

ROCKET_BITMAP = (
    "00011000",
    "00111100",
    "00111100",
    "00111100",
    "00111100",
    "00111100",
    "01111110",
    "11111111",
    "11111111",
    "11111111",
)


def draw_rocket(display, x, y, scale=1):
    draw_bitmap(display, x, y, ROCKET_BITMAP, scale=scale)


# --- Signal bars, cell-phone style -------------------------------------------

SIGNAL_W, SIGNAL_H = 8, 5

SIGNAL_BARS = {
    4: (
        "00000011",
        "00001111",
        "00111111",
        "11111111",
        "11111111",
    ),
    3: (
        "00000000",
        "00001100",
        "00111100",
        "11111100",
        "11111111",
    ),
    2: (
        "00000000",
        "00000000",
        "00110000",
        "11110000",
        "11111111",
    ),
    1: (
        "00000000",
        "00000000",
        "00000000",
        "11000000",
        "11111111",
    ),
    0: (
        "00000000",
        "00000000",
        "00000000",
        "00000000",
        "11111111",
    ),
}


def signal_level(rssi):
    """Bucket an RSSI reading (dBm) into a 0-4 bar count. None -> 0."""
    if rssi is None:
        return 0
    if rssi >= -50:
        return 4   
    if rssi >= -70:
        return 3
    if rssi >= -90:
        return 2
    if rssi >= -110:
        return 1
    return 0


def draw_signal(display, x, y, rssi, scale=1):
    draw_bitmap(display, x, y, SIGNAL_BARS[signal_level(rssi)], scale=scale)


def signal_percent(rssi):
    """Same 0-4 bucket as the bar icon, just as a percentage for tables/text."""
    return signal_level(rssi) * 25


# --- Battery -------------------------------------------------------------

BATT_W, BATT_H = 20, 10

# 1S LiPo rest-voltage discharge curve, highest voltage first. LiPo voltage
# sags fast in the last ~20% and stays nearly flat across the top ~30%, so a
# straight-line (or 4-bucket) volts-to-percent mapping put a 95%-charged pack
# and a 75%-charged pack in the same bucket. Interpolated linearly between
# these anchor points instead. The 3.30 V/0% anchor is an assumed empty-pack
# cutoff (not one of the measured points) -- adjust if flight data disagrees.
BATT_CURVE = (
    (4.20, 100),
    (4.18, 95),
    (4.10, 90),
    (4.05, 85),
    (4.00, 80),
    (3.95, 75),
    (3.90, 70),
    (3.85, 65),
    (3.82, 60),
    (3.78, 50),
    (3.72, 40),
    (3.68, 30),
    (3.60, 20),
    (3.50, 10),
    (3.30, 0),
)


def battery_percent(volts):
    """Interpolate a rest voltage onto the discharge curve. None -> 0."""
    if volts is None:
        return 0
    if volts >= BATT_CURVE[0][0]:
        return BATT_CURVE[0][1]
    if volts <= BATT_CURVE[-1][0]:
        return BATT_CURVE[-1][1]
    for (v_hi, p_hi), (v_lo, p_lo) in zip(BATT_CURVE, BATT_CURVE[1:]):
        if volts >= v_lo:
            frac = (volts - v_lo) / (v_hi - v_lo)
            return round(p_lo + frac * (p_hi - p_lo))
    return 0  # unreachable -- range is fully covered by the checks above


def battery_level(volts):
    """Bucket battery percent into a 0-4 fill level for the bar icon."""
    return min(4, battery_percent(volts) // 25)


def draw_battery(display, x, y, volts, color=0):
    display.rect(x, y, BATT_W, BATT_H, color)
    display.fill_rect(x + BATT_W, y + 3, 2, BATT_H - 6, color)  # nub
    level = battery_level(volts)
    seg_w = (BATT_W - 4) // 4
    for i in range(level):
        display.fill_rect(x + 2 + i * seg_w, y + 2, seg_w - 1, BATT_H - 4, color)


# --- Charging bolt ------------------------------------------------------------

BOLT_W, BOLT_H = 6, 10

BOLT_BITMAP = (
    "001100",
    "001100",
    "011000",
    "011000",
    "111111",
    "001110",
    "000110",
    "001100",
    "001100",
    "011000",
)


def draw_bolt(display, x, y, scale=1):
    draw_bitmap(display, x, y, BOLT_BITMAP, scale=scale)
