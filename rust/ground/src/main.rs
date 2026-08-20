//! LaunchCast ground station firmware -- Rust rewrite.
//!
//! Confirmed working on real hardware (2026-08-17): boots and renders to
//! the physical Sharp Memory LCD. Radio and GPS are not wired up yet. See
//! `docs/rust-rewrite.md` ("Strawman architecture", decided 2026-08-16)
//! for the split implemented here:
//!
//!   core0: radio RX + GPS (the subsystems where timing actually matters
//!          for correctness -- LoRa RX windows, GPS NMEA parsing). Idle
//!          apart from receiving dispatched button events for now, since
//!          neither radio nor GPS exist yet.
//!   core1: button sampling/dispatch + display rendering
//!
//! Buttons and display sharing core1 is deliberate, not a compromise
//! reached by accident: `SharpMemoryDisplay::show` uses blocking (not
//! DMA/async) SPI, so a ~50ms display refresh (twice a second, matching
//! `ground/code.py`'s `DISPLAY_HZ`) genuinely blocks whatever shares its
//! core for that window. Isolating buttons from that cost was the
//! original plan, but isolating radio/GPS instead was judged more
//! valuable -- a button response delayed up to 50ms is imperceptible to a
//! person and still ~20x tighter than the ~1s the Python implementation's
//! display draw used to block *everything* for.
//!
//! The display runs on a **PIO-backed SPI bus** (`display.rs`'s module
//! docs explain why), not the RP2040's hardware SPI1 -- SPI1's pins are
//! physically shared with the onboard RFM95 radio on this board, so if the
//! display used it, core0's radio work and core1's display refresh would
//! contend for the same hardware peripheral regardless of which cores
//! they're logically split across, reintroducing the exact blocking this
//! split exists to avoid. The PIO bus gives the display fully independent
//! hardware, with zero contention against the radio's SPI1.
//!
//! core0 and core1 each run their own `embassy_executor::Executor`
//! (`embassy_rp::multicore::spawn_core1`), not two tasks time-sliced on
//! one executor -- a dispatched button event crossing from core1 to core0
//! goes through `BUTTON_EVENTS`, a bounded `embassy_sync::channel::Channel`
//! guarded by a `CriticalSectionRawMutex` (the only way to share state
//! safely across the two cores; never raw shared mutable state).
#![no_std]
#![no_main]

mod battery;
mod buttons;
mod cmdlog;
mod display;
mod display_util;
mod flight_index;
mod frame;
mod gps;
mod handheld_art;
mod icon_bitmaps;
mod icons;
mod link;
mod lowbatt_art;
mod missing_art;
mod radio;
mod rocket_art;
mod screen;
mod screen_diagnostics;
mod screen_flight;
mod screen_flights;
mod screen_footer;
mod screen_header;
mod screen_missing;
mod screen_recovery;
mod screen_summary;
mod summary_request;

use core::ptr::addr_of_mut;

use buttons::Buttons;
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
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, PIO0};
use embassy_rp::pio::{InterruptHandler as PioInterruptHandler, Pio};
use embassy_rp::pio_programs::spi::Spi as PioSpi;
use embassy_rp::spi::{Config as SpiConfig, Spi};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{Delay, Instant, Timer};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use launchcast_common as common;
use launchcast_ground_logic::{link_status, nogo_reason, telemetry_missing, Edge, LinkStatus};
use panic_probe as _;
use portable_atomic::Ordering;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => PioInterruptHandler<PIO0>;
    DMA_IRQ_0 => DmaInterruptHandler<DMA_CH0>, DmaInterruptHandler<DMA_CH1>;
});

/// Matches `ground/code.py`'s `DISPLAY_HZ = 2.0` (also what services VCOM
/// -- see `display.rs`).
const DISPLAY_PERIOD_MS: u64 = 500;

/// Shown on the FLIGHT screen's CONTROLLER panel, mirroring the rocket's
/// own `FIRMWARE_VERSION` (`rocket/src/main.rs`) -- confirms a deploy to
/// *this* board actually took, without needing to trust "I definitely
/// just flashed it." Bump by hand on any flash meant to be
/// distinguishable from the last one.
pub const FIRMWARE_VERSION: u8 = 2;

/// Dispatched (not raw) button events only -- taps/holds, not every GPIO
/// edge -- so this channel only carries the handful of events a human
/// actually produces, never a debounce-cadence stream.
type ButtonEvent = (usize, Edge);
const BUTTON_EVENT_CAPACITY: usize = 8;

