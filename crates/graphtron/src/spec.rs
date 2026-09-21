use crate::color::HeatmapColorScale;
use crate::scale::ScaleKind;
use crate::series::{
    BandSeries, HBarSeries, HistogramSeries, OhlcSeriesData, SeriesData, StateTimelineSeries,
};
use crate::units::Unit;
use serde::{Deserialize, Serialize};

/// Chart kind tag. Derived from [`ChartData`] — overlay and hit-test logic
/// dispatch on it. Data and kind are coupled through `ChartData`, so a kind
/// can never be paired with the wrong series type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartKind {
    #[default]
    Line,
    Area,
    Bars,
    Scatter,
    /// Discrete samples without connecting lines.
    Point,
    Pie,
    Treemap,
    HostMap,
    Heatmap,
    /// Open-high-low-close candlestick (financial-style).
    Ohlc,
    /// Horizontal-then-vertical lines for discrete state transitions.
    Step,
    /// Bucket-aggregated frequency distribution.
    Histogram,
    /// Horizontal bars (ranked lists, budget allocation).
    HBar,
    /// Discrete states over time as colored horizontal segments (service
    /// health, deploy phases).
    StateTimeline,
    /// Center line with a shaded lower–upper band (percentile envelopes).
    Band,
}

/// The chart's data, coupled to its kind. Replaces the old 4-slice
/// `draw_data` signature where callers passed `&[], &[], &[]` for the three
/// slices they didn't use and nothing stopped a kind/data mismatch.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartData {
    Lines(Vec<SeriesData>),
    Areas(Vec<SeriesData>),
    Bars(Vec<SeriesData>),
    Scatter(Vec<SeriesData>),
    Points(Vec<SeriesData>),
    Pie(Vec<crate::partition::PartitionItem>),
    Treemap(Vec<crate::partition::PartitionItem>),
    HostMap(Vec<crate::partition::PartitionItem>),
    Heatmap(Vec<SeriesData>),
    Ohlc(Vec<OhlcSeriesData>),
    Step(Vec<SeriesData>),
    Histogram(Vec<HistogramSeries>),
    HBar(Vec<HBarSeries>),
    StateTimeline(Vec<StateTimelineSeries>),
    Band(Vec<BandSeries>),
}

impl Default for ChartData {
    fn default() -> Self {
        Self::Lines(vec![])
    }
}

impl ChartData {
    pub fn kind(&self) -> ChartKind {
        match self {
            Self::Lines(_) => ChartKind::Line,
            Self::Areas(_) => ChartKind::Area,
            Self::Bars(_) => ChartKind::Bars,
            Self::Points(_) => ChartKind::Point,
            Self::Pie(_) => ChartKind::Pie,
            Self::Treemap(_) => ChartKind::Treemap,
            Self::HostMap(_) => ChartKind::HostMap,
            Self::Scatter(_) => ChartKind::Scatter,
            Self::Heatmap(_) => ChartKind::Heatmap,
            Self::Ohlc(_) => ChartKind::Ohlc,
            Self::Step(_) => ChartKind::Step,
            Self::Histogram(_) => ChartKind::Histogram,
            Self::HBar(_) => ChartKind::HBar,
            Self::StateTimeline(_) => ChartKind::StateTimeline,
            Self::Band(_) => ChartKind::Band,
        }
    }

    /// Build point-series data for a runtime-chosen kind (e.g. a dashboard
    /// panel style). Returns `None` for kinds not backed by [`SeriesData`]
    /// (Ohlc, Histogram, HBar) — those have their own data types.
    pub fn from_point_kind(kind: ChartKind, series: Vec<SeriesData>) -> Option<Self> {
        match kind {
            ChartKind::Line => Some(Self::Lines(series)),
            ChartKind::Area => Some(Self::Areas(series)),
            ChartKind::Bars => Some(Self::Bars(series)),
            ChartKind::Point => Some(Self::Points(series)),
            ChartKind::Scatter => Some(Self::Scatter(series)),
            ChartKind::Heatmap => Some(Self::Heatmap(series)),
            ChartKind::Step => Some(Self::Step(series)),
            ChartKind::Pie
            | ChartKind::Treemap
            | ChartKind::HostMap
            | ChartKind::Ohlc
            | ChartKind::Histogram
            | ChartKind::HBar
            | ChartKind::StateTimeline
            | ChartKind::Band => None,
        }
    }

