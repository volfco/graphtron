use crate::color::heatmap_color_for_value;
use crate::layout::ChartLayout;
use crate::series::{
    BandSeries, HBarSeries, HistogramSeries, OhlcSeriesData, OhlcTick, SeriesData,
    StateTimelineSeries, ohlc_series_color, series_color,
};
use crate::spec::{ChartData, ChartSpec, XAxisKind};
use crate::ticks::format_ts_full;
use crate::units::Unit;

#[derive(Debug, Clone, PartialEq)]
pub struct HoverValue {
    /// Exact partition/cell geometry retained for O(1) overlay highlighting.
    pub shape: Option<crate::partition::Shape>,
    pub series: usize,
    pub name: String,
    pub y: f64,
    pub px: f64,
    pub py: f64,
    /// Explicit color for chart kinds that are not backed by `SeriesData`.
    pub color: Option<u32>,
    /// Preformatted semantic value (for example a state label). When absent,
    /// tooltips format `y` with the chart's y unit.
    pub formatted_value: Option<String>,
    pub extra: Vec<(String, f64)>,
    /// Non-numeric detail rows (state, category, and other metadata).
    pub extra_text: Vec<(String, String)>,
    /// The largest value among the values returned for this hover target.
    pub highlighted: bool,
}

impl HoverValue {
    pub fn new(series: usize, name: String, y: f64, px: f64, py: f64) -> Self {
        Self {
            shape: None,
            series,
            name,
            y,
            px,
            py,
            color: None,
            formatted_value: None,
            extra: vec![],
            extra_text: vec![],
            highlighted: false,
        }
    }

    pub fn with_color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_formatted_value(mut self, value: impl Into<String>) -> Self {
        self.formatted_value = Some(value.into());
        self
    }

    /// Return the plotted pixel y used by guides. `y` remains the source data
    /// value shown in a tooltip; the explicit suffix prevents callers from
    /// mistaking this coordinate for a data-space value.
    pub fn plotted_y_px(&self) -> f64 {
        self.py
    }

