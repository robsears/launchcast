//! LaunchCast rocket payload firmware -- Rust rewrite.
//!
//! Dual-core split (see `docs/rust-rewrite.md`'s Strawman architecture
//! for the full reasoning, including the flash-write/multicore-pause
//! finding that shaped it):
//!   - **core0**: radio (RX commands, TX telemetry -- phase-agnostic,
//!     see `link.rs`'s docs) + the raw-partition flash log's actual
//!     erase/program calls (forced there -- `embassy-rp`'s flash driver
//!     only allows those from core0).
//!   - **core1**: the shared I2C bus (BMP580 + LSM6DSOX + LIS3MDL + GPS,
//!     all on `STEMMA_I2C`) + the flight-state machine + a RAM ring
//!     buffer of pending log entries + buzzer/NeoPixel.
//!
//! Cross-core: `link::TELEMETRY` (core1 -> core0, latest computed
//! telemetry), `link::COMMANDS` (core0 -> core1, replay-checked command
//! bytes), `flash_log::FLUSH_REQUESTS`/`flash_log::ARM_CYCLE_EVENTS`
//! (core1 -> core0, log-buffer handoff and arm-cycle lifecycle).

#![no_std]
#![no_main]

mod battery;
mod bmp580;
mod buzzer;
mod flash_log;
mod gps;
mod i2c_bus;
mod imu;
mod link;
mod lis3mdl;
mod pixel;
mod radio;

use core::cell::RefCell;
use core::ptr::addr_of_mut;

use cortex_m_rt::entry;
use defmt_rtt as _;
use embassy_executor::Executor;
use embassy_rp::adc::{Adc, Channel as AdcChannel, Config as AdcConfig};
use embassy_rp::bind_interrupts;
use embassy_rp::config::Config;
use embassy_rp::dma::InterruptHandler as DmaInterruptHandler;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::i2c::{Config as I2cConfig, I2c};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler as PioInterruptHandler, Pio};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_rp::spi::{Config as SpiConfig, Spi};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_hal_bus::i2c::RefCellDevice;
use launchcast_common::{self as common, Command, Sensor, State};
use launchcast_rocket_logic::flash_log::LogEntry;
use launchcast_rocket_logic::{accel_magnitude, FlightState};
use panic_probe as _;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => PioInterruptHandler<PIO0>;
    DMA_IRQ_0 => DmaInterruptHandler<embassy_rp::peripherals::DMA_CH0>, DmaInterruptHandler<embassy_rp::peripherals::DMA_CH1>, DmaInterruptHandler<embassy_rp::peripherals::DMA_CH2>;
});

/// See `ground/src/main.rs`'s docs on why this had to grow to 128KB there
/// (a stack-overflow bug traced to a large object moving through the
/// spawn closure). Nothing this board's core1 owns is comparably large
/// (no display framebuffer) -- 64KB is a reasoned starting point, not a
/// value confirmed against real hardware yet.
static mut CORE1_STACK: Stack<65536> = Stack::new();
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static I2C_BUS: StaticCell<RefCell<I2c<'static, embassy_rp::peripherals::I2C1, embassy_rp::i2c::Blocking>>> = StaticCell::new();

/// Matches `code.py`'s `CHIRP_MS`.
const CHIRP_MS: u32 = 1000;
/// Matches `code.py`'s 600ms steady tone on an ARM refusal.
const ARM_REFUSED_CHIRP_MS: u32 = 600;
/// Matches `code.py`'s `TX_HZ_FLIGHT`/`TX_HZ_IDLE`.
const TX_PERIOD_FLIGHT_MS: u32 = 500;
const TX_PERIOD_IDLE_MS: u32 = 2000;

fn in_flight(state: u8) -> bool {
    matches!(state, State::ARMED | State::BOOST | State::COAST | State::APOGEE | State::DESCENT)
}

/// Phase-dependent IMU (accel/gyro) poll period. Fast from ARMED onward
/// (launch detection needs it in ARMED; BOOST onward needs it for
/// logging) -- see `docs/rust-rewrite.md`'s Strawman architecture table.
fn imu_period_ms(state: u8) -> u32 {
    match state {
        State::ARMED | State::BOOST | State::COAST | State::APOGEE | State::DESCENT => 10, // ~100Hz, matches code.py's IMU_HZ
        _ => 15_000,                                                                       // BOOT, IDLE, LANDED
    }
}

