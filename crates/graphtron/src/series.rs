use std::sync::Arc;

/// One series in struct-of-arrays form. `xs` are epoch milliseconds, sorted
/// ascending; `f64::NAN` in `ys` marks a gap (missing bucket).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeriesData {
    pub name: String,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    /// 0xRRGGBB; `None` = palette color by series index.
    pub color: Option<u32>,
}

/// A single OHLC tick (open, high, low, close).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OhlcTick {
    pub ts: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

/// OHLC series for candlestick charts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OhlcSeriesData {
    pub name: String,
    pub ticks: Vec<OhlcTick>,
    pub color: Option<u32>,
}

/// Data for histogram charts: pre-bucketed counts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HistogramSeries {
    pub name: String,
    /// Bucket boundaries (len = counts.len() + 1).
    pub buckets: Vec<f64>,
    /// Counts per bucket.
    pub counts: Vec<f64>,
    pub color: Option<u32>,
    pub cumulative: bool,
}

/// Data for horizontal bar charts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HBarSeries {
    pub name: String,
    /// Category labels (y-axis).
    pub categories: Vec<String>,
    pub values: Vec<f64>,
    pub color: Option<u32>,
}

/// A center line with a shaded lower–upper band (p50 with p10–p90, mean with
/// min–max). The single most common primitive for SLO/latency panels.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BandSeries {
    pub name: String,
    /// Epoch ms, ascending.
    pub xs: Vec<f64>,
    /// Center line (e.g. p50). NaN marks a gap.
    pub center: Vec<f64>,
    /// Band bottom (e.g. p10).
    pub lower: Vec<f64>,
    /// Band top (e.g. p90).
    pub upper: Vec<f64>,
    /// 0xRRGGBB; `None` = palette color by series index.
    pub color: Option<u32>,
}

/// One colored segment of a state timeline row.
#[derive(Debug, Clone, PartialEq)]
pub struct StateSegment {
    pub start_ms: f64,
    pub end_ms: f64,
    /// State label (e.g. "up", "degraded", "down") — used by tooltips/legends.
    pub label: String,
    pub color: u32,
}

/// One row of a state timeline (discrete states over time — service health,
/// deploy phases). A chart renders `Vec<StateTimelineSeries>` as stacked rows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateTimelineSeries {
    /// Row label, drawn on the left.
    pub name: String,
    pub segments: Vec<StateSegment>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OhlcSeries(pub Arc<Vec<OhlcSeriesData>>);

/// Cyberpunk neon palette.
pub const PALETTE: &[u32] = &[
    0x00FFFF, 0xFF00FF, 0x00FF41, 0xFF3366, 0x7B68EE, 0xFFD700, 0x00BFFF, 0xFF6EC7, 0x39FF14,
    0xBF00FF, 0xFF8C00, 0x20B2AA, 0xDC143C, 0xADFF2F, 0x1E90FF, 0xFF69B4, 0x8A2BE2, 0x00CED1,
    0xF4A460, 0x7FFF00,
];

/// Resolve an optional explicit color against the palette. Every series type
/// picks its color the same way; this is the one place that rule lives.
pub fn palette_color(explicit: Option<u32>, index: usize) -> u32 {
    explicit.unwrap_or(PALETTE[index % PALETTE.len()])
}

/// Add finite values without turning an overflowing histogram into infinity.
pub(crate) fn finite_add(a: f64, b: f64) -> f64 {
    let sum = a + b;
    if sum.is_finite() {
        sum
    } else if a.is_sign_positive() && b.is_sign_positive() {
        f64::MAX
    } else if a.is_sign_negative() && b.is_sign_negative() {
        -f64::MAX
    } else {
        sum
    }
}

pub fn series_color(series: &SeriesData, index: usize) -> u32 {
    palette_color(series.color, index)
}

pub fn band_series_color(series: &BandSeries, index: usize) -> u32 {
    palette_color(series.color, index)
}

pub fn hbar_series_color(series: &HBarSeries, index: usize) -> u32 {
    palette_color(series.color, index)
}

pub fn histogram_series_color(series: &HistogramSeries, index: usize) -> u32 {
    palette_color(series.color, index)
}

