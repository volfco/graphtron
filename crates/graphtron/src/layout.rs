use crate::scale::{LOG_EPSILON, LinearScale, ScaleKind};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

/// Result of a data draw: where the plot area landed and the scales used.
/// Mouse handlers keep the last layout to invert pixel → timestamp
/// synchronously, and the overlay draw uses it to position the crosshair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartLayout {
    /// Plot rect in CSS pixels (inside axes/margins).
    pub plot: Rect,
    /// x: epoch ms ↔ CSS px.
    pub x_scale: LinearScale,
    /// y in **transformed** space ↔ CSS px (range inverted: r0 = bottom).
    /// For a log axis its domain holds `log10` values, so prefer
    /// [`ChartLayout::y_px`], [`ChartLayout::y_at`], and
    /// [`ChartLayout::y_domain`] over reading `d0`/`d1` directly.
    pub y_scale: LinearScale,
    /// How y values map into `y_scale`'s domain.
    pub y_kind: ScaleKind,
}

pub const MARGIN_LEFT: f64 = 52.0;
pub const MARGIN_RIGHT: f64 = 8.0;
pub const MARGIN_TOP: f64 = 8.0;
pub const MARGIN_BOTTOM: f64 = 22.0;

impl ChartLayout {
    /// Compute the layout for a css_w × css_h canvas over [from_ms, to_ms]
    /// and the given y domain, reserving the standard axis margins.
    pub fn compute(
        css_w: f64,
        css_h: f64,
        from_ms: f64,
        to_ms: f64,
        y_min: f64,
        y_max: f64,
    ) -> Self {
        Self::compute_with_margins(
            css_w,
            css_h,
            from_ms,
            to_ms,
            y_min,
            y_max,
            MARGIN_LEFT,
            MARGIN_RIGHT,
            MARGIN_TOP,
            MARGIN_BOTTOM,
        )
    }

    /// As [`compute`] but with explicit margins — pass zeros for a chrome-less
    /// (sparkline) chart that fills the whole canvas.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_with_margins(
        css_w: f64,
        css_h: f64,
        from_ms: f64,
        to_ms: f64,
        y_min: f64,
        y_max: f64,
        ml: f64,
        mr: f64,
        mt: f64,
        mb: f64,
    ) -> Self {
        Self::compute_scaled(
            css_w,
            css_h,
            from_ms,
            to_ms,
            y_min,
            y_max,
            ml,
            mr,
            mt,
            mb,
            ScaleKind::Linear,
        )
    }

    /// As [`compute_with_margins`] but with an explicit y-axis scale kind.
    /// A log axis stores `log10` bounds in `y_scale` and snaps them to the
    /// enclosing 1-2-5 log boundaries instead of linear "nice" numbers.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_scaled(
        css_w: f64,
        css_h: f64,
        from_ms: f64,
        to_ms: f64,
        y_min: f64,
        y_max: f64,
        ml: f64,
        mr: f64,
        mt: f64,
        mb: f64,
        y_kind: ScaleKind,
    ) -> Self {
        let css_w = finite_or(css_w, 1.0).max(1.0);
        let css_h = finite_or(css_h, 1.0).max(1.0);
        let (ml, mr) = fit_margins(ml, mr, css_w);
        let (mt, mb) = fit_margins(mt, mb, css_h);
        let (from_ms, to_ms) = ordered_domain(from_ms, to_ms);
        let (y_min, y_max) = ordered_domain(y_min, y_max);
        let plot = Rect {
            x: ml,
            y: mt,
            w: (css_w - ml - mr).max(1.0),
            h: (css_h - mt - mb).max(1.0),
        };
        let x_scale = LinearScale::new(from_ms, to_ms, plot.x, plot.x + plot.w);
        let y_scale = match y_kind {
            ScaleKind::Linear => {
                let mut scale = LinearScale::new(y_min, y_max, plot.y + plot.h, plot.y);
                scale.nice(5);
                scale
            }
            ScaleKind::Log => {
                let (lo, hi) = positive_log_bounds(y_min, y_max);
                let (lo, hi) = crate::ticks::nice_log_domain(lo, hi);
                LinearScale::new(lo.log10(), hi.log10(), plot.y + plot.h, plot.y)
            }
        };
        Self {
            plot,
            x_scale,
            y_scale,
            y_kind,
        }
    }

    /// Data value → CSS pixel on the y axis, honoring the axis scale kind.
    /// Values a log axis cannot represent (zero, negative) yield `NaN`, which
    /// downstream path building already treats as a gap.
    pub fn y_px(&self, value: f64) -> f64 {
        self.y_scale.to_px(self.y_kind.forward(value))
    }

    /// CSS pixel → data value on the y axis.
    pub fn y_at(&self, px: f64) -> f64 {
        self.y_kind.inverse(self.y_scale.from_px(px))
    }

    /// The y domain in **data** space (`y_scale.d0`/`d1` are transformed).
    pub fn y_domain(&self) -> (f64, f64) {
        (
            self.y_kind.inverse(self.y_scale.d0),
            self.y_kind.inverse(self.y_scale.d1),
        )
    }

    /// Pixel row for a bar, stack, or fill edge. Values a log axis cannot
    /// represent (zero and below) collapse onto the baseline rather than
    /// vanishing, so a log bar chart still draws its columns.
    pub fn y_edge_px(&self, value: f64) -> f64 {
        let px = self.y_px(value);
        if px.is_finite() {
            px
        } else {
            self.y_baseline_px()
        }
    }

    /// Pixel row that bars, areas, and fills rest on: zero when the domain
    /// contains it, otherwise the nearer domain edge. A log axis has no zero,
    /// so its baseline is the bottom of the plot.
    pub fn y_baseline_px(&self) -> f64 {
        match self.y_kind {
            ScaleKind::Linear => {
                let (lo, hi) = (self.y_scale.d0, self.y_scale.d1);
                self.y_scale.to_px(0.0f64.clamp(lo.min(hi), hi.max(lo)))
            }
            ScaleKind::Log => self.plot.y + self.plot.h,
        }
    }

    /// Pixel x → epoch ms, clamped to the plot.
    pub fn ts_at(&self, px: f64) -> f64 {
        let clamped = px.clamp(self.plot.x, self.plot.x + self.plot.w);
        self.x_scale.from_px(clamped)
    }
}

