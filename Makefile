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
#   make deploy-rocket     copy payload firmware to the rocket board
#   make deploy-ground     copy handheld firmware to the ground station
#   make libs-rocket       install CircuitPython libraries on the rocket board
#   make pull-log          retrieve flight.bin from the rocket board
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
GROUND_FILES := ground/code.py $(SHARED)

ROCKET_LIBS := adafruit_rfm9x adafruit_gps adafruit_bmp5xx \
               adafruit_lsm6ds adafruit_lis3mdl neopixel
GROUND_LIBS := adafruit_rfm9x adafruit_gps adafruit_sharpmemorydisplay \
               adafruit_framebuf

.PHONY: help test lint check fmt deploy-rocket deploy-ground \
        libs-rocket libs-ground pull-log clean-log monitor volumes doctor

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