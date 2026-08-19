MEMORY {
    /* RP2040: first 256 bytes reserved for the second-stage bootloader
     * that embassy-rp/rp2040-boot2 links in separately (same as ground). */
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100

    /* This board's external flash is confirmed 8MB (GD25Q64C/W25Q64JVxQ --
     * "Q64" = 64Mbit -- per this board's mpconfigboard.mk in the
     * CircuitPython source, `EXTERNAL_FLASH_DEVICES = "GD25Q64C,W25Q64JVxQ"`),
     * not the 2MB `ground/memory.x` conservatively declares (ground never
     * needed more). The linker's FLASH region below is deliberately capped
     * at 1MB -- generous headroom over the firmware's actual size, but a
     * hard boundary the linker itself will refuse to cross. Everything
     * beyond it, up to the physical 8MB, is the raw flight-log partition
     * (see rocket/src/flash_log.rs) -- reserved by convention/offset
     * constant, not represented as its own MEMORY region, since nothing
     * ever links code or data there.
     */
    FLASH : ORIGIN = 0x10000100, LENGTH = 1024K - 0x100
    RAM   : ORIGIN = 0x20000000, LENGTH = 256K
}

EXTERN(BOOT2_FIRMWARE)

SECTIONS {
    .boot2 ORIGIN(BOOT2) :
    {
        KEEP(*(.boot2));
    } > BOOT2
} INSERT BEFORE .text;
