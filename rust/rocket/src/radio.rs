//! RFM95 (SX1276) LoRa radio driver -- rocket side. Same over-the-air
//! config as the ground station's own radio (`ground/src/radio.rs`:
//! SF7/BW125/CR4-5, sync word 0x12 -- see that module's docs for the
//! full sync-word bug history, which applies identically here), same
//! `lora-phy` SX127x driver, same board (this is physically the same
//! Feather RP2040 RFM95 product on both ends, confirmed via this board's
//! CircuitPython `pins.c`: `RFM_CS`/`RFM_RST`/`RFM_IO0`/`RFM_IO1` map to
//! the same GPIO numbers either side). Not shared as a single crate
//! (would need a new shared firmware crate this session didn't take on)
//! -- adapted here with the payload direction reversed: this board
//! **sends** telemetry and **receives** commands, the opposite of
//! `ground`'s.
//!
//! `ground/src/radio.rs`'s scrolling `RADIO_LOG`/`rssi_probe_loop`
//! bring-up diagnostics aren't ported -- those exist specifically to
//! render on a display this board doesn't have. `error_blink_code` is,
//! since an LED-blink diagnostic doesn't need one.

use embassy_rp::gpio::{Input, Output};
use embassy_rp::peripherals::SPI1;
use embassy_rp::spi::{Async, Spi};
use embassy_time::Delay;
use embedded_hal_bus::spi::ExclusiveDevice;
use launchcast_common as common;
use lora_phy::iv::GenericSx127xInterfaceVariant;
use lora_phy::mod_params::{Bandwidth, CodingRate, ModulationParams, PacketParams, RadioError, RxMode};
use lora_phy::mod_params::SpreadingFactor;
use lora_phy::sx127x::{Config as Sx127xConfig, Sx1276, Sx127x};
use lora_phy::LoRa;

const FREQUENCY_HZ: u32 = 915_000_000;
const TX_POWER_DBM: i32 = 20;
const PREAMBLE_LEN: u16 = 8;
/// Matches `ground/src/radio.rs`'s own `RX_SYMBOL_TIMEOUT` -- same
/// reasoning: long enough to reliably catch a beaconing frame (here, an
/// uplink command from the handheld), short enough that this board's own
/// telemetry TX doesn't wait long for its turn.
const RX_SYMBOL_TIMEOUT: u16 = 500;

/// See `ground/src/radio.rs`'s docs on `RH_HEADER_LEN` -- same RadioHead-
/// style 4-byte header both `adafruit_rfm9x.send()`/`receive()` calls
/// transparently add/strip, required for wire compatibility with the
/// ground station's own radio (which expects it).
const RH_HEADER_LEN: usize = 4;
const RH_BROADCAST: u8 = 0xFF;

/// Power-on-reset default sync word -- see `ground/src/radio.rs`'s full
/// bug-history comment on why this is 0x12, not `common::SYNC_WORD`
/// (0x2B).
const ACTUAL_SYNC_WORD: u8 = 0x12;