/// Coerce an arbitrary domain into strictly-positive bounds a log axis can
/// draw. Non-positive or absent bounds fall back to three decades below the
/// maximum, which is the readable default for latency and throughput data.
fn positive_log_bounds(y_min: f64, y_max: f64) -> (f64, f64) {
    let hi = if y_max.is_finite() && y_max > 0.0 {
        y_max.max(LOG_EPSILON * 10.0)
    } else {
        1.0
    };
    let lo = if y_min.is_finite() && y_min > 0.0 && y_min < hi {
        y_min.max(LOG_EPSILON)
    } else {
        (hi / 1000.0).max(LOG_EPSILON)
    };
    if lo >= hi { (hi / 10.0, hi) } else { (lo, hi) }
}

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

fn fit_margins(start: f64, end: f64, extent: f64) -> (f64, f64) {
    let mut start = finite_or(start, 0.0).max(0.0);
    let mut end = finite_or(end, 0.0).max(0.0);
    let budget = (extent - 1.0).max(0.0);
    let total = start + end;
    if total > budget && total > 0.0 {
        let scale = budget / total;
        start *= scale;
        end *= scale;
    }
    (start, end)
}

fn finite_add(a: f64, b: f64) -> f64 {
    let value = a + b;
    if value.is_finite() {
        value
    } else if a.is_sign_positive() {
        f64::MAX
    } else {
        -f64::MAX
    }
}

fn finite_sub(a: f64, b: f64) -> f64 {
    finite_add(a, -b)
}

fn ordered_domain(a: f64, b: f64) -> (f64, f64) {
    let mut lo = finite_or(a, 0.0);
    let mut hi = finite_or(b, 1.0);
    if lo > hi {
        std::mem::swap(&mut lo, &mut hi);
    }
    if lo == hi {
        let pad = if lo == 0.0 { 1.0 } else { lo.abs() * 0.1 };
        (finite_sub(lo, pad), finite_add(hi, pad))
    } else {
        (lo, hi)
    }
}

/// Auto y-domain across visible series, honoring pins and zero anchoring.
pub fn y_domain(
    series: &[crate::series::SeriesData],
    from_ms: f64,
    to_ms: f64,
    y_min: Option<f64>,
    y_max: Option<f64>,
    zero_anchored: bool,
) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for s in series {
        if let Some((lo, hi)) = s.y_extent(from_ms, to_ms) {
            min = min.min(lo);
            max = max.max(hi);
        }
    }
    finish_domain(
        min,
        max,
        y_min,
        y_max,
        zero_anchored,
        series_all_non_negative(series),
    )
}

/// Shared domain finishing: default for empty data, zero anchoring, flat-series
/// widening, 5% padding, and pin overrides. `non_negative` suppresses padding
/// below zero for all-positive data so a zero-anchored axis starts exactly at 0.
pub(crate) fn finish_domain(
    mut min: f64,
    mut max: f64,
    pin_min: Option<f64>,
    pin_max: Option<f64>,
    zero_anchored: bool,
    non_negative: bool,
) -> (f64, f64) {
    if !min.is_finite() || !max.is_finite() {
        (min, max) = (0.0, 1.0);
    }
    if zero_anchored {
        min = min.min(0.0);
        max = max.max(0.0);
    }
    if min == max {
        // Flat series: open up a window around the value.
        let pad = if min == 0.0 { 1.0 } else { min.abs() * 0.1 };
        min -= pad;
        max += pad;
        if zero_anchored {
            min = 0.0;
        }
    } else {
        let span = max - min;
        let pad = if span.is_finite() {
            span * 0.05
        } else {
            (max.abs() * 0.05 + min.abs() * 0.05).min(f64::MAX)
        };
        if !zero_anchored || min != 0.0 {
            min = finite_sub(min, pad);
        }
        max = finite_add(max, pad);
        if zero_anchored && min < 0.0 && non_negative {
            min = 0.0;
        }
    }
    let lo = pin_min.filter(|value| value.is_finite()).unwrap_or(min);
    let hi = pin_max.filter(|value| value.is_finite()).unwrap_or(max);
    ordered_domain(lo, hi)
}

fn series_all_non_negative(series: &[crate::series::SeriesData]) -> bool {
    series
        .iter()
        .all(|s| s.ys.iter().all(|y| !y.is_finite() || *y >= 0.0))
}

fn sample_y_domain(
    series: &[crate::series::SeriesData],
    from_ms: f64,
    to_ms: f64,
    y_min: Option<f64>,
    y_max: Option<f64>,
    zero_anchored: bool,
) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for s in series {
        if let Some((lo, hi)) = s.sample_y_extent(from_ms, to_ms) {
            min = min.min(lo);
            max = max.max(hi);
        }
    }
    finish_domain(
        min,
        max,
        y_min,
        y_max,
        zero_anchored,
        series_all_non_negative(series),
    )
}

fn step_y_domain(
    series: &[crate::series::SeriesData],
    from_ms: f64,
    to_ms: f64,
    y_min: Option<f64>,
    y_max: Option<f64>,
    zero_anchored: bool,
) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for s in series {
        if let Some((lo, hi)) = s.step_y_extent(from_ms, to_ms) {
            min = min.min(lo);
            max = max.max(hi);
        }
    }
    finish_domain(
        min,
        max,
        y_min,
        y_max,
        zero_anchored,
        series_all_non_negative(series),
    )
}