    /// The point-series slice for kinds backed by [`SeriesData`]; empty for
    /// OHLC/histogram/hbar. Hover, hit-testing, and legends operate on this.
    pub fn point_series(&self) -> &[SeriesData] {
        match self {
            Self::Lines(s)
            | Self::Areas(s)
            | Self::Bars(s)
            | Self::Points(s)
            | Self::Scatter(s)
            | Self::Heatmap(s)
            | Self::Step(s) => s,
            _ => &[],
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Pie(items) | Self::Treemap(items) => {
                !items.iter().any(|i| i.value.is_finite() && i.value > 0.0)
            }
            Self::HostMap(items) => items.is_empty(),
            Self::Lines(s)
            | Self::Areas(s)
            | Self::Bars(s)
            | Self::Points(s)
            | Self::Scatter(s)
            | Self::Heatmap(s)
            | Self::Step(s) => s.iter().all(|series| {
                series
                    .xs
                    .iter()
                    .zip(&series.ys)
                    .all(|(x, y)| !x.is_finite() || !y.is_finite())
            }),
            Self::Ohlc(s) => s
                .iter()
                .all(|series| series.ticks.iter().all(|tick| !tick.ts.is_finite())),
            Self::Histogram(s) => s.iter().all(|series| {
                series
                    .counts
                    .iter()
                    .take(series.buckets.len().saturating_sub(1))
                    .all(|count| !count.is_finite())
            }),
            Self::HBar(s) => s.iter().all(|series| {
                series
                    .values
                    .iter()
                    .take(series.categories.len())
                    .all(|value| !value.is_finite())
            }),
            Self::StateTimeline(s) => s.iter().all(|series| {
                series.segments.iter().all(|segment| {
                    !segment.start_ms.is_finite()
                        || !segment.end_ms.is_finite()
                        || segment.end_ms <= segment.start_ms
                })
            }),
            Self::Band(s) => s.iter().all(|series| {
                let n = series
                    .xs
                    .len()
                    .min(series.center.len())
                    .min(series.lower.len())
                    .min(series.upper.len());
                (0..n).all(|i| {
                    !series.xs[i].is_finite()
                        || (!series.center[i].is_finite()
                            && !series.lower[i].is_finite()
                            && !series.upper[i].is_finite())
                })
            }),
        }
    }

    /// Validate shape, ordering, and chart-specific invariants.
    ///
    /// Rendering and hit-testing are deliberately defensive and use the safe
    /// intersection of malformed arrays, but calling this at ingestion time
    /// gives applications precise diagnostics instead of silently dropping
    /// unusable values.
    pub fn validate(&self) -> Result<(), Vec<crate::validation::DataIssue>> {
        let issues = crate::validation::validate_chart_data(self);
        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues)
        }
    }

    /// As [`Self::validate`], plus issues that depend on the spec — today,
    /// values a logarithmic y axis will silently draw as gaps. Prefer this at
    /// the ingestion boundary whenever the spec is already in hand.
    pub fn validate_with(&self, spec: &ChartSpec) -> Result<(), Vec<crate::validation::DataIssue>> {
        let issues = crate::validation::validate_chart_data_for_spec(self, spec);
        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues)
        }
    }
}

/// What the x axis measures. The old layout hardwired x to `[from_ms, to_ms]`
/// and always drew time ticks, so histograms (x = value buckets) and hbars
/// (x = values) were mislabeled whenever chrome was on.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum XAxisKind {
    /// Epoch-milliseconds time axis driven by the viewport (the default).
    #[default]
    Time,
    /// Linear value axis. `None` bounds are derived from the data (histogram
    /// bucket edges, hbar value extents, or series x extents).
    Linear { min: Option<f64>, max: Option<f64> },
    /// Categorical axis: one band per label.
    Category { labels: Vec<String> },
}

/// How multi-series bars/areas combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeriesLayout {
    /// Series stack on each other's baselines (the default — log volume by
    /// severity, request count by status).
    #[default]
    Stacked,
    /// Bars sit side-by-side within each bucket; areas overlap translucently.
    Grouped,
    /// Stacked and normalized so each column sums to 100%.
    StackedPercent,
}

/// How to decimate dense data before rendering.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum DecimationKind {
    /// M4 min-max per pixel column (preserves spikes).
    #[default]
    M4,
    /// Largest Triangle Three Buckets with the given threshold.
    Lttb(usize),
}

