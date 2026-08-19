//! Raw-partition flight log -- flash I/O and the RAM ring buffer. Record
//! format/encoding lives in `rocket-logic::flash_log` (hardware-free,
//! host-tested); this module is the hardware-touching half.
//!
//! **Split by core, not by convenience**: `embassy-rp`'s flash driver
//! only allows `blocking_erase`/`blocking_write` to be issued from core0,
//! and each call forcibly pauses core1 for its duration (see
//! `docs/rust-rewrite.md`'s Strawman architecture section for the full
//! finding, checked against the driver's actual source, not assumed).
//! So [`LogWriter`] (core1) only ever touches two static, double-
//! buffered `LogBatch`es in SRAM -- appending a sample is just a memory
//! write, never blocks on flash -- while [`LogArchive`] (core0) is the
//! only thing that ever calls into the flash peripheral. The two hand
//! off *which buffer index is ready*, not the data itself: core1 keeps
//! filling the other buffer while core0 flushes one, so a slow flash
//! operation on core0 never stalls core1's sampling loop. (A single
//! shared buffer behind one mutex would reintroduce exactly that stall
//! the moment core1's next sample landed while core0 still held the
//! lock -- the whole reason for double-buffering here, not just
//! following a pattern for its own sake.)
//!
//! **Why appends can't just erase whatever sector they land in**: once
//! any bytes have been programmed into a sector, erasing it to make room
//! for more would destroy what's already there (NOR flash erases a whole
//! sector at a time, never a partial one). So a sector is only ever
//! erased the *first* time an append enters it (detected by `write_ptr`
//! landing exactly on a sector boundary); every later append into the
//! same sector just programs into its still-erased tail directly.
//!
//! **Why DISARM-without-boost is safe to implement as a plain erase**:
//! every arm cycle's data starts at a sector-aligned offset
//! ([`LogArchive::start_arm_cycle`] rounds `write_ptr` up to the next
//! boundary before capturing it) specifically so that erasing the
//! sectors an aborted cycle wrote into ([`LogArchive::rewind_without_boost`])
//! can never also erase the tail end of an earlier real flight's data --
//! that data, if any, is guaranteed to end at or before that same
//! boundary, never mid-sector alongside the aborted cycle's bytes. (User
//! call, 2026-08-18: rewind to this arm cycle's start, not a full-log
//! wipe like `rocket/code.py`'s literal `open(path, "wb")` -- see
//! `docs/rust-rewrite.md`.)

use embassy_rp::flash::{Blocking, Flash};
use embassy_rp::peripherals::FLASH;
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant};
use launchcast_rocket_logic::flash_log::{
    align_up_to_sector, decode_record, encode_record, LogEntry, FLASH_SIZE, LOG_PARTITION_OFFSET,
    LOG_PARTITION_SIZE, RECORD_SIZE, SECTOR_SIZE,
};

const FLASH_SIZE_USIZE: usize = FLASH_SIZE as usize;

/// Flush trigger: whichever comes first. User-specified, 2026-08-18 --
/// covers a full boost+coast phase (worst case ~2.8s for the largest
/// Estes motor this project flies) without ever flushing mid-phase, at a
/// trivial (tens of KB) SRAM cost.
const MAX_BATCH_ENTRIES: usize = 500;
const BATCH_BUF_SIZE: usize = MAX_BATCH_ENTRIES * RECORD_SIZE; // 24000 bytes
const FLUSH_WINDOW_MS: u64 = 5000;

struct LogBatch {
    data: [u8; BATCH_BUF_SIZE],
    len: usize,
}

impl LogBatch {
    const fn empty() -> Self {
        Self { data: [0; BATCH_BUF_SIZE], len: 0 }
    }

    /// Append one already-encoded record. Returns `true` if the buffer
    /// is now full.
    fn push(&mut self, record: &[u8; RECORD_SIZE]) -> bool {
        self.data[self.len..self.len + RECORD_SIZE].copy_from_slice(record);
        self.len += RECORD_SIZE;
        self.len >= BATCH_BUF_SIZE
    }
}

