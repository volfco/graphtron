/// Easing functions for smooth animations. All take `t` in [0, 1] and return [0, 1].
///
/// Decelerate smoothly — fast start, gentle stop. Default for zoom transitions.
pub fn ease_out_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Accelerate then decelerate — smooth start and stop. Good for data morphing.
pub fn ease_in_out_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Exponential decelerate — very fast start, long tail. Dramatic reveal effect.
pub fn ease_out_expo(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    if t >= 1.0 {
        1.0
    } else {
        1.0 - 2.0_f64.powf(-10.0 * t)
    }
}

/// Linear — no easing. Useful as a baseline or for constant-speed panning.
pub fn linear(t: f64) -> f64 {
    t.clamp(0.0, 1.0)
}

/// Snap to target instantly after a short delay. Returns 0.0 until `delay`,
/// then jumps to 1.0. Useful for debounced hover effects.
pub fn snap_after(t: f64, delay: f64) -> f64 {
    if t >= delay { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_out_cubic_boundary() {
        assert!((ease_out_cubic(0.0) - 0.0).abs() < 1e-9);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ease_in_out_cubic_midpoint() {
        let mid = ease_in_out_cubic(0.5);
        assert!((mid - 0.5).abs() < 1e-9);
    }

    #[test]
    fn ease_out_expo_approaches_one() {
        assert!(ease_out_expo(0.99) > 0.99);
        assert!((ease_out_expo(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn linear_passthrough() {
        assert!((linear(0.3) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn clamp_out_of_range() {
        assert!((ease_out_cubic(-0.5) - 0.0).abs() < 1e-9);
        assert!((ease_out_cubic(1.5) - 1.0).abs() < 1e-9);
    }
}