/// A visual annotation drawn on the chart overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// Y-axis value for horizontal lines, or y0 for bands.
    pub y: f64,
    /// Optional second y for a filled band [y, y2].
    pub y2: Option<f64>,
    pub label: Option<String>,
    /// Optional second label line (dimmer), e.g. a description.
    pub subtitle: Option<String>,
    pub color: u32,
    /// Optional timestamp to anchor a vertical marker.
    pub ts: Option<f64>,
    /// Optional end timestamp; when set with `ts`, draws a filled vertical
    /// band across the plot from `ts` to `ts2` (a time-region highlight).
    pub ts2: Option<f64>,
    /// Dash pattern: None = solid, Some(&[4, 4]) = dashed.
    pub dash: Option<Vec<f64>>,
}

impl Annotation {
    pub fn threshold(y: f64, label: impl Into<String>, color: u32) -> Self {
        Self {
            y,
            y2: None,
            label: Some(label.into()),
            subtitle: None,
            color,
            ts: None,
            ts2: None,
            dash: Some(vec![4.0, 4.0]),
        }
    }

    pub fn band(y0: f64, y1: f64, color: u32) -> Self {
        Self {
            y: y0,
            y2: Some(y1),
            label: None,
            subtitle: None,
            color,
            ts: None,
            ts2: None,
            dash: None,
        }
    }

    pub fn marker(ts: f64, label: impl Into<String>, color: u32) -> Self {
        Self {
            y: 0.0,
            y2: None,
            label: Some(label.into()),
            subtitle: None,
            color,
            ts: Some(ts),
            ts2: None,
            dash: None,
        }
    }

    /// A vertical time band from `from_ms` to `to_ms` spanning the plot height
    /// (used to highlight a region of a time-series graph).
    pub fn time_band(from_ms: f64, to_ms: f64, label: impl Into<String>, color: u32) -> Self {
        Self {
            y: 0.0,
            y2: None,
            label: Some(label.into()),
            subtitle: None,
            color,
            ts: Some(from_ms),
            ts2: Some(to_ms),
            dash: None,
        }
    }
}

/// Canvas legend configuration.
///
/// The Dioxus adapter also offers an interactive DOM legend for toggling
/// series. This configuration controls the framework-independent legend drawn
/// by [`crate::draw`] and is useful for exported canvases and lightweight
/// embeds.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LegendConfig {
    /// Whether the renderer reserves space and draws the legend.
    pub show: bool,
    /// Edge of the canvas on which the legend is placed.
    pub position: LegendPosition,
    /// Whether entries show their series name, latest/aggregate value, or both.
    pub format: LegendFormat,
    /// Per-series summary statistics appended to each entry. Empty by
    /// default; `[Min, Max, Mean]` turns the legend into a compact
    /// at-a-glance summary table.
    pub stats: Vec<LegendStat>,
}

impl LegendConfig {
    /// A visible legend with name and latest value.
    pub fn shown(position: LegendPosition) -> Self {
        Self {
            show: true,
            position,
            format: LegendFormat::NameAndValue,
            stats: vec![],
        }
    }

    /// A visible legend that also summarizes each series (min/max/mean).
    pub fn with_stats(position: LegendPosition) -> Self {
        Self {
            show: true,
            position,
            format: LegendFormat::NameAndValue,
            stats: vec![LegendStat::Min, LegendStat::Max, LegendStat::Mean],
        }
    }
}

/// Canvas legend placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegendPosition {
    /// Horizontal legend above the plot.
    #[default]
    Top,
    /// Horizontal legend below the x axis.
    Bottom,
    /// Vertical legend to the right of the plot.
    Right,
    /// Vertical legend to the left of the y axis.
    Left,
}

/// Text shown for each legend entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegendFormat {
    /// Series name only.
    #[default]
    NameOnly,
    /// Series name followed by its latest or aggregate value.
    NameAndValue,
    /// Latest or aggregate value only.
    ValueOnly,
}

/// A per-series summary statistic a legend can report.
///
/// Reading "which series peaked, and how does its mean compare" off a dense
/// chart is guesswork; a stats legend answers it exactly. Statistics are
/// computed over the finite values in the drawn window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegendStat {
    Min,
    Max,
    Mean,
    /// Arithmetic sum of the finite values.
    Total,
    /// Most recent finite value (the same value `NameAndValue` reports).
    Last,
}