/// Median gap between consecutive values (e.g. tick timestamps). Used to derive
/// bar/candle width from the real data spacing instead of a hardcoded interval.
/// Returns `None` when there are fewer than two values or no positive gaps.
pub fn median_gap(values: &[f64]) -> Option<f64> {
    let mut gaps: Vec<f64> = values
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|gap| gap.is_finite() && *gap > 0.0)
        .collect();
    if gaps.is_empty() {
        return None;
    }
    let middle = gaps.len() / 2;
    let (_, median, _) = gaps.select_nth_unstable_by(middle, f64::total_cmp);
    Some(*median)
}

/// A bounded, allocation-free spacing estimate for geometry. It samples
/// evenly through long histories so neither draw nor hover allocates and sorts
/// the complete dataset.
pub fn sampled_median_gap(values: &[f64]) -> Option<f64> {
    const MAX_SAMPLES: usize = 128;
    if values.len() < 2 {
        return None;
    }
    let gap_count = values.len() - 1;
    let stride = gap_count.saturating_add(MAX_SAMPLES - 1) / MAX_SAMPLES;
    let mut samples = [0.0; MAX_SAMPLES];
    let mut len = 0;
    let mut i = 0;
    while i < gap_count && len < MAX_SAMPLES {
        let gap = values[i + 1] - values[i];
        if gap.is_finite() && gap > 0.0 {
            samples[len] = gap;
            len += 1;
        }
        i = i.saturating_add(stride.max(1));
    }
    if len == 0 {
        return None;
    }
    samples[..len].sort_unstable_by(|a, b| a.total_cmp(b));
    Some(samples[len / 2])
}

pub fn ohlc_series_color(series: &OhlcSeriesData, index: usize) -> u32 {
    palette_color(series.color, index)
}

pub fn css_color(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0x00ff_ffff)
}

pub fn css_color_alpha(rgb: u32, alpha: f64) -> String {
    let r = (rgb >> 16) & 0xff;
    let g = (rgb >> 8) & 0xff;
    let b = rgb & 0xff;
    format!("rgba({r},{g},{b},{alpha})")
}

pub fn rgb_to_components(rgb: u32) -> (u8, u8, u8) {
    let r = ((rgb >> 16) & 0xff) as u8;
    let g = ((rgb >> 8) & 0xff) as u8;
    let b = (rgb & 0xff) as u8;
    (r, g, b)
}

pub fn glow_color(rgb: u32, alpha: f64) -> String {
    let (r, g, b) = rgb_to_components(rgb);
    format!("rgba({r},{g},{b},{alpha})")
}

impl SeriesData {
    pub fn y_extent(&self, x0: f64, x1: f64) -> Option<(f64, f64)> {
        interpolated_extent(&self.xs, &self.ys, x0, x1)
    }

    pub fn positive_y_extent(&self, x0: f64, x1: f64) -> Option<(f64, f64)> {
        interpolated_extent_where(&self.xs, &self.ys, x0, x1, |value| value > 0.0)
    }

    /// Extent of finite samples whose x values are inside the interval.
    /// Scatter points and bar columns have no connecting geometry to include.
    pub fn sample_y_extent(&self, x0: f64, x1: f64) -> Option<(f64, f64)> {
        let (from, to) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for (&x, &y) in self.xs.iter().zip(&self.ys) {
            if x >= from && x <= to && y.is_finite() {
                min = min.min(y);
                max = max.max(y);
            }
        }
        (min.is_finite() && max.is_finite()).then_some((min, max))
    }

    pub fn sample_positive_y_extent(&self, x0: f64, x1: f64) -> Option<(f64, f64)> {
        let (from, to) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for (&x, &y) in self.xs.iter().zip(&self.ys) {
            if x >= from && x <= to && y.is_finite() && y > 0.0 {
                min = min.min(y);
                max = max.max(y);
            }
        }
        (min.is_finite() && max.is_finite()).then_some((min, max))
    }

