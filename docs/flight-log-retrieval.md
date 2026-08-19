# Retrieving Flight Logs (Rust Firmware)

How the rocket's flight log actually works under the Rust firmware, and the
runbook for getting it off the board after a flight. Rust-only -- the
CircuitPython firmware wrote a plain `flight.bin` file to a mounted FAT
volume instead (`make pull-log`), which doesn't apply here at all; see
`docs/rust-rewrite.md` for why that changed.

## How it works

The rocket's flash chip is 8MB. The first 1MB is reserved for firmware code;
everything from there to the end of the chip -- **7MB** -- is a raw log
partition (`rocket-logic/src/flash_log.rs`'s `LOG_PARTITION_OFFSET`/
`LOG_PARTITION_SIZE`). Raw, not a filesystem: no FAT, no `flight.bin` file,
just fixed-size 48-byte records written back to back. That's deliberate --
CircuitPython's FAT filesystem corrupted itself more than once during
development (`docs/filesystem-recovery.md`), and a raw partition can't suffer
that particular failure mode at all.

**Logging starts at ARM, not at boost-detect.** Every state from `ARMED`
onward gets logged (ARMED, BOOST, COAST, APOGEE, DESCENT, LANDED) -- one
record per main-loop tick, capped at 200Hz (~48 bytes every 5ms, ~9.6KB/s).
At that rate, 7MB holds roughly **12-13 minutes** of continuous logging --
far more than a single flight from arm to landed needs.

**Arming does not erase previous flights.** The write pointer persists
across power cycles (the firmware re-scans the partition on every boot to
find where it left off) and only ever advances forward. A new `ARM` just
rounds up to the next flash sector boundary from wherever the last one
ended -- it appends, it doesn't overwrite. The *only* thing that erases
anything is a `DISARM` sent while still `ARMED` (before boost) -- that
specific aborted attempt's own bytes get rewound, and only those bytes;
everything from earlier flights is untouched, because the erase range can
never reach backward past where the current arm cycle started. Bottom line:
**you do not need to pull the log before every flight.** You have room for
many flights before this becomes a concern, and even then the firmware
degrades to "stop logging, keep flying" rather than losing or corrupting
anything already written.

**Power loss is safe, with one caveat.** Whatever's already been flushed to
flash survives a crash or power cycle -- flash is non-volatile, and the
firmware resumes exactly where it left off. Samples are buffered in RAM
first and flushed to flash every 500 entries or 5 seconds, whichever comes
first, so in the worst case the last few seconds before a sudden power loss
could be lost. The transition into `LANDED` also force-flushes immediately,
so once a beacon starts chirping, that flight's data is already durably on
flash regardless of what happens next.

## Retrieval runbook

1. **Get the board into BOOTSEL mode.** Two ways:
   - **Double-tap RESET.** As of the 2026-08-19 firmware, this is
     implemented in the Rust firmware itself (a watchdog-scratch-register
     check, ~500ms window) -- it is *not* an RP2040 hardware feature, and
     wasn't implemented by anything in our toolchain before that date (see
     `docs/rust-rewrite.md`). If your board predates that fix, or the
     timing doesn't land, this may not work.
   - **Hold BOOT, then RESET** (or hold BOOT while plugging in USB), release
     BOOT after a second or two. This *is* a genuine RP2040 silicon-level
     check the boot ROM does before any firmware runs, independent of
     whatever's flashed -- use this if double-tap isn't landing. Confirm
     you're in with `lsusb` (`2e8a:0003 Raspberry Pi RP2 Boot`) if unsure.
   - Either way: **entering BOOTSEL never touches flash contents.** Nothing
     is erased just by being in the bootloader -- the flight data is safe
     regardless of which method gets you there.

2. **Pull and decode:**
   ```
   make pull-log-rust
   ```
   This reads the raw partition via `picotool` (memory-mapped range
   `0x10100000..0x10800000` -- see `docs/rust-rewrite.md` if you want the
   flash-offset-to-XIP-address math spelled out), saves it to
   `flights/<timestamp>-rust.bin`, then immediately runs `log-decode` on it
   to produce one CSV per flight session into the same directory.

3. **If `picotool` fails with "unable to connect... try sudo"**: this is a
   USB permissions issue, not a project bug. On NixOS, add the package's own
   udev rules to your system config:
   ```nix
   services.udev.packages = [ pkgs.picotool ];
   ```
   then `sudo nixos-rebuild switch` and replug the board. `nixpkgs`'s
   `picotool` ships the correct rule already (`uaccess`-tagged, grants the
   active login session direct access, no group membership needed) -- you
   just have to install it. **Don't reach for `sudo make pull-log-rust`** as
   a permanent fix: that also runs `cargo run` as root, which leaves
   root-owned files under `rust/target/` that a later unprivileged `cargo`
   invocation can't overwrite (`Permission denied` on some `.fingerprint`
   file). If that's already happened, `sudo chown -R "$USER" rust/target`
   to clean it up, or just delete `rust/target` and let it rebuild.

## What the output means

`log-decode` prints one line per session found, e.g.:
```
session 0: 5917 records, t=28402..69551ms (41.1s) -> flights/<stamp>-rust-session0.csv
```
`t_ms` is uptime-since-boot *for that specific power cycle* -- it resets to
near-zero on every reboot, so it's only meaningful within one session, not
across them or against a wall clock.

**A "session" is one arm cycle**, split from its neighbors on either of two
signals: an invalid/blank record (the obvious case -- ran out of real data),
or `t_ms` going *backward* between two otherwise-valid records. The second
one exists because a new arm cycle can start immediately after the previous
one's data with **zero gap** -- `ArmCycleEvent::Start` only guarantees a
sector-aligned start, not a blank one, so if the previous cycle's last
record happened to land exactly on a sector boundary already, there's no
invalid byte between them to split on. `t_ms` resetting is the only
reliable signal in that case. (This was found the hard way on the very first
real pull off this board: one "session" showed `-12.5 hPa` pressure and
timestamps that jumped `64623 -> 11024` partway through -- two unrelated
arm cycles from different testing sessions, spliced together. Fixed in
`log-decode`; see `docs/rust-rewrite.md`'s dated entry if you want the full
diagnosis.)

**Expect several sessions on a board that's been used for bench
testing.** Nothing erases the partition automatically -- every arm/disarm
cycle from every testing session since the board was last wiped is still in
there. If you want a clean pull that only reflects real flights going
forward, erase first (see below) *before* your next real flight, not after.

## Bulk-erasing old flights

```
make clean-log-rust
```
Same BOOTSEL requirement as retrieval, same `picotool` under the hood (an
`erase -r` over the same address range), same explicit type-`yes`-to-confirm
safety as the old CircuitPython-era `clean-log`. Pull anything worth keeping
first -- this is irreversible, and unlike a `DISARM`-without-boost (which
only ever touches its own aborted cycle's bytes), this erases *everything*
in the partition, every flight ever recorded on the board.
