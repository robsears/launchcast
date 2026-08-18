use launchcast_ground_logic::{link_status, LinkStatus, LINK_LOST_MS, LINK_STALE_MS};

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