    /// Recover the x coordinate represented by this row's plotted geometry.
    /// For point/band/OHLC rows this is the source sample x; grouped bars may
    /// return the slot center because their `px` is deliberately shifted for
    /// the grouped layout. The exact pointer coordinate is
    /// [`HoverInfo::ts`].
    pub fn plotted_x(&self, layout: &ChartLayout) -> f64 {
        layout.x_scale.from_px(self.px)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HoverInfo {
    pub ts: f64,
    /// Fully formatted x/category label. Older hit-test helpers leave this as
    /// `None`, in which case the tooltip treats `ts` as epoch milliseconds.
    pub header: Option<String>,
    pub values: Vec<HoverValue>,
}

/// Prefix sums prepared when histogram data changes. Pointer moves can then
/// read cumulative values without rebuilding a prefix for each series and
/// marker geometry. The cache is intentionally separate from
/// `HistogramSeries` so the public data structs remain literal-compatible.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HistogramHoverCache {
    prefixes: Vec<Vec<f64>>,
}

pub fn prepare_histogram_hover(series: &[HistogramSeries]) -> HistogramHoverCache {
    HistogramHoverCache {
        prefixes: series
            .iter()
            .map(|series| {
                let mut total = 0.0;
                series
                    .counts
                    .iter()
                    .map(|count| {
                        if count.is_finite() {
                            total += count;
                        }
                        total
                    })
                    .collect()
            })
            .collect(),
    }
}

fn cached_plot_count(
    series: &HistogramSeries,
    index: usize,
    cache: Option<&HistogramHoverCache>,
    series_index: usize,
) -> Option<f64> {
    let value = *series.counts.get(index)?;
    if !series.cumulative {
        return Some(value);
    }
    cache
        .and_then(|cache| cache.prefixes.get(series_index))
        .and_then(|prefix| prefix.get(index).copied())
        .or_else(|| series.plot_count_at(index))
}

impl HoverInfo {
    fn finish(mut self) -> Option<Self> {
        let highlighted = self
            .values
            .iter()
            .enumerate()
            .filter(|(_, value)| value.y.is_finite())
            .max_by(|(_, a), (_, b)| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(index, _)| index)
            .or_else(|| (!self.values.is_empty()).then_some(0));
        if let Some(index) = highlighted {
            self.values[index].highlighted = true;
            Some(self)
        } else {
            None
        }
    }
}

/// Annotate a live hover with per-series deltas against a frozen reference,
/// turning the freeze cursor into a measuring tape ("how much did p99 move
/// between the deploy and now?").
///
/// Series are matched by index, so both hovers must come from the same chart.
/// A series missing from the reference, or one whose value is not finite, is
/// left untouched rather than annotated with a fabricated zero. The delta row
/// is text so it can carry a sign and a percentage the y unit cannot express.
pub fn annotate_delta(hover: &mut HoverInfo, frozen: &HoverInfo, unit: Unit) {
    for value in &mut hover.values {
        let Some(reference) = frozen
            .values
            .iter()
            .find(|candidate| candidate.series == value.series)
        else {
            continue;
        };
        if !value.y.is_finite() || !reference.y.is_finite() {
            continue;
        }
        let delta = value.y - reference.y;
        let sign = if delta >= 0.0 { "+" } else { "-" };
        let mut text = format!("{sign}{}", unit.format(delta.abs()));
        // A percentage against a zero baseline is undefined, not infinite.
        if reference.y != 0.0 {
            let percent = delta / reference.y.abs() * 100.0;
            text.push_str(&format!(" ({sign}{:.1}%)", percent.abs()));
        }
        value.extra_text.push(("Δ".into(), text));
    }
}

pub fn hit_test(layout: &ChartLayout, series: &[SeriesData], x_px: f64) -> Option<HoverInfo> {
    let ts = layout.ts_at(x_px);
    let mut values = Vec::new();
    for (idx, s) in series.iter().enumerate() {
        let n = s.xs.len().min(s.ys.len());
        if let Some(i) = nearest_index(&s.xs[..n], ts) {
            let y = s.ys[i];
            let py = layout.y_px(y);
            if y.is_finite() && py.is_finite() {
                values.push(
                    HoverValue::new(idx, s.name.clone(), y, layout.x_scale.to_px(s.xs[i]), py)
                        .with_color(series_color(s, idx)),
                );
            }
        }
    }
    HoverInfo {
        ts,
        header: None,
        values,
    }
    .finish()
}

/// Hit-test a step-after series against the geometry users see. Between two
/// samples the horizontal segment retains the left sample's value; exactly at
/// a transition the vertical edge has reached the right sample, so the right
/// value wins. Invalid log values terminate the visible step and are skipped.
fn hit_test_step(layout: &ChartLayout, series: &[SeriesData], mouse_x: f64) -> Option<HoverInfo> {
    let ts = layout.ts_at(mouse_x);
    let mut values = Vec::new();
    for (idx, s) in series.iter().enumerate() {
        let n = s.xs.len().min(s.ys.len());
        if n == 0 {
            continue;
        }
        let i = {
            let next = s.xs[..n].partition_point(|x| *x <= ts);
            next.saturating_sub(1).min(n - 1)
        };
        let y = s.ys[i];
        let px = layout.x_scale.to_px(s.xs[i]);
        let py = layout.y_px(y);
        if !s.xs[i].is_finite() || !y.is_finite() || !px.is_finite() || !py.is_finite() {
            continue;
        }
        values
            .push(HoverValue::new(idx, s.name.clone(), y, px, py).with_color(series_color(s, idx)));
    }
    HoverInfo {
        ts,
        header: None,
        values,
    }
    .finish()
}

pub fn hit_test_ohlc(
    layout: &ChartLayout,
    series: &[OhlcSeriesData],
    x_px: f64,
) -> Option<HoverInfo> {
    let ts = layout.ts_at(x_px);
    let mut values = Vec::new();
    for (idx, s) in series.iter().enumerate() {
        if s.ticks.is_empty() {
            continue;
        }
        let Some(i) = nearest_index_ticks(&s.ticks, ts) else {
            continue;
        };
        let t = &s.ticks[i];
        // Validation reports malformed OHLC input at ingestion time, but a
        // pointer event can still race that result. Never let an invalid candle
        // create NaN overlay geometry or a contradictory O/H/L/C tooltip.
        if !ohlc_tick_is_drawable(t) {
            continue;
        }
        let px = layout.x_scale.to_px(t.ts);
        let py = layout.y_px(t.close);
        if !py.is_finite() {
            continue;
        }
        let mut v = HoverValue::new(idx, s.name.clone(), t.close, px, py)
            .with_color(ohlc_series_color(s, idx));
        v.extra = vec![
            ("O".into(), t.open),
            ("H".into(), t.high),
            ("L".into(), t.low),
            ("C".into(), t.close),
        ];
        values.push(v);
    }
    HoverInfo {
        ts,
        header: None,
        values,
    }
    .finish()
}

pub fn hit_test_scatter(
    layout: &ChartLayout,
    series: &[SeriesData],
    mouse_x: f64,
    mouse_y: f64,
    radius_px: f64,
) -> Option<HoverInfo> {
    let ts = layout.ts_at(mouse_x);
    let mut values = Vec::new();
    let r2 = radius_px * radius_px;

    for (idx, s) in series.iter().enumerate() {
        let mut best_dist = f64::INFINITY;
        let mut best_i = None;
        // SeriesData promises sorted x values, so only points whose x pixels
        // can fall inside the hover radius need an expensive y/distance check.
        // Keep the intersection explicit: malformed struct-of-arrays input is
        // diagnosed by ChartData::validate, but pointer handling must never
        // index past the shorter side while that diagnosis is in flight.
        let n = s.xs.len().min(s.ys.len());
        let (start, end) = scatter_x_window(layout, &s.xs[..n], mouse_x, radius_px);

        for (offset, (x, y)) in s.xs[start..end].iter().zip(&s.ys[start..end]).enumerate() {
            if !y.is_finite() {
                continue;
            }
            let px = layout.x_scale.to_px(*x);
            let py = layout.y_px(*y);
            if !px.is_finite() || !py.is_finite() {
                continue;
            }
            let dx = px - mouse_x;
            let dy = py - mouse_y;
            let d2 = dx * dx + dy * dy;
            if d2 < r2 && d2 < best_dist {
                best_dist = d2;
                best_i = Some(start + offset);
            }
        }

        if let Some(i) = best_i {
            let y = s.ys[i];
            values.push(
                HoverValue::new(
                    idx,
                    s.name.clone(),
                    y,
                    layout.x_scale.to_px(s.xs[i]),
                    layout.y_px(y),
                )
                .with_color(series_color(s, idx)),
            );
        }
    }

    HoverInfo {
        ts,
        header: None,
        values,
    }
    .finish()
}

/// The slice of sorted x values whose projected pixels could lie within a
/// scatter hover radius. `partition_point` keeps the hot path logarithmic;
/// invalid pointer geometry falls back to the safe whole slice.
fn scatter_x_window(
    layout: &ChartLayout,
    xs: &[f64],
    mouse_x: f64,
    radius_px: f64,
) -> (usize, usize) {
    if xs.is_empty() || !mouse_x.is_finite() || !radius_px.is_finite() || radius_px < 0.0 {
        return (0, xs.len());
    }
    let left = layout.ts_at(mouse_x - radius_px);
    let right = layout.ts_at(mouse_x + radius_px);
    if !left.is_finite() || !right.is_finite() {
        return (0, xs.len());
    }
    let (lo, hi) = if left <= right {
        (left, right)
    } else {
        (right, left)
    };
    let start = xs.partition_point(|x| *x < lo);
    let end = xs.partition_point(|x| *x <= hi);
    (start.min(xs.len()), end.clamp(start, xs.len()))
}

/// Hit-test any chart kind with one API.
///
/// Unlike the legacy point-series helper, this understands bucket ranges,
/// OHLC details, band bounds, heatmap/state rows, and horizontal-bar
/// categories. `mouse_y` is used for row-oriented and scatter charts.
pub fn hit_test_chart(
    layout: &ChartLayout,
    spec: &ChartSpec,
    data: &ChartData,
    mouse_x: f64,
    mouse_y: f64,
) -> Option<HoverInfo> {
    hit_test_chart_with_cache(layout, spec, data, mouse_x, mouse_y, None)
}

pub fn hit_test_chart_with_cache(
    layout: &ChartLayout,
    spec: &ChartSpec,
    data: &ChartData,
    mouse_x: f64,
    mouse_y: f64,
    histogram_cache: Option<&HistogramHoverCache>,
) -> Option<HoverInfo> {
    let mut info = match data {
        ChartData::Pie(items) | ChartData::Treemap(items) | ChartData::HostMap(items) => {
            crate::partition::hit_test(
                layout,
                data,
                &crate::partition::geometry(data.kind(), items, layout.plot),
                mouse_x,
                mouse_y,
            )
        }
        ChartData::Points(series) | ChartData::Scatter(series) => {
            hit_test_scatter(layout, series, mouse_x, mouse_y, 18.0)
        }
        ChartData::Heatmap(series) => hit_test_heatmap(layout, spec, series, mouse_x, mouse_y),
        ChartData::Ohlc(series) => hit_test_ohlc(layout, series, mouse_x),
        ChartData::Histogram(series) => {
            hit_test_histogram(layout, spec, series, mouse_x, histogram_cache)
        }
        ChartData::Bars(series) => {
            hit_test_composed_series(layout, series, mouse_x, spec.layout, true)
        }
        ChartData::Areas(series) => {
            hit_test_composed_series(layout, series, mouse_x, spec.layout, false)
        }
        ChartData::HBar(series) => hit_test_hbar(layout, spec, series, mouse_x, mouse_y),
        ChartData::StateTimeline(series) => {
            hit_test_state_timeline(layout, series, mouse_x, mouse_y)
        }
        ChartData::Band(series) => hit_test_band(layout, series, mouse_x),
        ChartData::Step(series) => hit_test_step(layout, series, mouse_x),
        _ => hit_test(layout, data.point_series(), mouse_x),
    }?;

    if info.header.is_none() {
        let source_x = source_x_for_hover(data, info.ts, &info).unwrap_or(info.ts);
        info.header = Some(x_header(spec, source_x));
    }
    Some(info)
}

fn x_header(spec: &ChartSpec, x: f64) -> String {
    match &spec.x_axis {
        XAxisKind::Time => format_ts_full(x as i64),
        XAxisKind::Linear { .. } => spec.x_unit.format(x),
        XAxisKind::Category { labels } => {
            let index = x.floor().max(0.0) as usize;
            labels
                .get(index)
                .cloned()
                .unwrap_or_else(|| spec.x_unit.format(x))
        }
    }
}

/// Resolve the source x represented by a hover independently from the guide
/// pixel. Grouped bars shift `HoverValue::px` into a slot center, so inverting
/// that pixel would produce a false timestamp in the tooltip header.
fn source_x_for_hover(data: &ChartData, pointer_ts: f64, info: &HoverInfo) -> Option<f64> {
    let series = match data {
        ChartData::Lines(series)
        | ChartData::Areas(series)
        | ChartData::Bars(series)
        | ChartData::Points(series)
        | ChartData::Scatter(series)
        | ChartData::Heatmap(series)
        | ChartData::Step(series) => Some(series),
        _ => None,
    }?;
    let target = pointer_ts;
    let first = info.values.first()?;
    let s = series.get(first.series)?;
    let n = s.xs.len().min(s.ys.len());
    let index = if matches!(data, ChartData::Step(_)) {
        s.xs[..n]
            .partition_point(|x| *x <= target)
            .saturating_sub(1)
            .min(n.saturating_sub(1))
    } else {
        nearest_index(&s.xs[..n], target)?
    };
    s.xs.get(index).copied().filter(|x| x.is_finite())
}

fn hit_test_histogram(
    layout: &ChartLayout,
    spec: &ChartSpec,
    series: &[HistogramSeries],
    mouse_x: f64,
    histogram_cache: Option<&HistogramHoverCache>,
) -> Option<HoverInfo> {
    let x = layout.ts_at(mouse_x);
    let mut values = Vec::new();
    let mut header = None;
    let stacked = matches!(
        spec.layout,
        crate::spec::SeriesLayout::Stacked | crate::spec::SeriesLayout::StackedPercent
    );
    let stack_bucket = stacked
        .then(|| series.iter().find_map(|s| histogram_bucket(&s.buckets, x)))
        .flatten();
    let stack_values = stack_bucket.map(|bucket| {
        series
            .iter()
            .enumerate()
            .map(|(si, series)| cached_plot_count(series, bucket, histogram_cache, si))
            .collect::<Vec<_>>()
    });
    let stack_tops = stack_values
        .as_deref()
        .map(|values| histogram_stack_tops(values, spec.layout));
    for (si, s) in series.iter().enumerate() {
        let n = s.counts.len().min(s.buckets.len().saturating_sub(1));
        let Some(edges) = s.buckets.get(..n.saturating_add(1)) else {
            // Validation reports malformed shapes, but an input can arrive
            // between validation and a pointer event. Keep hover defensive.
            continue;
        };
        let Some(index) = histogram_bucket(edges, x) else {
            continue;
        };
        let Some(raw_value) = s.counts.get(index).copied() else {
            continue;
        };
        let draw_value = if stacked && stack_bucket == Some(index) {
            stack_values
                .as_ref()
                .and_then(|values| values.get(si).copied().flatten())
        } else {
            cached_plot_count(s, index, histogram_cache, si)
        };
        let Some(draw_value) = draw_value else {
            continue;
        };
        if !raw_value.is_finite() || !draw_value.is_finite() {
            continue;
        }
        let lo = s.buckets[index];
        let hi = s.buckets[index + 1];
        header.get_or_insert_with(|| {
            format!("{} – {}", spec.x_unit.format(lo), spec.x_unit.format(hi))
        });
        let Some((px, py)) = histogram_marker_geometry(
            layout,
            series,
            draw_value,
            stack_tops.as_deref().unwrap_or(&[]),
            si,
            index,
            spec.layout,
        ) else {
            continue;
        };
        if !px.is_finite() || !py.is_finite() {
            continue;
        }
        let mut hover =
            // `y` deliberately remains the source count. Cumulative and
            // stacked modes change only the rendered bar top, not the value
            // users should read in a tooltip.
            HoverValue::new(si, s.name.clone(), raw_value, px, py)
                .with_color(crate::series::palette_color(s.color, si));
        if s.cumulative && meaningfully_distinct(draw_value, raw_value) {
            hover.extra.push(("Cumulative".into(), draw_value));
        }
        values.push(hover);
    }
    HoverInfo {
        ts: x,
        header,
        values,
    }
    .finish()
}

fn meaningfully_distinct(left: f64, right: f64) -> bool {
    (left - right).abs() > 1e-9 * left.abs().max(right.abs()).max(1.0)
}

/// Marker position for a histogram bar, mirroring `draw_histogram` without
/// changing the raw count returned to the tooltip.
fn histogram_marker_geometry(
    layout: &ChartLayout,
    series: &[HistogramSeries],
    draw_value: f64,
    stack_tops: &[Option<f64>],
    series_index: usize,
    bucket: usize,
    mode: crate::spec::SeriesLayout,
) -> Option<(f64, f64)> {
    let current = series.get(series_index)?;
    if !draw_value.is_finite() {
        return None;
    }
    match mode {
        crate::spec::SeriesLayout::Grouped => {
            let lo = *current.buckets.get(bucket)?;
            let hi = *current.buckets.get(bucket + 1)?;
            if !lo.is_finite() || !hi.is_finite() || lo == hi {
                return None;
            }
            let x0 = layout.x_scale.to_px(lo);
            let x1 = layout.x_scale.to_px(hi);
            let sub_width = (x1 - x0).abs() / series.len().max(1) as f64;
            let left = x0.min(x1) + series_index as f64 * sub_width + 0.5;
            let width = (sub_width - 1.0).max(1.0);
            Some((left + width / 2.0, layout.y_px(draw_value)))
        }
        crate::spec::SeriesLayout::Stacked | crate::spec::SeriesLayout::StackedPercent => {
            // The renderer uses the first series having this bucket's edge
            // pair for the shared stack geometry.
            let (lo, hi) = series.iter().find_map(|series| {
                series
                    .buckets
                    .get(bucket)
                    .zip(series.buckets.get(bucket + 1))
                    .map(|(lo, hi)| (*lo, *hi))
            })?;
            if !lo.is_finite() || !hi.is_finite() || lo == hi {
                return None;
            }
            let top = stack_tops.get(series_index).copied().flatten()?;
            let px = (layout.x_scale.to_px(lo) + layout.x_scale.to_px(hi)) / 2.0;
            Some((px, layout.y_px(top)))
        }
    }
}

fn histogram_stack_tops(
    values: &[Option<f64>],
    mode: crate::spec::SeriesLayout,
) -> Vec<Option<f64>> {
    let (positive_total, negative_total) =
        values
            .iter()
            .flatten()
            .copied()
            .fold((0.0, 0.0), |(positive, negative), value| {
                if value >= 0.0 {
                    (positive + value, negative)
                } else {
                    (positive, negative + value.abs())
                }
            });
    let mut positive = 0.0;
    let mut negative = 0.0;
    values
        .iter()
        .map(|value| {
            let value = value.as_ref().copied()?;
            let value = if matches!(mode, crate::spec::SeriesLayout::StackedPercent) {
                if value >= 0.0 && positive_total > 0.0 {
                    value / positive_total * 100.0
                } else if value < 0.0 && negative_total > 0.0 {
                    value / negative_total * 100.0
                } else {
                    0.0
                }
            } else {
                value
            };
            Some(if value >= 0.0 {
                positive += value;
                positive
            } else {
                negative += value;
                negative
            })
        })
        .collect()
}

/// Hover values for bars and areas. The label remains the raw source value,
/// while `px`/`py` are the actual rendered bar/stack edge so guides and dots
/// do not imply a geometry that is absent from the chart.
fn hit_test_composed_series(
    layout: &ChartLayout,
    series: &[SeriesData],
    mouse_x: f64,
    mode: crate::spec::SeriesLayout,
    bars: bool,
) -> Option<HoverInfo> {
    let ts = layout.ts_at(mouse_x);
    if matches!(mode, crate::spec::SeriesLayout::Grouped) {
        let mut info = hit_test(layout, series, mouse_x)?;
        if bars {
            for value in &mut info.values {
                if let Some(center) = grouped_bar_center(layout, series, value.series, value.px) {
                    value.px = center;
                }
            }
        }
        return Some(info);
    }

    let reference = if bars {
        series
            .iter()
            .find(|series| series.xs.len() > 1)
            .or_else(|| series.iter().find(|series| series.xs.len() == 1))
    } else {
        series.iter().find(|series| !series.xs.is_empty())
    }?;
    let bucket = nearest_index(&reference.xs, ts)?;
    let mut positive_total = 0.0;
    let mut negative_total = 0.0;
    for series in series {
        let Some(value) = series
            .ys
            .get(bucket)
            .copied()
            .filter(|value| value.is_finite())
        else {
            continue;
        };
        if value >= 0.0 {
            positive_total += value;
        } else {
            negative_total += value.abs();
        }
    }

    let mut positive = 0.0;
    let mut negative = 0.0;
    let mut values = Vec::new();
    for (index, series) in series.iter().enumerate() {
        let (Some(raw), Some(x)) = (
            series
                .ys
                .get(bucket)
                .copied()
                .filter(|value| value.is_finite()),
            if bars {
                series.xs.get(bucket).copied()
            } else {
                reference.xs.get(bucket).copied()
            },
        ) else {
            continue;
        };
        if !x.is_finite() {
            continue;
        }
        let drawn = if matches!(mode, crate::spec::SeriesLayout::StackedPercent) {
            if raw >= 0.0 && positive_total > 0.0 {
                raw / positive_total * 100.0
            } else if raw < 0.0 && negative_total > 0.0 {
                raw / negative_total * 100.0
            } else {
                0.0
            }
        } else {
            raw
        };
        let top = if drawn >= 0.0 {
            positive += drawn;
            positive
        } else {
            negative += drawn;
            negative
        };
        let px = layout.x_scale.to_px(x);
        let py = layout.y_px(top);
        if !px.is_finite() || !py.is_finite() {
            continue;
        }
        values.push(
            HoverValue::new(index, series.name.clone(), raw, px, py)
                .with_color(series_color(series, index)),
        );
    }
    HoverInfo {
        ts,
        header: None,
        values,
    }
    .finish()
}

fn grouped_bar_center(
    layout: &ChartLayout,
    series: &[SeriesData],
    series_index: usize,
    point_px: f64,
) -> Option<f64> {
    let reference = series.iter().find(|series| series.xs.len() > 1)?;
    let step = crate::series::sampled_median_gap(&reference.xs)?;
    let step_px =
        layout.x_scale.to_px(layout.x_scale.d0 + step) - layout.x_scale.to_px(layout.x_scale.d0);
    let group_width = (step_px - 2.0).max(1.0);
    let sub_width = (group_width / series.len().max(1) as f64).max(1.0);
    let left = point_px - group_width / 2.0 + series_index as f64 * sub_width;
    Some(left + (sub_width - 1.0).max(1.0) / 2.0)
}

fn histogram_bucket(edges: &[f64], x: f64) -> Option<usize> {
    if edges.len() < 2 || !x.is_finite() || x < edges[0] || x > *edges.last()? {
        return None;
    }
    if x == *edges.last()? {
        return Some(edges.len() - 2);
    }
    let upper = edges.partition_point(|edge| *edge <= x);
    upper
        .checked_sub(1)
        .filter(|index| *index + 1 < edges.len())
}

fn hit_test_hbar(
    layout: &ChartLayout,
    spec: &ChartSpec,
    series: &[HBarSeries],
    mouse_x: f64,
    mouse_y: f64,
) -> Option<HoverInfo> {
    let rows = series
        .iter()
        .map(|s| s.categories.len().min(s.values.len()))
        .max()
        .unwrap_or(0);
    let row = row_at(layout, mouse_y, rows)?;
    let row_h = row_height(layout, rows)?;
    let group_h = row_h * 0.8;
    let sub_h = group_h / series.len().max(1) as f64;
    let mut values = Vec::new();
    let mut category = None;
    for (si, s) in series.iter().enumerate() {
        let (Some(label), Some(value)) = (s.categories.get(row), s.values.get(row)) else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }
        category.get_or_insert_with(|| label.clone());
        let py = layout.plot.y
            + row as f64 * row_h
            + (row_h - group_h) / 2.0
            + (si as f64 + 0.5) * sub_h;
        if !py.is_finite() {
            continue;
        }
        values.push(
            HoverValue::new(si, s.name.clone(), *value, layout.x_scale.to_px(*value), py)
                .with_color(crate::series::palette_color(s.color, si)),
        );
    }
    HoverInfo {
        ts: layout.ts_at(mouse_x),
        header: category.or_else(|| Some(spec.x_unit.format(layout.ts_at(mouse_x)))),
        values,
    }
    .finish()
}

