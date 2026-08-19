//! Hardware-free half of the BMP580 barometer driver: register map,
//! computed config bytes, and raw-bytes-to-physical-units conversion.
//! No sensor crate exists mature enough to trust for this project's most
//! flight-critical sensor (apogee detection depends entirely on it) --
//! see `docs/rust-rewrite.md`'s ecosystem check -- so this is a from-
//! scratch port, not a wrapper around someone else's driver.
//!
//! Ported directly from the actual CircuitPython driver this project
//! already runs in production, `adafruit_bmp5xx` (installed locally at
//! `~/.local/share/circup/.../lib/adafruit_bmp5xx.py`, MIT, Tim Cocks for
//! Adafruit) -- not from the Bosch datasheet from scratch, so the exact
//! register values/sequencing below are known-working on this exact chip,
//! not a fresh (and unverifiable, without hardware) reading of the spec.
//! The actual I2C transactions live in `rocket/src/bmp580.rs` (needs
//! hardware); this module is everything that doesn't.
//!
//! **2026-08-19, real hardware**: the first version of this driver
//! collapsed several of `adafruit_bmp5xx`'s individual RWBits/RWBit
//! property-setter writes into one precomputed "final byte" write per
//! register, reasoning that every bit not explicitly touched was already
//! 0 after a soft reset. On real hardware that produced a frozen, wrong
//! temperature reading (previously correct under CircuitPython, same
//! physical sensor) -- strong evidence the chip never actually reached
//! continuous measurement mode, i.e. that assumption was wrong somewhere.
//! The config constants below are now `(mask, shifted_value)` pairs for a
//! genuine read-modify-write per field, matching what the Python
//! descriptors actually do on the wire, one call per field in the same
//! order `__init__` makes them -- removing the assumption entirely
//! rather than trying to guess which specific bit was the problem.

/// Default Adafruit I2C address (confirmed against CLAUDE.md).
pub const I2C_ADDR: u8 = 0x47;

pub const REG_CMD: u8 = 0x7E;
pub const REG_CHIP_ID: u8 = 0x01;
pub const REG_STATUS: u8 = 0x28;
pub const REG_INT_STATUS: u8 = 0x27;
pub const REG_TEMP_DATA_XLSB: u8 = 0x1D;
pub const REG_OSR_CONFIG: u8 = 0x36;
pub const REG_ODR_CONFIG: u8 = 0x37;
pub const REG_DSP_IIR: u8 = 0x31;
pub const REG_DSP_CONFIG: u8 = 0x30;
pub const REG_INT_SOURCE: u8 = 0x15;

pub const SOFT_RESET_CMD: u8 = 0xB6;

/// `chip_id` reads one of these for every part in the BMP58x family this
/// driver's register map is valid for (580/581/585) -- matches the
/// Python driver checking the same set, even though this board is
/// specifically a BMP580.
pub const VALID_CHIP_IDS: [u8; 2] = [0x50, 0x51];

// --- STATUS (0x28) bits ----------------------------------------------------
pub const STATUS_NVM_READY_BIT: u8 = 1;
pub const STATUS_NVM_ERR_BIT: u8 = 2;

// --- INT_STATUS (0x27) bits -------------------------------------------------
pub const INT_STATUS_DATA_READY_BIT: u8 = 0;
pub const INT_STATUS_POR_BIT: u8 = 4;

// --- Config bitfields, as (mask, shifted_value) pairs for a genuine
// read-modify-write per field -- one pair per `adafruit_bmp5xx`
// RWBits/RWBit property setter `__init__` touches, in the *same order*.
//
// An earlier version of this driver collapsed several of these into one
// precomputed "final byte" write per register, on the assumption that
// every bit this driver doesn't explicitly set is already 0 after a
// soft reset. On real hardware (2026-08-19) that assumption produced a
// frozen, wrong temperature reading (previously correct under
// CircuitPython on the same physical sensor) -- strong evidence the
// chip wasn't actually reaching continuous measurement mode, i.e. that
// assumption was wrong for at least one bit somewhere in this sequence.
// Switched to genuine RMW, identical in shape to what the Python
// property descriptors actually do on the wire, removing the assumption
// entirely rather than trying to guess which specific bit was the
// problem.