    /// Extent of step-after geometry in a visible interval. A segment keeps
    /// its left sample value until the transition at the right x coordinate.
    pub fn step_y_extent(&self, x0: f64, x1: f64) -> Option<(f64, f64)> {
        self.step_extent_where(x0, x1, |value| value.is_finite())
    }

    pub fn positive_step_y_extent(&self, x0: f64, x1: f64) -> Option<(f64, f64)> {
        self.step_extent_where(x0, x1, |value| value.is_finite() && value > 0.0)
    }

    fn step_extent_where(
        &self,
        x0: f64,
        x1: f64,
        accept: impl Fn(f64) -> bool,
    ) -> Option<(f64, f64)> {
        let (from, to) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let n = self.xs.len().min(self.ys.len());
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let mut take = |value: f64| {
            if accept(value) {
                min = min.min(value);
                max = max.max(value);
            }
        };
        for i in 0..n {
            let (x, y) = (self.xs[i], self.ys[i]);
            if x >= from && x <= to {
                take(y);
            }
            if i + 1 >= n {
                continue;
            }
            let (next_x, next_y) = (self.xs[i + 1], self.ys[i + 1]);
            if !x.is_finite() || !next_x.is_finite() || !y.is_finite() || !next_y.is_finite() {
                continue;
            }
            let segment_from = x.min(next_x);
            let segment_to = x.max(next_x);
            if segment_to >= from && segment_from <= to {
                take(y);
            }
            if next_x >= from && next_x <= to {
                take(next_y);
            }
        }
        (min.is_finite() && max.is_finite()).then_some((min, max))
    }
}

/// Extent of samples and the portions of finite line segments intersecting a
/// visible x interval. Keeping the crossing points matters when a sparse
/// series is panned between observations: the rendered segment still has a
/// substantial y value even though neither endpoint lies inside the window.
fn interpolated_extent(xs: &[f64], ys: &[f64], x0: f64, x1: f64) -> Option<(f64, f64)> {
    interpolated_extent_where(xs, ys, x0, x1, |_| true)
}

fn interpolated_extent_where(
    xs: &[f64],
    ys: &[f64],
    x0: f64,
    x1: f64,
    accept: impl Fn(f64) -> bool,
) -> Option<(f64, f64)> {
    let n = xs.len().min(ys.len());
    let (from, to) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut take = |value: f64| {
        if value.is_finite() && accept(value) {
            min = min.min(value);
            max = max.max(value);
        }
    };

    for i in 0..n {
        let (x, y) = (xs[i], ys[i]);
        if x.is_finite() && y.is_finite() && x >= from && x <= to {
            take(y);
        }
        if i + 1 >= n {
            continue;
        }
        let (xa, ya, xb, yb) = (xs[i], ys[i], xs[i + 1], ys[i + 1]);
        if !xa.is_finite() || !xb.is_finite() || !ya.is_finite() || !yb.is_finite() {
            continue;
        }
        let seg_from = xa.min(xb);
        let seg_to = xa.max(xb);
        if seg_to < from || seg_from > to {
            continue;
        }
        if xa == xb {
            take(ya);
            take(yb);
            continue;
        }
        let left = from.max(seg_from).min(seg_to);
        let right = to.min(seg_to).max(seg_from);
        let y_at = |x: f64| {
            let t = (x - xa) / (xb - xa);
            // Avoid an overflowing endpoint difference when the segment
            // crosses zero; this expression remains a convex combination of
            // the two finite samples.
            if ya.signum() != yb.signum() {
                ya * (1.0 - t) + yb * t
            } else {
                ya + (yb - ya) * t
            }
        };
        // Smooth rendering uses monotone cubic segments. Their y values stay
        // within the endpoint range, but a viewport can intersect the middle
        // of a segment without containing either endpoint. Retain both
        // endpoints as a conservative bound so the smoothed curve cannot be
        // clipped by an auto domain. Linear rendering may get extra headroom,
        // which is preferable to dropping visible geometry.
        take(ya);
        take(yb);
        take(y_at(left));
        take(y_at(right));
    }
    (min.is_finite() && max.is_finite()).then_some((min, max))
}