impl LegendStat {
    /// Short column heading (also the inline prefix in a compact legend).
    pub fn label(self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Max => "max",
            Self::Mean => "mean",
            Self::Total => "total",
            Self::Last => "last",
        }
    }
}

/// A statistical reference line derived from the visible data.
///
/// Reference lines answer the questions a raw stroke does not: where the
/// average sits, whether the current value is inside the usual band, and how
/// far the tail runs. They are recomputed per frame from the visible window,
/// so zooming re-answers the question for the range actually on screen
/// instead of freezing a whole-dataset constant onto the panel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatLine {
    Mean,
    Median,
    Min,
    Max,
    /// Quantile in `0..=1` (`0.95` for p95).
    Quantile(f64),
    /// A shaded band `mean ± n × σ`, plus its two edge lines.
    Sigma(f64),
}

impl StatLine {
    /// Short label drawn at the line (`"mean"`, `"p95"`, `"±2σ"`).
    pub fn label(self) -> String {
        match self {
            Self::Mean => "mean".to_string(),
            Self::Median => "median".to_string(),
            Self::Min => "min".to_string(),
            Self::Max => "max".to_string(),
            Self::Quantile(q) => {
                let percent = (q.clamp(0.0, 1.0) * 100.0 * 10.0).round() / 10.0;
                if (percent - percent.round()).abs() < f64::EPSILON {
                    format!("p{}", percent.round() as i64)
                } else {
                    format!("p{percent}")
                }
            }
            Self::Sigma(n) => {
                let n = (n * 100.0).round() / 100.0;
                if (n - n.round()).abs() < f64::EPSILON {
                    format!("±{}σ", n.round() as i64)
                } else {
                    format!("±{n}σ")
                }
            }
        }
    }

    /// Resolve against a pre-computed summary and the sorted sample. Sigma
    /// lines resolve to their upper edge; the band itself is drawn from the
    /// summary. Returns `None` when the statistic is undefined.
    pub fn resolve(self, summary: &crate::stats::Summary, sorted: &[f64]) -> Option<f64> {
        match self {
            Self::Mean => Some(summary.mean),
            Self::Median => crate::stats::quantile_sorted(sorted, 0.5),
            Self::Min => Some(summary.min),
            Self::Max => Some(summary.max),
            Self::Quantile(q) => crate::stats::quantile_sorted(sorted, q),
            Self::Sigma(n) => Some(summary.mean + n.abs() * summary.stddev),
        }
    }
}

/// Automatic statistical reference lines drawn behind the series.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ReferenceLines {
    /// Statistics to draw. Empty (the default) draws nothing.
    pub lines: Vec<StatLine>,
    /// One line per series in the series color, rather than a single pooled
    /// line across every visible value. Pooled is the default because the
    /// usual question ("what is normal for this panel?") is about the panel.
    pub per_series: bool,
    /// Draw the statistic's name and formatted value at the right edge.
    pub show_labels: bool,
}

impl ReferenceLines {
    /// Mean plus a ±1σ band — the "is this normal?" preset.
    pub fn normal_band() -> Self {
        Self {
            lines: vec![StatLine::Mean, StatLine::Sigma(1.0)],
            per_series: false,
            show_labels: true,
        }
    }

    /// Median with p95 and p99 tail markers — the latency-panel preset.
    pub fn tail_quantiles() -> Self {
        Self {
            lines: vec![
                StatLine::Median,
                StatLine::Quantile(0.95),
                StatLine::Quantile(0.99),
            ],
            per_series: false,
            show_labels: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// A fitted overlay drawn on top of each series.
///
/// A dense, noisy series hides its direction. A trend overlay states it:
/// a least-squares line for "which way, and how fast", or a moving average
/// for "what does this look like with the jitter taken out".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrendKind {
    /// Ordinary least-squares straight line across the visible window.
    Linear,
    /// Centered simple moving average over `window` samples.
    MovingAverage { window: usize },
    /// Exponentially weighted moving average with smoothing factor `alpha`.
    Exponential { alpha: f64 },
}

impl TrendKind {
    /// Short label for a legend or callout.
    pub fn label(self) -> String {
        match self {
            Self::Linear => "trend".to_string(),
            Self::MovingAverage { window } => format!("sma {window}"),
            Self::Exponential { alpha } => format!("ema {alpha:.2}"),
        }
    }
}

/// Trend-overlay configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrendConfig {
    /// The fit to draw. `None` (the default) draws no trend.
    pub kind: Option<TrendKind>,
    /// Draw the trend dashed so it never reads as another data series.
    pub dashed: bool,
    /// Stroke opacity in `0..=1`.
    pub opacity: f64,
    /// Stroke width in CSS pixels.
    pub width: f64,
    /// For [`TrendKind::Linear`], annotate the line with its fit quality
    /// (`r²`) and per-unit slope at the right edge.
    pub show_fit_label: bool,
}

impl Default for TrendConfig {
    fn default() -> Self {
        Self {
            kind: None,
            dashed: true,
            opacity: 0.75,
            width: 1.5,
            show_fit_label: false,
        }
    }
}

impl TrendConfig {
    /// A labeled least-squares trend line.
    pub fn linear() -> Self {
        Self {
            kind: Some(TrendKind::Linear),
            show_fit_label: true,
            ..Default::default()
        }
    }