/// Extent of per-column positive and negative sums across index-aligned series
/// (stacked bars/areas).
/// Non-finite values contribute nothing; columns outside [x0, x1] are skipped.
fn stacked_extent(series: &[crate::series::SeriesData], x0: f64, x1: f64) -> (f64, f64) {
    let n = series.iter().map(|s| s.ys.len()).max().unwrap_or(0);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for i in 0..n {
        let mut positive = 0.0;
        let mut negative = 0.0;
        let mut any = false;
        for s in series {
            let (Some(x), Some(y)) = (s.xs.get(i), s.ys.get(i)) else {
                continue;
            };
            if *x < x0 || *x > x1 || !y.is_finite() {
                continue;
            }
            if *y >= 0.0 {
                positive = crate::series::finite_add(positive, *y);
            } else {
                negative = crate::series::finite_add(negative, *y);
            }
            any = true;
        }
        if any {
            lo = lo.min(negative);
            hi = hi.max(positive);
        }
    }
    if lo.is_finite() && hi.is_finite() {
        (lo, hi)
    } else {
        (f64::INFINITY, f64::NEG_INFINITY)
    }
}

/// Extent of the composed geometry of a stacked area. In addition to source
/// samples, retain neighboring samples when a segment crosses the window;
/// those are the points that remain visible during a sparse pan. Stacked
/// rendering is index-aligned to the first series' x grid, so use that same
/// grid here instead of independently interpolating every series.
fn stacked_area_extent(series: &[crate::series::SeriesData], x0: f64, x1: f64) -> (f64, f64) {
    let (from, to) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let Some(first) = series.iter().find(|s| !s.xs.is_empty()) else {
        return (f64::INFINITY, f64::NEG_INFINITY);
    };
    let n = first.xs.len();
    if n == 0 {
        return (f64::INFINITY, f64::NEG_INFINITY);
    }
    // Keep one source neighbor on either side of the viewport. This is both
    // the renderer's visible-window contract and the conservative bound for
    // smooth cubic edges that cross a narrow viewport.
    let start = first.xs.partition_point(|x| *x < from).saturating_sub(1);
    let end = first
        .xs
        .partition_point(|x| *x <= to)
        .saturating_add(1)
        .min(n);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for index in start.min(end)..end {
        let mut positive = 0.0;
        let mut negative = 0.0;
        let mut any = false;
        for s in series {
            let Some(value) = s.ys.get(index).copied().filter(|value| value.is_finite()) else {
                continue;
            };
            if value >= 0.0 {
                positive = crate::series::finite_add(positive, value);
            } else {
                negative = crate::series::finite_add(negative, value);
            }
            any = true;
        }
        if any {
            lo = lo.min(negative);
            hi = hi.max(positive);
        }
    }
    if lo.is_finite() && hi.is_finite() {
        (lo, hi)
    } else {
        (f64::INFINITY, f64::NEG_INFINITY)
    }
}

fn stacked_has_negative(series: &[crate::series::SeriesData], x0: f64, x1: f64) -> bool {
    series.iter().any(|series| {
        series
            .xs
            .iter()
            .zip(&series.ys)
            .any(|(x, y)| *x >= x0 && *x <= x1 && y.is_finite() && *y < 0.0)
    })
}

fn histogram_stacked_extent(series: &[crate::series::HistogramSeries]) -> (f64, f64) {
    let n = series
        .iter()
        .map(|series| series.counts.len())
        .max()
        .unwrap_or(0);
    let plotted: Vec<Vec<f64>> = series.iter().map(|series| series.plot_counts()).collect();
    let mut lo = 0.0f64;
    let mut hi = 0.0f64;
    for bucket in 0..n {
        let mut positive = 0.0;
        let mut negative = 0.0;
        for counts in &plotted {
            let Some(value) = counts.get(bucket).copied().filter(|v| v.is_finite()) else {
                continue;
            };
            if value >= 0.0 {
                positive = crate::series::finite_add(positive, value);
            } else {
                negative = crate::series::finite_add(negative, value);
            }
        }
        lo = lo.min(negative);
        hi = hi.max(positive);
    }
    (lo, hi)
}

/// Y domain for any [`ChartData`] variant, honoring pins and zero anchoring.
/// Point-series kinds delegate to [`y_domain`]; OHLC uses low/high extents;
/// histograms span `[0, max drawn count]` (cumulative-aware); hbar rows are
/// index-positioned so the y scale spans the row count. For stacked
/// bars/areas the extent is the per-column **sum** of series — a stack can
/// exceed any individual series' max.
pub fn data_y_domain(
    data: &crate::spec::ChartData,
    from_ms: f64,
    to_ms: f64,
    y_min: Option<f64>,
    y_max: Option<f64>,
    zero_anchored: bool,
    layout: crate::spec::SeriesLayout,
) -> (f64, f64) {
    use crate::spec::{ChartData, SeriesLayout};
    // Stacked composition changes the extent semantics for Bars/Areas.
    if matches!(data, ChartData::Bars(_) | ChartData::Areas(_)) {
        match layout {
            SeriesLayout::StackedPercent => {
                let has_negative = stacked_has_negative(data.point_series(), from_ms, to_ms);
                return finish_domain(
                    if has_negative { -100.0 } else { 0.0 },
                    100.0,
                    y_min,
                    y_max,
                    !has_negative,
                    !has_negative,
                );
            }
            SeriesLayout::Stacked => {
                let series = data.point_series();
                if series.len() > 1 {
                    let (lo, hi) = match data {
                        ChartData::Areas(_) => stacked_area_extent(series, from_ms, to_ms),
                        _ => stacked_extent(series, from_ms, to_ms),
                    };
                    return finish_domain(lo, hi, y_min, y_max, true, lo >= 0.0);
                }
            }
            SeriesLayout::Grouped => {}
        }
    }
    match data {
        ChartData::Ohlc(series) => {
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            for s in series {
                if let Some((lo, hi)) = s.y_extent(from_ms, to_ms) {
                    min = min.min(lo);
                    max = max.max(hi);
                }
            }
            finish_domain(min, max, y_min, y_max, zero_anchored, min >= 0.0)
        }
        ChartData::Histogram(series) => {
            if matches!(layout, SeriesLayout::Stacked | SeriesLayout::StackedPercent)
                && series.len() > 1
            {
                if matches!(layout, SeriesLayout::StackedPercent) {
                    let has_negative = series.iter().any(|series| {
                        series
                            .plot_counts()
                            .iter()
                            .any(|value| value.is_finite() && *value < 0.0)
                    });
                    return finish_domain(
                        if has_negative { -100.0 } else { 0.0 },
                        100.0,
                        y_min,
                        y_max,
                        !has_negative,
                        !has_negative,
                    );
                }
                let (lo, hi) = histogram_stacked_extent(series);
                return finish_domain(lo, hi, y_min, y_max, true, lo >= 0.0);
            }
            let mut max = f64::NEG_INFINITY;
            for s in series {
                if let Some((_, hi)) = s.y_extent() {
                    max = max.max(hi);
                }
            }
            finish_domain(0.0, max, y_min, y_max, true, true)
        }
        ChartData::HBar(series) => {
            let rows = series.iter().map(|s| s.categories.len()).max().unwrap_or(0);
            (0.0, (rows as f64).max(1.0))
        }
        ChartData::StateTimeline(series) => (0.0, (series.len() as f64).max(1.0)),
        ChartData::Band(series) => {
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            let mut non_negative = true;
            for s in series {
                if let Some((lo, hi)) = s.y_extent(from_ms, to_ms) {
                    min = min.min(lo);
                    max = max.max(hi);
                    if lo < 0.0 {
                        non_negative = false;
                    }
                }
            }
            finish_domain(min, max, y_min, y_max, zero_anchored, non_negative)
        }
        ChartData::Points(series) | ChartData::Scatter(series) | ChartData::Bars(series) => {
            sample_y_domain(series, from_ms, to_ms, y_min, y_max, zero_anchored)
        }
        ChartData::Step(series) => {
            step_y_domain(series, from_ms, to_ms, y_min, y_max, zero_anchored)
        }
        ChartData::Areas(series) => y_domain(series, from_ms, to_ms, y_min, y_max, zero_anchored),
        _ => y_domain(
            data.point_series(),
            from_ms,
            to_ms,
            y_min,
            y_max,
            zero_anchored,
        ),
    }
}

