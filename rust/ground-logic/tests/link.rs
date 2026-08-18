use launchcast_ground_logic::{
    link_status, telemetry_missing, LinkStatus, LINK_LOST_MS, LINK_STALE_MS, TELEMETRY_MISSING_MS,
};

#[test]
fn no_packet_ever_is_waiting() {
    assert_eq!(link_status(None), LinkStatus::Waiting);
}

#[test]
fn fresh_packet_is_live() {
    assert_eq!(link_status(Some(0)), LinkStatus::Live);
    assert_eq!(link_status(Some(LINK_STALE_MS)), LinkStatus::Live);
}

#[test]
fn stale_boundary() {
    assert_eq!(link_status(Some(LINK_STALE_MS + 1)), LinkStatus::Stale);
    assert_eq!(link_status(Some(LINK_LOST_MS)), LinkStatus::Stale);
}

#[test]
fn lost_boundary() {
    assert_eq!(link_status(Some(LINK_LOST_MS + 1)), LinkStatus::Lost);
    assert_eq!(link_status(Some(u32::MAX)), LinkStatus::Lost);
}

#[test]
fn names_match_python_strings() {
    assert_eq!(LinkStatus::Waiting.name(), "WAITING");
    assert_eq!(LinkStatus::Live.name(), "LIVE");
    assert_eq!(LinkStatus::Stale.name(), "STALE");
    assert_eq!(LinkStatus::Lost.name(), "LOST");
}

#[test]
fn telemetry_missing_when_nothing_ever_arrived() {
    assert!(telemetry_missing(None));
}

#[test]
fn telemetry_missing_boundary() {
    assert!(!telemetry_missing(Some(TELEMETRY_MISSING_MS - 1)));
    assert!(telemetry_missing(Some(TELEMETRY_MISSING_MS)));
    assert!(telemetry_missing(Some(TELEMETRY_MISSING_MS + 1)));
}

#[test]
fn telemetry_missing_is_much_coarser_than_link_lost() {
    // A LOST link (per link_status) isn't automatically "missing" --
    // MISSING is a much longer, distinct threshold.
    assert_eq!(link_status(Some(LINK_LOST_MS + 1)), LinkStatus::Lost);
    assert!(!telemetry_missing(Some(LINK_LOST_MS + 1)));
}
