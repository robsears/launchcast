use launchcast_common::epoch::EpochOffset;
use launchcast_common::nmea::UtcDateTime;

fn dt() -> UtcDateTime {
    UtcDateTime { year: 2026, month: 8, day: 19, hour: 17, minute: 30, second: 0, millis: 0 }
}

#[test]
fn wall_clock_at_capture_moment_equals_the_fix() {
    let offset = EpochOffset::capture(&dt(), 12_345);
    assert_eq!(offset.wall_clock_ms(12_345), 1_787_160_600_000);
}

#[test]
fn wall_clock_advances_with_the_monotonic_clock() {
    let offset = EpochOffset::capture(&dt(), 12_345);
    assert_eq!(offset.wall_clock_ms(12_345 + 5_000), 1_787_160_600_000 + 5_000);
}

#[test]
fn capturing_at_zero_still_works() {
    // The common real case: captured shortly after boot, while now_ms
    // is still small.
    let offset = EpochOffset::capture(&dt(), 0);
    assert_eq!(offset.wall_clock_ms(0), 1_787_160_600_000);
    assert_eq!(offset.wall_clock_ms(1_000), 1_787_160_601_000);
}
