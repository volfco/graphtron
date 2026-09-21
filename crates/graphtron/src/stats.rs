//! Descriptive statistics and trend fitting over series values.
//!
//! Dense telemetry panels are read for *shape*, and shape questions ("is this
//! drifting up?", "where does p95 sit?", "is that spike outside two sigma?")
//! are answered by derived geometry, not by squinting at the raw stroke. This
//! module supplies the numbers; [`crate::draw`] paints them as reference lines
//! and trend overlays driven by [`crate::spec::ChartSpec`].
//!
//! Everything here is pure computation over `f64` slices and ignores
//! non-finite values, which the rest of the crate already treats as gaps.

/// Summary statistics over the finite values of a sample.
///
/// Built with Welford's algorithm, so a long series of large values keeps its
/// precision instead of losing it to catastrophic cancellation in a
/// sum-of-squares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    /// Number of finite values that contributed.
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub sum: f64,
    /// Population standard deviation (`0.0` for a single value).
    pub stddev: f64,
}

impl Summary {
    /// Summarize the finite values of an iterator, or `None` when it holds
    /// none. Callers that pass entirely non-finite data get no statistics
    /// rather than a fabricated zero.
    pub fn of(values: impl IntoIterator<Item = f64>) -> Option<Self> {
        let mut count = 0usize;
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let mut mean = 0.0f64;
        let mut m2 = 0.0f64;
        let mut sum = 0.0f64;
        for value in values {
            if !value.is_finite() {
                continue;
            }
            count += 1;
            sum += value;
            min = min.min(value);
            max = max.max(value);
            let delta = value - mean;
            mean += delta / count as f64;
            m2 += delta * (value - mean);
        }
        if count == 0 {
            return None;
        }
        Some(Self {
            count,
            min,
            max,
            mean,
            sum,
            stddev: (m2 / count as f64).sqrt(),
        })
    }

    /// Summarize a slice (the common case).
    pub fn of_slice(values: &[f64]) -> Option<Self> {
        Self::of(values.iter().copied())
    }
}

/// Finite values of a slice, sorted ascending — the input every quantile
/// query needs. Returns an empty vector when nothing is finite.
pub fn sorted_finite(values: &[f64]) -> Vec<f64> {
    let mut out: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Linearly interpolated quantile of an **already sorted** finite slice
/// (the R-7 / NumPy default definition). `q` is clamped to `0..=1`.
pub fn quantile_sorted(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() || !q.is_finite() {
        return None;
    }
    let q = q.clamp(0.0, 1.0);
    let position = q * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return Some(sorted[lower]);
    }
    let weight = position - lower as f64;
    Some(sorted[lower] * (1.0 - weight) + sorted[upper] * weight)
}

/// Quantile of an unsorted slice. Sorts a filtered copy; prefer
/// [`sorted_finite`] plus [`quantile_sorted`] when asking for several
/// quantiles of the same sample.
pub fn quantile(values: &[f64], q: f64) -> Option<f64> {
    quantile_sorted(&sorted_finite(values), q)
}

/// Median (the 0.5 quantile) of the finite values.
pub fn median(values: &[f64]) -> Option<f64> {
    quantile(values, 0.5)
}

/// Centered simple moving average with the given odd-ish `window`.
///
/// The output is the same length as the input and preserves every non-finite
/// source position, so gaps stay gaps instead of being bridged by a smoothed
/// line. Windows below 2 return the input unchanged.
pub fn simple_moving_average(values: &[f64], window: usize) -> Vec<f64> {
    if window < 2 || values.is_empty() {
        return values.to_vec();
    }
    let half = window / 2;
    let mut out = Vec::with_capacity(values.len());
    // Sliding running sum rather than a re-scan per index: a million-point
    // panel with a wide window would otherwise be O(n × window) per frame.
    // Incremental addition and subtraction drifts by roughly n × eps of the
    // running magnitude — around 1e-10 relative at a million points, far
    // below one pixel of a drawn curve.
    let mut sum = 0.0f64;
    let mut count = 0usize;
    let mut start = 0usize;
    let mut end = 0usize;
    for index in 0..values.len() {
        let want_start = index.saturating_sub(half);
        let want_end = (index + half + 1).min(values.len());
        while end < want_end {
            if values[end].is_finite() {
                sum += values[end];
                count += 1;
            }
            end += 1;
        }
        while start < want_start {
            if values[start].is_finite() {
                sum -= values[start];
                count -= 1;
            }
            start += 1;
        }
        // A centered window may contain finite neighbors on both sides of a
        // missing observation, but smoothing must not invent a sample at the
        // missing timestamp. Preserve the source gap while retaining the
        // normal centered average at adjacent points.
        out.push(if !values[index].is_finite() || count == 0 {
            f64::NAN
        } else {
            sum / count as f64
        });
    }
    out
}

