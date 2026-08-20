use launchcast_common::nmea::{checksum, from_unix_ms, parse_rmc, unix_ms, NmeaLineReader, UtcDateTime};

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
fn textbook_example_date_and_time_fields_parse_correctly() {
    // Time field 123519 -> 12:35:19; date field 230394 -> day 23, month
    // 03. The textbook example is a real 1994 sentence, but this parser
    // deliberately always assumes `2000 + yy` for the 2-digit year (this
    // hardware will never see a real pre-2000 fix) -- so this sentence's
    // *year* decodes to 2094, not the sentence's real original 1994.
    // That's expected, not a bug; `unix_ms_matches_known_reference_
    // points` below verifies the actual calendar math against
    // unambiguous modern dates instead.
    let fix = parse_rmc(EXAMPLE_RMC).expect("valid sentence should parse");
    let utc = fix.utc.expect("time and date fields are both present");
    assert_eq!(
        utc,
        UtcDateTime { year: 2094, month: 3, day: 23, hour: 12, minute: 35, second: 19, millis: 0 }
    );
}

#[test]
fn unix_ms_matches_known_reference_points() {
    // 2000-01-01T00:00:00Z (946684800s) and 2026-08-19T17:30:00Z, hand-
    // verified against days-since-epoch computed independently of
    // days_from_civil's own implementation.
    let y2k = UtcDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0, second: 0, millis: 0 };
    assert_eq!(unix_ms(&y2k), 946_684_800_000);

    let recent = UtcDateTime { year: 2026, month: 8, day: 19, hour: 17, minute: 30, second: 0, millis: 0 };
    assert_eq!(unix_ms(&recent), 1_787_160_600_000);
}

#[test]
fn from_unix_ms_matches_known_reference_points() {
    // Inverse of unix_ms_matches_known_reference_points -- same two
    // reference instants, going the other direction.
    let y2k = UtcDateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0, second: 0, millis: 0 };
    assert_eq!(from_unix_ms(946_684_800_000), y2k);

    let recent = UtcDateTime { year: 2026, month: 8, day: 19, hour: 17, minute: 30, second: 0, millis: 0 };
    assert_eq!(from_unix_ms(1_787_160_600_000), recent);
}

#[test]
fn unix_ms_round_trips_through_from_unix_ms() {
    let dt = UtcDateTime { year: 2026, month: 12, day: 31, hour: 23, minute: 59, second: 58, millis: 123 };
    assert_eq!(from_unix_ms(unix_ms(&dt)), dt);
}

#[test]
fn fractional_seconds_are_parsed_as_milliseconds() {
    let sentence = "$GPRMC,123519.500,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*71";
    let fix = parse_rmc(sentence).expect("valid sentence should parse");
    let utc = fix.utc.expect("time and date fields are both present");
    assert_eq!(utc.second, 19);
    assert_eq!(utc.millis, 500);
}

#[test]
fn missing_time_field_leaves_utc_none() {
    // A receiver that hasn't decoded the time yet leaves the field
    // blank -- must not be treated as "midnight," just unknown.
    let sentence = "$GPRMC,,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*67";
    let fix = parse_rmc(sentence).expect("still parses -- utc just isn't available");
    assert_eq!(fix.utc, None);
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