fn hit_test_state_timeline(
    layout: &ChartLayout,
    series: &[StateTimelineSeries],
    mouse_x: f64,
    mouse_y: f64,
) -> Option<HoverInfo> {
    let row = row_at(layout, mouse_y, series.len())?;
    let ts = layout.ts_at(mouse_x);
    let series_row = &series[row];
    let segment = state_segment_at(&series_row.segments, ts)?;
    let row_h = row_height(layout, series.len())?;
    let cell = state_cell(layout, series.len(), row, segment)?;
    if !cell.contains(mouse_x, mouse_y) {
        return None;
    }
    let mut value = HoverValue::new(
        row,
        series_row.name.clone(),
        0.0,
        layout.x_scale.to_px(ts),
        layout.plot.y + (row as f64 + 0.5) * row_h,
    )
    .with_color(segment.color)
    .with_formatted_value(segment.label.clone());
    value.shape = Some(crate::partition::Shape::Tile(cell));
    value.extra_text.push((
        "Duration".into(),
        Unit::Millis.format(segment.end_ms - segment.start_ms),
    ));
    HoverInfo {
        ts,
        header: Some(format_ts_full(ts as i64)),
        values: vec![value],
    }
    .finish()
}

/// Locate a segment in O(log n) for the validated (start-sorted) timeline
/// representation. If malformed input violates that contract, the result may
/// be absent, but it stays safe and never yields an invalid range.
pub(crate) fn state_segment_at(
    segments: &[crate::series::StateSegment],
    ts: f64,
) -> Option<&crate::series::StateSegment> {
    if !ts.is_finite() {
        return None;
    }
    let end = segments.partition_point(|segment| segment.start_ms <= ts);
    let segment = segments.get(end.checked_sub(1)?)?;
    (segment.start_ms.is_finite()
        && segment.end_ms.is_finite()
        && segment.end_ms > segment.start_ms
        && ts <= segment.end_ms)
        .then_some(segment)
}