/// Phase-dependent barometer poll period. Only fast once there's
/// altitude to actually track (BOOST onward) -- ARMED itself only needs
/// a reasonably fresh pressure reading to capture as the ground
/// reference at the moment ARM is confirmed, not a tight sample rate.
fn baro_period_ms(state: u8) -> u32 {
    match state {
        State::BOOST | State::COAST | State::APOGEE | State::DESCENT => 40, // ~25Hz, matches code.py's BARO_HZ
        _ => 15_000,                                                        // BOOT, IDLE, ARMED, LANDED
    }
}

/// Phase-dependent GPS poll period. `None` = paused entirely -- not
/// useful mid-flight (nothing reads it until recovery), and pausing
/// frees I2C bus time for the sensors that matter during that window.
fn gps_period_ms(state: u8) -> Option<u32> {
    match state {
        State::BOOT | State::IDLE => Some(15_000),
        State::LANDED => Some(1_000), // matches code.py's GPS_HZ -- recovery wants fresh position
        _ => None,
    }
}

#[entry]
fn main() -> ! {
    let p = embassy_rp::init(Config::default());
    defmt::info!("launchcast-rocket: boot ok, spawning core1 (sensors + flight state)");

    spawn_core1(
        p.CORE1,
        unsafe { &mut *addr_of_mut!(CORE1_STACK) },
        move || {
            // Shared I2C bus -- BMP580(0x47)/LSM6DSOX(0x6a)/LIS3MDL(0x1c)/
            // GPS(0x10) all on STEMMA_I2C (confirmed via CLAUDE.md's
            // address list). SDA=GPIO2/SCL=GPIO3 map to I2C1 on this
            // chip (same confirmation method used for the ground
            // station: the compiler's own `SclPin`/`SdaPin` trait bounds).
            let i2c = I2c::new_blocking(p.I2C1, p.PIN_3, p.PIN_2, I2cConfig::default());
            let i2c_bus = I2C_BUS.init(RefCell::new(i2c));

            // Buzzer -- differential drive across D5/D6 (GPIO5/GPIO6).
            let buzz = buzzer::Buzzer::new(p.PWM_SLICE3, p.PIN_6, p.PWM_SLICE2, p.PIN_5);

            // Status NeoPixel -- PIO0, GPIO4.
            let Pio { mut common, sm0, .. } = Pio::new(p.PIO0, Irqs);
            let ws2812_program = PioWs2812Program::new(&mut common);
            let pixel_driver: PioWs2812<'static, PIO0, 0, 1, embassy_rp::pio_programs::ws2812::Grb> =
                PioWs2812::new(&mut common, sm0, p.DMA_CH2, Irqs, p.PIN_4, &ws2812_program);

            // Battery -- A0/GPIO26/ADC0, same external 2:1 divider as the
            // ground station's own battery sense.
            let adc = Adc::new_blocking(p.ADC, AdcConfig::default());
            let adc_channel = AdcChannel::new_pin(p.PIN_26, Pull::None);

            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                spawner.spawn(gps::gps_task(RefCellDevice::new(i2c_bus)).unwrap());
                spawner.spawn(
                    flight_task(
                        RefCellDevice::new(i2c_bus),
                        RefCellDevice::new(i2c_bus),
                        RefCellDevice::new(i2c_bus),
                        adc,
                        adc_channel,
                        buzz,
                        pixel_driver,
                    )
                    .unwrap(),
                );
            });
        },
    );

    // Radio gets exclusive ownership of hardware SPI1 -- this board has
    // no display to contend with it (unlike the ground station), so no
    // PIO-SPI workaround is needed here at all.
    let mut radio_spi_config = SpiConfig::default();
    radio_spi_config.frequency = 1_000_000;
    let radio_spi = Spi::new(p.SPI1, p.PIN_14, p.PIN_15, p.PIN_8, p.DMA_CH0, p.DMA_CH1, Irqs, radio_spi_config);
    let radio_cs = Output::new(p.PIN_16, Level::High);
    let radio_spi_device = radio::RadioSpiDevice::new(radio_spi, radio_cs, Delay).unwrap();
    let radio_reset = Output::new(p.PIN_17, Level::High);
    let radio_dio0 = Input::new(p.PIN_21, Pull::None);
    let radio_dio1 = Input::new(p.PIN_22, Pull::None);
    // Onboard LED (D13/GPIO13) -- same no-debug-probe diagnostic role as
    // the ground station's own `radio_led`.
    let led = Output::new(p.PIN_13, Level::Low);

    let executor0 = EXECUTOR0.init(Executor::new());
    executor0.run(|spawner| {
        spawner.spawn(core0_task(p.FLASH, radio_spi_device, radio_reset, radio_dio0, radio_dio1, led).unwrap());
    });
}