    /// A centered moving average over `window` samples, drawn solid because a
    /// smoothed series is meant to be read as a curve.
    pub fn moving_average(window: usize) -> Self {
        Self {
            kind: Some(TrendKind::MovingAverage { window }),
            dashed: false,
            ..Default::default()
        }
    }

    /// An exponentially weighted moving average.
    pub fn exponential(alpha: f64) -> Self {
        Self {
            kind: Some(TrendKind::Exponential { alpha }),
            dashed: false,
            ..Default::default()
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.kind.is_some()
    }
}

/// Which visible values [`HighlightConfig`] calls out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightMode {
    /// The largest values (the default — peaks are what dashboards are read
    /// for).
    #[default]
    Max,
    /// The smallest values — troughs, drops to zero, availability dips.
    Min,
    /// Both ends: the callout budget is split between largest and smallest.
    Extremes,
    /// The most recent value of each series, whatever its magnitude.
    Last,
}

/// Automatic emphasis for the largest visible values.
///
/// Candidates are selected across the visible viewport, de-duplicated by
/// screen-space distance, and capped by `top_n`, so dense charts gain useful
/// callouts without turning every point into a label.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HighlightConfig {
    /// Enable automatic value markers.
    pub show: bool,
    /// Which visible values to call out.
    pub mode: HighlightMode,
    /// Maximum number of callouts across the chart. `0` disables callouts.
    pub top_n: usize,
    /// Draw the formatted value next to each marker.
    pub show_values: bool,
    /// Minimum horizontal distance between callouts in CSS pixels.
    pub min_spacing_px: f64,
}

impl Default for HighlightConfig {
    fn default() -> Self {
        Self {
            show: true,
            mode: HighlightMode::Max,
            top_n: 1,
            show_values: true,
            min_spacing_px: 48.0,
        }
    }
}

/// When line/step/band charts draw a dot at each data point.
///
/// Sparse series read as ambiguous without markers ("is that a real sample or
/// an interpolation?"), while dense series turn into a solid smear with them.
/// `Auto` resolves that per frame from the visible point count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointMarkers {
    Never,
    /// Draw markers only while the visible points are sparse enough to stay
    /// legible (see [`PointMarkers::AUTO_MAX_POINTS`]).
    #[default]
    Auto,
    Always,
}

impl PointMarkers {
    /// Above this many drawn points per series, `Auto` stops drawing markers.
    pub const AUTO_MAX_POINTS: usize = 60;

    /// Whether a series with `drawn` visible points should get markers.
    pub fn enabled_for(self, drawn: usize) -> bool {
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::Auto => drawn > 1 && drawn <= Self::AUTO_MAX_POINTS,
        }
    }
}