/// Exponentially weighted moving average with smoothing factor `alpha`
/// (0 < alpha <= 1; larger reacts faster). Non-finite samples do not update
/// the accumulator and are emitted as gaps, and leading gaps stay gaps until
/// the first finite sample seeds the average.
pub fn exponential_moving_average(values: &[f64], alpha: f64) -> Vec<f64> {
    let alpha = if alpha.is_finite() {
        alpha.clamp(f64::EPSILON, 1.0)
    } else {
        return values.to_vec();
    };
    let mut out = Vec::with_capacity(values.len());
    let mut accumulator: Option<f64> = None;
    for value in values {
        if !value.is_finite() {
            out.push(f64::NAN);
            continue;
        }
        let next = match accumulator {
            None => *value,
            Some(previous) => alpha * value + (1.0 - alpha) * previous,
        };
        accumulator = Some(next);
        out.push(next);
    }
    out
}

/// A least-squares straight line through a sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearFit {
    pub slope: f64,
    pub intercept: f64,
    /// Coefficient of determination in `0..=1`; `1.0` for a perfect fit and
    /// `0.0` when the sample has no y variance to explain.
    pub r2: f64,
    /// Number of finite (x, y) pairs the fit used.
    pub count: usize,
}

impl LinearFit {
    /// Fitted y at `x`.
    pub fn at(&self, x: f64) -> f64 {
        self.slope * x + self.intercept
    }
}