// 4096 (the original size) silently overflowed: SharpMemoryDisplay's
// framebuffer field alone is FRAME_BYTES (12000) bytes, and moving it by
// value into `display_task`'s spawn (`spawner.spawn(display_task(display)
// ...)`) is not guaranteed to elide every transient stack copy through
// that chain, even under LTO. A stack overflow with no debug probe
// attached just silently hangs the core (no fault message) -- diagnosed by
// bisecting with an LED-blink checkpoint at each setup step, isolating the
// hang to exactly that spawn call (see docs/rust-rewrite.md bug log,
// 2026-08-17). 32KB still wasn't enough; 128KB is confirmed working on
// real hardware with real margin to spare -- affordable at this size given
// the RP2040's 256KB total SRAM.
static mut CORE1_STACK: Stack<131072> = Stack::new();
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static BUTTON_EVENTS: Channel<CriticalSectionRawMutex, ButtonEvent, BUTTON_EVENT_CAPACITY> =
    Channel::new();

#[entry]
fn main() -> ! {
    let p = embassy_rp::init(Config::default());

    // Cold-boot-on-battery-only fix (reported 2026-08-18): powering on
    // from the LiPo alone never got core1 (buttons+display) running --
    // nothing ever reached the screen -- while core0 (radio) came up
    // fine every time, confirmed alive via its own heartbeat LED blink.
    // A reset-button press (not a power cycle) then fixed it every time.
    // That specific signature -- reset fixes it, fresh power-on doesn't
    // -- points at a power-rail settling race, not a logic bug: a reset
    // only restarts code execution, it doesn't recycle power, so by the
    // time it fires the rail has already been up and stable for a few
    // seconds; the exact same code then succeeds. LiPo boost-converter
    // rails commonly ramp up slower/less cleanly than USB's regulated 5V,
    // so firmware can start configuring hardware (here: PIO/SPI for the
    // display, on core1) before the rail is fully settled. Fixed by
    // giving the rail a moment before touching any peripheral, on either
    // core -- applied once here rather than guessing it's core1
    // specifically, since a 100ms cold-boot delay is free at this
    // project's timescales either way. cortex_m::asm::delay (not
    // embassy_time::Timer) because no async executor is running yet to
    // poll one. Unverified against real hardware -- next thing to
    // confirm once this is flashed and tested on battery power.
    cortex_m::asm::delay(12_500_000); // ~100ms at the 125MHz clock init() just configured

    defmt::info!("launchcast-ground: boot ok, spawning core1 (buttons + display)");

    spawn_core1(
        p.CORE1,
        unsafe { &mut *addr_of_mut!(CORE1_STACK) },
        move || {
            let buttons = Buttons::new(p.PIN_9, p.PIN_10, p.PIN_11);

            // PIO-backed bus, not hardware SPI1 -- see display.rs and this
            // module's docs for why. CLK=D5/GPIO5, MOSI=D12/GPIO12 (moved
            // off the general-purpose SPI header pins, which are the
            // radio's SPI1 pins on this board). The PIO SPI program is
            // inherently full-duplex, so it still needs a MISO pin
            // argument even though the display is write-only -- GPIO1
            // (D0/UART RX) is unused elsewhere in this firmware and wired
            // to nothing; its rx data is simply never read.
            // Baudrate matches adafruit_sharpmemorydisplay's default (2 MHz).
            // Local var deliberately not named `common` -- that's shadowed
            // at module scope by `launchcast_common as common` (unused in
            // this closure, but shadowing it here would be a trap for
            // later edits).
            let Pio {
                common: mut pio_common,
                sm0,
                ..
            } = Pio::new(p.PIO0, Irqs);
            let mut spi_config = SpiConfig::default();
            spi_config.frequency = 2_000_000;
            let spi = PioSpi::new_blocking(&mut pio_common, sm0, p.PIN_5, p.PIN_12, p.PIN_1, spi_config);
            // Active-high CS (see display.rs) -- idle low.
            let cs = Output::new(p.PIN_6, Level::Low);
            let display = display::SharpMemoryDisplay::new(spi, cs);

            // Own GPS (PA1010D over I2C1, STEMMA QT) -- SDA=GPIO2/SCL=GPIO3
            // (confirmed via the same CircuitPython board source used
            // elsewhere: DEFAULT_I2C_BUS_SDA/SCL -- and independently by
            // the compiler itself: `SclPin<I2C0>`/`SdaPin<I2C0>` aren't
            // implemented for these pins at all, only `I2C1`). Blocking
            // mode: polled every 200ms (gps.rs), nowhere near fast enough
            // to need DMA.
            let i2c = I2c::new_blocking(p.I2C1, p.PIN_3, p.PIN_2, I2cConfig::default());

            // Own battery -- A0/GPIO26/ADC0, external 2:1 divider onto BAT
            // (matches the payload's own A0 wiring, see CLAUDE.md).
            let adc = Adc::new_blocking(p.ADC, AdcConfig::default());
            let adc_channel = AdcChannel::new_pin(p.PIN_26, Pull::None);

            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                spawner.spawn(button_task(buttons, BUTTON_EVENTS.sender()).unwrap());
                spawner.spawn(display_task(display).unwrap());
                spawner.spawn(gps::gps_task(i2c).unwrap());
                spawner.spawn(battery::battery_task(adc, adc_channel).unwrap());
            });
        },
    );

    // Radio gets exclusive ownership of hardware SPI1 -- safe now that the
    // display moved to its own PIO bus (see display.rs). Async/DMA mode,
    // not blocking: lora-phy needs an embedded-hal-async SpiDevice.
    // Baudrate is local to this MCU<->radio-chip link only (doesn't affect
    // anything over the air), kept modest for signal margin over the
    // board's short SPI traces.
    let mut radio_spi_config = SpiConfig::default();
    radio_spi_config.frequency = 1_000_000;
    let radio_spi = Spi::new(
        p.SPI1,
        p.PIN_14,
        p.PIN_15,
        p.PIN_8,
        p.DMA_CH0,
        p.DMA_CH1,
        Irqs,
        radio_spi_config,
    );
    // RFM_CS -- standard active-low SPI chip select (unlike the display's
    // unusual active-high CS), so ExclusiveDevice's built-in cs.set_low()
    // /set_high() handling is correct here without any hand-rolled driver.
    let radio_cs = Output::new(p.PIN_16, Level::High);
    let radio_spi_device = radio::RadioSpiDevice::new(radio_spi, radio_cs, Delay).unwrap();
    // RFM_RST -- idle high; GenericSx127xInterfaceVariant::reset() pulses
    // it low to actually reset the chip.
    let radio_reset = Output::new(p.PIN_17, Level::High);
    // RFM_IO0 -- DIO0, carries RxDone/TxDone/CadDone. No pull needed; the
    // radio module drives this line itself.
    let radio_dio0 = Input::new(p.PIN_21, Pull::None);
    // RFM_IO1 -- DIO1, carries RxTimeout only (see radio.rs's docs on
    // Radio::new -- required, not optional, for RxMode::Single to ever
    // return on an empty receive window instead of hanging forever).
    let radio_dio1 = Input::new(p.PIN_22, Pull::None);
    // Onboard red LED (D13) -- free for core0's use now that core1's
    // temporary boot diagnostics were removed. A real link-activity
    // indicator, not a throwaway debugging aid: with no debug probe
    // attached, defmt::info! output isn't visible at all, so this is the
    // only way to observe RX/TX activity on real hardware. See
    // core0_task's use of it for the blink pattern.
    let radio_led = Output::new(p.PIN_13, Level::Low);

    let executor0 = EXECUTOR0.init(Executor::new());
    executor0.run(|spawner| {
        spawner.spawn(
            core0_task(
                BUTTON_EVENTS.receiver(),
                radio_spi_device,
                radio_reset,
                radio_dio0,
                radio_dio1,
                radio_led,
            )
            .unwrap(),
        );
    });
}