/// Rendering spec: axes, scaling, and geometry knobs. The chart kind lives on
/// [`ChartData`]; visual styling (theme, glow) lives on `RenderOptions`.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSpec {
    /// Y-axis unit (tick label formatting).
    pub unit: Unit,
    /// X-axis unit for `XAxisKind::Linear` tick labels.
    pub x_unit: Unit,
    pub x_axis: XAxisKind,
    /// Linear or base-10 logarithmic y axis.
    pub y_scale: ScaleKind,
    /// Axis title drawn rotated beside the y axis (requires `chrome`).
    pub y_label: Option<String>,
    /// Axis title drawn under the x axis (requires `chrome`).
    pub x_label: Option<String>,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    pub zero_anchored: bool,
    pub chrome: bool,
    pub smooth: bool,
    pub decimation: DecimationKind,
    /// Multi-series composition for Bars and Areas.
    pub layout: SeriesLayout,
    /// Color mapping for heatmap cells.
    pub heatmap_scale: HeatmapColorScale,
    /// Print each series' current value where it can be read without a hover:
    /// at the right edge for line/area/bar/step/scatter/band/OHLC charts, and
    /// at the end of each bar for HBar charts.
    pub last_value_label: bool,
    /// Optional framework-independent canvas legend.
    pub legend: LegendConfig,
    /// Automatic callouts for the most notable visible values.
    pub highlights: HighlightConfig,
    /// Statistical reference lines computed from the visible window.
    pub reference_lines: ReferenceLines,
    /// Fitted trend overlay drawn on top of each series.
    pub trend: TrendConfig,
    /// Per-point dot markers on line, step, and band charts.
    pub points: PointMarkers,
    /// Message painted in the plot area when the data holds nothing drawable.
    /// `None` leaves an empty chart silently blank.
    pub empty_message: Option<String>,
    pub annotations: Vec<Annotation>,
}

impl Default for ChartSpec {
    fn default() -> Self {
        Self {
            unit: Unit::Short,
            x_unit: Unit::Short,
            x_axis: XAxisKind::Time,
            y_scale: ScaleKind::Linear,
            y_label: None,
            x_label: None,
            y_min: None,
            y_max: None,
            zero_anchored: false,
            chrome: true,
            smooth: true,
            decimation: DecimationKind::M4,
            layout: SeriesLayout::Stacked,
            heatmap_scale: HeatmapColorScale::Viridis,
            last_value_label: false,
            legend: LegendConfig::default(),
            highlights: HighlightConfig::default(),
            reference_lines: ReferenceLines::default(),
            trend: TrendConfig::default(),
            points: PointMarkers::default(),
            empty_message: Some("No data".to_string()),
            annotations: vec![],
        }
    }
}

impl ChartSpec {
    /// Switch the y axis to base-10 logarithmic. Non-positive values become
    /// gaps, and zero anchoring is dropped because a log axis has no zero.
    pub fn with_log_y(mut self) -> Self {
        self.y_scale = ScaleKind::Log;
        self.zero_anchored = false;
        self
    }

    /// Set axis titles. Pass `None` to leave an axis untitled.
    pub fn with_axis_labels(
        mut self,
        x_label: Option<impl Into<String>>,
        y_label: Option<impl Into<String>>,
    ) -> Self {
        self.x_label = x_label.map(Into::into);
        self.y_label = y_label.map(Into::into);
        self
    }

    /// Replace the legend configuration.
    pub fn with_legend(mut self, legend: LegendConfig) -> Self {
        self.legend = legend;
        self
    }

    /// Control per-point dot markers.
    pub fn with_points(mut self, points: PointMarkers) -> Self {
        self.points = points;
        self
    }

    /// Draw statistical reference lines computed from the visible window.
    pub fn with_reference_lines(mut self, reference_lines: ReferenceLines) -> Self {
        self.reference_lines = reference_lines;
        self
    }

    /// Draw a fitted trend overlay on top of each series.
    pub fn with_trend(mut self, trend: TrendConfig) -> Self {
        self.trend = trend;
        self
    }

    /// Choose which visible values get automatic callouts.
    pub fn with_highlights(mut self, highlights: HighlightConfig) -> Self {
        self.highlights = highlights;
        self
    }

    pub fn line(unit: Unit) -> Self {
        Self {
            unit,
            ..Default::default()
        }
    }

    pub fn area(unit: Unit) -> Self {
        Self {
            unit,
            zero_anchored: true,
            ..Default::default()
        }
    }

    pub fn bars(unit: Unit) -> Self {
        Self {
            unit,
            zero_anchored: true,
            ..Default::default()
        }
    }

    /// Scatter styling; choose a linear x-axis for numeric correlations.
    pub fn scatter(unit: Unit) -> Self {
        Self {
            unit,
            ..Default::default()
        }
    }

    /// Point plot with a time x-axis; use `scatter` and a linear x-axis for correlations.
    pub fn point(unit: Unit) -> Self {
        Self::line(unit)
    }

    pub fn pie(unit: Unit) -> Self {
        Self {
            unit,
            chrome: false,
            x_axis: XAxisKind::Category { labels: vec![] },
            empty_message: Some("No data".into()),
            ..Self::sparkline()
        }
    }
    pub fn treemap(unit: Unit) -> Self {
        Self::pie(unit)
    }
    pub fn host_map(unit: Unit) -> Self {
        Self::pie(unit)
    }

