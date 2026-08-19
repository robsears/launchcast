use launchcast_rocket_logic::flash_log::{
    align_up_to_sector, decode_record, encode_record, LogEntry, RECORD_SIZE, SECTOR_SIZE,
};

fn sample_entry() -> LogEntry {
    LogEntry {
        t_ms: 123_456,
        state: 4,
        alt_m: 305.2,
        vel_mps: -12.5,
        pressure_hpa: 987.3,
        temp_c: 21.4,
        accel_mps2: [0.1, -9.8, 0.3],
        gyro_dps: [1.0, -2.0, 3.5],
    }
}

#[test]
fn round_trips_through_encode_decode() {
    let entry = sample_entry();
    let buf = encode_record(&entry);
    let decoded = decode_record(&buf).expect("a freshly encoded record should decode");
    assert_eq!(decoded, entry);
}

#[test]
fn record_size_matches_pythons_log_size_plus_framing() {
    // rocket/code.py's LOG_SIZE (payload only) is 45 bytes; this format
    // wraps that in magic+version+checksum (3 extra bytes).
    assert_eq!(RECORD_SIZE, 48);
}

#[test]
fn erased_flash_does_not_decode() {
    // A blank (erased) flash region reads back as all 0xFF.
    let buf = [0xFFu8; RECORD_SIZE];
    assert_eq!(decode_record(&buf), None);
}

#[test]
fn all_zero_bytes_do_not_decode() {
    let buf = [0u8; RECORD_SIZE];
    assert_eq!(decode_record(&buf), None);
}

#[test]
fn a_single_corrupted_byte_is_detected() {
    let entry = sample_entry();
    let mut buf = encode_record(&entry);
    buf[20] ^= 0xFF; // flip a byte in the middle of the payload
    assert_eq!(decode_record(&buf), None);
}

#[test]
fn corrupted_checksum_byte_itself_is_detected() {
    let entry = sample_entry();
    let mut buf = encode_record(&entry);
    let last = RECORD_SIZE - 1;
    buf[last] ^= 0x01;
    assert_eq!(decode_record(&buf), None);
}

#[test]
fn align_up_leaves_aligned_offsets_unchanged() {
    assert_eq!(align_up_to_sector(0), 0);
    assert_eq!(align_up_to_sector(SECTOR_SIZE), SECTOR_SIZE);
    assert_eq!(align_up_to_sector(SECTOR_SIZE * 3), SECTOR_SIZE * 3);
}

#[test]
fn align_up_rounds_forward_to_the_next_boundary() {
    assert_eq!(align_up_to_sector(1), SECTOR_SIZE);
    assert_eq!(align_up_to_sector(SECTOR_SIZE - 1), SECTOR_SIZE);
    assert_eq!(align_up_to_sector(SECTOR_SIZE + 1), SECTOR_SIZE * 2);
}