/// Y domain for a chart, honoring the axis scale kind.
///
/// A log axis cannot show zero or negative values, so its domain is the
/// strictly-positive extent of the visible data (padded by a fifth of a
/// decade) rather than the zero-anchored linear extent. Explicit non-positive
/// pins are ignored on a log axis instead of collapsing the scale.
#[allow(clippy::too_many_arguments)]
pub fn data_y_domain_scaled(
    data: &crate::spec::ChartData,
    from_ms: f64,
    to_ms: f64,
    y_min: Option<f64>,
    y_max: Option<f64>,
    zero_anchored: bool,
    layout: crate::spec::SeriesLayout,
    y_kind: crate::scale::ScaleKind,
) -> (f64, f64) {
    if matches!(y_kind, crate::scale::ScaleKind::Linear) {
        return data_y_domain(data, from_ms, to_ms, y_min, y_max, zero_anchored, layout);
    }
    let (mut lo, mut hi) = positive_extent(data, from_ms, to_ms, layout);
    if !lo.is_finite() || !hi.is_finite() || lo <= 0.0 || hi <= 0.0 {
        // No positive data to show: fall back to a single readable decade.
        (lo, hi) = (1.0, 10.0);
    } else {
        // A fifth of a decade of headroom keeps extremes off the frame edge.
        lo /= 10f64.powf(0.2);
        hi *= 10f64.powf(0.2);
    }
    let lo = y_min.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(lo);
    let hi = y_max.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(hi);
    if lo < hi { (lo, hi) } else { (hi / 10.0, hi) }
}