    pub fn heatmap(unit: Unit) -> Self {
        Self {
            unit,
            ..Default::default()
        }
    }

    pub fn ohlc(unit: Unit) -> Self {
        Self {
            unit,
            ..Default::default()
        }
    }

    pub fn step(unit: Unit) -> Self {
        Self {
            unit,
            smooth: false,
            ..Default::default()
        }
    }

    /// Histogram with a linear bucket x-axis auto-fit to the bucket edges.
    ///
    /// `bucket_unit` formats bucket boundaries and x-axis labels; counts use
    /// [`Unit::Short`]. Use [`Self::histogram_with_units`] when count labels
    /// need a different unit.
    pub fn histogram(bucket_unit: Unit) -> Self {
        Self::histogram_with_units(bucket_unit, Unit::Short)
    }

    /// Histogram with independently formatted bucket and count values.
    pub fn histogram_with_units(bucket_unit: Unit, count_unit: Unit) -> Self {
        Self {
            unit: count_unit,
            x_unit: bucket_unit,
            zero_anchored: true,
            layout: SeriesLayout::Grouped,
            x_axis: XAxisKind::Linear {
                min: None,
                max: None,
            },
            ..Default::default()
        }
    }

    /// Horizontal bars: x is a linear value axis auto-fit to the values.
    pub fn hbar(unit: Unit) -> Self {
        Self {
            unit,
            x_unit: unit,
            x_axis: XAxisKind::Linear {
                min: None,
                max: None,
            },
            chrome: true,
            ..Default::default()
        }
    }

    /// Band (percentile envelope) chart.
    pub fn band(unit: Unit) -> Self {
        Self {
            unit,
            ..Default::default()
        }
    }

    /// State timeline: colored segments per row, no y ticks.
    pub fn state_timeline() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn sparkline() -> Self {
        Self {
            chrome: false,
            highlights: HighlightConfig {
                show: false,
                ..Default::default()
            },
            points: PointMarkers::Never,
            empty_message: None,
            ..Default::default()
        }
    }