impl OhlcSeriesData {
    pub fn y_extent(&self, x0: f64, x1: f64) -> Option<(f64, f64)> {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for t in &self.ticks {
            if t.ts >= x0 && t.ts <= x1 {
                min = min.min(t.low);
                max = max.max(t.high);
            }
        }
        (min.is_finite() && max.is_finite()).then_some((min, max))
    }
}

impl HistogramSeries {
    /// Counts as drawn: a running (cumulative) sum when `cumulative` is set,
    /// otherwise the raw counts. A non-finite raw count contributes nothing but
    /// the running total carries forward, so a cumulative histogram is
    /// monotonically non-decreasing.
    pub fn plot_counts(&self) -> Vec<f64> {
        if self.cumulative {
            let mut acc = 0.0;
            self.counts
                .iter()
                .map(|&c| {
                    if c.is_finite() {
                        acc = finite_add(acc, c);
                    }
                    acc
                })
                .collect()
        } else {
            self.counts.clone()
        }
    }

    /// Value drawn for one bucket without allocating the complete cumulative
    /// vector. This is the efficient path for pointer-rate hit-testing.
    pub fn plot_count_at(&self, index: usize) -> Option<f64> {
        let value = *self.counts.get(index)?;
        if self.cumulative {
            Some(
                self.counts[..=index]
                    .iter()
                    .copied()
                    .filter(|count| count.is_finite())
                    .fold(0.0, finite_add),
            )
        } else {
            Some(value)
        }
    }

    pub fn y_extent(&self) -> Option<(f64, f64)> {
        let max = self
            .plot_counts()
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        max.is_finite().then_some((0.0, max))
    }
}

impl HBarSeries {
    pub fn y_extent(&self) -> Option<(f64, f64)> {
        let max = self
            .values
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .fold(f64::NEG_INFINITY, f64::max);
        let min = self
            .values
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .fold(f64::INFINITY, f64::min);
        (min.is_finite() && max.is_finite()).then_some((min.min(0.0), max.max(0.0)))
    }
}

impl BandSeries {
    pub fn y_extent(&self, x0: f64, x1: f64) -> Option<(f64, f64)> {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for values in [&self.lower, &self.upper, &self.center] {
            if let Some((lo, hi)) = interpolated_extent(&self.xs, values, x0, x1) {
                min = min.min(lo);
                max = max.max(hi);
            }
        }
        (min.is_finite() && max.is_finite()).then_some((min, max))
    }

