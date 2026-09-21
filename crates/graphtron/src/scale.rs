use serde::{Deserialize, Serialize};

/// How a value axis maps its domain onto pixels.
///
/// A log axis is the difference between a latency panel that shows p50 and
/// p999 at once and one where p50 is a flat line on the floor. Only strictly
/// positive values are representable: non-positive values become gaps, the
/// same treatment `NaN` already gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleKind {
    #[default]
    Linear,
    /// Base-10 logarithmic.
    Log,
}

/// Smallest positive value a log axis will represent. Guards `log10(0)`.
pub const LOG_EPSILON: f64 = 1e-12;

impl ScaleKind {
    /// Data value -> transformed (scale) space. `NaN` for values a log axis
    /// cannot represent, so paths break at them instead of exploding.
    pub fn forward(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Log => {
                if value > 0.0 {
                    value.log10()
                } else {
                    f64::NAN
                }
            }
        }
    }

    /// Transformed space -> data value.
    pub fn inverse(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Log => 10f64.powf(value),
        }
    }
}

/// Linear domain↔pixel mapping. Time axes use it with epoch-millisecond
/// domains (f64 is exact to 2^53, comfortably beyond any timestamp).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearScale {
    pub d0: f64,
    pub d1: f64,
    pub r0: f64,
    pub r1: f64,
}

impl LinearScale {
    pub fn new(d0: f64, d1: f64, r0: f64, r1: f64) -> Self {
        Self { d0, d1, r0, r1 }
    }

    pub fn to_px(&self, v: f64) -> f64 {
        if self.d1 == self.d0 {
            return (self.r0 + self.r1) / 2.0;
        }
        self.r0 + (v - self.d0) / (self.d1 - self.d0) * (self.r1 - self.r0)
    }

    pub fn from_px(&self, px: f64) -> f64 {
        if self.r1 == self.r0 {
            return self.d0;
        }
        self.d0 + (px - self.r0) / (self.r1 - self.r0) * (self.d1 - self.d0)
    }

    /// Expand the domain to the enclosing "nice" tick boundaries.
    pub fn nice(&mut self, max_ticks: usize) {
        let ticks = crate::ticks::linear_ticks(self.d0, self.d1, max_ticks);
        if let (Some(first), Some(last)) = (ticks.first(), ticks.last()) {
            if *first < self.d0 {
                self.d0 = *first;
            }
            if *last > self.d1 {
                self.d1 = *last;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = LinearScale::new(100.0, 200.0, 0.0, 500.0);
        assert_eq!(s.to_px(100.0), 0.0);
        assert_eq!(s.to_px(200.0), 500.0);
        assert_eq!(s.to_px(150.0), 250.0);
        assert!((s.from_px(s.to_px(133.0)) - 133.0).abs() < 1e-9);
    }

    #[test]
    fn inverted_range_for_y_axis() {
        // Canvas y grows downward: range is (height, 0).
        let s = LinearScale::new(0.0, 10.0, 100.0, 0.0);
        assert_eq!(s.to_px(0.0), 100.0);
        assert_eq!(s.to_px(10.0), 0.0);
    }

    #[test]
    fn log_scale_round_trips_and_rejects_non_positive() {
        let kind = ScaleKind::Log;
        assert!((kind.inverse(kind.forward(250.0)) - 250.0).abs() < 1e-9);
        assert!(kind.forward(0.0).is_nan());
        assert!(kind.forward(-1.0).is_nan());
        assert_eq!(ScaleKind::Linear.forward(-1.0), -1.0);
    }

    #[test]
    fn degenerate_domain_is_safe() {
        let s = LinearScale::new(5.0, 5.0, 0.0, 100.0);
        assert_eq!(s.to_px(5.0), 50.0);
    }
}