    /// Geometry-minimal preset: straight segments, M4 decimation. Pair with
    /// `Aesthetic::clean()` in `RenderOptions` for the fastest render path.
    pub fn flat(unit: Unit) -> Self {
        Self {
            unit,
            smooth: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_disables_automatic_highlights() {
        let spec = ChartSpec::sparkline();
        assert!(!spec.chrome);
        assert!(!spec.highlights.show);
    }

    #[test]
    fn histogram_units_distinguish_bucket_axis_from_counts() {
        let default_counts = ChartSpec::histogram(Unit::Millis);
        assert_eq!(default_counts.x_unit, Unit::Millis);
        assert_eq!(default_counts.unit, Unit::Short);

        let custom_counts = ChartSpec::histogram_with_units(Unit::Seconds, Unit::None);
        assert_eq!(custom_counts.x_unit, Unit::Seconds);
        assert_eq!(custom_counts.unit, Unit::None);
        assert!(matches!(
            custom_counts.x_axis,
            XAxisKind::Linear {
                min: None,
                max: None
            }
        ));
    }

    #[test]
    fn auto_point_markers_only_fire_while_a_series_is_sparse() {
        assert!(PointMarkers::Auto.enabled_for(12));
        assert!(PointMarkers::Auto.enabled_for(PointMarkers::AUTO_MAX_POINTS));
        assert!(!PointMarkers::Auto.enabled_for(PointMarkers::AUTO_MAX_POINTS + 1));
        // One point has no line to disambiguate, and zero has nothing to mark.
        assert!(!PointMarkers::Auto.enabled_for(1));
        assert!(!PointMarkers::Never.enabled_for(3));
        assert!(PointMarkers::Always.enabled_for(100_000));
    }

    #[test]
    fn log_y_builder_drops_zero_anchoring() {
        let spec = ChartSpec::area(Unit::Millis).with_log_y();
        assert_eq!(spec.y_scale, ScaleKind::Log);
        // A log axis has no zero to anchor to.
        assert!(!spec.zero_anchored);
    }

    #[test]
    fn stats_legend_preset_is_visible_and_summarizes() {
        let legend = LegendConfig::with_stats(LegendPosition::Right);
        assert!(legend.show);
        assert_eq!(legend.position, LegendPosition::Right);
        assert_eq!(
            legend.stats,
            vec![LegendStat::Min, LegendStat::Max, LegendStat::Mean]
        );
        assert!(LegendConfig::shown(LegendPosition::Top).stats.is_empty());
        // Legends stay off unless asked for.
        assert!(!LegendConfig::default().show);
    }

    #[test]
    fn charts_explain_themselves_when_empty_but_sparklines_stay_silent() {
        assert_eq!(
            ChartSpec::default().empty_message.as_deref(),
            Some("No data")
        );
        assert!(ChartSpec::sparkline().empty_message.is_none());
        assert!(ChartData::Lines(vec![]).is_empty());
    }

    #[test]
    fn stat_lines_label_themselves_readably() {
        assert_eq!(StatLine::Mean.label(), "mean");
        assert_eq!(StatLine::Quantile(0.95).label(), "p95");
        assert_eq!(StatLine::Quantile(0.999).label(), "p99.9");
        assert_eq!(StatLine::Quantile(0.5).label(), "p50");
        assert_eq!(StatLine::Sigma(2.0).label(), "±2σ");
        assert_eq!(StatLine::Sigma(1.5).label(), "±1.5σ");
    }

    #[test]
    fn stat_lines_resolve_against_a_sample() {
        let values = [2.0, 4.0, 4.0, 6.0];
        let summary = crate::stats::Summary::of_slice(&values).unwrap();
        let sorted = crate::stats::sorted_finite(&values);
        assert_eq!(StatLine::Mean.resolve(&summary, &sorted), Some(4.0));
        assert_eq!(StatLine::Median.resolve(&summary, &sorted), Some(4.0));
        assert_eq!(StatLine::Min.resolve(&summary, &sorted), Some(2.0));
        assert_eq!(StatLine::Max.resolve(&summary, &sorted), Some(6.0));
        // A sigma line resolves to its upper edge; the band carries the rest.
        let upper = StatLine::Sigma(1.0).resolve(&summary, &sorted).unwrap();
        assert!((upper - (4.0 + 2.0f64.sqrt())).abs() < 1e-9);
    }

    #[test]
    fn reference_line_presets_are_visible_but_the_default_draws_nothing() {
        assert!(ReferenceLines::default().is_empty());
        assert!(ChartSpec::default().reference_lines.is_empty());
        let band = ReferenceLines::normal_band();
        assert_eq!(band.lines, vec![StatLine::Mean, StatLine::Sigma(1.0)]);
        assert!(band.show_labels);
        assert!(!band.per_series);
        assert_eq!(ReferenceLines::tail_quantiles().lines.len(), 3);
    }

    #[test]
    fn trend_presets_enable_a_fit_and_the_default_does_not() {
        assert!(!TrendConfig::default().is_enabled());
        assert!(!ChartSpec::default().trend.is_enabled());
        let linear = TrendConfig::linear();
        assert_eq!(linear.kind, Some(TrendKind::Linear));
        assert!(linear.dashed);
        assert!(linear.show_fit_label);
        // A smoothed series is meant to read as a curve, so it is solid.
        assert!(!TrendConfig::moving_average(5).dashed);
        assert_eq!(TrendKind::MovingAverage { window: 5 }.label(), "sma 5");
        assert_eq!(
            TrendConfig::exponential(0.2).kind,
            Some(TrendKind::Exponential { alpha: 0.2 })
        );
    }

    #[test]
    fn highlights_default_to_calling_out_peaks() {
        assert_eq!(HighlightConfig::default().mode, HighlightMode::Max);
        let spec = ChartSpec::line(Unit::Short).with_highlights(HighlightConfig {
            mode: HighlightMode::Extremes,
            top_n: 4,
            ..Default::default()
        });
        assert_eq!(spec.highlights.mode, HighlightMode::Extremes);
        assert_eq!(spec.highlights.top_n, 4);
    }

    #[test]
    fn axis_label_builder_sets_both_titles() {
        let spec = ChartSpec::line(Unit::Short).with_axis_labels(Some("time"), Some("req/s"));
        assert_eq!(spec.x_label.as_deref(), Some("time"));
        assert_eq!(spec.y_label.as_deref(), Some("req/s"));

        let none = ChartSpec::line(Unit::Short).with_axis_labels(None::<String>, Some("req/s"));
        assert!(none.x_label.is_none());
    }
}