/// core1: sensor init, then `code.py`'s main loop -- sense, log, state,
/// buzzer, publish telemetry for core0 to transmit.
#[embassy_executor::task]
async fn flight_task(
    i2c_baro: i2c_bus::SharedI2cDevice,
    i2c_imu: i2c_bus::SharedI2cDevice,
    mut i2c_mag: i2c_bus::SharedI2cDevice,
    mut adc: Adc<'static, embassy_rp::adc::Blocking>,
    mut adc_channel: AdcChannel<'static>,
    mut buzz: buzzer::Buzzer<'static>,
    pixel_driver: PioWs2812<'static, PIO0, 0, 1, embassy_rp::pio_programs::ws2812::Grb>,
) {
    let mut pixel = pixel::StatusPixel::new(pixel_driver);

    // -- init_all(), each sensor failing independently ---------------------
    let mut baro = bmp580::Bmp580::new(i2c_baro).await.ok();
    let mut imu_dev = imu::Imu::new(i2c_imu).ok();
    let mag_present = lis3mdl::probe(&mut i2c_mag);

    let mut sensors: u8 = 0;
    if baro.is_some() {
        sensors |= Sensor::BARO;
    }
    if imu_dev.is_some() {
        sensors |= Sensor::IMU;
    }
    if mag_present {
        sensors |= Sensor::MAG;
    }
    // No real presence check for either -- see gps.rs's/flash_log.rs's
    // own docs on why that's an acceptable, deliberate simplification.
    sensors |= Sensor::GPS;
    sensors |= Sensor::LOG;

    let mut batt = 0.0f32;
    if let Ok(v) = battery::read_volts(&mut adc, &mut adc_channel) {
        batt = v;
        sensors |= Sensor::BATT;
    }

    if !Sensor::flight_ready(sensors) {
        defmt::warn!("NOT FLIGHT READY -- barometer, IMU, and log are required");
    }

    // -- sensor settling: the BMP580 needs a few reads before trusting it --
    let settle_deadline = Instant::now() + Duration::from_millis(2000);
    while Instant::now() < settle_deadline {
        if let Some(b) = baro.as_mut() {
            let _ = b.measurements();
        }
        Timer::after_millis(50).await;
    }

    let mut fs = FlightState::new();
    fs.transition(State::IDLE, Instant::now().as_millis() as u32);
    pixel.set_state(State::IDLE).await;
    defmt::info!("IDLE -- waiting for ARM from handheld");

    let mut last_imu: u32 = 0;
    let mut last_baro: u32 = 0;
    let mut last_gps: u32 = 0;
    let mut last_batt: u32 = 0;
    let mut chirp_until: u32 = 0;

    let mut accel = [0.0f32; 3];
    let mut gyro = [0.0f32; 3];
    let mut pressure = 0.0f32;
    let mut temp_c = 0.0f32;
    let mut accel_g_mag = 0.0f32;
    let mut has_fix = false;
    let mut lat = 0.0f32;
    let mut lon = 0.0f32;

    let mut log_writer = flash_log::LogWriter::new(Instant::now());

    loop {
        let now = Instant::now();
        let now_ms = now.as_millis() as u32;

        // -- sense: IMU (highest rate) --------------------------------------
        if now_ms.wrapping_sub(last_imu) >= imu_period_ms(fs.state) {
            last_imu = now_ms;
            if let Some(dev) = imu_dev.as_mut() {
                if let Some((a, g)) = dev.read() {
                    accel = a;
                    gyro = g;
                    accel_g_mag = accel_magnitude(a);
                }
            }
        }

        // -- sense: barometer -------------------------------------------------
        if now_ms.wrapping_sub(last_baro) >= baro_period_ms(fs.state) {
            last_baro = now_ms;
            if let Some(b) = baro.as_mut() {
                if let Ok((t, p)) = b.measurements() {
                    temp_c = t;
                    pressure = p;
                    fs.update_altitude(p, now_ms);
                }
            }
        }

        // -- log: from ARMED onward, every loop tick (matches code.py's
        // unconditional per-iteration write) ----------------------------
        if !matches!(fs.state, State::BOOT | State::IDLE) {
            log_writer
                .push(&LogEntry {
                    t_ms: now_ms,
                    state: fs.state,
                    alt_m: fs.alt_m,
                    vel_mps: fs.vel_mps,
                    pressure_hpa: pressure,
                    temp_c,
                    accel_mps2: accel,
                    gyro_dps: gyro,
                })
                .await;
        }
        log_writer.maybe_flush_on_timer(now).await;

        // -- state machine ------------------------------------------------
        if fs.update(accel_g_mag, now_ms) {
            pixel.set_state(fs.state).await;
            defmt::info!("-> {} alt={} vel={}", fs.state, fs.alt_m, fs.vel_mps);
            if fs.state == State::LANDED {
                log_writer.flush_if_any().await;
            }
        }

        // -- sense: GPS (paused during flight) -------------------------------
        if let Some(period) = gps_period_ms(fs.state) {
            if now_ms.wrapping_sub(last_gps) >= period {
                last_gps = now_ms;
                let g = *gps::GPS_FIX.lock().await;
                has_fix = g.has_fix;
                lat = g.lat;
                lon = g.lon;
            }
        }

        // -- battery: slow, and never during powered flight -------------------
        if now_ms.wrapping_sub(last_batt) > 5000 && !matches!(fs.state, State::BOOST | State::COAST) {
            last_batt = now_ms;
            if let Ok(v) = battery::read_volts(&mut adc, &mut adc_channel) {
                batt = v;
            }
        }

        // -- commands from core0 ---------------------------------------------
        if let Ok(cmd) = link::COMMANDS.try_receive() {
            if cmd == Command::ARM && fs.state == State::IDLE {
                if !Sensor::flight_ready(sensors) {
                    defmt::warn!("ARM REFUSED -- not flight ready");
                    chirp_until = now_ms.wrapping_add(ARM_REFUSED_CHIRP_MS);
                } else {
                    fs.set_ground_reference(pressure);
                    fs.transition(State::ARMED, now_ms);
                    pixel.set_state(State::ARMED).await;
                    flash_log::ARM_CYCLE_EVENTS.send(flash_log::ArmCycleEvent::Start).await;
                    defmt::info!("ARMED ground_p={}", pressure);
                }
            } else if cmd == Command::DISARM && fs.state == State::ARMED {
                fs.transition(State::IDLE, now_ms);
                pixel.set_state(State::IDLE).await;
                // Rewind to this arm cycle's start, not a full-log wipe
                // like code.py's literal open(path, "wb") -- user call,
                // 2026-08-18, see docs/rust-rewrite.md.
                flash_log::ARM_CYCLE_EVENTS.send(flash_log::ArmCycleEvent::RewindWithoutBoost).await;
                defmt::info!("DISARMED (log rewound)");
            } else if cmd == Command::CHIRP {
                chirp_until = now_ms.wrapping_add(CHIRP_MS);
            }
        }

        // -- buzzer -----------------------------------------------------------
        if fs.state == State::LANDED {
            // DOT-DOT-DOT beacon: three 1/6s pulses, then rest, every 2s.
            let phase = now_ms % 2000;
            let on = phase < 167 || (334..500).contains(&phase) || (667..834).contains(&phase);
            if on {
                buzz.on();
            } else {
                buzz.off();
            }
        } else if now_ms < chirp_until {
            buzz.on();
        } else {
            buzz.off();
        }

        // -- publish telemetry for core0 to transmit ---------------------------
        *link::TELEMETRY.lock().await = Some(link::LatestTelemetry {
            state: fs.state,
            lat,
            lon,
            alt_baro_m: fs.alt_m,
            speed_mps: fs.vel_mps,
            temp_c,
            accel_g: accel.map(|a| a / 9.806_65),
            gyro_dps: gyro,
            batt_volts: batt,
            has_fix,
            satellites: 0, // see gps.rs's docs on the satellite-count simplification
            sensors,
        });

        Timer::after_millis(5).await;
    }
}

