//! BMP580 barometer -- the I2C transaction layer. Register map, computed
//! config bytes, and the raw-to-physical-units math all live in
//! `rocket-logic::bmp580` (hardware-free, host-tested); this module is
//! only "make the actual bus calls in the right order."

use embassy_time::{Duration, Instant, Timer};
use embedded_hal::i2c::I2c;
use launchcast_rocket_logic::bmp580::{
    decode_temp_press, DSP_IIR_FLUSH_FORCED_MASK, DSP_IIR_FLUSH_FORCED_SHIFTED, DSP_PRESS_SHADOW_MASK,
    DSP_PRESS_SHADOW_AFTER_SHIFTED, DSP_TEMP_SHADOW_AFTER_SHIFTED, DSP_TEMP_SHADOW_MASK, IIR_PRESS_COEFF_1_SHIFTED,
    IIR_PRESS_MASK, IIR_TEMP_COEFF_1, IIR_TEMP_MASK, INT_SRC_DATA_READY_EN_MASK, INT_SRC_DATA_READY_EN_SHIFTED,
    INT_STATUS_DATA_READY_BIT, INT_STATUS_POR_BIT, I2C_ADDR, ODR_DEEP_DISABLED_MASK, ODR_DEEP_DISABLED_SHIFTED,
    ODR_MODE_MASK, ODR_MODE_NORMAL_SHIFTED, ODR_MODE_STANDBY_SHIFTED, ODR_RATE_50HZ_SHIFTED, ODR_RATE_MASK,
    OSR_PRESS_16X_SHIFTED, OSR_PRESS_ENABLED_MASK, OSR_PRESS_ENABLED_SHIFTED, OSR_PRESS_MASK, OSR_TEMP_2X,
    OSR_TEMP_MASK, REG_CHIP_ID, REG_CMD, REG_DSP_CONFIG, REG_DSP_IIR, REG_INT_SOURCE, REG_INT_STATUS,
    REG_ODR_CONFIG, REG_OSR_CONFIG, REG_STATUS, REG_TEMP_DATA_XLSB, SOFT_RESET_CMD, STATUS_NVM_ERR_BIT,
    STATUS_NVM_READY_BIT, VALID_CHIP_IDS,
};

#[derive(Debug, defmt::Format)]
pub enum Bmp580Error {
    I2c,
    UnexpectedChipId(u8),
    NvmNotReady,
    NvmError,
    PowerOnResetNotConfirmed,
}

pub struct Bmp580<I> {
    i2c: I,
}

