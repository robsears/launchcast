//! RFM95 (SX1276) LoRa radio driver, built on `lora-phy`'s SX127x
//! `RadioKind` (MIT OR Apache-2.0, maintained under the lora-rs/embassy-rs
//! org -- see `docs/rust-rewrite.md`'s ecosystem check).
//!
//! Over-the-air parameters are ported directly from `rocket/code.py`'s and
//! `ground/code.py`'s `_init_radio` (`adafruit_rfm9x.RFM9x(spi, cs, rst,
//! 915.0)`, `tx_power = 20`, `spreading_factor = 7`, `signal_bandwidth =
//! 125000`, `coding_rate = 5`, `enable_crc = True`, `sync_word =
//! packet.SYNC_WORD`), not chosen fresh -- this ground station has to
//! interoperate with the real, still-running CircuitPython rocket
//! firmware, not just any LoRa config. `LoRa::with_syncword` takes the
//! sync word in the same legacy single-byte form `adafruit_rfm9x.sync_word`
//! uses, so `launchcast_common::SYNC_WORD` (0x2B) is passed as-is.
//!
//! Peripheral construction (SPI1 + DMA, CS/reset/DIO0 pins) happens in
//! `main.rs`, matching this codebase's existing pattern (`display.rs` and
//! `buttons.rs` take already-constructed transport objects too) -- this
//! module only owns LoRa protocol logic. SPI1 is safe to give the radio
//! exclusive ownership of now: the display moved to a PIO-backed bus
//! specifically so it would stop sharing this peripheral (see
//! `display.rs`'s module docs).

use embassy_rp::gpio::{Input, Output};
use embassy_rp::peripherals::SPI1;
use embassy_rp::spi::{Async, Spi};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Delay, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use heapless::{Deque, String as HString};
use launchcast_common as common;
use lora_phy::iv::GenericSx127xInterfaceVariant;
use lora_phy::mod_params::{Bandwidth, CodingRate, ModulationParams, PacketParams, RadioError, RxMode, SpreadingFactor};
use lora_phy::mod_traits::IrqState;
use lora_phy::sx127x::{Config as Sx127xConfig, Sx1276, Sx127x};
use lora_phy::LoRa;
use portable_atomic::{AtomicU32, Ordering};

/// Count of frames that made it through `RxDone` (a structurally valid
/// LoRa packet) but failed `unpack_telemetry`'s own validation
/// (MAGIC/length/CRC) -- matches `code.py`'s `Link.rejects`, shown on the
/// NO TELEMETRY fallback screen. Plain RX timeouts (nothing arrived at
/// all) never touch this, same as Python's `radio.receive(timeout=...)`
/// returning `None` doesn't increment `Link.rejects` either.
pub static REJECT_COUNT: AtomicU32 = AtomicU32::new(0);

/// Count of successfully decoded telemetry frames -- matches `code.py`'s
/// `Link.packets`, shown on the DIAG screen.
pub static PACKET_COUNT: AtomicU32 = AtomicU32::new(0);

/// TEMP diagnostic (see docs/rust-rewrite.md bug log, 2026-08-17): a
/// scrolling text log rendered on the Sharp display, built specifically
/// because there's no debug probe attached and `defmt` output is
/// otherwise invisible on real hardware. `lora-phy`'s own `complete_rx`
/// (which we'd normally just call) is an opaque black box from the
/// outside -- to see what's actually happening call by call, this module
/// replicates its loop directly using `LoRa`'s own public lower-level
/// methods (`wait_for_irq`, `process_irq_event`, `get_rx_result`, all
/// exported for exactly this kind of use), logging a line each iteration.
/// No crate fork/patch needed.
pub const LOG_CAPACITY: usize = 20;
pub static RADIO_LOG: Mutex<CriticalSectionRawMutex, Deque<HString<48>, LOG_CAPACITY>> = Mutex::new(Deque::new());

pub async fn log_line(args: core::fmt::Arguments<'_>) {
    let mut s: HString<48> = HString::new();
    let _ = core::fmt::write(&mut s, args);
    let mut log = RADIO_LOG.lock().await;
    if log.len() == LOG_CAPACITY {
        log.pop_front();
    }
    let _ = log.push_back(s);
}

const FREQUENCY_HZ: u32 = 915_000_000;
const TX_POWER_DBM: i32 = 20;
/// adafruit_rfm9x's default preamble length, in symbols.
const PREAMBLE_LEN: u16 = 8;
/// RxMode::Single's timeout, in LoRa symbols (sx127x doesn't support a
/// time-based RX timeout, only symbol-counted -- see lora-phy's RxMode
/// docs). At SF7/BW125 one symbol is 2^7/125000 =~ 1.02ms, so this is
/// roughly a half-second RX window per poll: long enough to reliably catch
/// a beaconing downlink frame, short enough that a queued uplink command
/// (ARM/DISARM/CHIRP) doesn't wait long to get its turn to transmit.
const RX_SYMBOL_TIMEOUT: u16 = 500;

