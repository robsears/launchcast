# Rust Rewrite — Planning Notes

Starting point for a dedicated thread on rewriting the LaunchCast firmware
(both boards) in Rust. This captures the decision, the research behind it,
and a strawman plan — not a committed design. Hardware wiring/pinout/parts
are documented in `wiring.md` and `CLAUDE.md`; this doc doesn't repeat them.

## Status

Started 2026-08-16. Decisions made to unblock the first slice of work (see
"Open questions" below, now partially resolved):
- **Framework: `embassy-rp`**, not bare `rp2040-hal` — async executor, and
  its channel primitives are the inter-core communication path called for
  in the strawman architecture below.
- **Port order: ground station first** — lower blast radius than the
  rocket, and it's the board with the concrete open complaint (button
  responsiveness / GC pauses) this rewrite exists to fix.
- **First slice: Cargo workspace + `common/packet.py` ported and
  host-tested**, no hardware yet.

Done so far, at `rust/` (new directory, workspace root):
- `rust/common` (crate `launchcast-common`): full port of `common/packet.py`
  — `pack_telemetry`/`unpack_telemetry`, `pack_command`/`unpack_command`,
  `State`, `Sensor`, `Command`, and the scaling helpers. `#![no_std]` (only
  `std` in `cfg(test)` builds), alloc-free, one dependency (`libm`, for the
  `f32::round` that bare `core` doesn't provide). Builds clean for both the
  host and `thumbv6m-none-eabi`.
- `rust/common/tests/packet.rs`: line-for-line translation of every case in
  `tests/test_packet.py` (43 tests, all passing) — the Python suite's
  *behavior* used as the spec, per the migration strategy below. Keep the
  two files in sync when either changes.
- `rust/ground-logic` (crate `launchcast-ground-logic`): hardware-free
  ground-station logic, same `no_std`/host-testable shape as `common`.
  Currently just `HoldTracker` — the tap/hold/bounce-bridging state machine
  from `ground/hold_tracker.py`, including the grace-window bounce fix.
  Keys are addressed by a small integer `key_number` (array index) rather
  than the Python version's `name` string dict lookup — the natural
  no_std/no-alloc shape for a handful of physical buttons; mapping a fired
  event back to a name for logging is left to the firmware layer that
  hasn't been built yet. `rust/ground-logic/tests/hold_tracker.rs` is a
  1:1 translation of all 10 cases in `tests/test_hold_tracker.py`,
  including the bounce-bridging regression cases, all passing. Also has
  `icons` — the `BATT_CURVE` discharge-curve interpolation
  (`battery_percent`/`battery_level`) from `ground/icons.py`, ported and
  tested the same way (`tests/icons.rs`, 8 cases from `test_icons.py`).
  `icons.py`'s bitmap-drawing functions (`draw_battery`, `draw_bitmap`,
  `signal_level`/`signal_percent`, ...) aren't ported — they depend on a
  `display`-shaped sink and are left for the eventual `embedded-graphics`
  `DrawTarget` wiring. Also has `nav` (`ground/nav.py`'s
  `haversine_m`/`bearing_deg`/`compass_point`/`relative_arrow`, using
  `libm` for the trig — `f32::rem_euclid` needs `std`, so Python's
  always-nonnegative `%` is hand-rolled instead) and `units`
  (`ground/units.py`'s C→F/m→ft conversions). `units.py`'s `UNITS` is a
  mutable module-level string toggled by a future settings screen; the
  Rust port makes that an explicit `Copy` enum (`Units::Imperial` /
  `Units::Metric`) passed to each call instead of a mutable global —
  `no_std` has no implicit interior mutability without pulling in a
  synchronization primitive, and it's the more idiomatic shape either way.
  `rust/ground-logic/tests/nav.rs` and `tests/units.rs` port all 16 + 6
  cases from `test_nav.py`/`test_units.py`; the two SF↔LA/bearing symmetry
  tolerances were loosened slightly (1e-6 → 1e-3/1e-4) since this port is
  f32 throughout and the Python reference ran at f64. Also has `imu`
  (`ground/imu.py`'s `accel_magnitude_g` — the ground-display sibling of
  `rocket-logic`'s `accel_magnitude`, already in g-units so no gravity
  division), 4 cases from `test_imu.py`.

  **Every hardware-free file in the tree is now ported** — checked directly:
  `display_util.py`, all `screen_*.py`, `rocket_art.py`, `handheld_art.py`,
  and both `boot.py`s all take a `display`/filesystem object and have no
  corresponding test file, confirming they're display/hardware-dependent,
  not pure-logic candidates.
- `rust/ground` (crate `launchcast-ground`): an `embassy-rp` firmware crate,
  **not yet flashed to real hardware**. Buttons are now wired to real GPIO
  (`src/buttons.rs`): `PIN_9`/`PIN_10`/`PIN_11` (confirmed against
  `docs/images/feather pinout.png`, not guessed — these are the RP2040 GPIO
  numbers behind `board.D9`/`D10`/`D11`), active-low with internal pull-up,
  matching `ground/code.py`'s `Hardware._init_buttons` exactly. Sampled on
  a fixed `DEBOUNCE_MS` (50 ms) timer — a raw level change between two
  samples that far apart is treated as a real edge, the same poll-interval
  debounce tradeoff `keypad.Keys(interval=DEBOUNCE_MS/1000.0)` makes on the
  Python side, just without a background supervisor scan doing the
  sampling. Edges feed straight into `launchcast_ground_logic::HoldTracker`
  (now an actual dependency, not just a sibling crate), dispatching a
  `BUTTON EVENT: name event` defmt log matching `ground/code.py`'s print.
  Radio/GPS/display are still untouched, so the actual ARM/DISARM/CHIRP/
  MENU actions those events should trigger are a `TODO` in `main.rs`
  pointing at `ground/code.py`'s reference dispatch (~L449-465) — nothing
  to wire them to yet.

  **Display**: since neither ecosystem crate is usable (see the ecosystem
  table), `src/display.rs` is a hand-rolled `SharpMemoryDisplay` driver for
  the LS027B7DH01 (400x240), implementing `embedded_graphics::DrawTarget`.
  Confirmed via the actual Feather pinout (`docs/images/feather
  pinout.png`) and the user's direct wiring confirmation: SCK/MOSI on the
  shared `board.SPI()` pins (GPIO14/15), CS on `PIN_6` (GPIO6), no MISO —
  the panel is write-only. The wire protocol (command byte, per-line
  bit-reversed address + raw pixel data + trailing zero, final trailing
  zero, VCOM toggle every frame) was read directly out of Adafruit's
  CircuitPython driver source (`adafruit_sharpmemorydisplay`, fetched and
  quoted, not recalled from memory) rather than assumed — notably chip
  select is **active-high**, confirmed from that source
  (`cs_active_value=True`), which is the opposite of the more common
  active-low convention and an easy thing to get backwards guessing.
  `ground/code.py`'s `DISPLAY_HZ = 2.0` comment ("also services VCOM") is
  mirrored directly: there's no separate lightweight VCOM-only path, just
  a periodic full-frame `show()`.
  `show()` uses blocking (not DMA/async) SPI, so a ~50ms transfer twice a
  second genuinely blocks whatever shares its core — see the multicore
  entry below for where that cost ended up living.
  **Protocol correctness is unverified against real hardware** — nothing
  in `rust/ground` has been flashed to a board yet.

  **Multicore split — built, not just planned.** `main.rs` now runs two
  independent `embassy_executor::Executor`s via
  `embassy_rp::multicore::spawn_core1`, not two tasks time-sliced on one
  executor: **core1** runs `button_task` + `display_task` (so the ~50ms
  blocking display refresh only ever costs button latency, never
  radio/GPS timing); **core0** runs `core0_task`, which for now just
  receives dispatched button events over `BUTTON_EVENTS` — a bounded
  `embassy_sync::channel::Channel<CriticalSectionRawMutex, (usize, Edge),
  8>` — and logs them, standing in for the eventual ARM/DISARM/CHIRP
  radio dispatch. This is the split decided above, not the original
  strawman (buttons alone on core0): revised specifically because of the
  ~50ms blocking display cost discovered while writing `display.rs`, on
  the reasoning that radio RX timing and GPS parsing matter more than
  shaving human-imperceptible milliseconds off button response.
  `CriticalSectionRawMutex` (not a plain mutex) is required for anything
  shared across the two cores; needed enabling `embassy-rp`'s
  `critical-section-impl` feature. Two real toolchain snags hit and fixed
  along the way, both because `thumbv6m-none-eabi` (Cortex-M0+) has no
  native atomic compare-exchange: `static_cell` needed its transitive
  `portable-atomic` dependency's `critical-section` feature enabled
  directly (Cargo feature unification doesn't reach through a crate that
  doesn't expose the flag itself) to fall back to a critical-section-based
  CAS emulation instead of a hardware instruction that doesn't exist on
  this chip.

  **A real, session-long linker bug found and fixed while producing the
  first flashable artifact.** Every `cargo build`/`clippy` for
  `thumbv6m-none-eabi` up to this point had been emitting a `rust-lld:
  cannot find entry symbol _start` warning, dismissed early on as a
  cosmetic rust-lld quirk (a plausible-sounding but WRONG guess, not
  verified). It was real: `rust/ground/.cargo/config.toml` (holding the
  `-Tlink.x -Tdefmt.x` rustflags cortex-m-rt/defmt need to place a real
  vector table and set the entry point) was never being discovered by
  Cargo, because Cargo walks *up* from the invocation directory looking
  for `.cargo/config.toml` and every build this session ran from `rust/`
  (the workspace root), never from `rust/ground/` itself. So the rustflags
  silently never applied, and every "successful" `thumbv6m-none-eabi`
  build/clippy pass was quietly producing a non-bootable binary — entry
  point `0x0`, no `.vector_table`/`.text` sections mapped at all — that
  happened not to hard-error. `cargo build` and `clippy` don't catch this
  class of bug; `elf2uf2-rs` refusing to convert the ELF ("entry point is
  not in mapped part of file") is what surfaced it. Fixed by moving the
  config to `rust/.cargo/config.toml` (the workspace root), scoped to
  `[target.thumbv6m-none-eabi]` so it only affects firmware crates, not
  `common`/`ground-logic`/`rocket-logic`'s host builds. Confirmed fixed:
  the rebuilt ELF has a valid entry point, a `Reset` symbol, and a real
  `.vector_table`/`.boot2`/`.text`/`.rodata`/`.data`/`.bss` layout, and
  `elf2uf2-rs` now converts it to a `.uf2` successfully.
- `rust/rocket-logic` (crate `launchcast-rocket-logic`): hardware-free
  rocket-side logic, same shape as `ground-logic`. `FlightState` (the
  BOOST/COAST/APOGEE/DESCENT/LANDED transition table and the barometric
  velocity EMA) and `accel_magnitude`, ported from `rocket/code.py`.
  Depends on `launchcast-common` for the `State` constants, mirroring the
  Python side's `from packet import State`. `FlightLog` and everything
  sensor/radio/flash-shaped stayed in Python — out of scope for a
  hardware-free port. `rust/rocket-logic/tests/flight_state.rs` ports the
  full synthesized-D12-flight harness from `test_flight_state.py`
  (`d12_profile`, `alt_to_pressure`, `run_profile`) plus all 29 of its
  cases, including the tight boost/coast/apogee/landed timing-window
  assertions — all passing on the first run, no threshold adjustments
  needed despite the port switching from Python's f64 to f32 throughout
  (matching what the embedded firmware will actually use).
- Toolchain management moved off `rustup` entirely: the devshell now gets
  its Rust toolchain from `rust-overlay`/`crane` (`nix/overlays.nix`,
  `craneLib.devShell` in `nix/devshells.nix`), including the
  `thumbv6m-none-eabi` target declared directly in the overlay's
  `rust-bin` override. `rust/rust-toolchain.toml` (the old rustup-based
  pin) was removed as dead config — a plain `rust-overlay`-provided
  `rustc`/`cargo` doesn't read rustup toolchain files, so once `rustup`
  came out of `nix/common.nix` nothing was consulting it. A C toolchain
  (`gcc`, needed for the host test linker) is still in `nix/common.nix`.

**First real hardware flash (2026-08-17): confirmed working.** `ground`'s
multicore firmware (buttons + display on core1, radio/GPS-stub on core0)
now boots on a real Feather RP2040 RFM95 handheld and renders a boot screen
(border + "LAUNCHCAST" / "ground -- rust boot ok") on the physical Sharp
Memory LCD. Getting there required diagnosing a real bug, not just flashing
and hoping:

- **Bisection method, since there's no debug probe attached and no serial
  console once CircuitPython is overwritten.** Built a ladder of
  increasingly-isolated smoke-test binaries (`src/bin/hello.rs`: blink the
  onboard LED (D13/GPIO13) with no SPI/display/multicore at all;
  `src/bin/display_clear.rs`: single-core SPI-only Sharp panel clear
  command; `src/bin/display_fill.rs`: single-core full-frame black/white
  toggle through the *real* `display.rs` driver via `#[path]`, not a copy)
  to separate "does the toolchain/flash/boot pipeline work at all" from
  "does the display driver/protocol work standalone" from "does it work
  under the real multicore/executor firmware." Each stage passed before
  moving to the next, which is what made the eventual bug findable instead
  of just a pile of confounded symptoms.
- **A wiring red herring along the way.** Adafruit's own pinout diagram
  page, read via an automated fetch, returned wrong GPIO numbers for the
  general-purpose SPI header (claimed GPIO18/19/8; actually GPIO14/15/8)
  and for `D6` (claimed GPIO6... an unrelated separate chart claimed
  GPIO8). Diagrams/scraped pinout pages are not reliable for this kind of
  exact-pin-number question. Settled conclusively by pulling
  `adafruit/circuitpython`'s actual compiled board definition for this
  exact board
  (`ports/raspberrypi/boards/adafruit_feather_rp2040_rfm/pins.c` and
  `mpconfigboard.h`) directly from GitHub — the real, board-specific
  source of truth: `SCK=GPIO14, MOSI=GPIO15, MISO=GPIO8, D6=GPIO6`,
  confirming the original pins in `main.rs`/`display.rs` were right all
  along.
- **The real bug: `CORE1_STACK` was far too small.** Declared as
  `Stack<4096>` (4KB). `SharpMemoryDisplay`'s framebuffer field alone
  (`buffer: [u8; FRAME_BYTES]`, 12000 bytes) is 3x that; constructing it in
  `SharpMemoryDisplay::new()` and moving it by value into
  `spawner.spawn(display_task(display).unwrap())` is not guaranteed to
  elide every transient stack copy through that chain, even under LTO, and
  a stack overflow with no debug probe attached produces total silence —
  no fault message, no defmt output, just a hung core. Found by adding an
  LED-blink checkpoint after every setup step in `main.rs`'s core1 closure
  (a temporary, deliberately crude technique — no probe, no serial, so the
  only observable signal available was the onboard LED) and narrowing the
  hang down to exactly `spawner.spawn(display_task(display).unwrap())`:
  `button_task`'s spawn (tiny captured state) succeeded every time;
  `display_task`'s spawn (captures the 12KB display struct by value) never
  did. 32KB was tried and still wasn't enough; 128KB (`Stack<131072>`)
  is confirmed working on real hardware with real margin — affordable
  given the RP2040's 256KB total SRAM. All temporary diagnostic code
  (blink helpers, checkpoint LED calls) removed from `main.rs` afterward;
  the three smoke-test binaries in `src/bin/` were kept as reusable
  hardware bring-up tools for future board/display changes.

**Display moved off hardware SPI1 onto a PIO-backed bus (2026-08-17),
before starting the radio port.** Investigating the LoRa driver crate
ecosystem (see below) surfaced a real architectural conflict: the onboard
RFM95's SPI pins (`SCK=GPIO14, MOSI=GPIO15, MISO=GPIO8`, confirmed via the
same CircuitPython board source used for the earlier pin mixup) are
physically the *same* pins as the general-purpose SPI header the display
is wired to — the radio and display share one hardware SPI1 peripheral,
differing only by CS pin. That's a problem specifically because of the
current core split (radio on core0, display on core1): a single hardware
peripheral can't be driven safely by two independent cores without real
cross-core arbitration, and even with a lock/hand-off protocol, one core's
in-flight transaction would still stall the other's — reintroducing
exactly the "button press, unclear if it registered" latency this
multicore split exists to eliminate. Checked whether the display could
instead move to a separate hardware SPI0 bus: no, every SPI0-capable
SCK/MOSI pin on this board is already claimed by the radio or by I2C
(reserved for the ground station's own future GPS). Resolved by giving the
display a fully independent **PIO-backed SPI bus**
(`embassy_rp::pio_programs::spi`, official embassy-rp code, blocking mode
via `Spi::new_blocking` — no DMA/interrupt complexity needed) on
previously-free GPIOs: CLK=D5/GPIO5, MOSI=D12/GPIO12 (CS stays on D6/GPIO6,
since it's just a manually-toggled GPIO, not part of the SPI peripheral
proper). The PIO SPI program is inherently full-duplex, so it still needs
a MISO argument even though the display never reads back — GPIO1 (D0/UART
RX) is passed for that and is wired to nothing. This is a real second
independent hardware state machine, not main-loop bit-banging, so it has
zero contention with the radio's SPI1 regardless of what either core is
doing. `display.rs`, `main.rs`, and both `display_clear.rs`/
`display_fill.rs` smoke tests updated and confirmed still building/
clippy-clean. **Confirmed on real hardware (2026-08-17)**: rewired
(CLK→D5, MOSI→D12), reflashed, boot screen rendered instantly.

**RFM95 (SX1276) LoRa radio driver added (2026-08-17), core0-side.** Built
on `lora-phy`'s SX127x `RadioKind` (MIT OR Apache-2.0, maintained under the
lora-rs/embassy-rs org — a real ecosystem check, same as the display driver
search: `sx127x_lora`/`radio-sx127x` were also found but `lora-phy` is the
one actually built for `embedded-hal-async`/embassy, not adapted to it).
Radio now has **exclusive** ownership of hardware SPI1 (async/DMA mode,
`embedded-hal-bus`'s `ExclusiveDevice`) — safe now that the display moved
off it. Pins: CS=GPIO16, RST=GPIO17, DIO0=GPIO21 (all confirmed via the
same CircuitPython board source used earlier). Over-the-air parameters
ported directly from `rocket/code.py`/`ground/code.py`'s `_init_radio`
(915MHz, SF7/BW125/CR4-5, tx_power=20dBm, CRC on) — not chosen fresh, since
this has to interoperate with the real, still-running rocket firmware.

**Dependency snag: crates.io's published `lora-phy` (3.0.1) can't set a
custom sync word at all.** `LoRa::new` only picks between the two
hardcoded LoRaWAN sync words (public 0x34 / private 0x12); checked all the
way down to `RadioKind::init_lora`, which at 3.0.1 takes
`is_public_network: bool`, not a raw word, at any level. This project's
`packet.SYNC_WORD` (0x2B) is neither LoRaWAN word, and there's no way
through 3.0.1's API to use it. `LoRa::with_syncword(radio_kind, sync_word:
u8, delay)`, which takes the sync word directly in the same legacy
single-byte form `adafruit_rfm9x.sync_word` already uses, exists on
`lora-rs/lora-rs`'s `main` branch but is unreleased as of this writing.
Pinned `ground/Cargo.toml`'s `lora-phy` dependency to that exact commit via
git (`rev = "5cd9cb3097770f6d976de012b2265676956cadc7"`, not floating on
`main`) rather than either vendoring a patch or giving up sync-word
compatibility — documented prominently in the Cargo.toml comment itself
since a future `cargo update`/version bump could silently lose this if
lora-phy publishes a release before adding the feature.

**Cross-core data flow, per the design discussed 2026-08-17:** UI core
(core1: buttons+display) and radio core (core0) stay fully separated, no
shared-bus arbitration needed between them (that's the whole point of the
PIO display move above) — they only talk through two `embassy_sync`
primitives, chosen for their actual shapes rather than reusing one
primitive for both directions:
- `BUTTON_EVENTS` (`Channel`, already existed): core1→core0, a FIFO of
  discrete tap/hold events, each meant to be seen exactly once.
- `ROCKET_STATE` (`Mutex<CriticalSectionRawMutex, Option<Telemetry>>`,
  new): core0→core1, a "latest value" slot overwritten on every received
  frame, not a queue — matches "UI polls for current state" rather than
  "UI must process every historical update." Not yet read by anything;
  real telemetry screens (still just the static boot screen) are the next
  piece that will poll it.

`core0_task` now: on each loop iteration, non-blockingly checks
`BUTTON_EVENTS` and sends a command if one's queued (ARM/DISARM share the
D9 hold, disambiguated by whether `ROCKET_STATE` currently reads exactly
`ARMED`; D10 tap sends CHIRP; D11/MENU never reaches this channel, core1
handles its screen-cycle response entirely locally) — never blocking the
RX loop below on a slow or absent human. Then attempts one
`RxMode::Single` receive (~500 symbols ≈ half a second at SF7/BW125) and
updates `ROCKET_STATE` on a valid frame. Half-duplex LoRa means TX and RX
genuinely can't happen simultaneously; interleaving single-shot RX
attempts with checking the command queue (rather than continuous RX plus
`select`-cancelling it when a command arrives) was the deliberate choice,
since `lora-phy`'s own docs warn `tx()`/`complete_rx()`-family calls are
**not safe to drop/cancel** — a cancelled RX could leave the radio in a
bad state, so a timeout-bounded single-shot receive is the only sound way
to interleave TX checks without ever cancelling an in-flight radio
operation.

**No debug probe attached, so `defmt::info!` output is invisible** on real
hardware (same limitation hit during the earlier boot-screen bring-up).
Added a permanent (not throwaway-diagnostic) link-activity LED on the
onboard red LED (D13, free now that core1's temporary boot-diagnostic
blinks were removed): 3 quick blinks on successful radio init, 5 slow
blinks on init failure, one ~150ms flash per command sent, a toggle per
telemetry frame received — the only way to visually confirm any of this
works without a probe.

**Found and fixed before any hardware test: `adafruit_rfm9x` wraps every
payload in a 4-byte RadioHead-style header, invisibly to `packet.py`.**
`send()` prepends `[to, from, id, flags]` before the application payload;
`receive()` strips it back off before returning — both automatic, and both
`rocket/code.py`/`ground/code.py` use the defaults (`with_header=False`),
so `common/packet.py`'s own encode/decode functions have never had to know
this header exists. `radio.rs`'s first pass read/wrote raw LoRa payloads
directly, with no knowledge of it at all — which would have silently
broken interop in both directions: real telemetry frames actually arrive
as `[4-byte header][40-byte payload]`, so `unpack_telemetry` would see the
header's first byte where it expects `MAGIC` and reject every genuine
frame; a bare 7-byte command sent from here would have 4 bytes stripped by
the rocket's `receive()`, leaving only 3 bytes for its `unpack_command` to
reject on a length check. Confirmed via the actual `adafruit_rfm9x`
source (not assumed) that neither Python side customizes
destination/node/identifier/flags, and — importantly — that both stay at
`node = 0xFF` (broadcast), which unconditionally bypasses the library's
destination-address filter regardless of header content; so the fix only
needs to get the 4-byte *framing* right, not match specific header byte
values. Fixed in `radio.rs`: `send_command` now prepends a 4-byte
broadcast header before the packed command; `try_receive_telemetry` now
strips 4 bytes before calling `unpack_telemetry`, and `rx_pkt_params`'s
`max_payload_length` grew from 40 to 44 to actually capture the full
on-air frame. This was caught by re-reading the actual CircuitPython
library source specifically because of the user's proposed test (RX
against the real, unmodified rocket firmware) — exactly the kind of gap
that "builds and clippy-checks clean" alone would never have surfaced.

**First real RX test against the real rocket hardware (2026-08-17):
radio initialized, then hung forever on the first receive attempt.**
Tested with the payload Feather (still running the original CircuitPython
firmware, unmodified) actually broadcasting. Ground station's onboard LED
showed the 3-blink init-success pattern, one 10ms heartbeat pulse (the
first loop iteration starting), then total silence -- no error blink, no
further heartbeat, nothing, until a manual reset. That ruled out both
"just not matching packets" and "erroring every attempt" (both of those
would still show *something* on the LED) -- it meant
`try_receive_telemetry` wasn't returning at all, in either direction.

**Root cause: only DIO0 was wired, but `RxMode::Single`'s timeout
depends on DIO1.** Sitting unread since the sx127x ecosystem research
earlier this session, `GenericSx127xInterfaceVariant`'s own doc comment
says it directly: "DIO0 carries RxDone/TxDone/CadDone, while RxTimeout is
only ever routed to DIO1... with only DIO0 wired, an RX window that hears
nothing never wakes the driver and the receive hangs forever." `Radio::new`
used the single-IRQ `GenericSx127xInterfaceVariant::new` constructor
(DIO0 only) — so the very first RX attempt with no packet yet in flight
(unavoidable; TX/RX are never perfectly synchronized) hit exactly that
hang, and would have hung the same way even with the rocket transmitting
correctly, since it never got past the *first* empty gap between frames.
Fixed by wiring DIO1 (RFM_IO1 = GPIO22, confirmed via the same
CircuitPython board source used for pins elsewhere) and switching to
`new_with_secondary_irq`, which watches both DIO0 and DIO1 and returns
whichever fires first. Builds/clippy-clean; **re-flash and retest
pending**.

This is the second time in this same investigation that a `lora-phy`
doc comment already contained the exact answer before the bug was ever
hit on hardware (the RadioHead framing issue was caught by re-reading
Python source; this one was sitting in already-fetched Rust doc comments)
— worth remembering to re-check documentation already gathered before
adding new diagnostic instrumentation next time a similar hang shows up.

**With DIO1 fixed, RX looped correctly but still never received a single
frame — root cause: the sync word. A real, previously-latent bug in the
CircuitPython codebase itself, invisible there since both sides failed
identically.** Bisection sequence: with no debug probe, `defmt` output is
invisible, so a scrolling text log rendered on the Sharp display
(`radio::RADIO_LOG`, a `Deque<heapless::String<48>, 20>` behind a
`Mutex`) was built specifically to get real visibility — `try_receive_telemetry`
now replicates `LoRa::complete_rx`'s loop by hand using only its public
building blocks (`process_irq_event`, `wait_for_irq`, `get_rx_result`,
`clear_irq_status`), instead of calling the opaque `complete_rx` directly,
logging each iteration. First observation: one "irq: none (x1)" line then
total silence for minutes, meaning the very first `wait_for_irq()` call
(waiting on DIO0 going high) never returned at all, even with the payload
actively transmitting the whole time. Ruled out DIO1 interference (retested
DIO0-only, identical result), ruled out `RxMode::Single` vs `Continuous`
(identical result either way), re-verified `LoRaMode::RxContinuous`'s
register bit value against the datasheet (correct), re-verified our own
pin wiring against the CircuitPython board source and confirmed the SPI
pin roles are compiler-type-checked correct. Decisive test: `LoRa::listen()`
+ repeated `get_rssi()` reads, bypassing all packet-detection/IRQ logic
entirely — showed RSSI clearly tracking the payload's real transmissions
(~-40dBm during TX, ~-108dBm ambient between/with payload off), proving
the RF front-end, frequency, and antenna path were never the problem: the
receiver hears the signal loud and clear but the digital demodulator never
locks on. That pointed squarely at the sync word (the chip's first-stage
correlation filter) — and re-reading `adafruit_rfm9x`'s actual source
found it has **no `sync_word` property at all**. Both `rocket/code.py`'s
and `ground/code.py`'s `self.radio.sync_word = packet.SYNC_WORD` (wrapped
in a bare `try/except Exception: pass`) has *always* raised
`AttributeError` and been silently swallowed — the real, actually-working
Python↔Python link has never used `packet.SYNC_WORD` (0x2B) at all, just
the SX1276's power-on-reset default (0x12). Harmless on the Python side
(both radios fail identically and land on the same true default) but a
real blocker here, since this port correctly implemented the *intended*
value from `packet.py`, which was never actually active on real hardware.
Fixed in `radio.rs`: hardcoded `ACTUAL_SYNC_WORD: u8 = 0x12` instead of
`common::SYNC_WORD`, with the full derivation in a comment. `RX_SYMBOL_TIMEOUT`
timeout logic and DIO1 restored to their production shape; the RSSI-probe
detour and DIO0-only test reverted. `Radio::rssi_probe_loop` and the
scrolling display log were kept (not deleted) as reusable tools — the log
in particular is what actually cracked this.

**Confirmed working end-to-end on real hardware (2026-08-17): both
directions of the actual radio link.** After the sync-word fix, reflashed
and retested: `RxDone!` fires and real telemetry decodes correctly --
`TELEMETRY #256 IDLE 4.1V` (plausible counter, flight state, and battery
voltage) appeared on the scrolling log, proving the full pipeline works,
not just "a packet arrived": RF reception, sync-word match, RadioHead
4-byte header stripping, and `unpack_telemetry`'s own validation (MAGIC/
length/CRC) all correct against the real, unmodified CircuitPython rocket
firmware. Combined with the earlier CHIRP-button TX test (payload audibly
beeped in response), **both TX and RX are now proven working against real
hardware** -- ARM/DISARM/CHIRP commands transmit correctly and downlink
telemetry decodes correctly. `ROCKET_STATE` (the cross-core "latest
telemetry" slot) is now being populated with real, valid data on every
received frame, ready for real telemetry screens to consume.

**FLIGHT screen ported (2026-08-17), decided next after both radio
directions were confirmed working: dial in the handheld to feature
freeze before starting the rocket firmware, rather than debugging two
half-built systems against each other.** Faithful port of
`screen_header.py`/`screen_flight.py`/`screen_footer.py`, including the
hand-illustrated rocket (idle + sunglasses-while-ARMED) and handheld art
the user specifically wanted kept. New modules: `frame.rs` (the
cross-screen `Frame` bundle, only the fields FLIGHT/header/footer
actually use so far -- RECOVERY/DIAG fields not added speculatively),
`display_util.rs` (text-size mapping + explicit `Baseline::Top` to match
CircuitPython's top-left text anchor, which `embedded-graphics` doesn't
default to), `icons.rs`/`icon_bitmaps.rs` (small header glyphs, signal
bars, battery, bolt), `rocket_art.rs`/`handheld_art.rs` (the two large
illustrations). `ground-logic` gained `signal_level`/`signal_percent`
(hardware-free, host-tested, matching `battery_level`/`battery_percent`'s
existing shape).

The large bitmap art (85x150 rocket x2, 100x88 handheld) was **generated
from the actual Python source**, not hand-transcribed -- `scratchpad/
gen_bitmaps.py` bit-packs `rocket_art.py`/`handheld_art.py`/`icons.py`'s
ASCII-art tuples (MSB-first per row, matching `display.rs`'s own
framebuffer convention) into Rust byte arrays, and a separate script
round-trip-verified every generated array against the original ASCII art
bit-for-bit before any of it was trusted. Pixel art is exactly the kind of
content a hand-transcription error could hide in plain sight; generating
it was the only way to be certain of fidelity.

Known, deliberate gaps, not oversights: the ground station's own GPS and
battery ADC aren't wired up yet, so the CONTROLLER table always shows GPS
SEARCH / no battery reading, and DIST always shows `--` (`frame.rs`
documents this); the ARM/DISARM pending-confirmation state machine
(`code.py`'s `tx_status`/`CMD_CONFIRM_FRAMES`) isn't ported, so
`tx_status` is a static placeholder; RECOVERY/DIAG don't exist, so the
footer's MENU hint self-loops on FLIGHT. Text size/position is a
best-effort match, not pixel-perfect: CircuitPython's built-in font is an
8x8 glyph CircuitPython itself scales by an integer factor, and
`embedded-graphics` has no equivalent scalable bitmap font, so `size`
maps onto the closest fixed preset (`FONT_6X10` / `FONT_10X20`) instead --
anchor coordinates are ported 1:1 from the Python screens regardless, so
layout structure matches even though exact glyph metrics don't. Builds
and clippy-checks clean on both host (119 tests passing) and
`thumbv6m-none-eabi`; **not yet flashed/confirmed on real hardware.**

**Handheld's own GPS/battery wired up, RECOVERY/DIAG screens ported, real
MENU/ARM screen-cycling implemented (2026-08-18).** Decided after both
radio directions were confirmed working: finish the handheld to feature
freeze before starting the rocket firmware, rather than debugging two
half-built systems against each other.

- **NMEA parsing is hand-written, hardware-free, host-tested**
  (`ground-logic/src/nmea.rs`) rather than pulled from a crate: the one
  plausible existing option, `adafruit_gps`, turned out to depend on
  `serialport` (a desktop-only crate, unusable on a no_std target) despite
  the name; `tiny-nmea` is real but only parses GLL/GSV, missing the
  speed/course fields RMC alone provides. `NmeaLineReader` replicates
  `adafruit_gps`'s exact I2C padding-filter rule (a bare `0x0A` not
  preceded by `0x0D` is filler, not a line terminator) byte-for-byte, read
  from the actual CircuitPython source rather than assumed. `parse_rmc`
  is checksum-validated and tested against the standard NMEA 0183
  reference sentence. Deliberately doesn't send `PMTK314`/`PMTK220`/etc.
  configuration commands `ground/code.py` does -- PA1010D/MTK3339-family
  chips emit RMC by factory default and MTK sentence config is
  session-only anyway, so this just relies on the chip's own default
  output.
- **GPS transport** (`ground/src/gps.rs`): I2C1 (not I2C0 -- SDA=GPIO2/
  SCL=GPIO3 map to I2C1 on this chip, confirmed the same way the SPI pins
  were earlier: the compiler's own trait bounds, `SclPin<I2C0>`/
  `SdaPin<I2C0>` aren't implemented for these pins at all), polled every
  200ms, blocking mode (no DMA needed at this rate).
- **Battery ADC** (`ground/src/battery.rs`): A0/GPIO26, 8-sample average
  matching `code.py`'s `BATT_SAMPLES`, formula adjusted for the RP2040's
  native 12-bit ADC vs CircuitPython's `analogio` always normalizing to
  16-bit. `my_charging` (`supervisor.runtime.usb_connected`) is not
  ported -- no bare-metal equivalent wired up yet; stays `false`.
- **Link state** (`ground/src/link.rs` for the parts needing a real clock,
  `ground-logic/src/link.rs` for the hardware-free WAITING/LIVE/STALE/LOST
  bucketing rule): properly separates the *latched* rocket GPS fix from
  *live* telemetry, matching `code.py`'s own description of the latch as
  "the single most valuable feature in the file" -- an unfixed or
  all-zero frame leaves a previously latched fix untouched.
- **Screen navigation redesigned, not just ported as-is**: `code.py`
  handles MENU/ARM-as-BACK in one single-threaded loop, but this firmware
  splits buttons+display (core1) from the radio (core0) specifically so
  neither blocks the other -- routing every button event through core0
  first (the original stub behavior) would have reintroduced exactly that
  coupling for screen navigation. Redesigned so MENU-advance and
  ARM-as-BACK are filtered and handled entirely on core1
  (`ground/src/screen.rs`, a single atomic, not a `Mutex` -- both
  reader/writer are already on the same executor); core0 now only ever
  sees an ARM/DISARM hold that already, unambiguously means "send the
  command."
- **RECOVERY** (`screen_recovery.rs`) and **DIAG** (`screen_diagnostics.rs`)
  ported faithfully, reusing already-ported `nav.rs`
  (`bearing_deg`/`compass_point`/`haversine_m`/`relative_arrow`) and
  `common::Sensor::present`/`missing`.

Builds and clippy-checks clean on host (133 tests passing) and
`thumbv6m-none-eabi`. **Flashed and confirmed working on the real ground
unit (2026-08-17)**: buttons noticeably more responsive than the
CircuitPython version, ARM/DISARM hold-to-arm confirmed over the real
radio link, CHIRP confirmed, screen cycling confirmed fast. GPS fix
acquisition on the handheld itself hasn't been separately confirmed
(DIST still reads `--`, but that's expected without an outdoor sky view,
not necessarily a bug).

### 2026-08-17/18 -- post-flash polish pass

User feedback after the above hardware confirmation, worked through as a
punch list:
- **Battery percent formula replaced** (`ground-logic/src/icons.rs`): the
  old 15-anchor piecewise-linear `BATT_CURVE` table is gone, replaced by a
  user-provided sigmoid, `123 * (1 - 1/((1 + (V/3.7)^80)^0.165))` --
  motivated by an observed real 4.1V->5.0V jump (battery reaching full
  charge, Feather switching to USB power) that the old table represented
  poorly. The new formula's own saturation behavior clamps any
  above-4.2V reading to 100% without a special case.
- **ALTITUDE shows "N/A" during BOOT/IDLE** (`ground/src/screen_flight.rs`):
  previously showed a frozen, meaningless ~0 before ARM captures the
  ground-pressure reference -- now reads as "not applicable yet"
  instead of "looks broken."
- **Off-FLIGHT ARM/DISARM button is a tap-to-go-back, not a hold**
  (`ground/src/main.rs`'s `button_task`): the 2s hold is still required
  to actually arm/disarm from FLIGHT, but on RECOVERY/DIAG the same
  button now navigates back on a single tap, matching MENU's tap-to-
  advance. Still filtered and handled entirely on core1, same reasoning
  as the original MENU/ARM-as-BACK redesign -- core0 never sees a press
  that isn't a genuine "send the command."
- **ARM/DISARM pending-confirmation + a command log panel**
  (`ground/src/cmdlog.rs`, new module): ports `code.py`'s
  `tx_status`/`pending`/`CMD_CONFIRM_FRAMES` state machine, resolving a
  sent ARM/DISARM to "ARMED OK"/"DISARMED OK"/"CMD FAILED -- retry"/"CMD
  FAILED -- link lost" by watching `radio::PACKET_COUNT` and
  `LinkStatus::Lost`, same as the Python original. Owned by core0 (the
  side that sends commands and sees the packet counter/link state),
  read by core1's display task through a `Mutex`-guarded snapshot
  (`CmdLogSnapshot`, cloned out rather than held across a render -- same
  pattern as `link::LINK`). Rendered as a 3-line scrolling log on the
  FLIGHT screen (`screen_flight.rs`) under the CONTROLLER panel, which
  was shifted up (handheld art now starts at y=40, the top bound
  screens are allowed to draw at, instead of y=44) to make room without
  moving the bottom status/alert line.
- **Themed MISSING screen, and it now also covers "gone stale mid-flight,"
  not just "before the first frame"** (`ground/src/screen_missing.rs`,
  `ground/src/missing_art.rs`, `ground-logic/src/link.rs`'s new
  `telemetry_missing`/`TELEMETRY_MISSING_MS`): replaces the plain "NO
  TELEMETRY" text fallback with a magnifying-glass glyph (user-supplied
  `docs/images/rocket-missing.png`, already 85x150 to match
  `rocket_art`'s footprint -- thresholded to 1-bit the same way, bit-
  packed the same way, round-trip-verified the same way, all via a
  one-off conversion script rather than hand-transcribing). Shown
  whenever nothing has ever arrived *or* the last frame is older than
  `TELEMETRY_MISSING_MS` (60s, deliberately much coarser than
  `LinkStatus::Lost`'s 15s, which stays a live-screen "link degraded"
  indicator on RECOVERY -- unchanged) -- so a payload that goes quiet
  mid-flight reverts to this screen too, not just pre-first-contact.
  Redesigned (2026-08-18, after first feedback on the plain version) to
  mirror `screen_flight.rs`'s layout pixel-for-pixel rather than being its
  own design -- same rocket-glyph position (magnifying glass instead),
  same "SYSTEMS CHECK" table (every row "??", header "SEARCHING" instead
  of "ROCKET") -- and, critically, calls the *same*
  `screen_flight::draw_controller_panel` on the right that FLIGHT does
  (extracted out to make this possible), so the handheld's own GPS lock/
  battery/command log stay live and visible even while the rocket itself
  is unheard from, instead of going dark along with the rest of the
  screen.

### 2026-08-18 -- second feedback pass

- **DIST was a hardcoded `"--"` placeholder, not actually wired up**
  (`screen_flight.rs`): despite `frame.my_lat`/`fix_lat` both being live
  by this point, the FLIGHT screen's DIST line never read them. Fixed by
  reusing the same `haversine_m` calculation `screen_recovery.rs` already
  did -- both screens now agree.
- **Footer said "HOLD:BACK" on RECOVERY/DIAG after the tap-to-back
  change** (`screen_footer.rs`): the label was never updated when the
  gesture changed from a hold to a tap (see the earlier "Tap, not hold"
  entry above) -- now reads "TAP:BACK>NAME".
- **MISSING screen redesigned to mirror FLIGHT instead of being a plain
  centered icon+label** (`screen_missing.rs`, `screen_flight.rs`'s new
  `pub draw_controller_panel`/`ROCKET_X`/`ROCKET_Y`/`TABLE_X`/`STATUS_Y`):
  see the updated MISSING entry above -- this was a full rework of the
  first version, not an addition to it.

### 2026-08-18 -- third feedback pass: GPS accuracy root-caused

- **Root-caused the "two co-located GPS fixes 60-120ft apart" complaint**:
  not a units/precision bug (checked `coord_to_decimal`'s `ddmm.mmmm`
  math by hand -- correct, and the f32 wire format only costs well under
  a meter). The real cause: `rocket/code.py` and the original
  `ground/code.py` both send `PMTK313,1` (enable SBAS search) and
  `PMTK301,2` (DGPS correction source = WAAS) at GPS init; the new Rust
  `gps.rs`/`nmea.rs` port skipped *all* `PMTK*` commands, including these
  two, under the mistaken assumption they were sentence-output config
  like `PMTK314`/`PMTK220` (which really are skippable, chip default
  covers them). `PMTK313`/`PMTK301` aren't sentence-output settings at
  all -- skipping them left the ground station's own GPS running
  uncorrected while the rocket's (still `rocket/code.py`, unchanged) had
  WAAS the whole time. Fixed in `gps.rs`: new `framed_command`/
  `send_command` (checksum via `ground-logic::nmea::checksum`, factored
  out of `parse_rmc`'s own verification -- same algorithm, new direction)
  sends both at boot, closing the gap with the existing, tested Python
  behavior.
- **Also added `PMTK397,0.2`** (static navigation threshold), beyond
  parity with Python: this GPS's only job is rangefinding a stationary
  handheld against a stationary/landed rocket, never in-motion tracking,
  so trading motion accuracy for at-rest stability is a clean win here.
  0.2 m/s is comfortably under walking pace, so carrying the handheld to
  search still updates position normally.
- **Command status was rendering twice** (`screen_flight.rs`,
  `screen_missing.rs`): the same text (`frame.tx_status`) appeared both
  as the last line of the CONTROLLER command log *and* again at the
  bottom status line under the rocket glyph. Fixed -- the bottom line on
  FLIGHT is now NO-GO-alert-only (blank otherwise); on MISSING it's
  gone entirely (no live battery to gate a NO-GO check on there anyway).
- **Low-battery glyph** (`lowbatt_art.rs`, new, same PNG->1-bit->bit-
  packed pipeline as `missing_art.rs`, source `docs/images/
  rocket-lowbatt.png`, user-provided): the FLIGHT screen's rocket slot
  now shows this in place of the normal idle/armed art whenever
  `tel.batt_volts < NOGO_BATT_V` (3.80, the same threshold that already
  gated the NO-GO text) -- the glyph and the banner text now agree,
  instead of only the text changing.

### 2026-08-18 -- fourth feedback pass: GPS fix averaging

Fix confirmed: DIST dropped from 60-120ft to 17-34ft after the PMTK313/
301/397 change above -- real improvement, but still visibly noisier than
it should be for two stationary units. User's proposal (having read the
PA1010D datasheet, `docs/downloads/CD_PA1010D_Datasheet_v.03.pdf` -- note:
this sandbox has no PDF text/render tooling available (`pdftoppm`/
`PyPDF2`/etc all missing), so this wasn't independently re-read here; the
implementation instead leans on `PMTK220`'s already-known semantics from
`rocket/code.py`'s own `PMTK220,1000` call and well-established public
MTK3339 command documentation): sample continuously, average over a
several-second rolling window, and only publish/display every 5-10s
rather than every fix.

- **`ground-logic/src/fix_average.rs`** (new): `FixAverage`, an O(1)
  running mean of lat/lon samples (sum + count, no stored history) --
  hardware-free, host-tested (`tests/fix_average.rs`, 4 cases). Plain
  arithmetic mean, not a proper geographic centroid -- deliberate, same
  "curvature doesn't matter at this scale" reasoning as the earlier
  haversine-vs-flat-earth discussion, just from the opposite direction
  (samples here are meters apart, not km).
- **`gps.rs`**: the read loop now folds every valid fix into a
  `FixAverage` continuously, and only publishes the mean to `MY_GPS`
  every `FIX_AVERAGE_WINDOW_MS` (5000ms, the responsive end of the user's
  suggested 5-10s), then resets for the next window. Heading is
  deliberately *not* averaged (circular quantity, and only valid while
  moving -- the one case this whole scheme doesn't optimize for) --
  still just the most recent in-window reading.
- **Also sends `PMTK220,100`** (request 10Hz fixes, up from the 1Hz both
  Python files request) to feed the average more raw samples per window,
  per the user's read of the datasheet. Deliberately left
  `POLL_PERIOD_MS`/`CHUNK_SIZE` (the I2C read loop's own cadence/chunk
  size) unchanged -- if the chip now produces more than this loop's
  existing read budget can drain, the PA1010D's I2C "streaming" interface
  is designed around exactly that (filler-padded reads, see
  `NmeaLineReader`'s docs), so the excess is just not captured that
  cycle, not lost/corrupted or a stall risk. This keeps the change
  strictly one-directional (more samples when available, never worse
  than before) without touching core1's existing blocking-I2C timing
  budget (see `main.rs`'s docs on why that budget matters for button
  responsiveness).

### 2026-08-18 -- fifth feedback pass: refuse to arm while charging

User observation: ARM could be sent to the rocket even while it was
charging over USB. Checked `rocket/code.py`'s `Sensor.flight_ready` (the
rocket's own ARM-refusal check, `common/packet.py`) -- it only covers
BARO/IMU/LOG, and `Sensor.CHG` is *deliberately* excluded from it (a live
power state, not a peripheral-health flag, normally 0 for an entire real
flight -- see `common/src/lib.rs`'s docs). So this was never actually
gated rocket-side either; the user's memory of "we had this figured out"
was likely about the handheld side needing to add its own check, which
didn't exist yet. Also folded in: the low-battery NO-GO gate
(`screen_flight.rs`) only ever affected the *banner text* -- the ARM
button itself was never actually disabled by it, at any point in this
session, which the user's ask surfaced as a second, related gap.

- **`ground-logic/src/nogo.rs`** (new; `ground-logic` now depends on
  `launchcast-common`, same pattern `rocket-logic` already uses):
  `nogo_reason(&Telemetry) -> Option<NogoReason>` (`LowBattery` |
  `Charging`, low-battery takes priority if somehow both apply), hardware-
  free and host-tested (`tests/nogo.rs`, 6 cases) -- the same threshold
  (`NOGO_BATT_V`, 3.80) `screen_flight.rs` already used, now centralized
  here instead of duplicated at each call site.
- **FLIGHT screen** (`screen_flight.rs`): BATTERY row's " CHG" text
  replaced with the header's own bolt icon (`icons::draw_bolt`) --
  bonus fix, the old text form silently overflowed this row past the
  CONTROLLER column by ~30px at a 3-digit percentage (100% CHG); the
  icon form only overflows by a few px in that same edge case. NO-GO
  banner now shows `NogoReason::message()` (either reason, not just low
  battery). Rocket glyph only swaps to the low-battery art specifically
  for `LowBattery` -- charging has no dedicated glyph (wasn't asked for,
  and the header bolt + BATTERY-row bolt already cover it).
- **Footer** (`screen_footer.rs`): "HOLD:ARM" doesn't render at all when
  `frame.nogo().is_some()` and the rocket isn't already armed -- blank
  instead, so nothing invites a press that would only be refused.
  DISARM is unaffected regardless of NOGO -- the gate is about
  preventing a *new* arm, never about trapping the user out of disarming
  one that's already active.
- **`main.rs`'s `core0_task`**: the actual send is refused too (`cmd =
  None`), not just the UI hint -- defense in depth if a stray/queued
  press reaches this point anyway. The footer suppression is the primary
  guard (nothing should invite the press); this is the guarantee that
  matters even if that primary guard is ever wrong.

**Considered and declined**: the handheld showing its own charging status
(bolt icon in the header/CONTROLLER panel) turns out to need real USB/
VBUS presence sensing, which RP2040 doesn't have natively (no dedicated
VBUS pin on the SoC at all -- it's board-wiring-dependent whether one's
broken out to a GPIO, unconfirmed for this board) and embassy-rp's USB
driver doesn't provide either (it force-overrides VBUS-detect to "always
present" rather than reading real hardware state -- literal `// TODO:
implement VBUS detection` in `embassy-rp-0.10.0/src/usb.rs`). The
alternative, a full `embassy-usb` CDC device mirroring how CircuitPython's
`supervisor.runtime.usb_connected` actually works (checks USB enumeration,
not a power-good signal), is real effort for a status icon. User's call:
not worth it -- the rocket is never charging on a launch pad, which was
the actual scenario this whole thread started from; `frame.my_charging`
stays hardcoded `false` (unchanged from before).

### 2026-08-18 -- rocket/payload firmware crate: first full build

`rust/rocket` (crate `launchcast-rocket`) exists now, builds clean, clippy-
clean (`-D warnings`, zero warnings, not just zero errors), and links to a
valid UF2 -- **not yet flashed or tested on real hardware.** Same
"reimplementing known-good, tested behavior on a different runtime, not a
redesign" discipline as the ground port, checked against the actual
Python driver sources where they exist locally (`adafruit_bmp5xx.py`,
`adafruit_lis3mdl.py`, both under `~/.local/share/circup/...`), not
re-derived from datasheets from scratch.

Ecosystem check (same practice as `embassy-rp`/`lora-phy` originally):
`lsm6dsox` crate adopted for the IMU (mature, embedded-hal 1.0 blocking,
actively maintained, OLFL-1.3 license -- a legitimate Apache-2.0-style
permissive license from Fraunhofer IML, not one of the common SPDX names
but not copyleft either). BMP580 and LIS3MDL hand-rolled instead: the
only BMP580 crate available (`bmp5xx`) is 6 weeks old with 91 downloads
and async-only (would force a bus-mode mismatch against `lsm6dsox`'s
blocking-only API on the same shared bus); the LIS3MDL crates are both
years-unmaintained and pre-`embedded-hal-1.0`.

**Real surprise found integrating `lsm6dsox`**: its `Accelerometer` trait
impl (`accel_norm()`) returns **g, not m/s²** despite the `accelerometer`
crate's trait convention -- confirmed by checking the crate's own
sensitivity constant (`0.000122` at `Accel4g`, which is the datasheet's
mg/LSB spec, not an m/s² figure), not assumed from the trait's label.
Trusting the label would have fed g-unit values into `FlightState`
(expects m/s², divides by gravity again internally) -- silently wrong by
~9.8x, specifically during boost detection. Worked around by reading raw
counts (`accel_raw()`/`angular_rate_raw()`) and scaling by hand against
the documented per-LSB sensitivity instead of trusting either "normalized"
method -- see `rocket-logic/src/imu.rs`'s docs.

Sensor-driver scope, deliberately trimmed against what's actually used,
not built out to match Python's full surface: LIS3MDL is presence-check
only (`rocket/code.py`'s main loop never actually reads `hw.mag`, and the
wire telemetry format has no field for it at all); GPS satellite count is
always 0 (only `$..RMC` is parsed, same scope as the ground station's own
GPS -- no `$..GGA` parser for one non-critical diagnostic field).

Two real, user-confirmed decisions made along the way, not defaulted
silently:
- **DISARM-without-boost rewinds to this arm cycle's start, not a full-
  log wipe** like `rocket/code.py`'s literal `open(path, "wb")`. Checked
  against the actual implementation, not just the docstring's stated
  intent ("that arm cycle produced no flight data worth keeping") --
  since `flight.bin` persists across reboots (opened in append mode),
  the literal Python behavior would wipe an *earlier real flight's* data
  too if a later bench DISARM ever followed it. User call, 2026-08-18:
  rewind only.
- **`Sensor::CHG` hardcoded to 0** for now -- same missing-VBUS-detection
  finding as the ground station's own charging status, but here it feeds
  a real safety gate (the ground station's NOGO-while-charging ARM
  refusal) rather than a cosmetic icon. User call, 2026-08-18: ship
  without it anyway rather than take on a full USB device stack; means
  that gate won't actually trigger via telemetry yet. Also noted: even
  Python's `usb_connected` only detects an active USB *data* connection
  to a host, not raw charging current, so it wouldn't catch a rocket
  charging from a dumb USB power brick either -- imperfect parity either way.

**Raw-partition flash log** (`rocket-logic/src/flash_log.rs` for the
record format, `rocket/src/flash_log.rs` for the flash I/O): the piece
that took the most real design work this session. Fixed 48-byte records
(magic + version + 45-byte payload matching `code.py`'s own `LOG_FMT` +
checksum), self-describing so a boot-time scan finds the resume point
without a separate persistent write-pointer header to maintain (and its
own torn-write problem). Flush trigger: 500 entries or 5 seconds,
whichever first (user-specified, 2026-08-18 -- covers a full boost+coast
phase for the largest motor this project flies, ~2.8s worst case, without
ever flushing mid-phase). Double-buffered in SRAM (`BATCHES[2]`, ~24KB
each) so core1's sampling never blocks on core0's flash write -- a single
shared buffer behind one mutex would have reintroduced exactly the stall
this whole scheme exists to avoid, the moment core1's next sample landed
while core0 still held the lock. Every arm cycle's data starts at a
sector-aligned offset specifically so the DISARM-rewind's erase can never
touch an earlier flight's data living in the same sector.

**Dual-core split, confirmed against source, not assumed**: `embassy-rp`'s
flash driver (`src/flash.rs`) only allows `blocking_erase`/`blocking_write`
to be called from core0 (`pac::SIO.cpuid()` checked, `Error::InvalidCore`
otherwise) and forcibly pauses core1 for the duration of every call
(`multicore::pause_core1()`, a real blocking FIFO handshake, not just a
comment) -- so core0 ended up owning both radio *and* the flash-flush,
core1 owns the I2C bus + flight-state machine + RAM buffering only, per
the revised Strawman architecture above. Radio TX/RX is deliberately
phase-agnostic (one unchanging loop covers the whole flight) since the
flight-state machine already refuses ARM outside IDLE and DISARM outside
ARMED with no path back once boosted -- core0 doesn't need to know or
care what phase the flight is in for that to be safe.

GPS's NMEA parsing (`checksum`/`framed_command`/`parse_rmc`/
`NmeaLineReader`) moved from `ground-logic` to `common` this session too
-- both boards' GPS need it now, not just the ground station's (re-
exported from `ground-logic` under its old path so nothing there had to
change).

Not yet done (at the time the entry above was written): flashing/testing
on real hardware, and the wireless flight-log-review feature, explicitly
deferred until after a first real flight with local logging.

### 2026-08-18 -- first real flight hardware, both boards

**Ground station**: cold-boot-on-battery-only never got core1 (buttons +
display) running -- nothing reached the screen -- while core0 (radio)
came up fine every time, confirmed alive via its own heartbeat LED. A
*reset* button press (not a power cycle) fixed it every time. That
signature -- reset fixes it, fresh power-on doesn't -- points at a power-
rail settling race, not a logic bug: reset only restarts code execution,
it doesn't recycle power, so by the time it fires the rail's already been
up and stable for a few seconds, and the same code then succeeds. LiPo
boost-converter rails commonly ramp up slower/less cleanly than USB's
regulated 5V. Fixed with a ~100ms `cortex_m::asm::delay` right after
clock init, before either core touches a peripheral -- confirmed working
by the user on real hardware.

**Rocket**: flashed via double-tap-reset into the UF2 bootloader (not
`import storage` -- that's for CircuitPython's own host-write permission
dance, unrelated to flashing a new `.uf2` at all, which needs the ROM
bootloader's own always-writable `RPI-RP2` drive instead). Broadcasting
immediately, CHIRP and ARM/DISARM both confirmed working over the real
link on first try.

Two real fixes from that first flight:
- **Battery percent formula rescaled** (153.75, was 123): the actual
  battery peaked around 4.0V/~80% on the original curve -- confirmed not
  a bug (4.00V's 79.1% on the original formula matches the observed ~80%
  almost exactly) but this board's charge IC capping below the standard
  4.2V LiPo-full voltage, a common deliberate longevity tradeoff. A
  battery that structurally can't reach 100% reads as "broken" forever,
  which is worse than treating this hardware's real achievable ceiling as
  100%.
- **`fw_version` field added** to telemetry (byte 39, a repurposed
  `cam_disk` -- see above): confirms a deploy actually took from
  telemetry alone. Shown on the ground station's DIAG screen.

Investigated but not resolved: a reported ~46°F temperature reading.
Traced the entire pipeline -- BMP580 register addresses, config byte
math, decode formula, byte ordering (checked directly against
`adafruit_register.register_bits`'s source, not inferred), the shared
wire pack/unpack (already tested), the ground station's C->F conversion
-- and found no bug anywhere in it; also confirmed (via `grep`) there's
no accidental unit conversion happening rocket-side at all. Root cause
still open -- needs a live hardware data point (does the reading change
at all; what does altitude look like) to narrow further.

~~Confirmed still correct as designed, not bugs: the rocket's GPS does
*not* do the ground station's rolling-average smoothing~~ -- superseded
2026-08-19, see below: the user did want averaging added, just gated by
flight phase rather than always-on. The NeoPixel
status-color-by-flight-state feature the user asked about already
existed (ported from `code.py`'s `PIXEL_FOR_STATE` in the original
rocket-port pass) -- current colors: BOOT dim blue, IDLE dim yellow,
ARMED green, BOOST orange, COAST cyan, APOGEE magenta, DESCENT teal,
LANDED red. Not the same mapping the user described from memory (green
for IDLE, yellow for ARMED, plus a NOGO color, which doesn't correspond
to any rocket-side flight state at all -- NOGO is a ground-station-side
judgment about the rocket's telemetry, not something the rocket itself
knows to indicate) -- open whether to change it, not decided.

### 2026-08-19 -- temperature bug root-caused; rocket-side GPS averaging added

**Temperature bug, root-caused.** The user's follow-up supplied the one
fact static analysis of the pipeline couldn't produce on its own: this
exact sensor read *correctly* under the CircuitPython firmware, on the
same hardware, and only went wrong after the Rust port -- ruling out an
environmental/hardware explanation and pointing squarely at the BMP580
driver port. Found it: the initial Rust driver collapsed several of
`adafruit_bmp5xx`'s individual `RWBits`/`RWBit` property-setter writes
into one precomputed "final byte" write per register (`OSR_CONFIG`,
`DSP_IIR`, `DSP_CONFIG`, `ODR_CONFIG`), on the assumption that every bit
this driver doesn't explicitly touch is already 0 after a soft reset.
That assumption was wrong somewhere in the sequence -- likely enough to
have kept the chip from ever actually reaching continuous NORMAL-mode
measurement, which would explain a frozen/wrong reading rather than a
merely-inaccurate one. Fixed by switching to genuine per-field
read-modify-write, matching what the Python descriptors actually do on
the wire, field-by-field in the same call order `__init__` makes them --
including implementing (not skipping, as the first pass had) the
conditional "force back to STANDBY first" branch in the mode-transition
sequence. See `rocket-logic/src/bmp580.rs` and `rocket/src/bmp580.rs`'s
docs for the byte-level detail. Build/clippy/host-test clean; **not yet
confirmed against real hardware** -- needs a reflash-and-observe cycle
before this can be called done.

**Rocket-side GPS averaging, added.** User's explicit spec: average
while IDLE or LANDED; don't average once BOOST/COAST/DESCENT ("in
motion") is detected. `FixAverage` (previously ground-only) moved to
`common` so both boards' GPS modules can share it, mirroring the earlier
`nmea` relocation. `rocket/src/gps.rs` gained a `FLIGHT_STATE: AtomicU8`
static, published every loop tick by `flight_task` (`main.rs`) -- both
tasks share core1's single-threaded executor, so `Relaxed` ordering is
enough, no real race exists. `should_average(state)` extends the user's
two named states to the "stationary" side of every state (BOOT, IDLE,
ARMED, LANDED) and the "in motion" side to the rest (BOOST, COAST,
APOGEE, DESCENT) -- the user only named IDLE/LANDED and BOOST/COAST/
"RECOVERY" (mapped to DESCENT, the only flight-state name that fits)
explicitly, but stated the actual criterion as "when we know it's in
motion," which this applies uniformly rather than leaving BOOT/ARMED as
an unspecified gap. Worth flagging back to the user: ARMED is currently
grouped with "stationary" (pre-launch, sitting on the pad, same as IDLE)
-- if that's wrong, the fix is a one-line change to `should_average`'s
`matches!`. On a transition between stationary and in-motion, the
accumulator resets rather than carrying over -- otherwise a window that
straddled liftoff or landing could publish an average blending airborne
and stationary samples. `has_fix` still flips immediately on every
sentence regardless of averaging state (unchanged from before this
change) -- only `lat`/`lon` go through the average; that preserves this
module's original deliberate difference from the ground station (no
latching through a lost fix). ~~Averaging window matches the ground
station's, 5000ms.~~ -- superseded same day, see below: the windowed
design didn't survive first real-hardware feedback. Build/clippy/host-test
clean across the whole workspace; not yet flight/bench-tested.

### 2026-08-19 -- GPS averaging rewritten as a ring buffer; FLIGHT screen layout changes

**GPS averaging: window-reset scheme replaced with a fixed-capacity ring
buffer.** Real-hardware feedback on the above: distance-to-rocket did
settle to a stable, accurate reading, but took a few real minutes after
cold start to get there. The 5-second reset-and-snapshot window itself
wasn't actually capable of causing multi-minute lag (each window only
ever held a few seconds of samples before being discarded), so the
likelier explanation is GPS-chip-level convergence (WAAS/SBAS lock,
ephemeris acquisition) rather than a software windowing artifact -- but
the user's proposed fix (keep the most recent 10-20 raw samples instead
of a periodic running average) is still a real improvement in its own
right: it publishes a continuously-updated mean instead of a chunky
once-per-window snapshot, and it sheds a stale run in a bounded number of
samples rather than however long is left until the next window boundary.
`common::fix_average::FixAverage` rewritten around a `WINDOW_SAMPLES =
15` ring buffer (middle of the user's 10-20 suggestion) -- same public
API (`new`/`add`/`mean`/`count`/`reset`), so both `ground/src/gps.rs` and
`rocket/src/gps.rs` needed only their window-deadline/`Instant` bookkeeping
removed, not a redesign. The rocket's flight-phase-transition reset (see
above) got *more* important under this design, not less: without it, a
stale ring buffer would sit untouched (not decaying) through an entire
flight, since `should_average` stops feeding it samples during
BOOST-through-DESCENT, and would otherwise take another 15 fresh samples
after landing to fully evict the pre-flight data mixed in.

**FLIGHT screen**: four layout/logic changes, all user-specified:
- Rocket's `fw_version` (already in telemetry, see above) now shown
  inline on the "SYSTEMS CHECK:" line (`SYSTEMS CHECK:   v{n}`) rather
  than as a separate row -- `screen_missing.rs` (which mirrors this
  layout when no telemetry has arrived) shows `v??` in the same spot for
  consistency with its other placeholder fields.
- Handheld's own firmware version (new `ground::FIRMWARE_VERSION`
  constant, mirroring the rocket's) now shown on its own line under the
  "CONTROLLER" title. The handheld's art glyph is already drawn at
  `y=40`, the topmost a screen is allowed to draw (see
  `screen_header.rs`), so it couldn't move up any further to make room --
  the version line was inserted right under the title instead, pushing
  GPS LOCK/BATTERY/DIST/the 3-line command log down 10px each. Checked
  there was enough slack before `FOOTER_Y` (222) for that: there was,
  with margin to spare.
- Header's "HH" label (next to the handheld's own battery icon) removed;
  the battery icon moved up into the row the label used to occupy. The
  ground glyph immediately to its left already identifies whose battery
  it is, same as the rocket icon does for the payload cluster next to it.
- `battery_level`'s bucketing thresholds changed from a plain `/25`
  (0-24/25-49/50-74/75-99/100 -> 0-4 bars) to user-specified thresholds
  aligned to round 20% boundaries: >80% is 4 bars, >60% is 3, >40% is 2,
  >20% is 1, 20% or under is 0 (each floor exclusive).

## Why

Two distinct problems, both experienced directly this session, not
theoretical:

1. **No real multicore.** The RP2040 is dual-core, but CircuitPython has no
   user-accessible multicore support — confirmed via Adafruit's own tracker
   ([adafruit/circuitpython#4106](https://github.com/adafruit/circuitpython/issues/4106)).
   Radio RX/TX, GPS polling, button sampling, and display redraws are all
   serialized on one interpreter loop on one core. This is structural, not a
   tuning problem — it's the root of the `SLOW LOOP` / `DRAW TOOK` / `GPS
   UPDATE TOOK` watchdog prints throughout `ground/code.py`, and plausibly of
   the still-unsolved "hold completes, but the ARM command doesn't send for
   several more seconds" bug. `rp2040-hal` and `embassy-rp` both have
   real, documented multicore support (spawn a task on core1, FIFO/spinlock
   for inter-core communication).
2. **Non-compacting GC under memory pressure.** Hit a real `MemoryError`
   this session (`memory allocation failed, allocating 9516 bytes`) purely
   from heap fragmentation, not being out of memory in aggregate. Mitigated
   with periodic `gc.collect()` + a low-memory watchdog, but it's a class of
   failure that doesn't exist in `no_std` Rust (mostly static/stack
   allocation, no GC to fragment).

Interpreter overhead (CircuitPython bytecode vs. compiled Rust) is a real,
secondary factor in the same direction but wasn't independently isolated —
the two points above are the ones with direct evidence.

## Ecosystem research (checked before committing, not assumed)

Real, existing crates cover every chip in this stack. Spot-check each one's
last-publish date and open issues before depending on it — "a crate exists"
isn't the same as "Adafruit's paid team maintains it," which is what the
CircuitPython side currently gets for free.

| Subsystem | Crate(s) | Notes |
|---|---|---|
| RFM95 / SX1276 LoRa radio | [`sx127x_lora`](https://crates.io/crates/sx127x_lora), [`radio-sx127x`](https://crates.io/crates/radio-sx127x) | `sx127x_lora` explicitly lists HopeRF RFM95W support |
| BMP580 barometer | [`bmp5xx`](https://docs.rs/bmp5xx/latest/bmp5xx/), [`bmp5`](https://crates.io/crates/bmp5) | `bmp5xx` is tested directly against the Adafruit BMP580 breakout — our exact part |
| LSM6DSOX IMU | [`lsm6dsox`](https://crates.io/crates/lsm6dsox) | embedded-hal, platform-agnostic |
| LIS3MDL magnetometer | [`lis3mdl-driver`](https://lib.rs/crates/lis3mdl-driver) / [`lis3mdl`](https://docs.rs/lis3mdl) | multiple independent implementations exist |
| PA1010D GPS | no chip-specific crate needed — it's plain NMEA over I2C | [`nmea0183`](https://crates.io/crates/nmea0183) or [`nmea`](https://crates.io/crates/nmea), both `no_std` |
| Sharp Memory Display (LS0xx family) | ~~[`sharp-memory-display`](https://crates.io/crates/sharp-memory-display/0.3.0)~~, ~~[`memory-lcd-spi`](https://lib.rs/crates/memory-lcd-spi)~~ | **Both ruled out, checked directly, not assumed.** `sharp-memory-display` is GPL-3.0+, incompatible with this project's Apache-2.0 license. `memory-lcd-spi` looked like the fix (MIT/Apache-2.0) but every version compatible with `embassy-rp 0.10`'s `embedded-hal 1.0` (0.0.6, 0.0.7) has `#![feature(generic_const_exprs)]` at its crate root — **nightly-only**, confirmed by an actual build attempt (`E0554`, not a docs read); earlier 0.0.x versions avoid that but only support a pre-1.0 `embedded-hal` alpha, incompatible with `embassy-rp`. No usable off-the-shelf driver exists on stable Rust as of this check. |
| RP2040 HAL + multicore | [`rp2040-hal`](https://docs.rs/rp2040-hal/latest/rp2040_hal/multicore/index.html), [`embassy-rp`](https://docs.embassy.dev/embassy-rp/git/rp2040/index.html) | both have first-class multicore APIs |

## Strawman architecture

Not a final decision — the new thread should work this out properly, but as
a starting point:

**Rocket — revised 2026-08-18, superseding the original strawman below**
(not started yet; discussed and largely settled before writing any code,
same "decide the split before building" discipline the ground station
used):

- **Core0 = radio (RX uplink commands, TX telemetry) + flash log flushes.**
  Radio behavior is deliberately **phase-agnostic** -- it runs one
  unchanging loop for the entire flight, always relaying whatever
  telemetry core1 last produced and always forwarding any received
  command. This works because the flight-state machine (`rocket-logic`)
  already refuses ARM outside IDLE and DISARM outside ARMED, with no path
  back to ARMED once boosted -- an uplink command arriving mid-flight is
  already inert by the state machine's own transition rules, so core0
  doesn't need to know or care what phase the flight is in.
- **Core1 = the shared I2C bus (BMP580 + LSM6DSOX + LIS3MDL + GPS, all on
  `STEMMA_I2C`) + the flight-state machine + a RAM ring buffer of
  pending log entries.** One core, not split further, because all four
  sensors share one physical bus -- unlike the ground station's SPI1
  (radio) vs. PIO-SPI (display) split, there's no second bus available
  here to hand a subset of sensors to the other core without adding
  cross-core bus-arbitration contention, which is exactly what the core
  split exists to avoid. Polling cadence per sensor is phase-dependent
  (see the flight-phase table below) but that's a scheduling policy
  *within* this one core's loop, not a core assignment.
- Cross-core plumbing mirrors the ground station's already-proven
  pattern, just with the roles reversed: a `Mutex`-guarded latest-
  `Telemetry` (core1 writes on every update, core0 reads for TX -- same
  shape as `ground/src/gps.rs`'s `MY_GPS`), a bounded `Channel` for
  inbound commands (core0 writes, core1 reads -- same shape as
  `BUTTON_EVENTS`), and a second bounded `Channel` for batched log-entry
  chunks (core1 writes, core0 reads and actually flushes to flash).

Proposed flight-phase polling policy (core1's own scheduling, not a core
assignment -- see `rocket-logic`'s existing `FlightState` for the phases
themselves):

| Phase | Accel/gyro | Baro | GPS | Log writes |
|---|---|---|---|---|
| BOOT/IDLE | ~1/15s | ~1/15s | ~1/15s | none (matches current `flight.bin` gating: logging starts only from ARMED onward) |
| ARMED | fast (launch detection is the priority) | slow | slow/paused | none yet |
| BOOST/COAST | fast (cheap, already being read) | fast (velocity/apogee needs it) | paused (not useful mid-flight, frees bus time for baro/accel) | fast, into the RAM ring buffer |
| APOGEE/DESCENT | fast | fast | paused | fast, into the RAM ring buffer |
| LANDED | slow | slow | resumed | stopped; final buffer flush |

**Why flash logging needs a RAM ring buffer, not straight-through writes**
(the one piece of this that took real digging, not just a plausible
guess -- verified against `embassy-rp 0.10.0`'s actual flash driver
source, `src/flash.rs`, not assumed):

- NOR flash (what's on this board, like nearly all MCU flash) can only
  flip bits `1->0` on a write; reusing a region for new data requires
  first *erasing* it back to all-`1`s, and erase only works in whole
  blocks -- RP2040's minimum erasable unit is a 4KB sector
  (`ERASE_SIZE = 4096` in `embassy-rp`'s flash driver). A page program
  (~256 bytes) is cheap (sub-ms to a couple ms on typical QSPI NOR
  flash), but crossing into a fresh sector costs a full sector erase --
  typically tens to low hundreds of milliseconds on typical QSPI NOR
  flash parts (this board's exact chip/timings not yet confirmed against
  a datasheet) -- and lands wherever a sector boundary happens to fall
  relative to sample timing, not somewhere you get to choose.
- RP2040 executes its own program code directly out of the same flash
  chip (XIP) -- there's no separate code/data flash split -- so nothing
  can execute from flash on *either* core while an erase/program is in
  flight. `embassy-rp`'s flash driver enforces this concretely, not just
  in theory: `blocking_erase`/`blocking_write` check `pac::SIO.cpuid()`
  and return `Error::InvalidCore` if not called from core0, then call
  `multicore::pause_core1()` -- a real blocking handshake over the
  inter-core FIFO that halts core1 and waits for it to confirm before
  proceeding -- then run the actual flash operation inside a
  `critical_section` (interrupts off on core0 too), then explicitly
  resume core1 afterward. So a sector erase isn't "core1's sensor loop
  stalls" -- it's **both cores fully frozen**, and since core0's
  interrupts are off too, any radio RX arriving during that window is
  simply missed, not queued.
- Concrete failure case: a ~20-30 byte BOOST-phase log entry at a
  generously-fast 50-100Hz sample rate fills a 4KB sector roughly every
  1.5-4 seconds -- well within a typical few-second motor burn. Writing
  straight-through, there's a real chance a 100+ms total-freeze lands
  *during* the burn itself, right when continuous, evenly-spaced samples
  matter most for reconstructing the acceleration/velocity curve for
  apogee detection -- and that window's samples are lost outright (data
  never captured, not just captured late), not merely delayed.
- The fix: core1 accumulates samples in an SRAM ring buffer (RP2040 has
  264KB -- buffering several seconds of samples costs tens of KB at
  most), which costs nothing in cross-core terms since it never touches
  flash. The freeze is only unavoidable at the moment flash is actually
  written, so buffering lets the flush *schedule* be chosen deliberately
  (e.g. once a second, or at phase transitions like BOOST->COAST) instead
  of happening wherever a sample's write happens to cross a sector
  boundary. Since only core0 may call the flash API, core1 hands batched
  entries to core0 over a channel rather than writing flash itself --
  see the core split above.
- Still open: exact flush cadence/triggers (time-based, buffer-fill-based,
  phase-transition-based, or some mix), and whether flight-critical data
  should also fsync-equivalent (force a flush) immediately on any phase
  transition, not just LANDED, in case of an unexpected early power loss
  (ejection charge failure, hard landing, etc.) -- not decided yet.

**Ground — decided 2026-08-16, revised from the original strawman below**:
**core0** = radio RX + GPS; **core1** = button sampling/dispatch + display
rendering. This is the opposite split from the first draft (which put
buttons alone on core0 and grouped them with radio/GPS/display on core1) —
revised once `rust/ground/src/display.rs` turned out to need blocking (not
async DMA) SPI, so a ~50ms transfer twice a second genuinely blocks
whatever shares its core. Rather than keep buttons isolated from that cost,
the call was to isolate radio + GPS instead: those are the subsystems where
timing actually matters for correctness (LoRa RX windows, GPS NMEA
parsing), whereas a button response delayed by up to ~50ms is still
imperceptible to a person and 20x tighter than the ~1s the Python
implementation's display draw used to block *everything* for. Button
sampling and display rendering sharing a core is therefore an accepted,
deliberate tradeoff, not an oversight — revisit only if async DMA SPI or a
core1 move for the display specifically removes the 50ms cost outright.
Inter-core communication (button events out, received telemetry in) should
go through `embassy`'s channel primitives or `rp2040-hal`'s FIFO/spinlock,
not shared mutable state.

Original strawman (superseded above, kept for the record): core0 = button
sampling + dispatch only; core1 = radio RX + GPS + display rendering.

Original *rocket* strawman (superseded above, kept for the record): core0 =
sense (I2C) + flight state machine + flash logging; core1 = radio TX only.
Reversed once the flash-write constraints above were actually checked
against `embassy-rp`'s source rather than assumed -- flash operations can
only be issued from core0, which doesn't fit cleanly with core0 being the
"radio only" core in the original draft.

## Migration strategy

The wire protocol (`common/packet.py`) is a well-specified, tested, versioned
binary format shared by both boards. That decouples the two boards' rewrites:

- **Port one board at a time**, keeping the other on CircuitPython, as long
  as the Rust side's packet encode/decode is bit-for-bit compatible with
  `common/packet.py` (same field order, sizes, `MAGIC`/`SYNC_WORD`, and same
  treatment of `Sensor.CHG` as excluded from health-flag semantics). This
  turns a two-board rewrite into two independent, individually-testable
  migrations instead of one big-bang cutover.
- Port pure-logic pieces first and prove them against the existing pytest
  suite's *behavior* (not the Python code itself) as a spec: `packet.py`'s
  encode/decode round-trip, `FlightState`'s transition table, `HoldTracker`'s
  tap/hold/bounce-bridging state machine, `icons.py`'s `BATT_CURVE`
  interpolation. These translate almost directly into `#[test]`s that run on
  the host, no hardware needed — mirroring how they're tested today.
- Sensor drivers and the dual-core split come after the logic is proven,
  since they're the parts that actually need real hardware to validate.

## Known hardware/firmware gotchas to carry over

Learned the hard way this session; a Rust rewrite doesn't get to skip these
just by changing language:

- **`boot.py`'s flight/dev-mode remount logic** (`storage.remount` gated on
  `supervisor.runtime.usb_connected`) has a CircuitPython-specific
  implementation, but the *problem* it solves — the board and the host can't
  both have filesystem write access at once — is a real constraint any
  firmware needs a story for, if flash-based logging/config is kept.
- **Repeated hard resets can corrupt the filesystem** (see
  `docs/filesystem-recovery.md`). A Rust build flashed via UF2/`picotool` is
  not automatically immune — same underlying flash filesystem risk if one
  is used at all. Worth deciding whether the Rust rewrite even needs a
  writable filesystem (e.g. logging to flash directly via a raw partition
  instead of a FAT volume sidesteps this class of corruption entirely).
- **The battery divider goes to BAT, not the regulated 3.3V rail.** Confirmed
  correct on both current boards — a rewrite reusing the same analog pin
  reads a real battery voltage already, no rewiring needed.
- **`Sensor.CHG` is deliberately not a "peripheral health" bit** — it's 0 for
  the entire duration of a real flight (USB unplugged), so it must stay out
  of whatever "flight ready" / "all systems nominal" check replaces
  `Sensor.flight_ready()`.
- **Flash logging must be gated to ARMED-onward**, not unconditional — the
  CircuitPython version originally logged from boot, and bench testing alone
  filled a 500mAh-scale board's flash to the point of `No space left on
  device` within one dev session.

## Open questions for the new thread

- ~~`rp2040-hal` vs. `embassy-rp`~~ — resolved: `embassy-rp` (see Status).
  Still open: whether the rocket's hard flight-state-machine timing ends up
  fighting the async executor's scheduling — unproven until that crate
  exists.
- Per-crate maintenance check (last publish, open issues, license) before
  committing to each one in the table above. Note versions actually in use
  now differ from what's written there: `embassy-rp 0.10.0` (not `0.2`, the
  RP2040-only pre-RP235x-split line), `embassy-executor 0.10.0`,
  `embassy-time 0.5`. Re-check before pinning anything further.
- Testing strategy: host-side `#[test]` for pure logic (direct pytest
  analog) is proven now (`rust/common/tests/packet.rs`,
  `rust/ground-logic/tests/hold_tracker.rs`). What for hardware-in-the-loop
  testing, if anything, is still open — nothing in `rust/ground` has
  touched real hardware yet.
- ~~Migration order: rocket first or ground first?~~ — resolved: ground
  first (see Status).
- Does flash-based flight logging move to a raw partition (sidesteps FAT
  corruption entirely) or stay FAT-based for easy host-side retrieval via
  `make pull-log`? These are in tension. Still open — not reached yet.
  Orthogonal to (doesn't resolve) the RAM-ring-buffer/flush-cadence
  question in the Strawman architecture section above — the erase/
  multicore-pause cost of a raw flash write applies either way; a FAT
  filesystem would just add its own bookkeeping writes on top.