pub(crate) fn state_cell(
    layout: &ChartLayout,
    rows: usize,
    row: usize,
    segment: &crate::StateSegment,
) -> Option<crate::Rect> {
    let h = row_height(layout, rows)?;
    let x0 = layout.x_scale.to_px(segment.start_ms);
    let x1 = layout.x_scale.to_px(segment.end_ms);
    if !x0.is_finite() || !x1.is_finite() || x1 <= x0 {
        return None;
    }
    Some(crate::partition::inset(crate::Rect {
        x: x0,
        y: layout.plot.y + row as f64 * h,
        w: x1 - x0,
        h,
    }))
}

fn hit_test_band(layout: &ChartLayout, series: &[BandSeries], mouse_x: f64) -> Option<HoverInfo> {
    let ts = layout.ts_at(mouse_x);
    let mut values = Vec::new();
    for (si, s) in series.iter().enumerate() {
        let n =
            s.xs.len()
                .min(s.center.len())
                .min(s.lower.len())
                .min(s.upper.len());
        let Some(index) = nearest_index(&s.xs[..n], ts) else {
            continue;
        };
        let center = s.center[index];
        let px = layout.x_scale.to_px(s.xs[index]);
        let py = layout.y_px(center);
        if !center.is_finite() || !px.is_finite() || !py.is_finite() {
            continue;
        }
        let mut value = HoverValue::new(si, s.name.clone(), center, px, py)
            .with_color(crate::series::palette_color(s.color, si));
        if s.lower[index].is_finite() {
            value.extra.push(("Lower".into(), s.lower[index]));
        }
        if s.upper[index].is_finite() {
            value.extra.push(("Upper".into(), s.upper[index]));
        }
        values.push(value);
    }
    HoverInfo {
        ts,
        header: None,
        values,
    }
    .finish()
}