/// Smallest and largest strictly-positive y across any chart data, restricted
/// to the visible x window for time-indexed kinds.
fn positive_extent(
    data: &crate::spec::ChartData,
    from_ms: f64,
    to_ms: f64,
    series_layout: crate::spec::SeriesLayout,
) -> (f64, f64) {
    use crate::spec::{ChartData, SeriesLayout};
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut take = |value: f64| {
        if value.is_finite() && value > 0.0 {
            lo = lo.min(value);
            hi = hi.max(value);
        }
    };
    match data {
        ChartData::Ohlc(series) => {
            for s in series {
                for tick in &s.ticks {
                    if tick.ts >= from_ms && tick.ts <= to_ms {
                        take(tick.low);
                        take(tick.high);
                    }
                }
            }
        }
        ChartData::Histogram(series) => {
            let counts: Vec<Vec<f64>> = series.iter().map(|s| s.plot_counts()).collect();
            if matches!(
                series_layout,
                SeriesLayout::Stacked | SeriesLayout::StackedPercent
            ) {
                let bins = counts.iter().map(Vec::len).max().unwrap_or(0);
                for bucket in 0..bins {
                    let (mut positive, mut negative) = (0.0, 0.0);
                    let (positive_total, negative_total) =
                        counts
                            .iter()
                            .fold((0.0, 0.0), |(positive, negative), values| {
                                match values.get(bucket).copied() {
                                    Some(value) if value.is_finite() && value >= 0.0 => {
                                        (crate::series::finite_add(positive, value), negative)
                                    }
                                    Some(value) if value.is_finite() => {
                                        (positive, crate::series::finite_add(negative, value.abs()))
                                    }
                                    _ => (positive, negative),
                                }
                            });
                    for values in &counts {
                        let Some(mut value) = values.get(bucket).copied() else {
                            continue;
                        };
                        if !value.is_finite() {
                            continue;
                        }
                        if matches!(series_layout, SeriesLayout::StackedPercent) {
                            value = if value >= 0.0 && positive_total > 0.0 {
                                value / positive_total * 100.0
                            } else if value < 0.0 && negative_total > 0.0 {
                                value / negative_total * 100.0
                            } else {
                                0.0
                            };
                        }
                        if value >= 0.0 {
                            positive = crate::series::finite_add(positive, value);
                        } else {
                            negative = crate::series::finite_add(negative, value);
                        }
                    }
                    take(positive);
                    take(negative);
                }
            } else {
                for values in counts {
                    for value in values {
                        take(value);
                    }
                }
            }
        }
        ChartData::Band(series) => {
            for s in series {
                if let Some((lo, hi)) = s.positive_y_extent(from_ms, to_ms) {
                    take(lo);
                    take(hi);
                }
            }
        }
        ChartData::Points(series) | ChartData::Scatter(series) => {
            for s in series {
                if let Some((lo, hi)) = s.sample_positive_y_extent(from_ms, to_ms) {
                    take(lo);
                    take(hi);
                }
            }
        }
        ChartData::Step(series) => {
            for s in series {
                if let Some((lo, hi)) = s.positive_step_y_extent(from_ms, to_ms) {
                    take(lo);
                    take(hi);
                }
            }
        }
        ChartData::HBar(series) => {
            for s in series {
                for value in &s.values {
                    take(*value);
                }
            }
        }
        ChartData::StateTimeline(series) => take(series.len() as f64),
        ChartData::Bars(series) | ChartData::Areas(series) => {
            if matches!(data, ChartData::Areas(_)) && matches!(series_layout, SeriesLayout::Stacked)
            {
                let (lo, hi) = stacked_area_extent(series, from_ms, to_ms);
                take(lo);
                take(hi);
            } else if matches!(
                series_layout,
                SeriesLayout::Stacked | SeriesLayout::StackedPercent
            ) {
                let n = series.iter().map(|s| s.ys.len()).max().unwrap_or(0);
                for i in 0..n {
                    let mut positive = 0.0;
                    let mut negative = 0.0;
                    let mut positive_total = 0.0;
                    let mut negative_total = 0.0;
                    for s in series {
                        let (Some(x), Some(value)) = (s.xs.get(i), s.ys.get(i)) else {
                            continue;
                        };
                        if *x < from_ms || *x > to_ms || !value.is_finite() {
                            continue;
                        }
                        if *value >= 0.0 {
                            positive_total = crate::series::finite_add(positive_total, *value);
                        } else {
                            negative_total = crate::series::finite_add(negative_total, value.abs());
                        }
                    }
                    for s in series {
                        let (Some(x), Some(raw)) = (s.xs.get(i), s.ys.get(i)) else {
                            continue;
                        };
                        if *x < from_ms || *x > to_ms || !raw.is_finite() {
                            continue;
                        }
                        let value = if matches!(series_layout, SeriesLayout::StackedPercent) {
                            if *raw >= 0.0 && positive_total > 0.0 {
                                *raw / positive_total * 100.0
                            } else if *raw < 0.0 && negative_total > 0.0 {
                                -*raw / negative_total * -100.0
                            } else {
                                0.0
                            }
                        } else {
                            *raw
                        };
                        if value >= 0.0 {
                            positive = crate::series::finite_add(positive, value);
                        } else {
                            negative = crate::series::finite_add(negative, value);
                        }
                    }
                    take(positive);
                    take(negative);
                }
            } else {
                for s in series {
                    let extent = if matches!(data, ChartData::Bars(_)) {
                        s.sample_positive_y_extent(from_ms, to_ms)
                    } else {
                        s.positive_y_extent(from_ms, to_ms)
                    };
                    if let Some((_, hi)) = extent {
                        take(hi);
                    }
                    if let Some((lo, _)) = extent {
                        take(lo);
                    }
                }
            }
        }
        _ => {
            for s in data.point_series() {
                if let Some((lo, hi)) = s.positive_y_extent(from_ms, to_ms) {
                    take(lo);
                    take(hi);
                }
            }
        }
    }
    (lo, hi)
}