#[embassy_executor::task]
async fn button_task(
    mut buttons: Buttons,
    events_out: Sender<'static, CriticalSectionRawMutex, ButtonEvent, BUTTON_EVENT_CAPACITY>,
) {
    loop {
        Timer::after_millis(buttons::DEBOUNCE_MS).await;

        // HoldTracker::poll's callback is synchronous (it can't `.await`
        // a channel send mid-poll), so collect here and send after --
        // capacity 3 matches the button count, since at most one event
        // per button can fire per poll.
        let mut fired: [Option<ButtonEvent>; 3] = [None; 3];
        let mut count = 0;
        buttons.poll(|key_number, edge| {
            // Matches ground/code.py's `print("BUTTON EVENT:", name, event)`.
            defmt::info!(
                "BUTTON EVENT: {} {}",
                buttons::BUTTON_NAMES[key_number],
                buttons::edge_name(edge)
            );
            if count < fired.len() {
                fired[count] = Some((key_number, edge));
                count += 1;
            }
        });

        for event in fired.into_iter().flatten() {
            match event {
                // MENU on SUMMARY goes back to FLIGHTS specifically, not
                // the next screen in the normal rotation -- and clears
                // any stale request state so a leftover Ready/Failed
                // from the last selection doesn't flash up early next
                // time. Entirely local, matching MENU's usual reach.
                (2, Edge::Tap) if screen::current() == screen::SUMMARY => {
                    screen::to_flights();
                    summary_request::reset().await;
                }
                // MENU elsewhere: always advances, entirely local to
                // core1 (only ever touches the display) -- never
                // forwarded.
                (2, Edge::Tap) => screen::advance(),
                // FLIGHTS: button 0 cycles the list cursor instead of
                // its usual "go back" meaning. A tap suffices -- nothing
                // here needs a hold's mis-press guard, there's no
                // command firing from a mere selection change.
                (0, Edge::Tap) if screen::current() == screen::FLIGHTS => {
                    let count = flight_index::FLIGHT_INDEX.lock().await.count();
                    screen::cycle_selected(count);
                }
                // SUMMARY: both buttons besides MENU are inert by design
                // (see screen_footer.rs) -- swallow them here rather
                // than falling through to ARM/DISARM-as-BACK or a
                // forwarded CHIRP.
                (0 | 1, _) if screen::current() == screen::SUMMARY => {}
                // ARM/DISARM-as-BACK: off FLIGHT (and not FLIGHTS/
                // SUMMARY, both handled above), either gesture navigates
                // back immediately -- a tap, not the full 2s hold, since
                // there's no command to arm on these screens to guard
                // against a mis-press. Entirely local, and specifically
                // NOT forwarded, so core0 never sees an ARM/DISARM press
                // that doesn't actually mean "send it".
                (0, _) if screen::current() != screen::FLIGHT => screen::back(),
                // On FLIGHT, only a genuine hold means "send ARM/DISARM"
                // -- a mere tap here does nothing, matching code.py
                // (which only ever dispatches this button on
                // `event == "hold"`).
                (0, Edge::Tap) => {}
                // Everything else needs the radio: CHIRP (on FLIGHT or
                // FLIGHTS -- FLIGHTS' meaning, "select and request this
                // flight's summary," is resolved core0-side, see
                // main.rs's button-forwarding handler, since it needs
                // screen::selected() and the radio, both already
                // reachable from there) and ARM/DISARM holds that got
                // here specifically because we're on FLIGHT.
                _ => events_out.send(event).await,
            }
        }
    }
}

