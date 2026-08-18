use launchcast_ground_logic::{checksum, parse_rmc, NmeaLineReader};

// The textbook example sentence used across NMEA 0183 references
// (including Wikipedia's NMEA 0183 article) -- checksum 6A is a known
// value for this exact line, not computed by this test.
const EXAMPLE_RMC: &str = "$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A";

#[test]
fn parses_the_textbook_example() {
    let fix = parse_rmc(EXAMPLE_RMC).expect("valid sentence should parse");
    assert!(fix.valid);
    assert!((fix.lat - 48.1173).abs() < 0.001); // 48 + 07.038/60
    assert!((fix.lon - 11.516_67).abs() < 0.001); // 011 + 31.000/60
    assert!((fix.speed_knots - 22.4).abs() < 0.001);
    assert_eq!(fix.track_deg, Some(84.4));
}

#[test]
fn south_and_west_are_negative() {
    let sentence = "$GPRMC,123519,A,4807.038,S,01131.000,W,022.4,084.4,230394,003.1,W*65";
    let fix = parse_rmc(sentence).expect("valid sentence should parse");
    assert!(fix.lat < 0.0);
    assert!(fix.lon < 0.0);
}

#[test]
fn status_v_is_not_valid() {
    let sentence = "$GPRMC,123519,V,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*7D";
    let fix = parse_rmc(sentence).expect("still parses -- just marked invalid");
    assert!(!fix.valid);
}

#[test]
fn bad_checksum_is_rejected() {
    let sentence = "$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*00";
    assert_eq!(parse_rmc(sentence), None);
}

#[test]
fn non_rmc_sentences_are_ignored() {
    let gga = "$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47";
    assert_eq!(parse_rmc(gga), None);
}

#[test]
fn empty_track_angle_is_none() {
    let sentence = "$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,,230394,003.1,W*4C";
    let fix = parse_rmc(sentence).expect("valid sentence should parse");
    assert_eq!(fix.track_deg, None);
}

#[test]
fn checksum_matches_the_textbook_example() {
    // "$PMTK220,100*2F" -- a known-good PMTK command/checksum pair
    // (also used as a reference value in the adafruit_gps Rust crate's
    // own test suite).
    assert_eq!(checksum("PMTK220,100"), 0x2F);
}

#[test]
fn checksum_matches_the_outgoing_gps_init_commands() {
    // Hand-computed (Python's `functools.reduce(xor)`), for the two
    // commands ground/src/gps.rs actually sends at boot.
    assert_eq!(checksum("PMTK313,1"), 0x2E);
    assert_eq!(checksum("PMTK301,2"), 0x2E);
}

#[test]
fn line_reader_assembles_a_sentence_split_across_feeds() {
    let mut reader: NmeaLineReader<128> = NmeaLineReader::new();
    for &byte in EXAMPLE_RMC.as_bytes() {
        assert_eq!(reader.feed(byte), None);
    }
    assert_eq!(reader.feed(b'\r'), None);
    let line = reader.feed(b'\n').expect("CRLF should terminate the sentence");
    assert_eq!(line.as_str(), EXAMPLE_RMC);
}

#[test]
fn line_reader_drops_filler_linefeeds_not_preceded_by_cr() {
    // The PA1010D pads idle I2C reads with bare 0x0A bytes -- these must
    // never be mistaken for real line terminators (which only ever
    // follow 0x0D), and must not corrupt the sentence being assembled.
    let mut reader: NmeaLineReader<128> = NmeaLineReader::new();
    for _ in 0..5 {
        assert_eq!(reader.feed(0x0A), None); // filler, before any real data
    }
    for &byte in EXAMPLE_RMC.as_bytes() {
        assert_eq!(reader.feed(byte), None);
    }
    for _ in 0..3 {
        assert_eq!(reader.feed(0x0A), None); // filler, mid-sentence-adjacent
    }
    assert_eq!(reader.feed(b'\r'), None);
    let line = reader.feed(b'\n').expect("CRLF should still terminate correctly");
    assert_eq!(line.as_str(), EXAMPLE_RMC);
}

#[test]
fn line_reader_handles_back_to_back_sentences() {
    let mut reader: NmeaLineReader<128> = NmeaLineReader::new();
    let mut lines: heapless::Vec<heapless::String<128>, 4> = heapless::Vec::new();
    let stream = alloc_two_sentences();
    for &byte in stream.as_bytes() {
        if let Some(line) = reader.feed(byte) {
            lines.push(line).unwrap();
        }
    }
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].as_str(), EXAMPLE_RMC);
    assert_eq!(lines[1].as_str(), EXAMPLE_RMC);
}

fn alloc_two_sentences() -> std::string::String {
    std::format!("{EXAMPLE_RMC}\r\n{EXAMPLE_RMC}\r\n")
}