/// The double buffer -- see module docs. Index is whatever `LogWriter`
/// says is "active" at any moment; ownership alternates by protocol
/// (only one core ever writes to a given index at a time), the `Mutex`
/// just makes that safe to the compiler too, not because real contention
/// is expected.
static BATCHES: [Mutex<CriticalSectionRawMutex, LogBatch>; 2] =
    [Mutex::new(LogBatch::empty()), Mutex::new(LogBatch::empty())];

/// core1 -> core0: "buffer index N is ready to flush." Capacity two, so
/// core1 is never forced to wait mid-send if core0 is a beat slow to
/// drain -- it still naturally blocks on `BATCHES[n]`'s own lock before
/// ever reusing a buffer core0 hasn't finished with, so this isn't
/// load-bearing for correctness, just avoids an avoidable stall.
pub static FLUSH_REQUESTS: Channel<CriticalSectionRawMutex, usize, 2> = Channel::new();

#[derive(Clone, Copy)]
pub enum ArmCycleEvent {
    /// Sent on ARM (IDLE -> ARMED). Aligns the write pointer to a fresh
    /// sector boundary -- see module docs.
    Start,
    /// Sent on DISARM without ever having reached BOOST. Erases and
    /// rewinds to where `Start` left off.
    RewindWithoutBoost,
}

/// core1 -> core0, same channel-ordering reasoning as `FLUSH_REQUESTS`:
/// arm-cycle events and flush requests are on separate channels, but
/// within this one, order is preserved, so a `RewindWithoutBoost` can
/// never be processed out of order relative to the `Start` it belongs to.
pub static ARM_CYCLE_EVENTS: Channel<CriticalSectionRawMutex, ArmCycleEvent, 2> = Channel::new();

/// Core1-side handle: encodes and buffers samples, decides when to ask
/// core0 to flush. Never touches flash.
pub struct LogWriter {
    active: usize,
    count: u32,
    window_start: Instant,
}

impl LogWriter {
    pub fn new(now: Instant) -> Self {
        Self { active: 0, count: 0, window_start: now }
    }

    /// Encode and append one sample, flushing immediately if this fills
    /// the buffer (the "500 entries" half of the flush trigger).
    pub async fn push(&mut self, entry: &LogEntry) {
        let record = encode_record(entry);
        let full = {
            let mut buf = BATCHES[self.active].lock().await;
            buf.push(&record)
        };
        self.count += 1;
        if full {
            self.flush_now().await;
        }
    }

    /// Call once per core1 loop iteration regardless of whether a sample
    /// was just pushed -- the "5 seconds" half of the flush trigger has
    /// to fire even between samples during a slow (BOOT/IDLE-cadence)
    /// stretch.
    pub async fn maybe_flush_on_timer(&mut self, now: Instant) {
        if self.count > 0 && (now - self.window_start) >= Duration::from_millis(FLUSH_WINDOW_MS) {
            self.flush_now().await;
        }
    }

    /// Force a flush of whatever's currently buffered, if anything --
    /// matches `code.py`'s `log.flush()` call on the transition into
    /// LANDED, rather than waiting for the count/time trigger.
    pub async fn flush_if_any(&mut self) {
        if self.count > 0 {
            self.flush_now().await;
        }
    }

    async fn flush_now(&mut self) {
        // Best-effort, matches this whole system's "a log failure must
        // never stall the flight loop" contract -- if the channel is
        // somehow full (shouldn't happen at capacity 2 given how far
        // apart flushes are), drop this flush request rather than block.
        let _ = FLUSH_REQUESTS.try_send(self.active);
        self.active = 1 - self.active;
        self.count = 0;
        self.window_start = Instant::now();
    }
}