fn hit_test_heatmap(
    layout: &ChartLayout,
    spec: &ChartSpec,
    series: &[SeriesData],
    mouse_x: f64,
    mouse_y: f64,
) -> Option<HoverInfo> {
    let row = row_at(layout, mouse_y, series.len())?;
    let s = &series[row];
    let n = s.xs.len().min(s.ys.len());
    let ts = layout.ts_at(mouse_x);
    let index = nearest_index(&s.xs[..n], ts)?;
    let value = s.ys[index];
    if !value.is_finite() {
        return None;
    }
    // Both the renderer and hover use the already-computed layout y domain,
    // avoiding an O(all cells) scan on every pointer move.
    let (heat_lo, heat_hi) = layout.y_domain();
    let mapped = heatmap_color_for_value(spec.heatmap_scale, value, heat_lo, heat_hi);
    let color = ((mapped.r as u32) << 16) | ((mapped.g as u32) << 8) | mapped.b as u32;
    let row_h = row_height(layout, series.len())?;
    HoverInfo {
        ts,
        header: None,
        values: vec![
            HoverValue::new(
                row,
                s.name.clone(),
                value,
                layout.x_scale.to_px(s.xs[index]),
                layout.plot.y + (row as f64 + 0.5) * row_h,
            )
            .with_color(color),
        ],
    }
    .finish()
}

/// Shared row height policy for heatmaps, horizontal bars, and timelines.
/// Keep fractional rows when there are more rows than CSS pixels so the
/// bottom row remains reachable and drawing and hit testing agree.
pub fn row_height(layout: &ChartLayout, rows: usize) -> Option<f64> {
    (rows > 0 && layout.plot.h.is_finite() && layout.plot.h > 0.0)
        .then_some(layout.plot.h / rows as f64)
        .filter(|height| height.is_finite() && *height > 0.0)
}

pub fn row_at(layout: &ChartLayout, mouse_y: f64, rows: usize) -> Option<usize> {
    if rows == 0 || !layout.plot.contains(layout.plot.x, mouse_y) {
        return None;
    }
    let row_h = row_height(layout, rows)?;
    Some(
        ((mouse_y - layout.plot.y) / row_h)
            .floor()
            .clamp(0.0, rows.saturating_sub(1) as f64) as usize,
    )
}

