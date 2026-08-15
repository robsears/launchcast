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

# Handwaved 1S LiPo rest-voltage bands. These packs don't discharge
# linearly, so this is "close enough to fly by," not a fuel gauge.
BATT_THRESHOLDS = (4.05, 3.95, 3.80, 3.65)  # volts at/above -> level 4..1; below all -> 0


def battery_level(volts):
    """Bucket a battery voltage into a 0-4 fill level. None -> 0."""
    if volts is None:
        return 0
    for i, threshold in enumerate(BATT_THRESHOLDS):
        if volts >= threshold:
            return 4 - i
    return 0


def draw_battery(display, x, y, volts, color=0):
    display.rect(x, y, BATT_W, BATT_H, color)
    display.fill_rect(x + BATT_W, y + 3, 2, BATT_H - 6, color)  # nub
    level = battery_level(volts)
    seg_w = (BATT_W - 4) // 4
    for i in range(level):
        display.fill_rect(x + 2 + i * seg_w, y + 2, seg_w - 1, BATT_H - 4, color)


def battery_percent(volts):
    """Same 0-4 bucket as the bar icon, just as a percentage for tables/text."""
    return battery_level(volts) * 25