/// Resolve the x domain from the axis kind, viewport, and data.
///
/// `Time` uses the viewport; `Linear` uses explicit bounds where given and
/// falls back to data extents (histogram bucket edges, hbar value range —
/// always including 0 — or series/tick x extents); `Category` spans one unit
/// per label. Degenerate domains are widened so scales stay invertible.
pub fn resolve_x_domain(
    x_axis: &crate::spec::XAxisKind,
    data: &crate::spec::ChartData,
    from_ms: f64,
    to_ms: f64,
) -> (f64, f64) {
    use crate::spec::{ChartData, XAxisKind};
    match x_axis {
        XAxisKind::Time => ordered_domain(from_ms, to_ms),
        XAxisKind::Category { labels } => (0.0, (labels.len() as f64).max(1.0)),
        XAxisKind::Linear { min, max } => {
            let auto = || -> (f64, f64) {
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                match data {
                    ChartData::Histogram(series) => {
                        for s in series {
                            for edge in &s.buckets {
                                if edge.is_finite() {
                                    lo = lo.min(*edge);
                                    hi = hi.max(*edge);
                                }
                            }
                        }
                    }
                    ChartData::HBar(series) => {
                        for s in series {
                            if let Some((vmin, vmax)) = s.y_extent() {
                                lo = lo.min(vmin);
                                hi = hi.max(vmax);
                            }
                        }
                    }
                    ChartData::Ohlc(series) => {
                        for s in series {
                            for tick in &s.ticks {
                                if tick.ts.is_finite() {
                                    lo = lo.min(tick.ts);
                                    hi = hi.max(tick.ts);
                                }
                            }
                        }
                    }
                    ChartData::Band(series) => {
                        for s in series {
                            for x in &s.xs {
                                if x.is_finite() {
                                    lo = lo.min(*x);
                                    hi = hi.max(*x);
                                }
                            }
                        }
                    }
                    ChartData::StateTimeline(series) => {
                        for s in series {
                            for seg in &s.segments {
                                lo = lo.min(seg.start_ms);
                                hi = hi.max(seg.end_ms);
                            }
                        }
                    }
                    _ => {
                        for s in data.point_series() {
                            for x in &s.xs {
                                if x.is_finite() {
                                    lo = lo.min(*x);
                                    hi = hi.max(*x);
                                }
                            }
                        }
                    }
                }
                if !lo.is_finite() || !hi.is_finite() {
                    (0.0, 1.0)
                } else {
                    (lo, hi)
                }
            };
            let (auto_lo, auto_hi) = if min.is_none() || max.is_none() {
                auto()
            } else {
                (0.0, 1.0)
            };
            let lo = min.unwrap_or(auto_lo);
            let hi = max.unwrap_or(auto_hi);
            ordered_domain(lo, hi)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::series::SeriesData;

    #[test]
    fn layout_inverts_px_to_ts() {
        let l = ChartLayout::compute(600.0, 200.0, 0.0, 60_000.0, 0.0, 100.0);
        let mid_px = l.plot.x + l.plot.w / 2.0;
        assert!((l.ts_at(mid_px) - 30_000.0).abs() < 1.0);
        // Clamps outside the plot.
        assert_eq!(l.ts_at(0.0), 0.0);
    }

    #[test]
    fn y_domain_pads_and_anchors() {
        let s = SeriesData {
            name: "s".into(),
            xs: vec![0.0, 1.0, 2.0],
            ys: vec![10.0, 20.0, 30.0],
            color: None,
        };
        let (lo, hi) = y_domain(std::slice::from_ref(&s), 0.0, 2.0, None, None, false);
        assert!(lo < 10.0 && hi > 30.0);
        let (lo, _) = y_domain(&[s], 0.0, 2.0, None, None, true);
        assert_eq!(lo, 0.0);
    }

    #[test]
    fn y_domain_flat_series() {
        let s = SeriesData {
            name: "s".into(),
            xs: vec![0.0, 1.0],
            ys: vec![5.0, 5.0],
            color: None,
        };
        let (lo, hi) = y_domain(&[s], 0.0, 1.0, None, None, false);
        assert!(lo < 5.0 && hi > 5.0);
    }

    #[test]
    fn x_domain_time_uses_viewport() {
        use crate::spec::{ChartData, XAxisKind};
        let d = ChartData::Lines(vec![]);
        assert_eq!(
            resolve_x_domain(&XAxisKind::Time, &d, 100.0, 200.0),
            (100.0, 200.0)
        );
    }

    #[test]
    fn x_domain_linear_auto_from_histogram_buckets() {
        use crate::series::HistogramSeries;
        use crate::spec::{ChartData, XAxisKind};
        let d = ChartData::Histogram(vec![HistogramSeries {
            name: "h".into(),
            buckets: vec![10.0, 20.0, 30.0, 40.0],
            counts: vec![1.0, 2.0, 3.0],
            color: None,
            cumulative: false,
        }]);
        let ax = XAxisKind::Linear {
            min: None,
            max: None,
        };
        // C2 fix: histogram x spans its bucket edges, not the time viewport.
        assert_eq!(resolve_x_domain(&ax, &d, 0.0, 1_000_000.0), (10.0, 40.0));
    }

    #[test]
    fn x_domain_linear_auto_from_hbar_includes_zero() {
        use crate::series::HBarSeries;
        use crate::spec::{ChartData, XAxisKind};
        let d = ChartData::HBar(vec![HBarSeries {
            name: "b".into(),
            categories: vec!["a".into(), "b".into()],
            values: vec![5.0, 9.0],
            color: None,
        }]);
        let ax = XAxisKind::Linear {
            min: None,
            max: None,
        };
        let (lo, hi) = resolve_x_domain(&ax, &d, 0.0, 1.0);
        assert_eq!(lo, 0.0);
        assert_eq!(hi, 9.0);
    }

    #[test]
    fn x_domain_category_spans_label_count() {
        use crate::spec::{ChartData, XAxisKind};
        let ax = XAxisKind::Category {
            labels: vec!["a".into(), "b".into(), "c".into()],
        };
        let d = ChartData::Lines(vec![]);
        assert_eq!(resolve_x_domain(&ax, &d, 0.0, 1.0), (0.0, 3.0));
    }

    #[test]
    fn x_domain_linear_explicit_pins_win() {
        use crate::spec::{ChartData, XAxisKind};
        let ax = XAxisKind::Linear {
            min: Some(-5.0),
            max: Some(5.0),
        };
        let d = ChartData::Lines(vec![]);
        assert_eq!(resolve_x_domain(&ax, &d, 0.0, 1.0), (-5.0, 5.0));
    }

    #[test]
    fn data_y_domain_ohlc_uses_low_high() {
        use crate::series::{OhlcSeriesData, OhlcTick};
        use crate::spec::ChartData;
        let d = ChartData::Ohlc(vec![OhlcSeriesData {
            name: "o".into(),
            ticks: vec![
                OhlcTick {
                    ts: 0.0,
                    open: 10.0,
                    high: 15.0,
                    low: 8.0,
                    close: 12.0,
                },
                OhlcTick {
                    ts: 1.0,
                    open: 12.0,
                    high: 20.0,
                    low: 11.0,
                    close: 19.0,
                },
            ],
            color: None,
        }]);
        let (lo, hi) = data_y_domain(&d, 0.0, 10.0, None, None, false, Default::default());
        // Padded 5% beyond [8, 20] — previously OHLC got the empty-series
        // default (0, 1) and candles rendered off-scale.
        assert!(lo < 8.0 && lo > 7.0, "lo = {lo}");
        assert!(hi > 20.0 && hi < 21.0, "hi = {hi}");
    }

    #[test]
    fn data_y_domain_histogram_cumulative_aware() {
        use crate::series::HistogramSeries;
        use crate::spec::ChartData;
        let d = ChartData::Histogram(vec![HistogramSeries {
            name: "h".into(),
            buckets: vec![0.0, 1.0, 2.0, 3.0],
            counts: vec![3.0, 1.0, 2.0],
            color: None,
            cumulative: true,
        }]);
        let (lo, hi) = data_y_domain(&d, 0.0, 10.0, None, None, true, Default::default());
        assert_eq!(lo, 0.0);
        // Cumulative total is 6; domain must cover it (plus padding).
        assert!(hi >= 6.0, "hi = {hi}");
    }

    #[test]
    fn data_y_domain_stacked_bars_covers_column_sums() {
        use crate::spec::{ChartData, SeriesLayout};
        let mk = |ys: Vec<f64>| SeriesData {
            name: "s".into(),
            xs: vec![0.0, 1.0, 2.0],
            ys,
            color: None,
        };
        let d = ChartData::Bars(vec![mk(vec![5.0, 8.0, 3.0]), mk(vec![4.0, 7.0, 2.0])]);
        let (lo, hi) = data_y_domain(&d, 0.0, 2.0, None, None, true, SeriesLayout::Stacked);
        assert_eq!(lo, 0.0);
        // Tallest column stacks to 15 — the domain must cover the stack, not
        // just the max single series (8).
        assert!(hi >= 15.0, "hi = {hi}");

        let (_, hi_grouped) = data_y_domain(&d, 0.0, 2.0, None, None, true, SeriesLayout::Grouped);
        assert!(
            hi_grouped < 15.0,
            "grouped should not sum columns: {hi_grouped}"
        );

        let (lo_pct, hi_pct) =
            data_y_domain(&d, 0.0, 2.0, None, None, true, SeriesLayout::StackedPercent);
        assert_eq!(lo_pct, 0.0);
        assert!((100.0..=110.0).contains(&hi_pct));
    }

    #[test]
    fn negative_stacks_keep_both_sides_of_the_zero_baseline_in_domain() {
        use crate::spec::{ChartData, SeriesLayout};
        let series = |name: &str, ys| SeriesData {
            name: name.into(),
            xs: vec![0.0, 1.0],
            ys,
            color: None,
        };
        let data = ChartData::Bars(vec![
            series("positive-negative", vec![4.0, -3.0]),
            series("positive-negative-2", vec![6.0, -5.0]),
        ]);

        let (lo, hi) = data_y_domain(&data, 0.0, 1.0, None, None, true, SeriesLayout::Stacked);
        assert!(lo <= -8.0, "negative stack clipped: {lo}");
        assert!(hi >= 10.0, "positive stack clipped: {hi}");

        let (lo_pct, hi_pct) = data_y_domain(
            &data,
            0.0,
            1.0,
            None,
            None,
            true,
            SeriesLayout::StackedPercent,
        );
        assert!(
            lo_pct <= -100.0 && hi_pct >= 100.0,
            "percent domain {lo_pct}..{hi_pct}"
        );
    }

    #[test]
    fn layout_normalizes_invalid_dimensions_and_reversed_domains() {
        let layout = ChartLayout::compute_with_margins(
            f64::NAN,
            -10.0,
            20.0,
            -20.0,
            5.0,
            -5.0,
            100.0,
            100.0,
            100.0,
            100.0,
        );
        assert!(layout.plot.w >= 1.0 && layout.plot.h >= 1.0);
        assert!(layout.x_scale.d0 < layout.x_scale.d1);
        assert!(layout.y_scale.d0 < layout.y_scale.d1);
        assert!(layout.ts_at(layout.plot.x).is_finite());
        assert!(layout.ts_at(layout.plot.x + layout.plot.w).is_finite());
    }

    #[test]
    fn extreme_linear_domains_remain_mappable() {
        let scale = LinearScale::new(-f64::MAX, f64::MAX, 0.0, 100.0);
        assert!(scale.to_px(0.0).is_finite());
        assert!(scale.from_px(50.0).is_finite());
        let layout = ChartLayout::compute(100.0, 100.0, -f64::MAX, f64::MAX, -f64::MAX, f64::MAX);
        assert!(layout.ts_at(50.0).is_finite());
    }

    #[test]
    fn subnormal_log_domain_is_clamped() {
        let layout = ChartLayout::compute_scaled(
            100.0,
            100.0,
            0.0,
            1.0,
            f64::from_bits(1),
            f64::from_bits(1),
            0.0,
            0.0,
            0.0,
            0.0,
            ScaleKind::Log,
        );
        assert!(layout.y_domain().0.is_finite());
        assert!(layout.y_domain().0 > 0.0);
    }

    #[test]
    fn stacked_extremes_saturate_instead_of_losing_the_domain() {
        let data = crate::spec::ChartData::Bars(vec![
            crate::series::SeriesData {
                name: "a".into(),
                xs: vec![0.0],
                ys: vec![f64::MAX],
                color: None,
            },
            crate::series::SeriesData {
                name: "b".into(),
                xs: vec![0.0],
                ys: vec![f64::MAX],
                color: None,
            },
        ]);
        let mut spec = crate::spec::ChartSpec::bars(crate::units::Unit::None);
        spec.layout = crate::spec::SeriesLayout::Stacked;
        let (lo, hi) = data_y_domain(&data, 0.0, 1.0, None, None, true, spec.layout);
        assert!(lo.is_finite() && hi.is_finite());
        assert_eq!(hi, f64::MAX);
    }

    #[test]
    fn data_y_domain_band_uses_envelope() {
        use crate::series::BandSeries;
        use crate::spec::ChartData;
        let d = ChartData::Band(vec![BandSeries {
            name: "p".into(),
            xs: vec![0.0, 1.0],
            center: vec![5.0, 5.0],
            lower: vec![1.0, 2.0],
            upper: vec![9.0, 8.0],
            color: None,
        }]);
        let (lo, hi) = data_y_domain(&d, 0.0, 1.0, None, None, false, Default::default());
        assert!(
            lo < 1.0 && hi > 9.0,
            "domain {lo}..{hi} must cover the band"
        );
    }

    #[test]
    fn log_layout_maps_decades_to_even_pixel_bands() {
        let layout = ChartLayout::compute_scaled(
            400.0,
            200.0,
            0.0,
            10.0,
            1.0,
            1000.0,
            40.0,
            8.0,
            8.0,
            20.0,
            ScaleKind::Log,
        );
        let (lo, hi) = layout.y_domain();
        assert!((lo - 1.0).abs() < 1e-9, "{lo}");
        assert!((hi - 1000.0).abs() < 1e-9, "{hi}");
        // Each decade must occupy the same pixel height.
        let d1 = layout.y_px(1.0) - layout.y_px(10.0);
        let d2 = layout.y_px(10.0) - layout.y_px(100.0);
        let d3 = layout.y_px(100.0) - layout.y_px(1000.0);
        assert!((d1 - d2).abs() < 1e-6 && (d2 - d3).abs() < 1e-6);
        assert!((layout.y_at(layout.y_px(37.0)) - 37.0).abs() < 1e-6);
    }

    #[test]
    fn log_axis_gaps_non_positive_values_but_keeps_bars_on_the_baseline() {
        let layout = ChartLayout::compute_scaled(
            400.0,
            200.0,
            0.0,
            10.0,
            1.0,
            100.0,
            40.0,
            8.0,
            8.0,
            20.0,
            ScaleKind::Log,
        );
        assert!(layout.y_px(0.0).is_nan());
        assert!(layout.y_px(-3.0).is_nan());
        // Bar and stack edges collapse onto the baseline instead of vanishing.
        assert_eq!(layout.y_edge_px(0.0), layout.y_baseline_px());
        assert_eq!(layout.y_baseline_px(), layout.plot.y + layout.plot.h);
    }

    #[test]
    fn linear_baseline_clamps_zero_into_the_domain() {
        let inside = ChartLayout::compute(400.0, 200.0, 0.0, 10.0, -5.0, 5.0);
        assert!((inside.y_baseline_px() - inside.y_px(0.0)).abs() < 1e-9);
        // A domain entirely above zero rests on its own floor.
        let above = ChartLayout::compute(400.0, 200.0, 0.0, 10.0, 10.0, 20.0);
        assert!((above.y_baseline_px() - above.y_px(above.y_scale.d0)).abs() < 1e-9);
    }

    #[test]
    fn zero_anchor_keeps_constant_negative_data_visible() {
        let data = vec![SeriesData {
            name: "negative".into(),
            xs: vec![0.0, 1.0],
            ys: vec![-10.0, -10.0],
            color: None,
        }];
        let (lo, hi) = y_domain(&data, 0.0, 1.0, None, None, true);
        assert!(lo < -10.0 && hi >= 0.0, "zero-anchored domain {lo}..{hi}");
    }

    #[test]
    fn log_domain_uses_positive_extent_and_ignores_non_positive_pins() {
        use crate::series::SeriesData;
        use crate::spec::{ChartData, SeriesLayout};
        let data = ChartData::Lines(vec![SeriesData {
            name: "latency".into(),
            xs: vec![0.0, 1.0, 2.0, 3.0],
            // The zero would collapse a naive log domain.
            ys: vec![0.0, 2.0, 800.0, f64::NAN],
            color: None,
        }]);
        let (lo, hi) = data_y_domain_scaled(
            &data,
            0.0,
            10.0,
            Some(0.0),
            None,
            true,
            SeriesLayout::Stacked,
            ScaleKind::Log,
        );
        assert!(lo > 0.0 && lo < 2.0, "{lo}");
        assert!(hi > 800.0, "{hi}");
    }

    #[test]
    fn log_domain_without_positive_data_stays_drawable() {
        use crate::series::SeriesData;
        use crate::spec::{ChartData, SeriesLayout};
        let data = ChartData::Lines(vec![SeriesData {
            name: "all-zero".into(),
            xs: vec![0.0, 1.0],
            ys: vec![0.0, -4.0],
            color: None,
        }]);
        let (lo, hi) = data_y_domain_scaled(
            &data,
            0.0,
            10.0,
            None,
            None,
            false,
            SeriesLayout::Stacked,
            ScaleKind::Log,
        );
        assert!(lo > 0.0 && hi > lo, "{lo}..{hi}");
    }

    #[test]
    fn log_domain_covers_composed_stack_and_percent_geometry() {
        use crate::scale::ScaleKind;
        use crate::spec::{ChartData, SeriesLayout};
        let series = (0..20)
            .map(|index| SeriesData {
                name: format!("s{index}"),
                xs: vec![0.0, 1.0],
                ys: vec![10.0, 10.0],
                color: None,
            })
            .collect::<Vec<_>>();
        let data = ChartData::Areas(series.clone());
        let (_, hi) = data_y_domain_scaled(
            &data,
            0.0,
            1.0,
            None,
            None,
            true,
            SeriesLayout::Stacked,
            ScaleKind::Log,
        );
        assert!(hi > 200.0, "stacked log domain clips the top: {hi}");

        let percent = ChartData::Bars(series);
        let (lo_pct, hi_pct) = data_y_domain_scaled(
            &percent,
            0.0,
            1.0,
            None,
            None,
            true,
            SeriesLayout::StackedPercent,
            ScaleKind::Log,
        );
        assert!(
            lo_pct < 100.0 && hi_pct > 100.0,
            "percent log domain {lo_pct}..{hi_pct}"
        );
    }

    #[test]
    fn stacked_area_domain_includes_a_crossing_segment() {
        use crate::spec::{ChartData, SeriesLayout};
        let data = ChartData::Areas(vec![
            SeriesData {
                name: "a".into(),
                xs: vec![0.0, 10.0],
                ys: vec![100.0, 200.0],
                color: None,
            },
            SeriesData {
                name: "b".into(),
                xs: vec![0.0, 10.0],
                ys: vec![20.0, 40.0],
                color: None,
            },
        ]);
        let (lo, hi) = data_y_domain(&data, 4.0, 6.0, None, None, true, SeriesLayout::Stacked);
        let (raw_lo, raw_hi) = stacked_area_extent(data.point_series(), 4.0, 6.0);
        assert_eq!((raw_lo, raw_hi), (0.0, 240.0));
        assert!(
            lo <= 0.0 && hi > 120.0 && hi < 300.0,
            "stacked area {lo}..{hi}"
        );
    }

    #[test]
    fn stacked_area_extent_handles_a_full_100k_point_window() {
        use crate::spec::{ChartData, SeriesLayout};
        let xs: Vec<f64> = (0..100_000).map(|index| index as f64).collect();
        let make = |value: f64| SeriesData {
            name: "large".into(),
            xs: xs.clone(),
            ys: vec![value; xs.len()],
            color: None,
        };
        let data = ChartData::Areas(vec![make(1.0), make(2.0)]);
        let (lo, hi) = data_y_domain(
            &data,
            0.0,
            99_999.0,
            None,
            None,
            true,
            SeriesLayout::Stacked,
        );
        assert_eq!(lo, 0.0);
        assert!(hi >= 3.0, "full-window stack lost its top: {hi}");
    }
}