#[embassy_executor::task]
async fn display_task(mut display: display::SharpMemoryDisplay<'static, PIO0, 0>) {
    // The scrolling debug log (radio::RADIO_LOG) that lived here during
    // radio bring-up is kept (not deleted -- see radio.rs) but no longer
    // rendered as the primary screen now that FLIGHT is real; it's still
    // populated in the background for whenever a DIAG screen wants it.
    loop {
        // Redrawn from a blank buffer every cycle -- the display has no
        // way to erase just the changed part without clearing first.
        let _ = display.clear(BinaryColor::Off);
        draw_current_screen(&mut display).await;
        // Also services VCOM -- see display.rs's module docs on why that
        // still matters even when content is unchanged between calls.
        display.show();
        Timer::after_millis(DISPLAY_PERIOD_MS).await;
    }
}

/// Port of `code.py`'s top-level `draw()`: header always, then either the
/// NO TELEMETRY fallback or the current screen's body, then the footer.
async fn draw_current_screen(display: &mut display::SharpMemoryDisplay<'static, PIO0, 0>) {
    // Copied out from behind the lock immediately (LinkState is Copy),
    // not held across the ~50ms SPI render below -- core0 updates this
    // same lock on every received frame.
    let link = *link::LINK.lock().await;
    let my_gps = *gps::MY_GPS.lock().await;
    let my_batt = *battery::MY_BATT.lock().await;
    let cmd_log = cmdlog::snapshot().await;
    let summary_request_state = *summary_request::REQUEST.lock().await;
    let flight_index_snapshot = flight_index::FLIGHT_INDEX.lock().await;
    let flight_index_state = flight_index_snapshot.state;
    let prefetch_progress = flight_index_snapshot.prefetch_progress();
    drop(flight_index_snapshot);
    let my_wall_clock_ms = gps::EPOCH_OFFSET
        .lock()
        .await
        .map(|o| o.wall_clock_ms(Instant::now().as_millis() as u32));

    let now = Instant::now();
    let age_ms = link.age_ms(now);
    let status = link_status(age_ms);
    let fix_age_ms = link.fix.map(|f| (now - f.latched_at).as_millis() as u32);

    let frame = frame::Frame {
        tel: link.latest.as_ref().map(|(t, _, _)| t),
        rssi: link.latest.as_ref().map(|(_, r, _)| *r),
        snr: link.latest.as_ref().map(|(_, _, s)| *s),
        packets: radio::PACKET_COUNT.load(Ordering::Relaxed),
        rejects: radio::REJECT_COUNT.load(Ordering::Relaxed),
        status,
        fix_lat: link.fix.map(|f| f.lat),
        fix_lon: link.fix.map(|f| f.lon),
        fix_age_ms,
        my_lat: my_gps.map(|f| f.lat),
        my_lon: my_gps.map(|f| f.lon),
        my_heading: my_gps.and_then(|f| f.heading),
        my_batt,
        // supervisor.runtime.usb_connected has no bare-metal equivalent
        // wired up yet -- see battery.rs's docs.
        my_charging: false,
        my_wall_clock_ms,
        // ARM/DISARM pending-confirmation status (code.py's `tx_status`,
        // `CMD_CONFIRM_FRAMES`) -- see cmdlog.rs, driven from core0_task.
        tx_status: if cmd_log.tx_status.is_empty() {
            "ready"
        } else {
            &cmd_log.tx_status
        },
        cmd_log: &cmd_log.lines,
        screen_name: screen::current_name(),
        next_screen_name: screen::next_name(),
        prev_screen_name: screen::prev_name(),
        selected_flight: screen::selected(),
        summary_request: summary_request_state,
        flight_index_state,
        prefetch_progress,
    };

    screen_header::draw(display, &frame);

    // MISSING replaces the current screen's body whenever nothing has
    // ever arrived, or the last frame is older than TELEMETRY_MISSING_MS
    // -- so a payload that goes quiet mid-flight reverts here too, not
    // just before the very first frame (see screen_missing.rs).
    if telemetry_missing(age_ms) {
        screen_missing::draw(display, &frame);
    } else if let Some(t) = frame.tel {
        match screen::current() {
            screen::RECOVERY => screen_recovery::draw(display, &frame),
            screen::DIAG => screen_diagnostics::draw(display, &frame, t),
            screen::FLIGHTS => screen_flights::draw(display, &frame),
            screen::SUMMARY => screen_summary::draw(display, &frame),
            _ => screen_flight::draw(display, &frame, t),
        }
    }

    screen_footer::draw(display, &frame, screen::current(), frame.prev_screen_name);
}