/// adafruit_rfm9x's `send()`/`receive()` transparently wrap every payload
/// in a 4-byte RadioHead-style header (to, from, id, flags) -- prepended
/// on send, and stripped on receive by default (`with_header=False`,
/// which is what both `rocket/code.py` and `ground/code.py` use, since
/// neither passes that kwarg). Skipping this entirely would break
/// interop in both directions: our RX would read what it thinks is a
/// plain 40-byte telemetry frame but is actually [4-byte header][40-byte
/// frame] (`unpack_telemetry` would see the header's first byte where it
/// expects MAGIC and reject every real frame); our TX would send a bare
/// 7-byte command that the rocket's `receive()` strips 4 bytes off of,
/// leaving only 3 bytes for its own `unpack_command` to reject on a
/// length check.
///
/// Neither Python side customizes `destination`/`node`/`identifier`/
/// `flags`, so both stay at the library's defaults, and critically both
/// have `self.node == _RH_BROADCAST_ADDRESS` -- which means
/// adafruit_rfm9x's destination-address filter
/// (`packet[0] not in {_RH_BROADCAST_ADDRESS, self.node}`) is
/// unconditionally bypassed on both ends regardless of header content.
/// So only the 4-byte *length* matters for correctness here, not the
/// actual header byte values -- broadcast (0xFF) is used to match what
/// the Python side would itself produce, not because it's required.
const RH_HEADER_LEN: usize = 4;
const RH_BROADCAST: u8 = 0xFF;

pub type RadioSpiDevice = ExclusiveDevice<Spi<'static, SPI1, Async>, Output<'static>, Delay>;
type RadioIv = GenericSx127xInterfaceVariant<Output<'static>, Input<'static>>;
type RadioKind = Sx127x<RadioSpiDevice, RadioIv, Sx1276>;

/// A successfully decoded telemetry frame plus the link-quality figures
/// it arrived with.
pub struct RxResult {
    pub telemetry: common::Telemetry,
    pub rssi: i16,
    pub snr: i16,
}

pub struct Radio {
    lora: LoRa<RadioKind, Delay>,
    mdltn_params: ModulationParams,
    tx_pkt_params: PacketParams,
    rx_pkt_params: PacketParams,
}

impl Radio {
    pub async fn new(
        spi: RadioSpiDevice,
        reset: Output<'static>,
        dio0: Input<'static>,
        dio1: Input<'static>,
    ) -> Result<Self, RadioError> {
        // No RF switch control pins -- the RFM95 module handles TX/RX
        // antenna switching onboard, unlike some bare SX127x breakouts.
        //
        // DIO1, not just DIO0, is required here: the sx127x only ever
        // signals RxDone/TxDone/CadDone on DIO0 -- RxTimeout is routed
        // exclusively to DIO1 (confirmed directly from this crate's own
        // `GenericSx127xInterfaceVariant` docs). `try_receive_telemetry`
        // uses `RxMode::Single`, which relies on that hardware RxTimeout
        // to end an empty receive window; with only DIO0 wired (the
        // single-IRQ `new()` constructor), an RX attempt that hears
        // nothing waits forever for an interrupt that can never arrive on
        // that pin. Found on real hardware (2026-08-17): the radio
        // initialized fine and the very first RX attempt hung forever,
        // with no timeout, no error, nothing -- see docs/rust-rewrite.md's
        // bug log. `new_with_secondary_irq` watches both. (DIO1 alone was
        // also independently ruled out as a cause of the deeper
        // never-receives bug via a temporary DIO0-only test -- the real
        // cause was the sync word, see the comment on `with_syncword`
        // below.)
        let iv = GenericSx127xInterfaceVariant::new_with_secondary_irq(reset, dio0, Some(dio1), None, None)?;
        let sx = Sx127x::new(
            spi,
            iv,
            Sx127xConfig {
                chip: Sx1276,
                // Crystal oscillator, not a TCXO, on this board.
                tcxo_used: false,
                // PA_BOOST output, not RFO -- required for tx_power=20dBm
                // to mean anything close to what it says; RFO tops out
                // much lower. Matches the RFM9x module's actual wiring
                // (PA_BOOST is what's bonded out to the antenna on RFM9x
                // boards, not RFO).
                tx_boost: true,
                rx_boost: false,
            },
        );

        // NOT common::SYNC_WORD (0x2B) -- found on real hardware
        // (2026-08-17) that the Python side's sync word assignment
        // (`self.radio.sync_word = packet.SYNC_WORD` in both
        // rocket/code.py and ground/code.py) has *never* actually taken
        // effect: `adafruit_rfm9x.RFM9x` has no `sync_word` property at
        // all, so that line has always raised `AttributeError`, silently
        // swallowed by the surrounding `except Exception: pass`. The real,
        // actually-functioning Python<->Python link runs on the SX1276's
        // power-on-reset default sync word (0x12) -- harmless on the
        // Python side, since both radios fail identically and land on the
        // same true default, but a real blocker here once one side (this
        // one) correctly implements the never-functional intended value.
        // Root-caused by RSSI clearly tracking the payload's real
        // transmissions (proving the RF front-end/frequency/antenna path
        // was never the problem) while zero preamble/RxDone events ever
        // fired -- pointing squarely at the demodulator's sync-word
        // correlation filter rejecting every frame before ever reporting
        // it.
        const ACTUAL_SYNC_WORD: u8 = 0x12;
        let mut lora = LoRa::with_syncword(sx, ACTUAL_SYNC_WORD, Delay).await?;

        let mdltn_params = lora.create_modulation_params(
            SpreadingFactor::_7,
            Bandwidth::_125KHz,
            CodingRate::_4_5,
            FREQUENCY_HZ,
        )?;
        let tx_pkt_params =
            lora.create_tx_packet_params(PREAMBLE_LEN, false, true, false, &mdltn_params)?;
        let rx_pkt_params = lora.create_rx_packet_params(
            PREAMBLE_LEN,
            false,
            (RH_HEADER_LEN + common::TELEMETRY_SIZE) as u8,
            true,
            false,
            &mdltn_params,
        )?;

        Ok(Self {
            lora,
            mdltn_params,
            tx_pkt_params,
            rx_pkt_params,
        })
    }