/// Ordinary least-squares fit of `ys` on `xs`, using only pairs where both
/// are finite.
///
/// Sums are accumulated about the sample means rather than about zero. That
/// matters here specifically: x is epoch milliseconds (~1.7e12), so a naive
/// `Σx²` lands near 3e24 and the normal equations lose most of their
/// significant digits before they produce a slope.
///
/// Returns `None` for fewer than two usable pairs or a degenerate (single-x)
/// sample, where no line is determined.
pub fn linear_fit(xs: &[f64], ys: &[f64]) -> Option<LinearFit> {
    let n = xs.len().min(ys.len());
    let mut count = 0usize;
    let mut mean_x = 0.0f64;
    let mut mean_y = 0.0f64;
    for index in 0..n {
        let (x, y) = (xs[index], ys[index]);
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        count += 1;
        mean_x += (x - mean_x) / count as f64;
        mean_y += (y - mean_y) / count as f64;
    }
    if count < 2 {
        return None;
    }
    let mut sxx = 0.0f64;
    let mut sxy = 0.0f64;
    let mut syy = 0.0f64;
    for index in 0..n {
        let (x, y) = (xs[index], ys[index]);
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        let dx = x - mean_x;
        let dy = y - mean_y;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    if sxx <= 0.0 {
        return None;
    }
    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;
    let r2 = if syy > 0.0 {
        (sxy * sxy / (sxx * syy)).clamp(0.0, 1.0)
    } else {
        // A flat sample is explained perfectly by a flat line.
        1.0
    };
    Some(LinearFit {
        slope,
        intercept,
        r2,
        count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn summary_ignores_non_finite_and_reports_population_sigma() {
        let s = Summary::of_slice(&[2.0, f64::NAN, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).unwrap();
        assert_eq!(s.count, 8);
        approx(s.mean, 5.0);
        approx(s.stddev, 2.0);
        approx(s.min, 2.0);
        approx(s.max, 9.0);
        approx(s.sum, 40.0);
    }

    #[test]
    fn summary_of_nothing_finite_is_none_rather_than_zero() {
        assert!(Summary::of_slice(&[]).is_none());
        assert!(Summary::of_slice(&[f64::NAN, f64::INFINITY]).is_none());
    }

    #[test]
    fn welford_keeps_precision_where_a_naive_sum_of_squares_would_not() {
        // Values far from zero with a tiny spread: the textbook
        // E[x²] - E[x]² formula returns garbage (often negative) here.
        let base = 1e9;
        let values = [base, base + 1.0, base + 2.0, base + 3.0, base + 4.0];
        let s = Summary::of_slice(&values).unwrap();
        approx(s.mean, base + 2.0);
        assert!((s.stddev - 2.0f64.sqrt()).abs() < 1e-6, "{}", s.stddev);
    }

    #[test]
    fn quantiles_interpolate_between_neighbors() {
        let sorted = sorted_finite(&[1.0, 2.0, 3.0, 4.0]);
        approx(quantile_sorted(&sorted, 0.0).unwrap(), 1.0);
        approx(quantile_sorted(&sorted, 1.0).unwrap(), 4.0);
        approx(quantile_sorted(&sorted, 0.5).unwrap(), 2.5);
        // 0.25 * 3 = 0.75 → between 1.0 and 2.0.
        approx(quantile_sorted(&sorted, 0.25).unwrap(), 1.75);
        approx(median(&[3.0, f64::NAN, 1.0, 2.0]).unwrap(), 2.0);
        assert!(quantile(&[f64::NAN], 0.5).is_none());
    }

    #[test]
    fn simple_moving_average_preserves_original_gaps() {
        let smoothed = simple_moving_average(&[1.0, f64::NAN, 3.0], 3);
        assert_eq!(smoothed.len(), 3);
        assert_eq!(smoothed[0], 1.0);
        assert!(smoothed[1].is_nan());
        assert_eq!(smoothed[2], 3.0);

        let leading = simple_moving_average(&[f64::NAN, 2.0, 4.0, f64::NAN], 3);
        assert!(leading[0].is_nan() && leading[3].is_nan());
        assert_eq!(leading[1], 3.0);
    }

    #[test]
    fn moving_average_smooths_without_bridging_a_gap() {
        let smoothed = simple_moving_average(&[1.0, 2.0, 3.0, 4.0, 5.0], 3);
        approx(smoothed[0], 1.5);
        approx(smoothed[2], 3.0);
        approx(smoothed[4], 4.5);
        // A window with nothing finite stays a gap.
        let gapped = simple_moving_average(&[f64::NAN, f64::NAN, f64::NAN], 3);
        assert!(gapped.iter().all(|v| v.is_nan()));
        // Degenerate windows pass through untouched.
        assert_eq!(simple_moving_average(&[1.0, 2.0], 1), vec![1.0, 2.0]);
    }

    #[test]
    fn exponential_average_seeds_on_the_first_finite_sample() {
        let out = exponential_moving_average(&[f64::NAN, 10.0, 20.0], 0.5);
        assert!(out[0].is_nan());
        approx(out[1], 10.0);
        approx(out[2], 15.0);
    }

    #[test]
    fn linear_fit_recovers_a_known_slope_at_epoch_millisecond_scale() {
        // The whole reason the fit is centered: x values near 1.7e12.
        let t0 = 1_700_000_000_000.0f64;
        let xs: Vec<f64> = (0..64).map(|i| t0 + i as f64 * 1000.0).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 3.0 * (x - t0) / 1000.0 + 7.0).collect();
        let fit = linear_fit(&xs, &ys).unwrap();
        assert!((fit.slope - 3.0 / 1000.0).abs() < 1e-12, "{}", fit.slope);
        assert!((fit.at(t0) - 7.0).abs() < 1e-6, "{}", fit.at(t0));
        approx(fit.r2, 1.0);
        assert_eq!(fit.count, 64);
    }

    #[test]
    fn linear_fit_declines_degenerate_samples() {
        assert!(linear_fit(&[1.0], &[1.0]).is_none());
        // No x variance: infinitely many lines fit.
        assert!(linear_fit(&[5.0, 5.0, 5.0], &[1.0, 2.0, 3.0]).is_none());
        // Pairs where either side is non-finite do not count.
        assert!(linear_fit(&[1.0, 2.0], &[f64::NAN, 2.0]).is_none());
    }

    #[test]
    fn r2_falls_as_noise_overwhelms_the_trend() {
        let xs: Vec<f64> = (0..32).map(|i| i as f64).collect();
        let clean: Vec<f64> = xs.iter().map(|x| 2.0 * x).collect();
        let noisy: Vec<f64> = xs
            .iter()
            .enumerate()
            .map(|(i, x)| 2.0 * x + if i % 2 == 0 { 30.0 } else { -30.0 })
            .collect();
        let clean_fit = linear_fit(&xs, &clean).unwrap();
        let noisy_fit = linear_fit(&xs, &noisy).unwrap();
        approx(clean_fit.r2, 1.0);
        assert!(noisy_fit.r2 < 0.6, "{}", noisy_fit.r2);
    }
}