impl<I> Bmp580<I>
where
    I: I2c,
{
    fn write_reg(&mut self, reg: u8, value: u8) -> Result<(), Bmp580Error> {
        self.i2c.write(I2C_ADDR, &[reg, value]).map_err(|_| Bmp580Error::I2c)
    }

    fn read_reg(&mut self, reg: u8) -> Result<u8, Bmp580Error> {
        let mut buf = [0u8; 1];
        self.i2c.write_read(I2C_ADDR, &[reg], &mut buf).map_err(|_| Bmp580Error::I2c)?;
        Ok(buf[0])
    }

    /// Read-modify-write one bitfield within a register -- matches what
    /// `adafruit_bmp5xx`'s `RWBits`/`RWBit` property descriptors actually
    /// do on the wire (read current byte, clear just this field's bits,
    /// OR in the new value, write back), not a precomputed "final byte"
    /// for the whole register. See `rocket-logic::bmp580`'s docs on why
    /// that distinction turned out to matter on real hardware.
    fn set_bits(&mut self, reg: u8, mask: u8, shifted_value: u8) -> Result<(), Bmp580Error> {
        let current = self.read_reg(reg)?;
        let new_byte = (current & !mask) | (shifted_value & mask);
        self.write_reg(reg, new_byte)
    }

    /// Reset and configure the sensor. Matches `adafruit_bmp5xx.BMP5XX.__init__`
    /// exactly (see `rocket-logic::bmp580`'s module docs for why this is a
    /// port of that driver, not a fresh datasheet reading) -- blocking,
    /// takes on the order of 15-45ms total (reset settle + first-sample
    /// wait), only ever called once at boot.
    pub async fn new(mut i2c: I) -> Result<Self, Bmp580Error> {
        // -- reset() --------------------------------------------------------
        i2c.write(I2C_ADDR, &[REG_CMD, SOFT_RESET_CMD]).map_err(|_| Bmp580Error::I2c)?;
        Timer::after_millis(12).await;

        let mut this = Bmp580 { i2c };

        let chip_id = this.read_reg(REG_CHIP_ID)?;
        if !VALID_CHIP_IDS.contains(&chip_id) {
            return Err(Bmp580Error::UnexpectedChipId(chip_id));
        }
        let status = this.read_reg(REG_STATUS)?;
        if status & (1 << STATUS_NVM_READY_BIT) == 0 {
            return Err(Bmp580Error::NvmNotReady);
        }
        if status & (1 << STATUS_NVM_ERR_BIT) != 0 {
            return Err(Bmp580Error::NvmError);
        }
        let int_status = this.read_reg(REG_INT_STATUS)?;
        if int_status & (1 << INT_STATUS_POR_BIT) == 0 {
            return Err(Bmp580Error::PowerOnResetNotConfirmed);
        }

        // -- __init__'s extra 2.5ms settle after reset() returns ------------
        Timer::after_micros(2500).await;

        // -- config (device still in standby the whole time): genuine RMW
        // per field, in the exact order adafruit_bmp5xx.BMP5XX.__init__
        // calls the corresponding property setters. -----------------------
        this.set_bits(REG_OSR_CONFIG, OSR_TEMP_MASK, OSR_TEMP_2X)?;
        this.set_bits(REG_OSR_CONFIG, OSR_PRESS_MASK, OSR_PRESS_16X_SHIFTED)?;
        this.set_bits(REG_ODR_CONFIG, ODR_RATE_MASK, ODR_RATE_50HZ_SHIFTED)?;
        this.set_bits(REG_OSR_CONFIG, OSR_PRESS_ENABLED_MASK, OSR_PRESS_ENABLED_SHIFTED)?;

        this.set_bits(REG_DSP_IIR, IIR_TEMP_MASK, IIR_TEMP_COEFF_1)?;
        this.set_bits(REG_DSP_IIR, IIR_PRESS_MASK, IIR_PRESS_COEFF_1_SHIFTED)?;
        this.set_bits(REG_DSP_CONFIG, DSP_TEMP_SHADOW_MASK, DSP_TEMP_SHADOW_AFTER_SHIFTED)?;
        this.set_bits(REG_DSP_CONFIG, DSP_PRESS_SHADOW_MASK, DSP_PRESS_SHADOW_AFTER_SHIFTED)?;
        this.set_bits(REG_DSP_CONFIG, DSP_IIR_FLUSH_FORCED_MASK, DSP_IIR_FLUSH_FORCED_SHIFTED)?;

        // -- mode = NORMAL: matches the `mode` setter's exact logic,
        // including the conditional forced-standby step this driver
        // previously assumed could never fire (it can -- see
        // rocket-logic::bmp580's docs). -------------------------------
        let odr_now = this.read_reg(REG_ODR_CONFIG)?;
        if odr_now & ODR_MODE_MASK != ODR_MODE_STANDBY_SHIFTED {
            this.set_bits(REG_ODR_CONFIG, ODR_DEEP_DISABLED_MASK, ODR_DEEP_DISABLED_SHIFTED)?;
            this.set_bits(REG_ODR_CONFIG, ODR_MODE_MASK, ODR_MODE_STANDBY_SHIFTED)?;
            Timer::after_micros(2500).await;
        }
        this.set_bits(REG_ODR_CONFIG, ODR_DEEP_DISABLED_MASK, ODR_DEEP_DISABLED_SHIFTED)?;
        this.set_bits(REG_ODR_CONFIG, ODR_MODE_MASK, ODR_MODE_NORMAL_SHIFTED)?;

        this.set_bits(REG_INT_SOURCE, INT_SRC_DATA_READY_EN_MASK, INT_SRC_DATA_READY_EN_SHIFTED)?;

        // -- _wait_first_data(timeout=0.03) ----------------------------------
        let deadline = Instant::now() + Duration::from_millis(30);
        while Instant::now() < deadline {
            let int_status = this.read_reg(REG_INT_STATUS)?;
            if int_status & (1 << INT_STATUS_DATA_READY_BIT) != 0 {
                break;
            }
            Timer::after_millis(1).await;
        }

        Ok(this)
    }

    /// `(temp_c, pressure_hpa)` from one consistent burst read. Matches
    /// `adafruit_bmp5xx.BMP5XX.measurements`.
    pub fn measurements(&mut self) -> Result<(f32, f32), Bmp580Error> {
        let mut buf = [0u8; 6];
        self.i2c
            .write_read(I2C_ADDR, &[REG_TEMP_DATA_XLSB], &mut buf)
            .map_err(|_| Bmp580Error::I2c)?;
        Ok(decode_temp_press(buf))
    }
}