    /// Diagnostic tool, not currently called anywhere -- kept for future
    /// bring-up work (antenna/range testing, a future diagnostics screen)
    /// rather than deleted, since it's what actually root-caused the
    /// 2026-08-17 sync-word bug (see docs/rust-rewrite.md): bypasses all
    /// packet-detection/IRQ logic entirely -- just `LoRa::listen()` (a
    /// self-contained helper: sets frequency/SF7/CR4-5 and puts the chip
    /// in RxContinuous) followed by repeated `get_rssi()` reads, logged
    /// once a second. Tests only whether the receiver's analog front-end
    /// is actually active and measuring real RF energy, independent of
    /// everything else (sync word, packet params, IRQ mapping) that could
    /// be wrong in the packet-detection path. Runs forever; only meant to
    /// be watched for a while, not to return.
    #[allow(dead_code)]
    pub async fn rssi_probe_loop(&mut self) -> ! {
        if let Err(e) = self.lora.listen(FREQUENCY_HZ, Bandwidth::_125KHz).await {
            log_line(format_args!("listen() err: {e:?}")).await;
        } else {
            log_line(format_args!("listening for RSSI...")).await;
        }
        loop {
            match self.lora.get_rssi().await {
                Ok(rssi) => log_line(format_args!("rssi = {rssi} dBm")).await,
                Err(e) => log_line(format_args!("get_rssi err: {e:?}")).await,
            }
            Timer::after_millis(1000).await;
        }
    }

    /// Send a command frame (ARM/DISARM/CHIRP/PING -- see
    /// `launchcast_common::Command`). Blocks (this task) until the
    /// transmission completes; LoRa is half-duplex, so nothing can be
    /// received while this runs.
    pub async fn send_command(&mut self, seq: u16, cmd: u8) -> Result<(), RadioError> {
        let payload = common::pack_command(seq, cmd);
        let mut buf = [0u8; RH_HEADER_LEN + common::COMMAND_SIZE];
        // RadioHead header the rocket's receive() expects and strips --
        // see this module's docs on RH_HEADER_LEN.
        buf[0] = RH_BROADCAST; // to
        buf[1] = RH_BROADCAST; // from
        buf[2] = 0; // id
        buf[3] = 0; // flags
        buf[RH_HEADER_LEN..].copy_from_slice(&payload);

        self.lora
            .prepare_for_tx(&self.mdltn_params, &mut self.tx_pkt_params, TX_POWER_DBM, &buf)
            .await?;
        let result = self.lora.tx().await;
        match &result {
            Ok(()) => log_line(format_args!("tx ok cmd={cmd} seq={seq}")).await,
            Err(e) => log_line(format_args!("tx err: {e:?}")).await,
        }
        result
    }

