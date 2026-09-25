//! Animation state machine for smooth transitions.
//!
//! Consumers drive animations via `requestAnimationFrame` (web) or their
//! platform's equivalent. The `Animation` struct interpolates between two
//! values over a duration with an easing function.

use crate::easing;

/// Easing function pointer type.
pub type EasingFn = fn(f64) -> f64;

/// A linear interpolation between two values over a time window.
#[derive(Debug, Clone, Copy)]
pub struct Animation {
    /// Start value (e.g. previous time range endpoint).
    pub from: f64,
    /// End value (e.g. new time range endpoint).
    pub to: f64,
    /// Absolute start time in ms (e.g. `performance.now()`).
    pub start_ms: f64,
    /// Duration in ms.
    pub duration_ms: f64,
    /// Easing function applied to the normalized progress.
    pub easing: EasingFn,
}

impl PartialEq for Animation {
    fn eq(&self, other: &Self) -> bool {
        self.from == other.from
            && self.to == other.to
            && self.start_ms == other.start_ms
            && self.duration_ms == other.duration_ms
            && [0.0, 0.25, 0.5, 0.75, 1.0]
                .iter()
                .all(|t| (self.easing)(*t) == (other.easing)(*t))
    }
}

impl Animation {
    /// Create a new animation with the default easing (`ease_out_cubic`).
    pub fn new(from: f64, to: f64, start_ms: f64, duration_ms: f64) -> Self {
        Self {
            from,
            to,
            start_ms,
            duration_ms: valid_duration(duration_ms),
            easing: easing::ease_out_cubic,
        }
    }

    /// Create with a custom easing function.
    pub fn with_easing(
        from: f64,
        to: f64,
        start_ms: f64,
        duration_ms: f64,
        easing: EasingFn,
    ) -> Self {
        Self {
            from,
            to,
            start_ms,
            duration_ms: valid_duration(duration_ms),
            easing,
        }
    }

    /// Interpolated value at the given time. Returns `from` before start,
    /// `to` after end, and the eased interpolation in between.
    pub fn value_at(&self, now_ms: f64) -> f64 {
        if !self.duration_ms.is_finite() || self.duration_ms <= 0.0 {
            return if now_ms < self.start_ms {
                self.from
            } else {
                self.to
            };
        }
        if now_ms < self.start_ms {
            return self.from;
        }
        let t = ((now_ms - self.start_ms) / self.duration_ms).clamp(0.0, 1.0);
        let eased = (self.easing)(t);
        self.from + (self.to - self.from) * eased
    }

    /// Whether the animation has completed.
    pub fn is_done(&self, now_ms: f64) -> bool {
        if !self.duration_ms.is_finite() || self.duration_ms <= 0.0 {
            now_ms >= self.start_ms
        } else {
            now_ms >= self.start_ms + self.duration_ms
        }
    }

    /// Progress in [0, 1] (before start = 0, after end = 1).
    pub fn progress(&self, now_ms: f64) -> f64 {
        if !self.duration_ms.is_finite() || self.duration_ms <= 0.0 {
            return if now_ms < self.start_ms { 0.0 } else { 1.0 };
        }
        ((now_ms - self.start_ms) / self.duration_ms).clamp(0.0, 1.0)
    }
}

fn valid_duration(duration_ms: f64) -> f64 {
    if duration_ms.is_finite() {
        duration_ms.max(0.0)
    } else {
        0.0
    }
}

/// Animate both endpoints of a range from a stable start toward a target.
///
/// `start` must be the range captured when the transition begins, not the
/// result from the previous frame. This keeps a zoom's left and right edges on
/// the same easing curve, so its span evolves continuously instead of holding
/// the old width until the final frame.
pub fn animate_range(
    start: (f64, f64),
    target: (f64, f64),
    now_ms: f64,
    anim: &Option<Animation>,
) -> (f64, f64) {
    match anim {
        Some(a) => {
            let eased = (a.easing)(a.progress(now_ms));
            (
                start.0 + (target.0 - start.0) * eased,
                start.1 + (target.1 - start.1) * eased,
            )
        }
        None => target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_interpolates() {
        let a = Animation::new(0.0, 100.0, 0.0, 1000.0);
        // At start
        assert!((a.value_at(0.0) - 0.0).abs() < 1e-9);
        // At end
        assert!((a.value_at(1000.0) - 100.0).abs() < 1e-9);
        // Midpoint with ease_out_cubic should be > 50 (front-loaded)
        let mid = a.value_at(500.0);
        assert!(mid > 50.0 && mid < 100.0);
    }

    #[test]
    fn non_positive_duration_snapshots_to_the_target() {
        let animation = Animation::new(1.0, 2.0, 10.0, 0.0);
        assert_eq!(animation.duration_ms, 0.0);
        assert_eq!(animation.value_at(10.0), 2.0);
        assert!(animation.is_done(10.0));
        assert_eq!(animation.progress(10.0), 1.0);
        assert!(Animation::new(1.0, 2.0, 0.0, -1.0).is_done(0.0));
    }

    #[test]
    fn easing_equality_compares_more_than_the_midpoint() {
        fn quarter(t: f64) -> f64 {
            t * t
        }
        fn different_midpoint(t: f64) -> f64 {
            if t == 0.5 { 0.5 } else { quarter(t) }
        }
        let a = Animation::with_easing(0.0, 1.0, 0.0, 1.0, quarter);
        let b = Animation::with_easing(0.0, 1.0, 0.0, 1.0, different_midpoint);
        assert_ne!(a, b);
    }

    #[test]
    fn animation_done() {
        let a = Animation::new(0.0, 100.0, 100.0, 500.0);
        assert!(!a.is_done(400.0));
        assert!(a.is_done(600.0));
    }

    #[test]
    fn animation_before_start_returns_from() {
        let a = Animation::new(50.0, 150.0, 1000.0, 500.0);
        assert!((a.value_at(0.0) - 50.0).abs() < 1e-9);
        assert!((a.value_at(999.0) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn animate_range_without_animation_returns_target() {
        let result = animate_range((0.0, 100.0), (50.0, 150.0), 0.0, &None);
        assert_eq!(result, (50.0, 150.0));
    }

    #[test]
    fn animate_range_interpolates_both_endpoints_from_stable_start() {
        let animation = Some(Animation::with_easing(
            0.0,
            50.0,
            0.0,
            1_000.0,
            easing::linear,
        ));
        assert_eq!(
            animate_range((0.0, 100.0), (50.0, 200.0), 500.0, &animation),
            (25.0, 150.0)
        );
    }
}
