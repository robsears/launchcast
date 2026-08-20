# LaunchCast
#
# Both boards run Rust firmware, built from the rust/ Cargo workspace
# (common, ground-logic, rocket-logic, ground, rocket, log-decode).
# Deploying means BOOTSEL: double-tap RESET to mount the board as
# RPI-RP2, then copy the built .uf2 onto it -- there is no CircuitPython/
# mass-storage volume to `cp` source files to anymore. The old Python
# prototype this replaced lives under prototyping/, untouched by any of
# this (frozen reference, not part of the maintained build).
#
#   make test         run the Rust host-side test suite
#   make clippy       clippy across every crate, host + thumbv6m-none-eabi
#   make check        test + clippy
#   make build-uf2    build release firmware for both boards -> .uf2 files
#   make pull-log     retrieve + decode the rocket's flight log
#   make clean-log    bulk-erase the rocket's entire flight-log partition

# Portable across NixOS, macOS, and CI:
SHELL := $(shell command -v bash)

RUST_MANIFEST   := rust/Cargo.toml
FIRMWARE_TARGET := thumbv6m-none-eabi
UF2_DIR         := rust/target/$(FIRMWARE_TARGET)/release

# Host-testable (no hardware, no target triple needed): pure logic +
# the log-decode CLI. The two firmware crates (ground, rocket) only
# build for $(FIRMWARE_TARGET) -- see clippy/build-uf2 below.
HOST_CRATES     := launchcast-common launchcast-ground-logic launchcast-rocket-logic launchcast-log-decode
FIRMWARE_CRATES := launchcast-ground launchcast-rocket

.PHONY: help test clippy check build-uf2 pull-log clean-log

help:
	@grep -E '^#   make' $(MAKEFILE_LIST) | sed 's/^#   //'

# --- CI ------------------------------------------------------------------

test:
	cargo test --manifest-path $(RUST_MANIFEST) $(foreach c,$(HOST_CRATES),-p $(c))

# The firmware-target line below `cd`s into rust/ instead of using
# --manifest-path like everything else here: rust/.cargo/config.toml
# carries the -Tlink.x/-Tdefmt.x rustflags thumbv6m-none-eabi needs, and
# Cargo's config-file discovery walks up from the *current directory*,
# not from --manifest-path's directory -- invoking from the repo root
# with --manifest-path silently misses it (confirmed: a `cargo build`
# done that way links a non-bootable binary with entry point 0x0, no
# vector table, and elf2uf2-rs refuses to convert it). Host-target lines
# are unaffected either way -- those rustflags are scoped to
# [target.thumbv6m-none-eabi] only.
clippy:
	cargo clippy --manifest-path $(RUST_MANIFEST) $(foreach c,$(HOST_CRATES),-p $(c)) -- -D warnings
	cd rust && cargo clippy --release --target $(FIRMWARE_TARGET) \
		$(foreach c,$(FIRMWARE_CRATES),-p $(c)) -- -D warnings

check: test clippy

# --- Build + flash ------------------------------------------------------------
# elf2uf2-rs converts the linked ELF into a UF2 image the RP2040's ROM
# bootloader can flash directly -- drag-and-drop (or `cp`) onto the
# RPI-RP2 volume that appears after a double-tap RESET into BOOTSEL.

build-uf2:
	cd rust && cargo build --release --target $(FIRMWARE_TARGET) \
		$(foreach c,$(FIRMWARE_CRATES),-p $(c))
	elf2uf2-rs $(UF2_DIR)/launchcast-ground $(UF2_DIR)/launchcast-ground.uf2
	elf2uf2-rs $(UF2_DIR)/launchcast-rocket $(UF2_DIR)/launchcast-rocket.uf2
	@echo ""
	@echo "built:"
	@echo "  $(UF2_DIR)/launchcast-ground.uf2"
	@echo "  $(UF2_DIR)/launchcast-rocket.uf2"
	@echo "flash: double-tap RESET on the target board (BOOTSEL/RPI-RP2), then copy"
	@echo "the matching .uf2 onto it."

# --- Flight data ---------------------------------------------------------------
# Flight logging lives on a raw flash partition, not a filesystem (see
# rust/rocket-logic/src/flash_log.rs's docs -- board-filesystem corruption
# was a real, recurring problem with the old CircuitPython/FAT approach,
# see docs/filesystem-recovery.md). Retrieval means reading that
# partition's raw bytes over USB via picotool while the board sits in its
# ROM bootloader -- the same double-tap-RESET -> RPI-RP2 state already
# used to flash a new .uf2 -- then decoding the result with
# launchcast-log-decode, which shares rust/rocket-logic's own decode
# function rather than reimplementing the on-flash record format.
#
# Address range: rust/rocket-logic/src/flash_log.rs's LOG_PARTITION_OFFSET
# (0x100000) through FLASH_SIZE (0x800000), offset by the RP2040's XIP base
# (0x10000000) -- picotool addresses are memory-mapped, not flash-relative.
LOG_PARTITION_START := 0x10100000
LOG_PARTITION_END   := 0x10800000

pull-log:
	@mkdir -p flights
	@echo "reading the rocket's log partition via picotool -- board must be in"
	@echo "BOOTSEL mode (double-tap RESET; same as flashing a new .uf2)."
	@echo "7MB over USB in bootloader mode -- expect this to take a minute or so."
	@stamp=$$(date +%Y%m%d-%H%M%S); \
	 raw="flights/$$stamp-rust.bin"; \
	 picotool save -r $(LOG_PARTITION_START) $(LOG_PARTITION_END) "$$raw" \
	   || { echo "picotool failed -- is the rocket actually in BOOTSEL mode?"; exit 1; }; \
	 cargo run --release --manifest-path $(RUST_MANIFEST) -p launchcast-log-decode -- "$$raw" flights

# Bulk-erase every flight ever recorded, not just the most recent one --
# see rust/rocket-logic/src/flash_log.rs's docs on why a normal
# DISARM-without-boost can't do this itself (it only ever erases its own
# aborted cycle's bytes, by design, never anything earlier). `picotool
# erase -r` operates entirely offline against the flash chip while the
# board sits in BOOTSEL -- the firmware isn't even running -- so on the
# next boot, LogArchive's resume scan finds nothing and starts write_ptr
# back at the partition's start, exactly as if the board had never
# logged a flight in its life. Deliberately not part of pull-log: erasing
# every flight you've ever recorded is not something to do as a side
# effect of retrieving one.
clean-log:
	@echo "WARNING: this erases the ENTIRE flight-log partition -- every flight"
	@echo "ever recorded on this board, not just the most recent one. Make sure"
	@echo "you have already pulled anything worth keeping (make pull-log)."
	@echo "Board must be in BOOTSEL mode (double-tap RESET)."
	@read -p "erase the whole log partition? type yes: " a; [ "$$a" = yes ]
	picotool erase -r $(LOG_PARTITION_START) $(LOG_PARTITION_END) \
	  || { echo "picotool failed -- is the rocket actually in BOOTSEL mode?"; exit 1; }
	@echo "erased -- next boot resumes logging from the start of the partition."