/// core0: radio (phase-agnostic) + flash-flush.
#[embassy_executor::task]
async fn core0_task(
    flash: embassy_rp::Peri<'static, embassy_rp::peripherals::FLASH>,
    radio_spi: radio::RadioSpiDevice,
    radio_reset: Output<'static>,
    radio_dio0: Input<'static>,
    radio_dio1: Input<'static>,
    mut led: Output<'static>,
) {
    let mut radio = match radio::Radio::new(radio_spi, radio_reset, radio_dio0, radio_dio1).await {
        Ok(r) => r,
        Err(e) => {
            defmt::error!("core0: radio init failed: {}", e);
            for _ in 0..5 {
                led.set_high();
                Timer::after_millis(300).await;
                led.set_low();
                Timer::after_millis(300).await;
            }
            return;
        }
    };
    defmt::info!("core0: radio init ok");

    let mut archive = flash_log::LogArchive::new(flash);
    let mut counter: u16 = 0;
    let mut last_seq: Option<u16> = None;
    let mut last_tx: u32 = 0;

    loop {
        // -- flash-flush / arm-cycle housekeeping (non-blocking checks) -------
        if let Ok(idx) = flash_log::FLUSH_REQUESTS.try_receive() {
            archive.handle_flush(idx).await;
        }
        if let Ok(event) = flash_log::ARM_CYCLE_EVENTS.try_receive() {
            archive.handle_arm_cycle_event(event);
        }

        // -- receive commands, replay-checked here (core0 sees the raw seq) ---
        match radio.try_receive_command().await {
            Ok(Some((seq, cmd))) => {
                if last_seq != Some(seq) {
                    last_seq = Some(seq);
                    let _ = link::COMMANDS.try_send(cmd);
                }
            }
            Ok(None) => {}
            Err(e) => {
                defmt::error!("core0: try_receive_command failed: {}", e);
                for _ in 0..radio::error_blink_code(&e) {
                    led.set_high();
                    Timer::after_millis(150).await;
                    led.set_low();
                    Timer::after_millis(150).await;
                }
            }
        }

        // -- transmit telemetry, phase-dependent rate --------------------------
        let now_ms = Instant::now().as_millis() as u32;
        if let Some(t) = *link::TELEMETRY.lock().await {
            let period = if in_flight(t.state) { TX_PERIOD_FLIGHT_MS } else { TX_PERIOD_IDLE_MS };
            if now_ms.wrapping_sub(last_tx) >= period {
                last_tx = now_ms;
                let input = common::TelemetryInput {
                    counter,
                    uptime_ms: now_ms,
                    state: t.state,
                    lat: t.lat,
                    lon: t.lon,
                    alt_baro_m: t.alt_baro_m,
                    speed_mps: t.speed_mps,
                    temp_c: t.temp_c,
                    accel_g: t.accel_g,
                    gyro_dps: t.gyro_dps,
                    batt_volts: t.batt_volts,
                    has_fix: t.has_fix,
                    satellites: t.satellites,
                    cam_rec: 0,
                    // CHG hardcoded 0 -- no bare-metal VBUS/USB-connected
                    // equivalent implemented yet. User call, 2026-08-18:
                    // ship without it rather than take on a full USB
                    // device stack -- see docs/rust-rewrite.md. Means the
                    // ground station's NOGO-while-charging gate won't
                    // actually trigger via telemetry yet.
                    sensors: t.sensors,
                    cam_disk: 0,
                };
                counter = counter.wrapping_add(1);
                if let Err(e) = radio.send_telemetry(&input).await {
                    defmt::error!("core0: send_telemetry failed: {}", e);
                }
                led.toggle();
            }
        }
    }
}
