use launchcast_common::fix_average::{FixAverage, WINDOW_SAMPLES};

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

#[test]
fn count_saturates_at_window_size() {
    let mut avg = FixAverage::new();
    for i in 0..WINDOW_SAMPLES * 2 {
        avg.add(i as f32, 0.0);
    }
    assert_eq!(avg.count() as usize, WINDOW_SAMPLES);
}

#[test]
fn oldest_sample_is_evicted_once_the_window_is_full() {
    // Fill the buffer with a stable reading, then push one wildly
    // different sample -- the mean should shift by only 1/WINDOW_SAMPLES
    // of the way to it, not track it directly (proves eviction is
    // happening one-at-a-time, not batched/reset).
    let mut avg = FixAverage::new();
    for _ in 0..WINDOW_SAMPLES {
        avg.add(0.0, 0.0);
    }
    avg.add(WINDOW_SAMPLES as f32, 0.0);
    let (lat, _lon) = avg.mean().unwrap();
    assert!((lat - 1.0).abs() < 1e-6, "expected mean shifted by exactly one sample's worth, got {lat}");
    assert_eq!(avg.count() as usize, WINDOW_SAMPLES);
}

#[test]
fn a_long_stale_run_is_fully_evicted_after_window_size_fresh_samples() {
    // Regression check for the real-world symptom that motivated this
    // rewrite: old, wrong samples must not linger indefinitely (or for
    // several minutes) once fresh, correct ones start arriving.
    let mut avg = FixAverage::new();
    for _ in 0..1000 {
        avg.add(999.0, 999.0); // simulates a long stale/cold-start run
    }
    for _ in 0..WINDOW_SAMPLES {
        avg.add(1.0, 2.0);
    }
    assert_eq!(avg.mean(), Some((1.0, 2.0)));
}
