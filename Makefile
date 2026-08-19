# LaunchCast
#
# There is no build step -- CircuitPython boards mount as USB mass storage and
# run code.py directly. "Deploy" is cp plus sync.
#
# The two boards are identical hardware running different firmware, so the
# deploy targets are deliberately separate and each verifies what it is about
# to overwrite. Flashing flight firmware onto the handheld is the easy mistake
# here; ROCKET_VOL and GROUND_VOL exist to make it hard.
#
#   make test              run the suite
#   make check             test + lint
#   make setup-rocket      label a fresh board as LC-ROCKET (one-time)
#   make setup-ground      label a fresh board as LC-GROUND (one-time)
#   make deploy-rocket     copy payload firmware to the rocket board
#   make deploy-ground     copy handheld firmware to the ground station
#   make libs-rocket       install CircuitPython libraries on the rocket board
#   make pull-log          retrieve flight.bin from the rocket board (CircuitPython)
#   make pull-log-rust     retrieve + decode the flight log (Rust firmware)
#   make clean-log-rust    bulk-erase every flight in the log partition (Rust firmware)
#   make monitor           open a serial console

# Portable across NixOS, macOS, and CI:
SHELL := $(shell command -v bash)

# --- Volume discovery --------------------------------------------------------
# Relabel each board via boot.py (LC-ROCKET / LC-GROUND) and these resolve
# unambiguously even with both plugged in. Falls back to CIRCUITPY for a
# freshly flashed board that has not run boot.py yet.

MOUNT_ROOTS := /run/media/$(USER) /media/$(USER) /Volumes /mnt

define find_vol
$(firstword $(wildcard $(foreach r,$(MOUNT_ROOTS),$(r)/$(1))))
endef

ROCKET_VOL ?= $(call find_vol,LC-ROCKET)
GROUND_VOL ?= $(call find_vol,LC-GROUND)
ANY_VOL    ?= $(call find_vol,CIRCUITPY)

PORT ?= /dev/ttyACM0
BAUD ?= 115200

# --- Files -------------------------------------------------------------------

SHARED       := common/packet.py
ROCKET_FILES := rocket/code.py rocket/boot.py $(SHARED)
GROUND_FILES := ground/code.py ground/boot.py ground/icons.py ground/rocket_art.py \
                ground/handheld_art.py ground/nav.py ground/hold_tracker.py \
                ground/units.py ground/imu.py \
                ground/display_util.py ground/screen_header.py ground/screen_footer.py \
                ground/screen_flight.py ground/screen_recovery.py ground/screen_diagnostics.py \
                ground/font5x8.bin $(SHARED)

ROCKET_LIBS := adafruit_rfm9x adafruit_gps adafruit_bmp5xx \
               adafruit_lsm6ds adafruit_lis3mdl neopixel
GROUND_LIBS := adafruit_rfm9x adafruit_gps adafruit_sharpmemorydisplay \
               adafruit_framebuf

.PHONY: help test lint check fmt deploy-rocket deploy-ground \
        libs-rocket libs-ground pull-log pull-log-rust clean-log clean-log-rust monitor volumes doctor \
		setup-rocket setup-ground

help:
	@grep -E '^#   make' $(MAKEFILE_LIST) | sed 's/^#   //'

# --- CI ----------------------------------------------------------------------

test:
	python -m pytest tests/ -q

lint:
	ruff check .

fmt:
	ruff format .

check: test lint

# --- First-time board setup --------------------------------------------------
# Deploy boot.py to a fresh (unlabeled) board so it self-labels on next reset.
# Plug in ONE board at a time. After running, press RESET, then `make volumes`
# to confirm the label took.

setup-rocket:
	@test -n "$(ANY_VOL)" || { echo "no CIRCUITPY volume -- plug in ONE fresh board"; exit 1; }
	@echo "deploying rocket boot.py to $(ANY_VOL)"
	cp rocket/boot.py "$(ANY_VOL)/boot.py"
	sync
	@echo ""
	@echo "  Done. Now press RESET on the board (or unplug/replug)."
	@echo "  Then run:  make volumes   -- should show rocket: .../LC-ROCKET"
	@echo "  If the label does not appear, unplug/replug once (host cache)."