/// Core0-side handle: owns the flash peripheral and the write pointer.
/// Only this type ever calls `blocking_erase`/`blocking_write`.
pub struct LogArchive<'d> {
    flash: Flash<'d, FLASH, Blocking, FLASH_SIZE_USIZE>,
    write_ptr: u32,
    arm_cycle_start: Option<u32>,
}

impl<'d> LogArchive<'d> {
    /// Scans the partition from its start for the last valid record --
    /// see `rocket-logic::flash_log`'s docs on why there's no separate
    /// persistent write-pointer header to maintain instead. XIP reads
    /// are effectively free (no `in_ram`/core0 restriction at all --
    /// only erase/program need that), so this is fast even worst-case.
    pub fn new(flash_peri: Peri<'d, FLASH>) -> Self {
        let mut flash = Flash::new_blocking(flash_peri);
        let write_ptr = Self::scan_for_resume_point(&mut flash);
        Self { flash, write_ptr, arm_cycle_start: None }
    }

    fn scan_for_resume_point(flash: &mut Flash<'d, FLASH, Blocking, FLASH_SIZE_USIZE>) -> u32 {
        let end = LOG_PARTITION_OFFSET + LOG_PARTITION_SIZE;
        let mut offset = LOG_PARTITION_OFFSET;
        let mut buf = [0u8; RECORD_SIZE];
        while offset + RECORD_SIZE as u32 <= end {
            if flash.blocking_read(offset, &mut buf).is_err() {
                break;
            }
            if decode_record(&buf).is_none() {
                break;
            }
            offset += RECORD_SIZE as u32;
        }
        offset
    }

    /// Handle one event from [`ARM_CYCLE_EVENTS`].
    pub fn handle_arm_cycle_event(&mut self, event: ArmCycleEvent) {
        match event {
            ArmCycleEvent::Start => {
                self.write_ptr = align_up_to_sector(self.write_ptr).min(LOG_PARTITION_OFFSET + LOG_PARTITION_SIZE);
                self.arm_cycle_start = Some(self.write_ptr);
            }
            ArmCycleEvent::RewindWithoutBoost => {
                if let Some(start) = self.arm_cycle_start {
                    if self.write_ptr > start {
                        let erase_end = align_up_to_sector(self.write_ptr);
                        let _ = self.flash.blocking_erase(start, erase_end);
                    }
                    self.write_ptr = start;
                }
                self.arm_cycle_start = None;
            }
        }
    }

    /// Handle one flush request from [`FLUSH_REQUESTS`]: read the
    /// indicated buffer out, write it to flash (erasing freshly-entered
    /// sectors as needed, never a sector already holding data), and
    /// reset that buffer for reuse.
    pub async fn handle_flush(&mut self, batch_index: usize) {
        let mut buf = BATCHES[batch_index].lock().await;
        self.append(&buf.data[..buf.len]);
        buf.len = 0;
    }

    fn append(&mut self, mut data: &[u8]) {
        let partition_end = LOG_PARTITION_OFFSET + LOG_PARTITION_SIZE;
        while !data.is_empty() {
            if self.write_ptr >= partition_end {
                // Partition full -- matches this system's "logging
                // degrades to a no-op, the flight continues" contract
                // rather than panicking or wrapping over old data.
                return;
            }
            let sector_start = (self.write_ptr / SECTOR_SIZE) * SECTOR_SIZE;
            let sector_end = sector_start + SECTOR_SIZE;

            // Only erase a sector the moment we're at its very start --
            // see module docs for why re-erasing mid-sector would
            // destroy data already written there this arm cycle.
            if self.write_ptr == sector_start {
                let _ = self.flash.blocking_erase(sector_start, sector_end.min(partition_end));
            }

            let chunk_end = sector_end.min(partition_end).min(self.write_ptr + data.len() as u32);
            let chunk_len = (chunk_end - self.write_ptr) as usize;
            let _ = self.flash.blocking_write(self.write_ptr, &data[..chunk_len]);

            self.write_ptr += chunk_len as u32;
            data = &data[chunk_len..];
        }
    }
}
