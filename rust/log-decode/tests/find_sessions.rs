//! `find_sessions` isn't exposed as a library function (this crate is a
//! plain CLI binary, not a lib) -- these tests build the same synthetic
//! dumps `log-decode` would actually be run against and check its
//! stdout/output files, exercising the compiled binary directly rather
//! than an internal function, since that's the real interface this tool
//! promises to get right.

use std::fs;
use std::process::Command;

use launchcast_rocket_logic::flash_log::{encode_record, LogEntry, RECORD_SIZE, SECTOR_SIZE};

fn entry(t_ms: u32, state: u8) -> LogEntry {
    LogEntry {
        t_ms,
        state,
        alt_m: 1.0,
        vel_mps: 2.0,
        pressure_hpa: 1000.0,
        temp_c: 20.0,
        accel_mps2: [0.0, 0.0, 9.8],
        gyro_dps: [0.0, 0.0, 0.0],
    }
}

fn pad_to_sector(data: &mut Vec<u8>) {
    let sector = SECTOR_SIZE as usize;
    let pad = sector - (data.len() % sector);
    if pad != sector {
        data.extend(std::iter::repeat_n(0xFFu8, pad));
    }
}

fn run_log_decode(dump_path: &std::path::Path, out_dir: &std::path::Path) -> String {
    let exe = env!("CARGO_BIN_EXE_log-decode");
    let output = Command::new(exe)
        .arg(dump_path)
        .arg(out_dir)
        .output()
        .expect("failed to run log-decode");
    assert!(output.status.success(), "log-decode exited non-zero: {:?}", output);
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn a_single_session_decodes_to_one_csv() {
    let dir = tempdir();
    let mut data = Vec::new();
    for i in 0..3u32 {
        data.extend_from_slice(&encode_record(&entry(1000 + i * 20, 3)));
    }
    let dump_path = dir.join("dump.bin");
    fs::write(&dump_path, &data).unwrap();

    let stdout = run_log_decode(&dump_path, &dir);
    assert!(stdout.contains("session 0: 3 records"));

    let csv = fs::read_to_string(dir.join("dump-session0.csv")).unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 4); // header + 3 records
    assert!(lines[1].starts_with("1000,3,"));
    assert!(lines[3].starts_with("1040,3,"));
}

#[test]
fn two_sessions_separated_by_a_sector_gap_produce_two_csvs() {
    let dir = tempdir();
    let mut data = Vec::new();
    for i in 0..3u32 {
        data.extend_from_slice(&encode_record(&entry(1000 + i * 20, 3)));
    }
    pad_to_sector(&mut data);
    for i in 0..2u32 {
        data.extend_from_slice(&encode_record(&entry(5000 + i * 20, 7)));
    }
    pad_to_sector(&mut data);
    let dump_path = dir.join("dump.bin");
    fs::write(&dump_path, &data).unwrap();

    let stdout = run_log_decode(&dump_path, &dir);
    assert!(stdout.contains("session 0: 3 records"));
    assert!(stdout.contains("session 1: 2 records"));

    let csv0 = fs::read_to_string(dir.join("dump-session0.csv")).unwrap();
    let csv1 = fs::read_to_string(dir.join("dump-session1.csv")).unwrap();
    assert_eq!(csv0.lines().count(), 4); // header + 3
    assert_eq!(csv1.lines().count(), 3); // header + 2
}

#[test]
fn two_arm_cycles_with_no_gap_between_them_still_split_on_the_time_reset() {
    // Regression test: a new arm cycle can start immediately after the
    // previous one's data with *zero* blank/invalid bytes in between --
    // `ArmCycleEvent::Start` only guarantees a sector-aligned start, not
    // a gap. This is exactly what an early real pull caught: two
    // separate flights spliced into one "session" with timestamps that
    // jumped backward partway through. t_ms resetting (uptime since
    // boot for a fresh power cycle) is the only signal available here,
    // since there's no decode failure to split on.
    let dir = tempdir();
    let mut data = Vec::new();
    for i in 0..3u32 {
        data.extend_from_slice(&encode_record(&entry(60_000 + i * 20, 3))); // first cycle, late uptime
    }
    for i in 0..2u32 {
        data.extend_from_slice(&encode_record(&entry(1000 + i * 20, 7))); // second cycle, fresh boot
    }
    pad_to_sector(&mut data);
    let dump_path = dir.join("dump.bin");
    fs::write(&dump_path, &data).unwrap();

    let stdout = run_log_decode(&dump_path, &dir);
    assert!(stdout.contains("session 0: 3 records"));
    assert!(stdout.contains("session 1: 2 records"));

    let csv0 = fs::read_to_string(dir.join("dump-session0.csv")).unwrap();
    let csv1 = fs::read_to_string(dir.join("dump-session1.csv")).unwrap();
    assert!(csv0.lines().nth(1).unwrap().starts_with("60000,3,"));
    assert!(csv1.lines().nth(1).unwrap().starts_with("1000,7,"));
}

#[test]
fn a_corrupted_record_ends_its_session_without_losing_earlier_ones() {
    let dir = tempdir();
    let mut data = Vec::new();
    data.extend_from_slice(&encode_record(&entry(1000, 3)));
    data.extend_from_slice(&encode_record(&entry(1020, 3)));
    let mut corrupted = encode_record(&entry(1040, 3));
    corrupted[RECORD_SIZE - 1] ^= 0xFF; // flip the checksum byte
    data.extend_from_slice(&corrupted);
    pad_to_sector(&mut data);
    let dump_path = dir.join("dump.bin");
    fs::write(&dump_path, &data).unwrap();

    let stdout = run_log_decode(&dump_path, &dir);
    assert!(stdout.contains("session 0: 2 records"));
    assert!(!stdout.contains("session 1"));
}

#[test]
fn an_empty_dump_reports_no_sessions_and_writes_nothing() {
    let dir = tempdir();
    let dump_path = dir.join("dump.bin");
    fs::write(&dump_path, vec![0xFFu8; SECTOR_SIZE as usize]).unwrap();

    let stdout = run_log_decode(&dump_path, &dir);
    assert!(stdout.contains("no valid records found"));
    assert!(!dir.join("dump-session0.csv").exists());
}

/// Bare-bones unique-directory helper -- this crate has no dev-dependency
/// on `tempfile`, and one std::env::temp_dir() + pid + counter is plenty
/// for tests that never run concurrently against the same path.
fn tempdir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("log-decode-test-{}-{}", std::process::id(), n));
    fs::create_dir_all(&dir).unwrap();
    dir
}