setup-ground:
	@test -n "$(ANY_VOL)" || { echo "no CIRCUITPY volume -- plug in ONE fresh board"; exit 1; }
	@echo "deploying ground boot.py to $(ANY_VOL)"
	cp ground/boot.py "$(ANY_VOL)/boot.py"
	sync
	@echo ""
	@echo "  Done. Now press RESET on the board (or unplug/replug)."
	@echo "  Then run:  make volumes   -- should show ground: .../LC-GROUND"
	@echo "  If the label does not appear, unplug/replug once (host cache)."

# --- Deploy ------------------------------------------------------------------
# Every deploy runs the tests first. A packet.py that fails its own round-trip
# test has no business going onto a board that is about to fly.

deploy-rocket: check
	@$(call require_vol,$(ROCKET_VOL),LC-ROCKET,rocket)
	@echo "--> $(ROCKET_VOL)"
	@test -d "$(ROCKET_VOL)" || { echo "not a directory: $(ROCKET_VOL)"; exit 1; }
	cp $(ROCKET_FILES) "$(ROCKET_VOL)/"
	sync
	@echo "rocket firmware deployed"

deploy-ground: check
	@$(call require_vol,$(GROUND_VOL),LC-GROUND,ground)
	@echo "--> $(GROUND_VOL)"
	@test -d "$(GROUND_VOL)" || { echo "not a directory: $(GROUND_VOL)"; exit 1; }
	cp $(GROUND_FILES) "$(GROUND_VOL)/"
	sync
	@echo "ground firmware deployed"

# Escape hatch for a board that has not been relabeled yet. Names the target
# explicitly so it cannot happen by accident.
deploy-rocket-unlabeled: check
	@test -n "$(ANY_VOL)" || { echo "no CIRCUITPY volume found"; exit 1; }
	@echo "WARNING: deploying ROCKET firmware to unlabeled $(ANY_VOL)"
	@read -p "type yes to continue: " a; [ "$$a" = yes ]
	cp $(ROCKET_FILES) "$(ANY_VOL)/"
	sync

deploy-ground-unlabeled: check
	@test -n "$(ANY_VOL)" || { echo "no CIRCUITPY volume found"; exit 1; }
	@echo "WARNING: deploying GROUND firmware to unlabeled $(ANY_VOL)"
	@read -p "type yes to continue: " a; [ "$$a" = yes ]
	cp $(GROUND_FILES) "$(ANY_VOL)/"
	sync

# --- Libraries ---------------------------------------------------------------
# circup reads the board's CircuitPython version and fetches matching .mpy
# files. Run once per board, and again after a CircuitPython upgrade.

libs-rocket:
	@$(call require_vol,$(ROCKET_VOL),LC-ROCKET,rocket)
	circup --path "$(ROCKET_VOL)" install $(ROCKET_LIBS)

libs-ground:
	@$(call require_vol,$(GROUND_VOL),LC-GROUND,ground)
	circup --path "$(GROUND_VOL)" install $(GROUND_LIBS)

libs-update:
	circup update

# --- Flight data -------------------------------------------------------------

pull-log:
	@$(call require_vol,$(ROCKET_VOL),LC-ROCKET,rocket)
	@test -f "$(ROCKET_VOL)/flight.bin" || { echo "no flight.bin on board"; exit 1; }
	@mkdir -p flights
	@stamp=$$(date +%Y%m%d-%H%M%S); \
	 cp "$(ROCKET_VOL)/flight.bin" "flights/$$stamp.bin"; \
	 echo "saved flights/$$stamp.bin ($$(stat -c%s "flights/$$stamp.bin" 2>/dev/null || stat -f%z "flights/$$stamp.bin") bytes)"

# The Rust firmware (rust/rocket) moved flight logging off a FAT filesystem
# entirely onto a raw flash partition (see rust/rocket-logic/src/flash_log.rs's
# docs -- board-filesystem corruption was a real, recurring CircuitPython
# problem, see docs/filesystem-recovery.md) -- there is no flight.bin file to
# `cp` for it, so pull-log doesn't apply to a Rust-flashed rocket at all.
# Retrieval instead means reading that partition's raw bytes over USB via
# picotool while the board sits in its ROM bootloader -- the same double-tap-
# RESET -> RPI-RP2 state already used to flash a new .uf2 (see
# docs/rust-rewrite.md) -- then decoding the result with launchcast-log-decode
# (rust/log-decode), which knows the on-flash record format because it shares
# rust/rocket-logic's decode function rather than reimplementing it.
#
# Address range: rust/rocket-logic/src/flash_log.rs's LOG_PARTITION_OFFSET
# (0x100000) through FLASH_SIZE (0x800000), offset by the RP2040's XIP base
# (0x10000000) -- picotool addresses are memory-mapped, not flash-relative.
LOG_PARTITION_START := 0x10100000
LOG_PARTITION_END   := 0x10800000