pub fn nearest_index(xs: &[f64], target: f64) -> Option<usize> {
    if xs.is_empty() {
        return None;
    }
    let i = xs.partition_point(|x| *x < target);
    if i == 0 {
        Some(0)
    } else if i >= xs.len() {
        Some(xs.len() - 1)
    } else if (target - xs[i - 1]).abs() <= (xs[i] - target).abs() {
        Some(i - 1)
    } else {
        Some(i)
    }
}

fn nearest_index_ticks(ticks: &[OhlcTick], target: f64) -> Option<usize> {
    if ticks.is_empty() || !target.is_finite() {
        return None;
    }
    let i = ticks.partition_point(|t| t.ts < target);
    if i == 0 {
        Some(0)
    } else if i >= ticks.len() {
        Some(ticks.len() - 1)
    } else if (target - ticks[i - 1].ts).abs() <= (ticks[i].ts - target).abs() {
        Some(i - 1)
    } else {
        Some(i)
    }
}

fn ohlc_tick_is_drawable(tick: &OhlcTick) -> bool {
    tick.ts.is_finite()
        && tick.open.is_finite()
        && tick.high.is_finite()
        && tick.low.is_finite()
        && tick.close.is_finite()
        && tick.low <= tick.high
        && tick.low <= tick.open
        && tick.low <= tick.close
        && tick.high >= tick.open
        && tick.high >= tick.close
}

#[derive(Debug, Clone)]
pub struct LegendEntry {
    pub name: String,
    pub color: u32,
    pub visible: bool,
    pub y_value: Option<f64>,
    pub formatted_value: Option<String>,
}

