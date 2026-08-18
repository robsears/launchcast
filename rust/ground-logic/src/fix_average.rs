//! Incremental mean of a stream of GPS lat/lon samples. Hardware-free --
//! the windowing/timing decision of *when* to snapshot-and-reset lives in
//! `ground/src/gps.rs`, which has a real clock; this is just the running
//! sum.
//!
//! A plain arithmetic mean of degrees, not a proper geographic centroid
//! (great-circle midpoint, etc.) -- deliberately: this only ever averages
//! samples a few meters apart (a stationary GPS's own noise spread), and
//! at that scale degrees behave linearly enough that the distinction is
//! well below the GPS's own noise floor (same reasoning as this
//! codebase's choice to keep `haversine_m` over a flat-earth
//! approximation for *distance* -- curvature/projection effects that
//! matter at km scale are irrelevant at meter scale, just from the
//! opposite direction: here it means the simpler math is already exact
//! enough, not that the fancier math would be wasted).

#[derive(Debug, Clone, Copy, Default)]
pub struct FixAverage {
    sum_lat: f32,
    sum_lon: f32,
    count: u32,
}

impl FixAverage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, lat: f32, lon: f32) {
        self.sum_lat += lat;
        self.sum_lon += lon;
        self.count += 1;
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    /// Mean lat/lon of everything added since the last `reset` (or
    /// construction). `None` if nothing has been added yet.
    pub fn mean(&self) -> Option<(f32, f32)> {
        if self.count == 0 {
            return None;
        }
        Some((self.sum_lat / self.count as f32, self.sum_lon / self.count as f32))
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
