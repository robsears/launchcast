use launchcast_ground_logic::FixAverage;

#[test]
fn empty_average_is_none() {
    let avg = FixAverage::new();
    assert_eq!(avg.mean(), None);
    assert_eq!(avg.count(), 0);
}

#[test]
fn single_sample_is_itself() {
    let mut avg = FixAverage::new();
    avg.add(42.5, -71.25);
    assert_eq!(avg.mean(), Some((42.5, -71.25)));
    assert_eq!(avg.count(), 1);
}

#[test]
fn averages_multiple_samples() {
    let mut avg = FixAverage::new();
    avg.add(42.0, -71.0);
    avg.add(42.0002, -71.0002);
    avg.add(41.9998, -70.9998);
    let (lat, lon) = avg.mean().expect("three samples should produce a mean");
    assert!((lat - 42.0).abs() < 1e-6);
    assert!((lon - (-71.0)).abs() < 1e-6);
    assert_eq!(avg.count(), 3);
}

#[test]
fn reset_clears_accumulated_samples() {
    let mut avg = FixAverage::new();
    avg.add(1.0, 2.0);
    avg.add(3.0, 4.0);
    avg.reset();
    assert_eq!(avg.mean(), None);
    assert_eq!(avg.count(), 0);

    avg.add(10.0, 20.0);
    assert_eq!(avg.mean(), Some((10.0, 20.0)));
}