#[embassy_executor::task]
async fn core0_task(
    events_in: Receiver<'static, CriticalSectionRawMutex, ButtonEvent, BUTTON_EVENT_CAPACITY>,
    radio_spi: radio::RadioSpiDevice,
    radio_reset: Output<'static>,
    radio_dio0: Input<'static>,
    radio_dio1: Input<'static>,
    mut led: Output<'static>,
) {
    let mut radio = match radio::Radio::new(radio_spi, radio_reset, radio_dio0, radio_dio1).await {
        Ok(r) => r,
        Err(e) => {
            // No radio, no point looping -- matches ground/code.py's
            // _init_radio, which appends to self.errors and leaves
            // self.radio unset rather than crashing, but this firmware
            // doesn't have a diagnostics screen to surface that to yet.
            // 5 slow blinks so an init failure is visually distinct from
            // "still booting"/"working fine" without needing a probe.
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
    // 3 quick blinks confirm radio init succeeded, distinct from the
    // failure pattern above and from the per-packet activity blinks
    // below -- visible proof of life with no debug probe attached.
    for _ in 0..3 {
        led.set_high();
        Timer::after_millis(80).await;
        led.set_low();
        Timer::after_millis(80).await;
    }

    let mut seq: u16 = 0;

    loop {
        // TEMP diagnostic (see docs/rust-rewrite.md bug log, 2026-08-17):
        // a brief pulse every iteration, regardless of outcome. With no
        // debug probe, "no blink" was ambiguous between "looping fine but
        // every RX attempt times out" and "core0 died/hung after the
        // first iteration" -- this makes those visually distinguishable:
        // a steady ~500ms-ish flicker means the loop is alive either way;
        // total silence means it isn't reaching here at all.
        led.set_high();
        Timer::after_millis(10).await;
        led.set_low();

        // Non-blocking: a queued button command gets sent this iteration,
        // but a human being slow (or absent) never stalls the RX poll
        // below -- ground/code.py's reference dispatch is at ~L449-465.
        // Screen navigation (MENU's advance, and ARM/DISARM-as-BACK off
        // FLIGHT) is filtered out entirely on core1 (see screen.rs and
        // button_task) -- it only touches the display, so it never
        // reaches this channel at all. Any (0, Hold) that does arrive
        // here is therefore already known to mean "on FLIGHT, send the
        // command" -- no screen check needed on this side.
        if let Ok((key_number, edge)) = events_in.try_receive() {
            defmt::info!(
                "core0: received {} {}",
                buttons::BUTTON_NAMES[key_number],
                buttons::edge_name(edge)
            );

            // FLIGHTS' CHIRP-button meaning when the cache came back
            // genuinely empty ("TAP:REFRESH", see screen_footer.rs) --
            // there's nothing to select, so re-check with the rocket
            // instead. Just an invalidate: the auto-fetch condition in
            // this same loop (below) picks up the resulting `Idle` state
            // on its very next tick since we're still on FLIGHTS with a
            // live link, no direct `GET_FLIGHT_INDEX` send needed here.
            if (key_number, edge) == (1, Edge::Tap)
                && screen::current() == screen::FLIGHTS
                && flight_index::FLIGHT_INDEX.lock().await.state == flight_index::IndexState::Empty
            {
                flight_index::invalidate().await;
                continue;
            }

            // FLIGHTS' CHIRP-button meaning ("select the highlighted
            // flight, request its summary") is resolved here rather
            // than in the generic cmd match below -- it isn't a
            // Command::* at all until this point (the index is only
            // known here, via screen::selected()), and its post-send
            // bookkeeping (summary_request::start, not cmdlog) and
            // screen transition are both different from every other
            // forwarded button press.
            if (key_number, edge) == (1, Edge::Tap) && screen::current() == screen::FLIGHTS {
                let idx = screen::selected();
                // Already cached (revisiting a flight already viewed
                // this session)? Jump straight to SUMMARY with no radio
                // round trip at all -- see flight_index.rs's docs.
                let cached = flight_index::FLIGHT_INDEX.lock().await.cached_summary(idx);
                if let Some(summary) = cached {
                    summary_request::show_cached(summary).await;
                    screen::to_summary();
                } else {
                    seq = seq.wrapping_add(1);
                    match radio.send_command(seq, common::Command::GET_SUMMARY_BASE + idx).await {
                        Ok(()) => {
                            summary_request::start(idx, radio::PACKET_COUNT.load(Ordering::Relaxed)).await;
                            screen::to_summary();
                        }
                        Err(e) => defmt::error!("core0: send_command (get_summary) failed: {}", e),
                    }
                }
                continue;
            }

            // DIAG's CHIRP-button meaning ("manually invalidate the
            // flight cache") -- repurposed there specifically (see
            // screen_footer.rs) rather than sending CHIRP, which DIAG
            // has less use for than a debug-oriented forced rebuild.
            if (key_number, edge) == (1, Edge::Tap) && screen::current() == screen::DIAG {
                flight_index::invalidate().await;
                continue;
            }

            let cmd = match (key_number, edge) {
                // ARM/DISARM share one button (a 2s hold), so which one
                // this hold means depends on the last known rocket state:
                // exactly ARMED means abort-and-rewind; anything past
                // that (BOOST through LANDED -- see Frame::recoverable,
                // broadened 2026-08-19 so a flight stuck mid-state-
                // machine isn't unrecoverable) means RECOVER instead.
                (0, Edge::Hold) => {
                    let latest = link::LINK.lock().await.latest;
                    let armed = latest.as_ref().is_some_and(|(t, _, _)| t.state == common::State::ARMED);
                    let recoverable = latest.as_ref().is_some_and(|(t, _, _)| {
                        matches!(
                            t.state,
                            common::State::BOOST
                                | common::State::COAST
                                | common::State::APOGEE
                                | common::State::DESCENT
                                | common::State::LANDED
                        )
                    });
                    if armed {
                        Some((common::Command::DISARM, false))
                    } else if recoverable {
                        // "RECOVER" (screen_footer.rs's "HOLD:RECOVER") --
                        // same wire command as DISARM, the rocket tells
                        // the two apart by its own current state
                        // (rocket/src/main.rs). Not gated on NOGO below:
                        // silencing the beacon isn't a launch-safety call.
                        // The `true` here is purely cosmetic -- it tells
                        // cmdlog to log "SENT RECOVER.../RECOVERED OK"
                        // instead of "SENT DISARM.../DISARMED OK", since
                        // it's the same wire command either way.
                        Some((common::Command::DISARM, true))
                    } else if latest.as_ref().is_some_and(|(t, _, _)| nogo_reason(t).is_some()) {
                        // Defense in depth, not the primary guard: the
                        // footer (screen_footer.rs) already suppresses
                        // "HOLD:ARM" and shouldn't invite this press at
                        // all when NOGO applies. This still refuses the
                        // send even if something forwarded the event
                        // anyway (a stray/queued press, a future screen
                        // that forgets the check, etc.) -- an ARM must
                        // never reach the radio while NOGO is active,
                        // full stop.
                        None
                    } else {
                        Some((common::Command::ARM, false))
                    }
                }
                (1, Edge::Tap) => Some((common::Command::CHIRP, false)),
                _ => None,
            };

            if let Some((cmd, recover)) = cmd {
                seq = seq.wrapping_add(1);
                match radio.send_command(seq, cmd).await {
                    Ok(()) => {
                        defmt::info!("core0: sent command {} (seq {})", cmd, seq);
                        // Snapshot PACKET_COUNT *before* it can advance any
                        // further -- matches code.py's `link.packets` at
                        // the moment of send, the baseline `cmdlog::poll`
                        // counts confirmation frames against.
                        cmdlog::record_send(cmd, radio::PACKET_COUNT.load(Ordering::Relaxed), recover).await;
                        // One deliberate, longer flash -- distinct from
                        // the per-packet RX flicker below -- so a command
                        // send is visually identifiable on its own.
                        led.set_high();
                        Timer::after_millis(150).await;
                        led.set_low();
                    }
                    Err(e) => defmt::error!("core0: send_command failed: {}", e),
                }
            }
        }

        match radio.try_receive_frame().await {
            Ok(Some(radio::RxFrame::Telemetry(radio::RxResult { telemetry, rssi, snr }))) => {
                defmt::info!(
                    "core0: telemetry counter={} state={}",
                    telemetry.counter,
                    telemetry.state_name()
                );
                radio::log_line(format_args!(
                    "TELEMETRY #{} {} {:.1}V",
                    telemetry.counter,
                    telemetry.state_name(),
                    telemetry.batt_volts
                ))
                .await;
                link::LINK.lock().await.record_rx(telemetry, rssi, snr, Instant::now());
                // Toggled (not blinked) each received frame: with the
                // rocket beaconing steadily, this reads as a flicker/
                // heartbeat proving live RX without needing a probe.
                led.toggle();
            }
            Ok(Some(radio::RxFrame::Summary(radio::SummaryRxResult { summary, rssi, snr }))) => {
                defmt::info!("core0: summary flight_index={} rssi={} snr={}", summary.flight_index, rssi, snr);
                radio::log_line(format_args!("SUMMARY flight {} rssi={rssi} snr={snr}", summary.flight_index)).await;
                summary_request::record_response(summary).await;
                // Also persist into the durable per-flight cache -- see
                // flight_index.rs's docs on the split between that (the
                // cache) and summary_request (the current request's
                // transient UI status).
                flight_index::record_summary(summary).await;
                led.toggle();
            }
            Ok(Some(radio::RxFrame::FlightIndex(radio::FlightIndexRxResult { timestamps }))) => {
                defmt::info!("core0: flight_index count={}", timestamps.len());
                radio::log_line(format_args!("FLIGHT_INDEX count={}", timestamps.len())).await;
                flight_index::record_response(&timestamps).await;
                led.toggle();
            }
            // Timeout (nothing arrived this ~0.5s window) or a frame that
            // failed unpack_telemetry's, unpack_summary's, and
            // unpack_flight_index's validation -- all normal and not
            // worth logging every cycle.
            Ok(None) => {}
            Err(e) => {
                defmt::error!("core0: try_receive_frame failed: {}", e);
                // TEMP diagnostic: blink the error's code (see
                // radio::error_blink_code) so the specific RadioError
                // variant can be identified and reported back without a
                // probe. Long gap before/after so this run is countable
                // and distinct from the 10ms heartbeat pulse above.
                Timer::after_millis(400).await;
                for _ in 0..radio::error_blink_code(&e) {
                    led.set_high();
                    Timer::after_millis(200).await;
                    led.set_low();
                    Timer::after_millis(200).await;
                }
            }
        }

        // -- confirm or fail a pending ARM/DISARM ------------------------
        // Every iteration, not just when a frame just arrived -- the
        // link-lost branch below has to fire even if nothing is arriving
        // at all to count. Matches ground/code.py ~L480-494.
        let snapshot = *link::LINK.lock().await;
        let current_state = snapshot.latest.as_ref().map(|(t, _, _)| t.state);
        let status = link_status(snapshot.age_ms(Instant::now()));
        let outcome = cmdlog::poll(current_state, radio::PACKET_COUNT.load(Ordering::Relaxed), status).await;
        if outcome == Some(cmdlog::PollOutcome::RecoverSucceeded) {
            // A new flight was just archived on the rocket -- whatever
            // FLIGHTS/SUMMARY have cached is now stale. See
            // flight_index.rs's docs on this being one of its two
            // invalidation triggers.
            flight_index::invalidate().await;
        }
        summary_request::poll_timeout(radio::PACKET_COUNT.load(Ordering::Relaxed), status).await;
        flight_index::poll_timeout(radio::PACKET_COUNT.load(Ordering::Relaxed), status).await;

        // -- auto-fetch the flight index whenever FLIGHTS needs one and
        // doesn't have one -------------------------------------------------
        // Checked every iteration rather than on a screen-transition
        // edge -- simpler and more robust than trying to detect "just
        // navigated here" across the core1/core0 boundary, and it
        // naturally covers both "just navigated to FLIGHTS" and "cache
        // just got invalidated while already there." See
        // flight_index.rs's docs.
        let idle_and_on_flights = {
            let idx = flight_index::FLIGHT_INDEX.lock().await;
            idx.state == flight_index::IndexState::Idle && screen::current() == screen::FLIGHTS
        };
        if idle_and_on_flights && matches!(status, LinkStatus::Live | LinkStatus::Stale) {
            seq = seq.wrapping_add(1);
            match radio.send_command(seq, common::Command::GET_FLIGHT_INDEX).await {
                Ok(()) => flight_index::start_fetch(radio::PACKET_COUNT.load(Ordering::Relaxed)).await,
                Err(e) => defmt::error!("core0: send_command (get_flight_index) failed: {}", e),
            }
        }

        // -- background summary prefetch -------------------------------
        // Once the index itself is Ready, walk every not-yet-cached
        // flight in the background so a full review works later with
        // the rocket powered off -- see flight_index.rs's docs. Not
        // gated on the FLIGHTS screen (unlike the index fetch above):
        // the whole point is to keep making progress even after the
        // user has moved on to another screen. Skipped while a manual
        // FLIGHTS selection has its own request outstanding -- only one
        // GET_SUMMARY_BASE in flight at a time on this half-duplex link.
        let manual_request_pending =
            matches!(*summary_request::REQUEST.lock().await, summary_request::SummaryRequest::Pending { .. });
        if !manual_request_pending && matches!(status, LinkStatus::Live | LinkStatus::Stale) {
            if let Some(idx) = flight_index::next_to_prefetch().await {
                seq = seq.wrapping_add(1);
                match radio.send_command(seq, common::Command::GET_SUMMARY_BASE + idx).await {
                    Ok(()) => flight_index::start_prefetch(idx, radio::PACKET_COUNT.load(Ordering::Relaxed)).await,
                    Err(e) => defmt::error!("core0: send_command (prefetch summary) failed: {}", e),
                }
            }
        }
    }
}
