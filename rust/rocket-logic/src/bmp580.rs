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

/// OSR_CONFIG (0x36): temperature_oversampling_rate=2x (bits[2:0]=0x01),
/// pressure_oversampling_rate=16x (bits[5:3]=0x04), pressure_enabled=1
/// (bit6) -- same values `adafruit_bmp5xx.BMP5XX.__init__` sets. A pure
/// config register with no side effects while still in standby (the
/// device doesn't start sampling until the mode write below), so unlike
/// ODR_CONFIG the three RWBits/RWBit writes Python does individually
/// collapse safely into one final-value write here.
pub const OSR_CONFIG_BYTE: u8 = 0x61;

/// DSP_IIR (0x31): temperature_iir_filter=COEFF_1 (bits[2:0]=0x01),
/// pressure_iir_filter=COEFF_1 (bits[5:3]=0x01).
pub const DSP_IIR_BYTE: u8 = 0x09;

/// DSP_CONFIG (0x30): pressure_shadow_iir=AFTER_IIR_FILTER (bit5),
/// temperature_shadow_iir=AFTER_IIR_FILTER (bit3), iir_flush_forced=1
/// (bit2). `*_fifo_iir` bits left at their post-reset default (0) --
/// Python's `__init__` doesn't set them either.
pub const DSP_CONFIG_BYTE: u8 = 0x2C;

/// ODR_CONFIG (0x37) has to be written as three separate steps, not one
/// final-value write like the config registers above -- this is the one
/// register where `adafruit_bmp5xx`'s `mode` setter has real, deliberate
/// sequencing (a Bosch-recommended pattern: set the output data rate
/// while still in standby, *then* transition into normal/measuring mode
/// as its own step) rather than converging to the same byte in any
/// order. Ported as the same three writes, not collapsed, specifically
/// to avoid introducing a divergence from known-working behavior that
/// would be unverifiable without hardware.
///
/// Step 1: output_data_rate=ODR_50_HZ (bits[6:2]=0x0F), mode/deep_disabled
/// left at their post-reset default (0/standby) -- matches Python setting
/// `output_data_rate` before ever touching `mode`.
pub const ODR_CONFIG_STEP1_ODR_ONLY: u8 = 0x3C;
/// Step 2: `deep_disabled=1` added (bit7) -- the first half of Python's
/// `mode` setter, unconditional. The setter's `if old_mode != STANDBY`
/// branch (an extra forced-standby step with a 2.5ms settle) is not
/// replicated here: this driver always calls this sequence immediately
/// after a fresh reset, where the mode is already guaranteed STANDBY, so
/// that branch is provably dead code in this driver's one call site --
/// unlike the general-purpose Python library, which can't assume that.
pub const ODR_CONFIG_STEP2_DEEP_DISABLED: u8 = 0xBC;
/// Step 3: `mode=NORMAL` added (bits[1:0]=0x01) -- starts continuous
/// measurement at the configured ODR/OSR.
pub const ODR_CONFIG_STEP3_MODE_NORMAL: u8 = 0xBD;

/// INT_SOURCE (0x15): data_ready_int_en=1 (bit0) only -- fifo/pressure-OOR
/// interrupts explicitly left disabled, matching Python.
pub const INT_SOURCE_BYTE: u8 = 0x01;

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