/// OSR_CONFIG (0x36) bitfields.
pub const OSR_TEMP_MASK: u8 = 0b0000_0111; // bits[2:0]
pub const OSR_TEMP_2X: u8 = 0x01;
pub const OSR_PRESS_MASK: u8 = 0b0011_1000; // bits[5:3]
pub const OSR_PRESS_16X_SHIFTED: u8 = 0x04 << 3;
pub const OSR_PRESS_ENABLED_MASK: u8 = 0b0100_0000; // bit6
pub const OSR_PRESS_ENABLED_SHIFTED: u8 = 1 << 6;

/// ODR_CONFIG (0x37) bitfields.
pub const ODR_RATE_MASK: u8 = 0b0111_1100; // bits[6:2]
pub const ODR_RATE_50HZ_SHIFTED: u8 = 0x0F << 2;
pub const ODR_MODE_MASK: u8 = 0b0000_0011; // bits[1:0]
pub const ODR_MODE_STANDBY_SHIFTED: u8 = 0x00;
pub const ODR_MODE_NORMAL_SHIFTED: u8 = 0x01;
pub const ODR_DEEP_DISABLED_MASK: u8 = 0b1000_0000; // bit7
pub const ODR_DEEP_DISABLED_SHIFTED: u8 = 1 << 7;

/// DSP_IIR (0x31) bitfields.
pub const IIR_TEMP_MASK: u8 = 0b0000_0111; // bits[2:0]
pub const IIR_TEMP_COEFF_1: u8 = 0x01;
pub const IIR_PRESS_MASK: u8 = 0b0011_1000; // bits[5:3]
pub const IIR_PRESS_COEFF_1_SHIFTED: u8 = 0x01 << 3;

/// DSP_CONFIG (0x30) bitfields.
pub const DSP_TEMP_SHADOW_MASK: u8 = 0b0000_1000; // bit3
pub const DSP_TEMP_SHADOW_AFTER_SHIFTED: u8 = 1 << 3;
pub const DSP_PRESS_SHADOW_MASK: u8 = 0b0010_0000; // bit5
pub const DSP_PRESS_SHADOW_AFTER_SHIFTED: u8 = 1 << 5;
pub const DSP_IIR_FLUSH_FORCED_MASK: u8 = 0b0000_0100; // bit2
pub const DSP_IIR_FLUSH_FORCED_SHIFTED: u8 = 1 << 2;

/// INT_SOURCE (0x15) bitfields.
pub const INT_SRC_DATA_READY_EN_MASK: u8 = 0b0000_0001; // bit0
pub const INT_SRC_DATA_READY_EN_SHIFTED: u8 = 1;

/// Sign-extend a 24-bit two's-complement value (as stored in the low 24
/// bits of an i32) to a full i32.
fn sign_extend_24(raw: i32) -> i32 {
    if raw & 0x0080_0000 != 0 {
        raw - 0x0100_0000
    } else {
        raw
    }
}

/// Decode one 6-byte burst read from `REG_TEMP_DATA_XLSB` (registers
/// 0x1D..=0x22: temp XLSB/LSB/MSB, then pressure XLSB/LSB/MSB, each a
/// little-endian 24-bit two's-complement value) into `(temp_c,
/// pressure_hpa)`. Matches `adafruit_bmp5xx.BMP5XX.measurements` exactly,
/// including its scale factors -- a single burst read, not two separate
/// register reads, so the pair is guaranteed to come from one consistent
/// sample (BMP58x datasheet sec 4.5.1: data shadowing only guarantees
/// consistency within one burst).
pub fn decode_temp_press(bytes: [u8; 6]) -> (f32, f32) {
    let raw_t = sign_extend_24(bytes[0] as i32 | (bytes[1] as i32) << 8 | (bytes[2] as i32) << 16);
    let raw_p = sign_extend_24(bytes[3] as i32 | (bytes[4] as i32) << 8 | (bytes[5] as i32) << 16);
    let temp_c = raw_t as f32 / 65536.0;
    let pressure_hpa = raw_p as f32 / 64.0 / 100.0;
    (temp_c, pressure_hpa)
}