    /// Listen for one telemetry frame for up to `RX_SYMBOL_TIMEOUT`
    /// symbols (roughly half a second). `Ok(None)` on a plain timeout --
    /// normal, since most polls won't catch a fresh frame -- vs `Err` for
    /// an actual radio fault. A malformed frame (too short to even hold a
    /// RadioHead header, or one that fails `unpack_telemetry`'s own
    /// validation -- bad MAGIC/CRC/length) also comes back as `Ok(None)`,
    /// indistinguishable from a timeout to the caller, since neither is
    /// actionable beyond "nothing usable this round."
    pub async fn try_receive_telemetry(&mut self) -> Result<Option<RxResult>, RadioError> {
        // Replicates `LoRa::complete_rx`'s own loop by hand, using only
        // its public building blocks (`process_irq_event`, `wait_for_irq`,
        // `get_rx_result`), instead of calling `complete_rx` itself --
        // which is an opaque black box from the outside. Built as a
        // temporary diagnostic (no debug probe attached, so this was the
        // only way to see what each iteration actually observed) but kept
        // permanently: the scrolling log it feeds (`RADIO_LOG`, rendered
        // on the display) was directly responsible for root-causing the
        // sync-word bug below, and stays valuable for the rocket-side
        // radio work ahead.
        self.lora
            .prepare_for_rx(RxMode::Single(RX_SYMBOL_TIMEOUT), &self.mdltn_params, &self.rx_pkt_params)
            .await?;
        self.lora.start_rx().await?;

        loop {
            // `LoRa::process_irq_event()` (the public wrapper we can
            // actually call) hardcodes `clear_interrupts=false` at this
            // commit -- `complete_rx` itself bypasses that by reaching
            // into a private field to pass `true` directly, which we
            // can't do from outside the crate. Replicated here with an
            // explicit `clear_irq_status()` call instead, same order
            // (read, then clear) as the original.
            let irq_result = self.lora.process_irq_event().await;
            let _ = self.lora.clear_irq_status().await;
            match irq_result {
                Ok(Some(IrqState::PreambleReceived)) => {
                    log_line(format_args!("preamble detected!")).await;
                }
                Ok(Some(IrqState::Done)) => {
                    log_line(format_args!("RxDone!")).await;
                    break;
                }
                Ok(None) => {}
                // A plain timeout (nothing arrived this ~0.5s window) is
                // normal and not logged every cycle; any other error is
                // real and gets surfaced.
                Err(RadioError::ReceiveTimeout) => return Ok(None),
                Err(e) => {
                    log_line(format_args!("irq err: {e:?}")).await;
                    return Err(e);
                }
            }
            self.lora.wait_for_irq().await?;
        }

        let mut buf = [0u8; RH_HEADER_LEN + common::TELEMETRY_SIZE];
        let (len, status) = self.lora.get_rx_result(&self.rx_pkt_params, &mut buf).await?;
        log_line(format_args!("got {len}B rssi={} snr={}", status.rssi, status.snr)).await;
        let len = len as usize;
        if len <= RH_HEADER_LEN {
            log_line(format_args!("too short for RH header")).await;
            return Ok(None);
        }
        // Strip the RadioHead header the rocket's send() prepended -- see
        // this module's docs on RH_HEADER_LEN.
        let telemetry = common::unpack_telemetry(&buf[RH_HEADER_LEN..len]);
        match telemetry {
            Some(t) => {
                PACKET_COUNT.fetch_add(1, Ordering::Relaxed);
                Ok(Some(RxResult {
                    telemetry: t,
                    rssi: status.rssi,
                    snr: status.snr,
                }))
            }
            None => {
                REJECT_COUNT.fetch_add(1, Ordering::Relaxed);
                log_line(format_args!("unpack_telemetry rejected frame")).await;
                Ok(None)
            }
        }
    }
}

/// TEMP diagnostic (see docs/rust-rewrite.md bug log, 2026-08-17): with no
/// debug probe attached, `defmt::error!("{}", e)` is invisible on real
/// hardware, so this maps a `RadioError` to a small blink count the caller
/// can flash on the onboard LED and the count reported back verbally --
/// the only way to identify which specific error is firing without a
/// probe. Grouped by rough cause, not 1:1 with every variant (some are
/// vanishingly unlikely to actually occur here and are lumped into 8).
pub fn error_blink_code(e: &RadioError) -> u32 {
    match e {
        RadioError::SPI => 1,
        RadioError::Busy => 2,
        RadioError::Irq | RadioError::DIO1 => 3,
        RadioError::InvalidConfiguration => 4,
        RadioError::InvalidRadioMode => 5,
        RadioError::InvalidSyncWord => 6,
        RadioError::PayloadSizeUnexpected(_) | RadioError::PayloadSizeMismatch(_, _) => 7,
        _ => 8,
    }
}