    pub fn positive_y_extent(&self, x0: f64, x1: f64) -> Option<(f64, f64)> {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for values in [&self.lower, &self.upper, &self.center] {
            if let Some((lo, hi)) =
                interpolated_extent_where(&self.xs, values, x0, x1, |value| value > 0.0)
            {
                min = min.min(lo);
                max = max.max(hi);
            }
        }
        (min.is_finite() && max.is_finite()).then_some((min, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_extent_covers_band_not_just_center() {
        let b = BandSeries {
            name: "lat".into(),
            xs: vec![0.0, 1.0, 2.0],
            center: vec![5.0, 6.0, 5.5],
            lower: vec![1.0, 2.0, 1.5],
            upper: vec![9.0, 10.0, 9.5],
            color: None,
        };
        assert_eq!(b.y_extent(0.0, 2.0), Some((1.0, 10.0)));
        // Out-of-range points excluded.
        assert_eq!(b.y_extent(0.0, 0.5), Some((1.0, 10.0)));
    }

    #[test]
    fn line_extent_includes_a_segment_crossing_the_viewport() {
        let s = SeriesData {
            name: "sparse".into(),
            xs: vec![0.0, 10.0],
            ys: vec![100.0, 200.0],
            color: None,
        };
        // The full segment is retained because smooth monotone rendering can
        // reach either endpoint's y range while the viewport cuts through it.
        assert_eq!(s.y_extent(4.0, 6.0), Some((100.0, 200.0)));
    }

    #[test]
    fn line_extent_bounds_smooth_curve_crossing_viewport() {
        let s = SeriesData {
            name: "smooth".into(),
            xs: vec![0.0, 10.0, 20.0],
            ys: vec![0.0, 100.0, 0.0],
            color: None,
        };
        let (lo, hi) = s.y_extent(4.0, 6.0).unwrap();
        assert!(lo <= 0.0 && hi >= 100.0);
    }

    #[test]
    fn sample_and_step_extents_preserve_their_geometry_semantics() {
        let s = SeriesData {
            name: "step".into(),
            xs: vec![0.0, 10.0],
            ys: vec![1.0, 100.0],
            color: None,
        };
        assert_eq!(s.sample_y_extent(4.0, 6.0), None);
        assert_eq!(s.step_y_extent(4.0, 6.0), Some((1.0, 1.0)));
    }

    #[test]
    fn band_extent_includes_crossing_envelope_segments() {
        let b = BandSeries {
            name: "band".into(),
            xs: vec![0.0, 10.0],
            center: vec![100.0, 200.0],
            lower: vec![80.0, 180.0],
            upper: vec![120.0, 220.0],
            color: None,
        };
        assert_eq!(b.y_extent(4.0, 6.0), Some((80.0, 220.0)));
    }

    #[test]
    fn median_gap_of_regular_spacing() {
        // 5-minute ticks → 300_000 ms spacing (C4: candle width must derive
        // from this, not a hardcoded 60_000).
        let ts: Vec<f64> = (0..10).map(|i| i as f64 * 300_000.0).collect();
        assert_eq!(median_gap(&ts), Some(300_000.0));
    }

    #[test]
    fn median_gap_ignores_a_single_wide_gap() {
        // Mostly 1s spacing with one large jump — the median is robust to it.
        let mut ts: Vec<f64> = (0..20).map(|i| i as f64 * 1000.0).collect();
        ts.push(10_000_000.0);
        assert_eq!(median_gap(&ts), Some(1000.0));
    }

    #[test]
    fn median_gap_needs_two_points() {
        assert_eq!(median_gap(&[]), None);
        assert_eq!(median_gap(&[42.0]), None);
    }

    #[test]
    fn sampled_gap_is_bounded_and_ignores_invalid_gaps() {
        let xs: Vec<f64> = (0..10_000).map(|i| i as f64 * 2.0).collect();
        assert_eq!(sampled_median_gap(&xs), Some(2.0));
        assert_eq!(sampled_median_gap(&[0.0, f64::NAN, 2.0]), None);
    }

    #[test]
    fn plot_counts_passthrough_when_not_cumulative() {
        let s = HistogramSeries {
            name: "h".into(),
            buckets: vec![0.0, 1.0, 2.0, 3.0],
            counts: vec![3.0, 1.0, 2.0],
            color: None,
            cumulative: false,
        };
        assert_eq!(s.plot_counts(), vec![3.0, 1.0, 2.0]);
    }

    #[test]
    fn cumulative_counts_saturate_instead_of_becoming_infinite() {
        let series = HistogramSeries {
            name: "large".into(),
            buckets: vec![0.0, 1.0, 2.0],
            counts: vec![f64::MAX, f64::MAX],
            color: None,
            cumulative: true,
        };
        let plotted = series.plot_counts();
        assert!(plotted.iter().all(|value| value.is_finite()));
        assert_eq!(plotted[1], f64::MAX);
    }

    #[test]
    fn css_color_masks_oversized_literals() {
        assert_eq!(css_color(0x1234_5678), "#345678");
    }

    #[test]
    fn plot_counts_is_monotonic_when_cumulative() {
        let s = HistogramSeries {
            name: "h".into(),
            buckets: vec![0.0, 1.0, 2.0, 3.0, 4.0],
            counts: vec![3.0, 1.0, f64::NAN, 2.0],
            color: None,
            cumulative: true,
        };
        let cc = s.plot_counts();
        assert_eq!(cc, vec![3.0, 4.0, 4.0, 6.0]);
        for pair in cc.windows(2) {
            assert!(
                pair[1] >= pair[0],
                "cumulative counts not monotonic: {cc:?}"
            );
        }
        // y_extent tops out at the running total, not the max bucket.
        assert_eq!(s.y_extent(), Some((0.0, 6.0)));
    }
}
