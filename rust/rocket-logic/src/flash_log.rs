//! Hardware-free half of the raw-partition flight log: record format
//! (encode/decode) and sector-alignment math. The actual flash I/O
//! (erase/program, the RAM ring buffer, the boot-time resume scan) lives
//! in `rocket/src/flash_log.rs` -- needs real hardware.
//!
//! Record layout (48 bytes, fixed): `[MAGIC(1), VERSION(1), t_ms:u32 LE,
//! state:u8, alt_m:f32 LE, vel_mps:f32 LE, pressure_hpa:f32 LE, temp_c:f32
//! LE, accel_mps2:[f32;3] LE, gyro_dps:[f32;3] LE, CHECKSUM(1)]` -- same
//! physical-unit fields as `rocket/code.py`'s `LOG_FMT`
//! (`"<IBffffffffff"`, 45-byte payload), wrapped in the same magic+
//! checksum framing convention already used for the wire packet format
//! (`common::MAGIC`, XOR checksum) rather than inventing a new one.
//!
//! Self-describing (magic + checksum) so a recovery/dump tool can scan
//! the raw partition for valid records and stop cleanly at the first
//! corrupt/blank one, rather than needing a separately-maintained index
//! of "how much of the partition is real data" -- the log doubles as its
//! own index. Fixed-size, not length-prefixed: this format never varies,
//! so a length field would be pure overhead. This is also how
//! `rocket/src/flash_log.rs` finds where to resume writing after a
//! reboot (scan forward from the partition start for the last valid
//! record) instead of maintaining a separate persistent write-pointer
//! header, which would have its own torn-write/corruption problem to
//! solve.

/// Matches `common::MAGIC` in spirit (same "does this look like a real
/// frame" role) -- not literally shared with it, since a log record and
/// a radio packet are never in the same byte stream to be confused.
pub const MAGIC: u8 = 0xA5;
pub const FORMAT_VERSION: u8 = 1;

// t_ms(4) + state(1) + alt/vel/pressure/temp(4*4=16) + accel(12) + gyro(12) = 45,
// matching rocket/code.py's LOG_SIZE exactly.
const PAYLOAD_SIZE: usize = 45;
/// magic(1) + version(1) + payload(45) + checksum(1).
pub const RECORD_SIZE: usize = 1 + 1 + PAYLOAD_SIZE + 1;

/// RP2040 flash erase granularity -- matches `embassy_rp::flash::ERASE_SIZE`.
pub const SECTOR_SIZE: u32 = 4096;

/// Total physical flash on this board -- confirmed 8MB
/// (GD25Q64C/W25Q64JVxQ, "Q64" = 64Mbit) via this board's CircuitPython
/// `mpconfigboard.mk`. See `rocket/memory.x`'s docs. Pure constants, no
/// hardware dependency, so they live here (not `rocket/src/flash_log.rs`)
/// specifically so a host-side recovery/decode tool can share the same
/// source of truth as the firmware instead of hand-copying these numbers
/// and risking drift.
pub const FLASH_SIZE: u32 = 8 * 1024 * 1024;
/// Matches `rocket/memory.x`'s reserved boundary: the linker's `FLASH`
/// region is capped at 1MB, so the log partition starts there and runs
/// to the end of physical flash. The linker itself refuses to link
/// firmware that grows past this, so it can never collide with the
/// partition.
pub const LOG_PARTITION_OFFSET: u32 = 1024 * 1024;
pub const LOG_PARTITION_SIZE: u32 = FLASH_SIZE - LOG_PARTITION_OFFSET;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogEntry {
    pub t_ms: u32,
    pub state: u8,
    pub alt_m: f32,
    pub vel_mps: f32,
    pub pressure_hpa: f32,
    pub temp_c: f32,
    pub accel_mps2: [f32; 3],
    pub gyro_dps: [f32; 3],
}

/// Encode one entry into its full framed, on-flash byte representation.
pub fn encode_record(e: &LogEntry) -> [u8; RECORD_SIZE] {
    let mut buf = [0u8; RECORD_SIZE];
    buf[0] = MAGIC;
    buf[1] = FORMAT_VERSION;
    buf[2..6].copy_from_slice(&e.t_ms.to_le_bytes());
    buf[6] = e.state;
    buf[7..11].copy_from_slice(&e.alt_m.to_le_bytes());
    buf[11..15].copy_from_slice(&e.vel_mps.to_le_bytes());
    buf[15..19].copy_from_slice(&e.pressure_hpa.to_le_bytes());
    buf[19..23].copy_from_slice(&e.temp_c.to_le_bytes());
    for i in 0..3 {
        buf[23 + i * 4..27 + i * 4].copy_from_slice(&e.accel_mps2[i].to_le_bytes());
    }
    for i in 0..3 {
        buf[35 + i * 4..39 + i * 4].copy_from_slice(&e.gyro_dps[i].to_le_bytes());
    }
    let checksum = buf[..RECORD_SIZE - 1].iter().fold(0u8, |acc, b| acc ^ b);
    buf[RECORD_SIZE - 1] = checksum;
    buf
}

/// Decode a record, validating magic/version/checksum. `None` for
/// anything that doesn't look like a genuine record -- erased flash
/// (`0xFF` bytes), a torn/partial write, or corruption -- exactly the
/// signal a boot-time resume scan or an offline recovery tool needs to
/// know "stop here."
pub fn decode_record(buf: &[u8; RECORD_SIZE]) -> Option<LogEntry> {
    if buf[0] != MAGIC || buf[1] != FORMAT_VERSION {
        return None;
    }
    let checksum = buf[..RECORD_SIZE - 1].iter().fold(0u8, |acc, b| acc ^ b);
    if checksum != buf[RECORD_SIZE - 1] {
        return None;
    }

    let f32_at = |off: usize| f32::from_le_bytes(buf[off..off + 4].try_into().unwrap());

    Some(LogEntry {
        t_ms: u32::from_le_bytes(buf[2..6].try_into().unwrap()),
        state: buf[6],
        alt_m: f32_at(7),
        vel_mps: f32_at(11),
        pressure_hpa: f32_at(15),
        temp_c: f32_at(19),
        accel_mps2: [f32_at(23), f32_at(27), f32_at(31)],
        gyro_dps: [f32_at(35), f32_at(39), f32_at(43)],
    })
}

/// Round `offset` up to the next flash sector boundary (or leave it
/// unchanged if already aligned). Used to align the *start* of every arm
/// cycle's log data to a sector boundary -- see `rocket/src/flash_log.rs`'s
/// docs on why: it's what makes a rewind-on-DISARM-without-boost safe to
/// implement as a plain sector erase, without any risk of that erase
/// touching a previous (real) flight's data living earlier in the same
/// sector.
pub fn align_up_to_sector(offset: u32) -> u32 {
    offset.div_ceil(SECTOR_SIZE) * SECTOR_SIZE
}