pull-log-rust:
	@mkdir -p flights
	@echo "reading the rocket's log partition via picotool -- board must be in"
	@echo "BOOTSEL mode (double-tap RESET; same as flashing a new .uf2)."
	@echo "7MB over USB in bootloader mode -- expect this to take a minute or so."
	@stamp=$$(date +%Y%m%d-%H%M%S); \
	 raw="flights/$$stamp-rust.bin"; \
	 picotool save -r $(LOG_PARTITION_START) $(LOG_PARTITION_END) "$$raw" \
	   || { echo "picotool failed -- is the rocket actually in BOOTSEL mode?"; exit 1; }; \
	 cargo run --release --manifest-path rust/Cargo.toml -p launchcast-log-decode -- "$$raw" flights

# Bulk-erase every flight ever recorded, not just the most recent one -- see
# rust/rocket-logic/src/flash_log.rs's docs on why a normal DISARM-without-
# boost can't do this itself (it only ever erases its own aborted cycle's
# bytes, by design, never anything earlier). `picotool erase -r` operates
# entirely offline against the flash chip while the board sits in BOOTSEL --
# the firmware isn't even running -- so on the next boot, LogArchive's resume
# scan finds nothing and starts write_ptr back at the partition's start,
# exactly as if the board had never logged a flight in its life. Deliberately
# not part of pull-log-rust, same reasoning as the CircuitPython-era
# clean-log above: erasing every flight you've ever recorded is not something
# to do as a side effect of retrieving one.
clean-log-rust:
	@echo "WARNING: this erases the ENTIRE flight-log partition -- every flight"
	@echo "ever recorded on this board, not just the most recent one. Make sure"
	@echo "you have already pulled anything worth keeping (make pull-log-rust)."
	@echo "Board must be in BOOTSEL mode (double-tap RESET)."
	@read -p "erase the whole log partition? type yes: " a; [ "$$a" = yes ]
	picotool erase -r $(LOG_PARTITION_START) $(LOG_PARTITION_END) \
	  || { echo "picotool failed -- is the rocket actually in BOOTSEL mode?"; exit 1; }
	@echo "erased -- next boot resumes logging from the start of the partition."

# Deliberately not part of pull-log. Erasing the only copy of a flight is not
# something to do as a side effect.
clean-log:
	@$(call require_vol,$(ROCKET_VOL),LC-ROCKET,rocket)
	@read -p "erase flight.bin on the board? type yes: " a; [ "$$a" = yes ]
	rm -f "$(ROCKET_VOL)/flight.bin"
	sync

# --- Serial ------------------------------------------------------------------

monitor:
	@test -e $(PORT) || { echo "$(PORT) not present -- set PORT=..."; exit 1; }
	minicom -D $(PORT) -b $(BAUD)

# --- Diagnostics -------------------------------------------------------------

volumes:
	@echo "rocket:    $(if $(ROCKET_VOL),$(ROCKET_VOL),not found)"
	@echo "ground:    $(if $(GROUND_VOL),$(GROUND_VOL),not found)"
	@echo "unlabeled: $(if $(ANY_VOL),$(ANY_VOL),none)"
	@echo "serial:    $(if $(wildcard $(PORT)),$(PORT),$(PORT) absent)"

doctor: volumes
	@echo
	@python -c "import struct,sys; sys.path.insert(0,'common'); import packet; \
	print('packet.py: telemetry', packet.TELEMETRY_SIZE, 'bytes, command', packet.COMMAND_SIZE, 'bytes')"
	@command -v circup >/dev/null && echo "circup:    $$(circup --version 2>&1 | head -1)" || echo "circup:    MISSING"
	@command -v ruff   >/dev/null && echo "ruff:      $$(ruff --version)" || echo "ruff:      MISSING"

# --- Helpers -----------------------------------------------------------------

define require_vol
if [ -z "$(1)" ]; then \
  echo "error: no volume labeled $(2) found."; \
  echo "  Is the $(3) board plugged in?"; \
  echo "  If it has not been relabeled yet, use: make deploy-$(3)-unlabeled"; \
  echo "  Or override: make deploy-$(3) $(shell echo $(3) | tr a-z A-Z)_VOL=/path/to/CIRCUITPY"; \
  exit 1; \
fi
endef