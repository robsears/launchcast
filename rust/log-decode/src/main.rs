//! Decodes a raw dump of the rocket's flash-log partition (pulled via
//! `picotool save -r ...` while the board is in BOOTSEL mode -- see the
//! Makefile's `pull-log-rust` target for the exact command) into one CSV
//! per flight session found in it.
//!
//! Usage: `log-decode <dump.bin> [output-dir]`
//!
//! A "session" is one arm cycle that actually logged something (BOOT/
//! IDLE never log, and DISARM-without-boost erases its own attempt, see
//! `rocket/src/flash_log.rs`). Multiple sessions routinely turn up in
//! one dump, since nothing erases the partition between flights unless
//! `make clean-log-rust` is run -- see [`find_sessions`] for exactly how
//! a run of records gets split into sessions (it's not just "stop at the
//! first blank/corrupt record" -- an early real pull caught a case where
//! that alone wasn't enough).

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use launchcast_rocket_logic::flash_log::{decode_record, LogEntry, RECORD_SIZE, SECTOR_SIZE};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let Some(input_path) = args.get(1) else {
        eprintln!("usage: log-decode <dump.bin> [output-dir]");
        return ExitCode::FAILURE;
    };

    let data = match fs::read(input_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error reading {input_path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let out_dir: PathBuf = match args.get(2) {
        Some(d) => PathBuf::from(d),
        None => Path::new(input_path).parent().map(Path::to_path_buf).unwrap_or_default(),
    };
    let stem = Path::new(input_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "flight".to_string());

    let sessions = find_sessions(&data);

    if sessions.is_empty() {
        println!("no valid records found in {input_path} -- empty partition, or wrong offset/range dumped?");
        return ExitCode::SUCCESS;
    }

    for (i, session) in sessions.iter().enumerate() {
        let out_path = out_dir.join(format!("{stem}-session{i}.csv"));
        if let Err(e) = write_csv(&out_path, session) {
            eprintln!("error writing {}: {e}", out_path.display());
            return ExitCode::FAILURE;
        }
        let t_start = session.first().map(|e| e.t_ms).unwrap_or(0);
        let t_end = session.last().map(|e| e.t_ms).unwrap_or(0);
        println!(
            "session {i}: {} records, t={}..{}ms ({:.1}s) -> {}",
            session.len(),
            t_start,
            t_end,
            t_end.saturating_sub(t_start) as f32 / 1000.0,
            out_path.display()
        );
    }

    ExitCode::SUCCESS
}

/// Scan `data` for runs of valid records, splitting into a new session
/// wherever a run ends. Two distinct signals split a run into separate
/// sessions:
///   - an invalid/blank record -- the obvious case, and the only one the
///     first version of this function checked for.
///   - `t_ms` going *backward* between two otherwise-valid consecutive
///     records -- `t_ms` is uptime since boot for whichever arm cycle
///     wrote it, so it can only decrease at a boundary between two
///     different power cycles. This matters because a new arm cycle can
///     start immediately after the previous one's data with **zero
///     gap**: `ArmCycleEvent::Start` only guarantees a *sector-aligned*
///     start, not a blank one -- if the previous cycle's last record
///     happened to land exactly on a sector boundary already, the next
///     cycle's first record follows immediately with no invalid byte in
///     between to catch. Missing this produced real, confusing output on
///     an early real pull: a "session" whose timestamps jumped
///     65s -> 11s partway through, silently splicing two unrelated arm
///     cycles into one.
///
/// Either way, the next search resumes at the *next* sector boundary
/// strictly after the current position -- not
/// [`launchcast_rocket_logic::flash_log::align_up_to_sector`], which
/// leaves an already-aligned offset unchanged (correct for finding where
/// a *new* write should start, wrong here: it would get stuck retrying
/// the same failed offset forever if a session happened to end exactly
/// on a boundary). Matches how the firmware only ever starts a new arm
/// cycle on a sector boundary, so this is both correct and lets the scan
/// skip whole sectors at a time across large blank stretches instead of
/// retrying one byte at a time.
fn find_sessions(data: &[u8]) -> Vec<Vec<LogEntry>> {
    let sector_size = SECTOR_SIZE as usize;
    let mut sessions = Vec::new();
    let mut current: Vec<LogEntry> = Vec::new();
    let mut offset = 0usize;

    while offset + RECORD_SIZE <= data.len() {
        let record: [u8; RECORD_SIZE] = data[offset..offset + RECORD_SIZE].try_into().unwrap();
        match decode_record(&record) {
            Some(entry) => {
                if current.last().is_some_and(|last: &LogEntry| entry.t_ms < last.t_ms) {
                    sessions.push(std::mem::take(&mut current));
                }
                current.push(entry);
                offset += RECORD_SIZE;
            }
            None => {
                if !current.is_empty() {
                    sessions.push(std::mem::take(&mut current));
                }
                offset = (offset / sector_size + 1) * sector_size;
            }
        }
    }
    if !current.is_empty() {
        sessions.push(current);
    }
    sessions
}

fn write_csv(path: &Path, session: &[LogEntry]) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    writeln!(
        f,
        "t_ms,state,alt_m,vel_mps,pressure_hpa,temp_c,accel_x_mps2,accel_y_mps2,accel_z_mps2,gyro_x_dps,gyro_y_dps,gyro_z_dps"
    )?;
    for e in session {
        writeln!(
            f,
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            e.t_ms,
            e.state,
            e.alt_m,
            e.vel_mps,
            e.pressure_hpa,
            e.temp_c,
            e.accel_mps2[0],
            e.accel_mps2[1],
            e.accel_mps2[2],
            e.gyro_dps[0],
            e.gyro_dps[1],
            e.gyro_dps[2],
        )?;
    }
    Ok(())
}