pub fn legend_entries(series: &[SeriesData], hover_info: Option<&HoverInfo>) -> Vec<LegendEntry> {
    series
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let hover_val = hover_info
                .as_ref()
                .and_then(|info| info.values.iter().find(|v| v.series == i).map(|v| v.y));
            LegendEntry {
                name: s.name.clone(),
                color: crate::series::series_color(s, i),
                visible: true,
                y_value: hover_val,
                formatted_value: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::series::{
        BandSeries, HBarSeries, HistogramSeries, StateSegment, StateTimelineSeries,
    };
    use crate::spec::ChartData;
    use crate::units::Unit;

    #[test]
    fn nearest() {
        let xs = vec![0.0, 10.0, 20.0, 30.0];
        assert_eq!(nearest_index(&xs, -5.0), Some(0));
        assert_eq!(nearest_index(&xs, 14.0), Some(1));
        assert_eq!(nearest_index(&xs, 16.0), Some(2));
        assert_eq!(nearest_index(&xs, 99.0), Some(3));
        assert_eq!(nearest_index(&[], 1.0), None);
    }

    #[test]
    fn hit_test_scatter_finds_nearest() {
        let layout = ChartLayout::compute(600.0, 200.0, 0.0, 100.0, 0.0, 100.0);
        let series = vec![SeriesData {
            name: "a".into(),
            xs: vec![10.0, 50.0, 90.0],
            ys: vec![10.0, 50.0, 90.0],
            color: None,
        }];
        let info = hit_test_scatter(&layout, &series, 322.0, 93.0, 30.0);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.values.len(), 1);
        assert!((info.values[0].y - 50.0).abs() < 0.1);
    }

    #[test]
    fn scatter_x_window_binary_slices_the_hover_radius() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 100.0, 0.0, 1.0);
        let xs = [0.0, 20.0, 40.0, 50.0, 60.0, 80.0, 100.0];
        let mouse_x = layout.x_scale.to_px(50.0);
        let (start, end) = scatter_x_window(&layout, &xs, mouse_x, 13.0);

        assert_eq!(&xs[start..end], &[50.0]);
    }

    #[test]
    fn legend_entries_match_series_count() {
        let series = vec![
            SeriesData {
                name: "a".into(),
                xs: vec![],
                ys: vec![],
                color: None,
            },
            SeriesData {
                name: "b".into(),
                xs: vec![],
                ys: vec![],
                color: None,
            },
        ];
        let entries = legend_entries(&series, None);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a");
        assert_eq!(entries[1].name, "b");
    }

    #[test]
    fn unified_hit_test_tolerates_mismatched_point_arrays() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 10.0, 0.0, 20.0);
        let data = ChartData::Lines(vec![SeriesData {
            name: "short-y".into(),
            xs: vec![0.0, 5.0, 10.0],
            ys: vec![7.0],
            color: None,
        }]);

        // Ingestion validation reports this shape, but interactivity must still
        // be safe for data that arrives before its validation result.
        let result = std::panic::catch_unwind(|| {
            hit_test_chart(
                &layout,
                &ChartSpec::default(),
                &data,
                layout.plot.x,
                layout.plot.y,
            )
        });
        let info = result
            .expect("mismatched point data must not panic")
            .expect("first point");
        assert_eq!(info.values[0].y, 7.0);
    }

    #[test]
    fn unified_hit_test_uses_histogram_bucket_and_cumulative_value() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 3.0, 0.0, 10.0);
        let data = ChartData::Histogram(vec![HistogramSeries {
            name: "latency".into(),
            buckets: vec![0.0, 1.0, 2.0, 3.0],
            counts: vec![2.0, 3.0, 5.0],
            color: None,
            cumulative: true,
        }]);
        let spec = ChartSpec::histogram(Unit::None);
        let x = layout.x_scale.to_px(1.4);

        let info = hit_test_chart(&layout, &spec, &data, x, layout.plot.y).expect("second bucket");
        assert_eq!(info.header.as_deref(), Some("1 – 2"));
        // The source count remains readable even though a cumulative bar is
        // geometrically placed at the running total (2 + 3).
        assert_eq!(info.values[0].y, 3.0);
        assert_eq!(info.values[0].py, layout.y_px(5.0));
        assert_eq!(info.values[0].extra, vec![("Cumulative".into(), 5.0)]);
        assert!(info.values[0].highlighted);
    }

    #[test]
    fn empty_histogram_hover_is_safe() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 1.0, 0.0, 1.0);
        let data = ChartData::Histogram(vec![HistogramSeries::default()]);
        let result = std::panic::catch_unwind(|| {
            hit_test_chart(
                &layout,
                &ChartSpec::default(),
                &data,
                layout.plot.x,
                layout.plot.y,
            )
        });
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn step_hover_matches_step_after_geometry() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 10.0, 0.0, 100.0);
        let data = ChartData::Step(vec![SeriesData {
            name: "state".into(),
            xs: vec![0.0, 10.0],
            ys: vec![1.0, 100.0],
            color: None,
        }]);
        let spec = ChartSpec::step(Unit::None);
        let before = hit_test_chart(
            &layout,
            &spec,
            &data,
            layout.x_scale.to_px(6.0),
            layout.plot.y,
        )
        .unwrap();
        let at = hit_test_chart(
            &layout,
            &spec,
            &data,
            layout.x_scale.to_px(10.0),
            layout.plot.y,
        )
        .unwrap();
        assert_eq!(before.values[0].y, 1.0);
        assert_eq!(at.values[0].y, 100.0);
    }

    #[test]
    fn hover_header_uses_sample_x_while_pointer_ts_stays_exact() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 10.0, 0.0, 100.0);
        let data = ChartData::Lines(vec![SeriesData {
            name: "line".into(),
            xs: vec![0.0, 10.0],
            ys: vec![1.0, 2.0],
            color: None,
        }]);
        let spec = ChartSpec::line(Unit::None);
        let info = hit_test_chart(
            &layout,
            &spec,
            &data,
            layout.x_scale.to_px(6.0),
            layout.plot.y,
        )
        .unwrap();
        assert!((info.ts - 6.0).abs() < 1e-9);
        assert_eq!(info.header.as_deref(), Some("01-01 00:00:00.010"));
    }

    #[test]
    fn log_invalid_hover_is_a_gap() {
        let layout = ChartLayout::compute_scaled(
            300.0,
            160.0,
            0.0,
            3.0,
            1.0,
            10.0,
            52.0,
            8.0,
            8.0,
            22.0,
            crate::scale::ScaleKind::Log,
        );
        let data = ChartData::Lines(vec![SeriesData {
            name: "log".into(),
            xs: vec![0.0],
            ys: vec![0.0],
            color: None,
        }]);
        assert!(
            hit_test_chart(
                &layout,
                &ChartSpec::line(Unit::None).with_log_y(),
                &data,
                layout.plot.x,
                layout.plot.y,
            )
            .is_none()
        );
    }

    #[test]
    fn dense_rows_use_fractional_shared_geometry() {
        let layout = ChartLayout::compute(300.0, 100.0, 0.0, 1.0, 0.0, 1.0);
        assert!(row_height(&layout, 200).unwrap() < 1.0);
        assert_eq!(
            row_at(&layout, layout.plot.y + layout.plot.h - 0.001, 200),
            Some(199)
        );
    }

    #[test]
    fn histogram_stack_markers_use_stack_tops_but_keep_raw_counts() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 1.0, 0.0, 10.0);
        let data = ChartData::Histogram(vec![
            HistogramSeries {
                name: "a".into(),
                buckets: vec![0.0, 1.0],
                counts: vec![2.0],
                color: None,
                cumulative: false,
            },
            HistogramSeries {
                name: "b".into(),
                buckets: vec![0.0, 1.0],
                counts: vec![3.0],
                color: None,
                cumulative: false,
            },
        ]);
        let mut spec = ChartSpec::histogram(Unit::None);
        spec.layout = crate::spec::SeriesLayout::Stacked;

        let info = hit_test_chart(
            &layout,
            &spec,
            &data,
            layout.x_scale.to_px(0.5),
            layout.plot.y,
        )
        .expect("stacked bucket");
        assert_eq!(
            info.values.iter().map(|value| value.y).collect::<Vec<_>>(),
            vec![2.0, 3.0]
        );
        assert_eq!(info.values[0].py, layout.y_px(2.0));
        assert_eq!(info.values[1].py, layout.y_px(5.0));
    }

    #[test]
    fn composed_series_marker_geometry_matches_stacks_and_grouped_bars() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 10.0, 0.0, 100.0);
        let series = || {
            vec![
                SeriesData {
                    name: "a".into(),
                    xs: vec![0.0, 10.0],
                    ys: vec![1.0, 2.0],
                    color: None,
                },
                SeriesData {
                    name: "b".into(),
                    xs: vec![0.0, 10.0],
                    ys: vec![3.0, 6.0],
                    color: None,
                },
            ]
        };
        let mut bars = ChartSpec::bars(Unit::None);
        bars.layout = crate::spec::SeriesLayout::Stacked;
        let stacked = hit_test_chart(
            &layout,
            &bars,
            &ChartData::Bars(series()),
            layout.x_scale.to_px(0.0),
            layout.plot.y,
        )
        .expect("stacked bars");
        assert_eq!(
            stacked
                .values
                .iter()
                .map(|value| value.y)
                .collect::<Vec<_>>(),
            vec![1.0, 3.0]
        );
        assert_eq!(stacked.values[0].py, layout.y_px(1.0));
        assert_eq!(stacked.values[1].py, layout.y_px(4.0));

        bars.layout = crate::spec::SeriesLayout::Grouped;
        let grouped = hit_test_chart(
            &layout,
            &bars,
            &ChartData::Bars(series()),
            layout.x_scale.to_px(0.0),
            layout.plot.y,
        )
        .expect("grouped bars");
        assert_ne!(grouped.values[0].px, grouped.values[1].px);
        assert_eq!(grouped.values[0].py, layout.y_px(1.0));

        let mut areas = ChartSpec::area(Unit::None);
        areas.layout = crate::spec::SeriesLayout::StackedPercent;
        let percent = hit_test_chart(
            &layout,
            &areas,
            &ChartData::Areas(series()),
            layout.x_scale.to_px(0.0),
            layout.plot.y,
        )
        .expect("percent areas");
        assert_eq!(percent.values[0].y, 1.0);
        assert_eq!(percent.values[1].y, 3.0);
        assert_eq!(percent.values[0].py, layout.y_px(25.0));
        assert_eq!(percent.values[1].py, layout.y_px(100.0));
    }

    #[test]
    fn unified_hit_test_uses_hbar_row_category() {
        let layout = ChartLayout::compute(300.0, 200.0, -10.0, 10.0, 0.0, 2.0);
        let data = ChartData::HBar(vec![HBarSeries {
            name: "budget".into(),
            categories: vec!["first".into(), "second".into()],
            values: vec![4.0, -3.0],
            color: None,
        }]);
        let mouse_y = layout.plot.y + layout.plot.h * 0.75;

        let info = hit_test_chart(
            &layout,
            &ChartSpec::hbar(Unit::None),
            &data,
            layout.x_scale.to_px(-3.0),
            mouse_y,
        )
        .expect("second horizontal-bar row");
        assert_eq!(info.header.as_deref(), Some("second"));
        assert_eq!(info.values[0].y, -3.0);
    }

    #[test]
    fn heatmap_hover_color_matches_renderer_normalization() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 10.0, 0.0, 1.0);
        let spec = ChartSpec::heatmap(Unit::None);
        let data = ChartData::Heatmap(vec![
            SeriesData {
                name: "selected".into(),
                xs: vec![0.0, 10.0],
                ys: vec![2.0, 4.0],
                color: None,
            },
            // Its large range must not make hit-testing scan every cell merely
            // to choose a marker shade; both paths use layout's y domain.
            SeriesData {
                name: "unrelated".into(),
                xs: vec![0.0, 10.0],
                ys: vec![-1_000_000.0, 1_000_000.0],
                color: None,
            },
        ]);

        let info = hit_test_chart(
            &layout,
            &spec,
            &data,
            layout.x_scale.to_px(10.0),
            layout.plot.y + layout.plot.h / 4.0,
        )
        .expect("heatmap cell");
        let (heat_lo, heat_hi) = layout.y_domain();
        let color = heatmap_color_for_value(spec.heatmap_scale, 4.0, heat_lo, heat_hi);
        assert_eq!(
            info.values[0].color,
            Some(((color.r as u32) << 16) | ((color.g as u32) << 8) | color.b as u32)
        );
        assert_eq!(info.values[0].y, 4.0);
    }

    #[test]
    fn unified_hit_test_returns_state_segment_label_and_duration() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 100.0, 0.0, 1.0);
        let data = ChartData::StateTimeline(vec![StateTimelineSeries {
            name: "api".into(),
            segments: vec![StateSegment {
                start_ms: 20.0,
                end_ms: 70.0,
                label: "degraded".into(),
                color: 0xffaa00,
            }],
        }]);

        let info = hit_test_chart(
            &layout,
            &ChartSpec::state_timeline(),
            &data,
            layout.x_scale.to_px(50.0),
            layout.plot.y + layout.plot.h / 2.0,
        )
        .expect("state segment");
        assert_eq!(info.values[0].formatted_value.as_deref(), Some("degraded"));
        assert_eq!(
            info.values[0].extra_text,
            vec![("Duration".into(), "50ms".into())]
        );
    }

    #[test]
    fn state_timeline_uses_binary_segment_lookup_and_rejects_bad_segments() {
        let valid = [
            StateSegment {
                start_ms: 0.0,
                end_ms: 10.0,
                label: "first".into(),
                color: 0,
            },
            StateSegment {
                start_ms: 10.0,
                end_ms: 20.0,
                label: "second".into(),
                color: 0,
            },
        ];
        assert_eq!(
            state_segment_at(&valid, 10.0).map(|segment| segment.label.as_str()),
            Some("second")
        );
        assert!(
            state_segment_at(
                &[StateSegment {
                    start_ms: 5.0,
                    end_ms: f64::NAN,
                    label: "bad".into(),
                    color: 0,
                }],
                6.0,
            )
            .is_none()
        );
    }

    #[test]
    fn malformed_ohlc_ticks_never_produce_hover_geometry() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 10.0, 0.0, 100.0);
        let series = [OhlcSeriesData {
            name: "bad".into(),
            ticks: vec![OhlcTick {
                ts: 5.0,
                open: 20.0,
                high: 10.0,
                low: f64::NAN,
                close: 15.0,
            }],
            color: None,
        }];
        assert!(hit_test_ohlc(&layout, &series, layout.x_scale.to_px(5.0)).is_none());
    }

    #[test]
    fn unified_hit_test_includes_band_bounds() {
        let layout = ChartLayout::compute(300.0, 160.0, 0.0, 100.0, 0.0, 100.0);
        let data = ChartData::Band(vec![BandSeries {
            name: "p90".into(),
            xs: vec![0.0, 50.0, 100.0],
            center: vec![20.0, 40.0, 60.0],
            lower: vec![10.0, 30.0, 50.0],
            upper: vec![30.0, 50.0, 70.0],
            color: None,
        }]);

        let info = hit_test_chart(
            &layout,
            &ChartSpec::band(Unit::None),
            &data,
            layout.x_scale.to_px(55.0),
            layout.plot.y,
        )
        .expect("nearest band point");
        assert_eq!(info.values[0].y, 40.0);
        assert_eq!(
            info.values[0].extra,
            vec![("Lower".into(), 30.0), ("Upper".into(), 50.0)]
        );
    }

    #[test]
    fn hover_geometry_follows_a_log_y_axis() {
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
            crate::scale::ScaleKind::Log,
        );
        let data = ChartData::Lines(vec![SeriesData {
            name: "latency".into(),
            xs: vec![0.0, 5.0, 10.0],
            ys: vec![10.0, 100.0, 1000.0],
            color: None,
        }]);
        let spec = ChartSpec::line(Unit::Millis).with_log_y();

        let info = hit_test_chart(
            &layout,
            &spec,
            &data,
            layout.x_scale.to_px(5.0),
            layout.plot.y,
        )
        .expect("middle sample");
        assert_eq!(info.values[0].y, 100.0);
        // The marker sits where the log axis actually draws the value, not
        // where a linear reading of the same domain would put it.
        assert_eq!(info.values[0].py, layout.y_px(100.0));
        assert!((layout.y_at(info.values[0].py) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn frozen_delta_annotates_matching_series_with_sign_and_percent() {
        let mut hover = HoverInfo {
            ts: 10.0,
            header: None,
            values: vec![
                HoverValue::new(0, "p50".into(), 120.0, 0.0, 0.0),
                HoverValue::new(1, "p99".into(), 50.0, 0.0, 0.0),
                HoverValue::new(2, "unmatched".into(), 7.0, 0.0, 0.0),
            ],
        };
        let frozen = HoverInfo {
            ts: 1.0,
            header: None,
            values: vec![
                HoverValue::new(0, "p50".into(), 100.0, 0.0, 0.0),
                HoverValue::new(1, "p99".into(), 200.0, 0.0, 0.0),
            ],
        };

        annotate_delta(&mut hover, &frozen, Unit::Millis);
        assert_eq!(
            hover.values[0].extra_text,
            vec![("Δ".to_string(), "+20ms (+20.0%)".to_string())]
        );
        assert_eq!(
            hover.values[1].extra_text,
            vec![("Δ".to_string(), "-150ms (-75.0%)".to_string())]
        );
        // No reference for this series: nothing invented.
        assert!(hover.values[2].extra_text.is_empty());
    }

    #[test]
    fn frozen_delta_omits_a_percentage_against_a_zero_baseline() {
        let mut hover = HoverInfo {
            ts: 10.0,
            header: None,
            values: vec![HoverValue::new(0, "errors".into(), 4.0, 0.0, 0.0)],
        };
        let frozen = HoverInfo {
            ts: 1.0,
            header: None,
            values: vec![HoverValue::new(0, "errors".into(), 0.0, 0.0, 0.0)],
        };
        annotate_delta(&mut hover, &frozen, Unit::None);
        assert_eq!(
            hover.values[0].extra_text,
            vec![("Δ".to_string(), "+4".to_string())]
        );
    }
}