pub type RadioSpiDevice = ExclusiveDevice<Spi<'static, SPI1, Async>, Output<'static>, Delay>;
type RadioIv = GenericSx127xInterfaceVariant<Output<'static>, Input<'static>>;
type RadioKind = Sx127x<RadioSpiDevice, RadioIv, Sx1276>;

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
        // DIO1 required -- see ground/src/radio.rs's docs (RxTimeout is
        // wired to DIO1 only; without it, RxMode::Single hangs forever
        // on an empty window).
        let iv = GenericSx127xInterfaceVariant::new_with_secondary_irq(reset, dio0, Some(dio1), None, None)?;
        let sx = Sx127x::new(
            spi,
            iv,
            Sx127xConfig {
                chip: Sx1276,
                tcxo_used: false,
                tx_boost: true,
                rx_boost: false,
            },
        );

        let mut lora = LoRa::with_syncword(sx, ACTUAL_SYNC_WORD, Delay).await?;

        let mdltn_params = lora.create_modulation_params(
            SpreadingFactor::_7,
            Bandwidth::_125KHz,
            CodingRate::_4_5,
            FREQUENCY_HZ,
        )?;
        let tx_pkt_params = lora.create_tx_packet_params(PREAMBLE_LEN, false, true, false, &mdltn_params)?;
        // Sized for an inbound *command* frame (this board receives
        // commands, unlike the ground station which receives telemetry).
        let rx_pkt_params = lora.create_rx_packet_params(
            PREAMBLE_LEN,
            false,
            (RH_HEADER_LEN + common::COMMAND_SIZE) as u8,
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

    /// Broadcast one telemetry frame. Blocks (this task) until the
    /// transmission completes; LoRa is half-duplex, so nothing can be
    /// received while this runs -- matches `code.py`'s own `hw.radio.send()`
    /// call, and its "a failed send must never stall the log loop"
    /// contract (the caller ignores `Err` the same way).
    pub async fn send_telemetry(&mut self, input: &common::TelemetryInput) -> Result<(), RadioError> {
        let payload = common::pack_telemetry(input);
        let mut buf = [0u8; RH_HEADER_LEN + common::TELEMETRY_SIZE];
        buf[0] = RH_BROADCAST; // to
        buf[1] = RH_BROADCAST; // from
        buf[2] = 0; // id
        buf[3] = 0; // flags
        buf[RH_HEADER_LEN..].copy_from_slice(&payload);

        self.lora
            .prepare_for_tx(&self.mdltn_params, &mut self.tx_pkt_params, TX_POWER_DBM, &buf)
            .await?;
        self.lora.tx().await
    }

    /// Broadcast one flight-summary frame, in response to a
    /// `Command::GET_SUMMARY_BASE` request. Same shape as
    /// [`Radio::send_telemetry`] -- explicit-header mode means
    /// `tx_pkt_params` isn't tied to a fixed payload length, so the same
    /// params are reused for this differently-sized frame with no
    /// separate `PacketParams` needed.
    pub async fn send_summary(&mut self, input: &common::SummaryInput) -> Result<(), RadioError> {
        let payload = common::pack_summary(input);
        let mut buf = [0u8; RH_HEADER_LEN + common::SUMMARY_SIZE];
        buf[0] = RH_BROADCAST; // to
        buf[1] = RH_BROADCAST; // from
        buf[2] = 0; // id
        buf[3] = 0; // flags
        buf[RH_HEADER_LEN..].copy_from_slice(&payload);

        self.lora
            .prepare_for_tx(&self.mdltn_params, &mut self.tx_pkt_params, TX_POWER_DBM, &buf)
            .await?;
        self.lora.tx().await
    }

    /// Broadcast one flight-index frame, in response to
    /// `Command::GET_FLIGHT_INDEX`. Variable length, unlike
    /// [`Radio::send_summary`]/[`Radio::send_telemetry`] -- see
    /// `common::pack_flight_index`'s docs -- so this builds a
    /// `heapless::Vec` sized for the worst case instead of a fixed
    /// array; explicit-header LoRa mode means `tx_pkt_params` doesn't
    /// care that the actual length varies call to call.
    pub async fn send_flight_index(&mut self, timestamps: &[u32]) -> Result<(), RadioError> {
        let payload = common::pack_flight_index(timestamps);
        let mut buf: heapless::Vec<u8, { RH_HEADER_LEN + common::FLIGHT_INDEX_MAX_SIZE }> = heapless::Vec::new();
        let _ = buf.push(RH_BROADCAST); // to
        let _ = buf.push(RH_BROADCAST); // from
        let _ = buf.push(0); // id
        let _ = buf.push(0); // flags
        let _ = buf.extend_from_slice(&payload);

        self.lora
            .prepare_for_tx(&self.mdltn_params, &mut self.tx_pkt_params, TX_POWER_DBM, &buf)
            .await?;
        self.lora.tx().await
    }

    /// Listen for one command frame for up to `RX_SYMBOL_TIMEOUT` symbols
    /// (roughly half a second). `Ok(None)` on a plain timeout or a frame
    /// that fails `unpack_command`'s own validation (magic/checksum) --
    /// both normal, neither actionable beyond "nothing this round" --
    /// matches `ground/src/radio.rs::try_receive_telemetry`'s same shape,
    /// receiving the opposite frame type.
    pub async fn try_receive_command(&mut self) -> Result<Option<(u16, u8)>, RadioError> {
        self.lora
            .prepare_for_rx(RxMode::Single(RX_SYMBOL_TIMEOUT), &self.mdltn_params, &self.rx_pkt_params)
            .await?;
        // `LoRa::rx` (start_rx + complete_rx in one call) instead of
        // ground's hand-rolled IRQ loop -- that loop exists there purely
        // to feed the scrolling bring-up log this board doesn't have; the
        // plain high-level call is correct and sufficient here.
        let mut buf = [0u8; RH_HEADER_LEN + common::COMMAND_SIZE];
        let (len, _status) = match self.lora.rx(&self.rx_pkt_params, &mut buf).await {
            Ok(result) => result,
            Err(RadioError::ReceiveTimeout) => return Ok(None),
            Err(e) => return Err(e),
        };
        let len = len as usize;
        if len <= RH_HEADER_LEN {
            return Ok(None);
        }
        // Strip the RadioHead header the ground station's send() prepends
        // -- see this module's docs on RH_HEADER_LEN.
        Ok(common::unpack_command(&buf[RH_HEADER_LEN..len]))
    }
}

/// See `ground/src/radio.rs`'s docs -- same purpose (identify a
/// `RadioError` by LED blink count, no debug probe attached), same
/// grouping.
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
