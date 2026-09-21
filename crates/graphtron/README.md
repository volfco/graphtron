# graphtron

A pure-Rust Canvas2D chart renderer for the Wyvrn observability console
(`obs`). No JavaScript charting library — series are stroked directly onto a
`<canvas>` via `web-sys`, which keeps the WASM bundle small and gives full
control over dense, information-rich rendering and cross-chart cursor sync.

`graphtron` is a **renderer**, not a widget: it knows how to turn data + a viewport
into pixels and how to interpret pointer input, but it has no opinion about UI
frameworks. You wire it into your framework of choice (Dioxus, in this
workspace) by owning two canvases and calling the draw functions. See
[`crates/graphtron-demo`](../graphtron-demo) for a complete, runnable Dioxus example.

## Design

Two layers, split by Cargo feature:

- **Geometry core** (always compiled) — scales, tick generation, decimation,
  hit-testing, the interaction state machine, smooth curves (monotone cubic
  Hermite + Catmull-Rom), easing functions, animation state machine, units,
  colors, and layout. Pure computation, no DOM, no framework. Unit-tested on
  the host target.
- **Web layer** (`web` feature) — the Canvas2D draw pipeline (`draw`) with
  themeable aesthetics (glow, gradient fills, vignette), plus a DPR-aware
  surface wrapper (`canvas`). Pulls in `web-sys` / `js-sys` / `wasm-bindgen`.

Each chart renders onto **two stacked `<canvas>` elements**:

| Canvas | Drawn by | Redraws when |
|--------|----------|--------------|
| **data layer** | `draw` | data, time range, size, or theme change |
| **overlay layer** | `draw_overlay` | the cursor moves (mousemove rate) |

The overlay is what makes a synced cursor across many panels cheap: hover
traffic only repaints the thin overlay (crosshairs, bucket bands, guides,
axis-value badges, dots, and tooltip), never the series geometry. Use the
unified `hit_test_chart` API so every `ChartData` variant supplies the hover
shape appropriate to its geometry.

All timestamps are **`f64`/`i64` epoch milliseconds** end to end (exact to
2⁵³ ms, far beyond any real timestamp).

## The data model

The chart kind and its data are **one type**: `ChartData`. You cannot pair a
line spec with OHLC candles — the variant carries the data.

```rust
pub enum ChartData {
    Lines(Vec<SeriesData>),
    Areas(Vec<SeriesData>),         // stacked / overlap / percent via SeriesLayout
    Bars(Vec<SeriesData>),          // stacked / grouped / percent via SeriesLayout
    Scatter(Vec<SeriesData>),
    Points(Vec<SeriesData>),        // discrete samples, without connecting lines
    Pie(Vec<PartitionItem>),
    Treemap(Vec<PartitionItem>),
    HostMap(Vec<PartitionItem>),
    Heatmap(Vec<SeriesData>),
    Ohlc(Vec<OhlcSeriesData>),
    Step(Vec<SeriesData>),
    Histogram(Vec<HistogramSeries>),
    HBar(Vec<HBarSeries>),
    StateTimeline(Vec<StateTimelineSeries>),  // discrete states over time
    Band(Vec<BandSeries>),                    // percentile envelope
}
```

`ChartData::kind()` returns the matching `ChartKind` tag;
`ChartData::from_point_kind(kind, series)` builds point-series variants for
runtime-chosen kinds (dashboard panel styles).

## Optional RRDtool presentation

`RenderOptions::rrdtool()` recreates the classic RRDtool presentation for
`Lines`, `Step`, and `Areas` (including stacked areas). The existing themes
remain the default. The preset includes the gray beveled frame, white plot,
gray minor/red major dashed grids, monospace typography, arrow-ended axes,
opaque staircase traces, and Current/Average/Maximum/Minimum table.

```rust,ignore
let mut options = graphtron::RenderOptions::rrdtool();
let style = options.rrdtool.as_mut().unwrap();
style.title = "router01 — Daily traffic".into();
style.line_series = vec![1]; // AREA inbound, LINE outbound
// Use ChartData::Areas and SeriesLayout::Grouped for this combination.
// Use SeriesLayout::Stacked for CPU/memory stacks.
```

`RrdOptions` also controls the table, slope mode, and provenance watermark.
Colors for data series are caller-selected, as with RRDtool's AREA/LINE
directives. `Theme::rrdtool()` provides only the palette; use the render-options
preset for the complete presentation. Other chart kinds keep their regular
geometry/chrome with the selected palette.

`graphtron-dioxus::GraphtronChart` automatically uses the matching staircase hit test
and remaps line overrides when legend items are hidden. Framework-independent
integrations should pair `draw` with `graphtron::rrd::hit_test` before calling
`draw_overlay`. Hover remains on the overlay canvas, and table statistics
recalculate on data/viewport changes. Printed statistics are arithmetic sample
statistics, using decimal SI abbreviations; missing current samples show
`nan`. Input x values should be finite, ascending and unique, and stacks must
use aligned timestamps. This style is intended for time/numeric axes.

The implementation follows the [upstream graph colors and layout](https://github.com/oetiker/rrdtool-1.x/blob/master/src/rrd_graph.c)
and [graph conventions](https://oss.oetiker.ch/rrdtool/doc/rrdgraph.en.html).
It does not read RRD files, evaluate RPN/GPRINT commands, perform RRA
consolidation, or promise pixel identity with Cairo/Pango; browser font
rasterization and tick selection differ. The table shows up to eight series.

The dedicated `graphtron-demo` page at `/rrdtool` contains three fixed snapshots
and the same three interactive graphs, plus PNG downloads. Static graphs
disable all pointer/keyboard interaction. The interactive set supports shared
cursor/zoom, freeze inspection, drag/pan, wheel/keyboard zoom, and legends.

The RRDtool preset also has a dark variant: `RenderOptions::rrdtool_dark()`.
To toggle an existing graph without replacing its title or other settings,
assign `options.theme = if dark { Theme::rrdtool_dark() } else { Theme::rrdtool() };`.
It uses the black background, white text, muted grids and red arrows of the
[Charles gallery reference](https://oss.oetiker.ch/rrdtool/gallery/charles.png).
Series colors remain application-controlled; the demo uses a matching
blue/cyan/white spectrum in dark mode. The `/rrdtool` page's **Dark mode**
toggle changes both example sets while preserving the interactive viewport.

## Axes

There is **one independent x axis and one independent y axis** per chart.
Dual y axes (left/right with separate units or scales), dual x axes
(top/bottom), and per-series axis assignment are **not supported**. Axis
titles and independently formatted x/y units do not provide secondary axes.
Use separate aligned panels for different units until an explicit secondary
axis model is implemented. Linear/log y mapping, numeric/category/time x
mapping, and fractional numeric domains are covered by the geometry and
adapter tests.

The x axis is not always time. `ChartSpec::x_axis` selects:

```rust
pub enum XAxisKind {
    Time,                                        // viewport-driven epoch ms (default)
    Linear { min: Option<f64>, max: Option<f64> }, // value axis; None = auto from data
    Category { labels: Vec<String> },            // one band per label
}
```

`ChartSpec::histogram()` and `ChartSpec::hbar()` preset `Linear` with auto
bounds, so bucket edges / value ranges label correctly out of the box.

The y axis picks its own mapping through `ChartSpec::y_scale`:

```rust
let spec = ChartSpec::line(Unit::Millis).with_log_y();   // ScaleKind::Log
```

A log axis is what makes p50 and p999 readable in one panel instead of
flattening p50 onto the floor. It stores `log10` bounds internally, so read
positions through `ChartLayout::y_px` / `y_at` / `y_domain` rather than
touching `y_scale.d0`/`d1`. Zero and negative values have no place on a log
axis: they become gaps (the same treatment `NaN` already gets), except for bar
and stack *edges*, which collapse onto the baseline via
`ChartLayout::y_edge_px` so a log bar chart still draws its columns. Domain
selection, tick generation (`log_ticks`, 1-2-5 within a few decades and
thinned powers of ten beyond), hit-testing, hover geometry, and threshold
annotations are all log-aware. `with_log_y()` also drops zero anchoring,
because a log axis has no zero to anchor to.

Both axes take optional titles, drawn in their own reserved bands outside
every other axis decoration:

```rust
let spec = spec.with_axis_labels(Some("time"), Some("latency"));
```
For histograms, `ChartSpec::histogram(bucket_unit)` formats those bucket/x
labels with `bucket_unit` and formats counts with `Unit::Short`; use
`histogram_with_units(bucket_unit, count_unit)` when count labels need a
specific unit.

## Validation, legends, and highlights

Rendering and hit-testing use the safe shared intersection of paired arrays,
so malformed input does not panic. Validate at the ingestion boundary to get
actionable diagnostics instead of silently omitted points:

```rust
if let Err(issues) = data.validate() {
    // Vec<DataIssue>: array-shape, finite-value, ordering, and
    // chart-specific invariant problems.
}

// Spec-aware: also reports values a log y axis will silently draw as gaps.
if let Err(issues) = data.validate_with(&spec) { /* … */ }
```

`ChartSpec::legend` enables a framework-independent canvas legend and reserves
space on its configured `Top`, `Bottom`, `Left`, or `Right` edge. **Heatmaps
get a color ramp instead of swatches** — the value domain drawn as the actual
`HeatmapColorScale` with low/mid/high labels, since a heatmap's cells are
colored by value, not by series index, and per-row palette swatches say
nothing about them. Its
`LegendFormat` chooses name, value, or both, and `LegendConfig::stats` appends
per-series summaries (`Min`, `Max`, `Mean`, `Total`, `Last`) computed over the
finite values, turning the legend into an at-a-glance summary table:

```rust
let spec = spec.with_legend(LegendConfig::with_stats(LegendPosition::Top));
// "p99: 12ms  (min 2ms  max 30ms  mean 9ms)"
```

A series with no finite values contributes no statistics rather than a
fabricated zero, and categorical state timelines get none at all. Values mean: latest finite value
for point, band, and OHLC series; total finite category value for HBar; raw
count total for a normal histogram; final cumulative count for a cumulative
histogram; and latest state label for a state timeline. A UI may instead use
the point-series `legend_entries` helper or its own DOM legend for
interaction/toggling.

`ChartSpec::highlights` selects the most notable visible candidates, applies a
screen-space spacing limit, and can label them with formatted values. It is
bounded (not proportional to full input size), and sorted point/band/OHLC
inputs are sliced to the visible x window before candidates are considered.
Validate sorted, finite x coordinates at ingestion with `ChartData::validate`.
Rendering performs bounded defensive checks; it does not fully validate every
historical sample on each redraw.

`HighlightMode` chooses *which* values earn a callout:

```rust
let spec = spec.with_highlights(HighlightConfig {
    mode: HighlightMode::Extremes,   // Max (default) | Min | Extremes | Last
    top_n: 4,
    ..Default::default()
});
```

`Extremes` splits the callout budget between peaks and troughs, peaks first.
`Last` marks each series' most recent visible value — "where does it stand
now", which is a different question from "where did it peak".

Stacked bars and areas are supported: their candidates are the **accumulated
column top**, valued at the column total, because that is the coordinate the
eye reads off a stack. Positive and negative stacks are called out separately,
matching their independent baselines. `StackedPercent` is excluded, since every
normalized column tops out at 100%.

`ChartSpec::points` dots each drawn sample on line and step charts.
`PointMarkers::Auto` (the default) draws them only while the series is sparse
enough for the dots to mean something — a sparse series otherwise reads as an
interpolation, a dense one turns into a smear — and `Never`/`Always` override
that judgement.

`ChartSpec::last_value_label` prints each series' current value where it can
be read without a hover: past the right edge for line, area, bar, step,
scatter, band, and OHLC charts (the gutter widens to fit), and at the end of
each bar for HBar charts, flipping inward when a bar runs to the plot edge.

`ChartSpec::empty_message` (default `"No data"`) paints a centered message when
the data holds nothing drawable, so an empty panel is distinguishable from a
broken one. Sparklines set it to `None`.

`hit::annotate_delta(&mut hover, &frozen, unit)` turns the freeze cursor into a
measuring tape: it adds a signed `Δ` row (with a percentage, unless the frozen
baseline is zero) to every series the two hovers share. The Dioxus adapter
applies it automatically while the cursor is frozen (`freeze_delta`, on by
default).

Pass `spec.annotations` to `draw_overlay` to paint horizontal thresholds,
horizontal bands, vertical markers, or vertical time bands. The overlay draws
frozen and hover crosshairs separately; optional horizontal guides and axis
badges use the highlighted hover value and the configured axis font.

## Statistics the chart works out for you

A dense stroke shows values; it does not answer *is this normal*, *which way is
it going*, or *how far does the tail run*. `ChartSpec` carries two derived
overlays that answer those directly, both recomputed per frame from the
**visible window** — zoom in and they re-answer for the range on screen instead
of freezing a whole-dataset constant onto the panel.

```rust
use graphtron::{ReferenceLines, StatLine, TrendConfig};

// Mean with a shaded ±1σ band.
let spec = ChartSpec::line(Unit::Millis)
    .with_reference_lines(ReferenceLines::normal_band());

// Median, p95, p99 — the latency-panel preset.
let spec = spec.with_reference_lines(ReferenceLines::tail_quantiles());

// Or build one: Mean | Median | Min | Max | Quantile(q) | Sigma(n)
let spec = spec.with_reference_lines(ReferenceLines {
    lines: vec![StatLine::Quantile(0.95), StatLine::Max],
    per_series: false,   // pooled across the panel (the default)
    show_labels: true,   // "p95 12ms" at the right edge
});
```

Sigma bands are filled **behind** the series so a wide band never hides the
data it describes; the lines and their labels are drawn on top, dashed, and
thinned so two statistics landing on the same pixel row do not overprint.

Trend overlays fit the visible window per series:

```rust
let spec = spec.with_trend(TrendConfig::linear());            // + rate and r²
let spec = spec.with_trend(TrendConfig::moving_average(7));   // centered SMA
let spec = spec.with_trend(TrendConfig::exponential(0.2));    // EWMA
```

`TrendConfig::linear()` labels the line with its direction, its rate (per hour
on a time axis, per x-unit otherwise), and the `r²` that says how much of the
variance the straight line actually explains. Moving averages keep gaps as
gaps rather than smoothing across them, and inherit the chart's decimation
budget so a per-sample average over a dense series stays cheap.

The underlying computation is public and framework-free in `graphtron::stats`:

```rust
graphtron::Summary::of_slice(&values)      // count/min/max/mean/sum/stddev (Welford)
graphtron::quantile(&values, 0.99)         // linearly interpolated, finite-only
graphtron::median(&values)
graphtron::simple_moving_average(&ys, 7)   // O(n) sliding window, gap-preserving
graphtron::exponential_moving_average(&ys, 0.2)
graphtron::linear_fit(&xs, &ys)            // -> LinearFit { slope, intercept, r2, count }
```

`linear_fit` accumulates about the sample means rather than about zero, which
matters here specifically: x is epoch milliseconds (~1.7e12), so naive normal
equations lose most of their significant digits before producing a slope.

## Themes and aesthetics

Colors (`Theme`) are separated from effects (`Aesthetic`), both owned values
that can be built at runtime:

```rust
graphtron::Theme::cyberpunk_dark()   // neon console look (default)
graphtron::Theme::dark()             // muted dark
graphtron::Theme::light()
graphtron::Theme::professional()     // clean high-contrast dark

graphtron::Aesthetic::neon()         // glow + vignette + gradient fills (default)
graphtron::Aesthetic::soft()         // no glow/vignette, gradients kept
graphtron::Aesthetic::clean()        // fastest: flat fills, no effects

graphtron::RenderOptions { theme, aesthetic, axis_font, overlay }
graphtron::RenderOptions::professional_dark()   // preset pairings
graphtron::RenderOptions::professional_light()
```

`RenderOptions::overlay` is an `OverlayOptions` value. It controls vertical
and horizontal guides, axis-value badges, snapping to data, tooltip display,
maximum guide/tooltip rows, whether the top hovered value is shown first, and
the free cursor readout. `OverlayOptions::default()` enables the practical
cursor defaults.

Two horizontal guides serve two different questions, and both are on by
default:

- **Snapped guides** (`show_horizontal_guides`) run from a hovered *data
  value* back to the y axis, so a series' value can be read off the axis
  exactly.
- **The free cursor readout** (`show_cursor_value`) draws a finely dashed line
  at the pointer's own row with the value under the cursor, which is how a
  chart gets read against its gridlines. It needs the pointer row:
  `CursorOverlay::hover_y_px`. Its badge is suppressed when a snapped badge
  already occupies that row. Set `hover_y_px` only on the chart the pointer is
  actually over — a synced sibling panel has no pointer row, and the Dioxus
  adapter already applies that rule.

Theme colors are `Rgba` values (`Rgba::hex(0x5794F2)`,
`Rgba::hex_a(0x00FFFF, 0.15)`, `.to_css()`), so user-chosen accents and
CSS-variable-driven palettes are possible. Axis text is never drawn with
shadow blur unless `Aesthetic::glow_text` is set — glow under labels destroys
legibility.

## Chart type coverage

| Kind | Hover | Callouts | Stats / trend | Layout / rendering notes |
|------|:---:|:---:|:---:|-------|
| `Lines`, `Step` | ✅ | ✅ | ✅ | Viewport-sliced M4 or LTTB; non-finite values break paths; optional per-sample markers. |
| `Areas`, `Bars` | ✅ | ✅ | ✅ | `Grouped`, signed `Stacked`, or signed `StackedPercent`; positive and negative values have separate zero baselines. Stacked callouts sit at the column total. |
| `Scatter` | ✅ | ✅ | ✅ | Batched dots; dense inputs are capped at roughly two points per pixel column. |
| `Heatmap` | ✅ | ✅ | — | Viridis/magma/neon scales with a color-ramp legend; flat and one-column matrices paint; chrome uses row labels instead of numeric y ticks. |
| `Ohlc` | ✅ | ✅ | ✅ | Column aggregation and median-gap candle width; theme supplies up/down colors; statistics use closes. |
| `Histogram` | ✅ | ✅ | — | Linear bucket axis; `Grouped`, `Stacked`, and `StackedPercent`; `cumulative` is honored. |
| `HBar` | ✅ | ✅ | — | Linear value axis, safe category/value intersections, side-by-side series rows, and optional end-of-bar value labels. |
| `StateTimeline` | ✅ | — | — | Crisp time segments, row labels, and semantic state/duration hover values. |
| `Band` | ✅ | ✅ | ✅ | Shaded lower–upper envelope and center line; fill and line do not bridge gaps; statistics use the center. |

Every kind honors `ScaleKind::Log` on the y axis. Row-indexed kinds (`HBar`,
`StateTimeline`) position rows by index, so a log axis has no effect on them.

Dense charts also **auto-degrade effects**: when `points × dpr²` exceeds 500k,
glow and vignette switch off for that frame rather than stalling it.

## Features

```toml
[dependencies]
graphtron = { path = "../graphtron" }              # geometry core only (host/native)
graphtron = { path = "../graphtron", features = ["web"] }   # + Canvas2D draw pipeline
```

- `default` — geometry core only.
- `web` — enables `canvas` and `draw`; intended for a browser/WASM target.

## Public API

```rust
// Spec — how to render (kind travels with the data)
graphtron::ChartSpec {
    unit, x_unit, x_axis, y_scale, y_label, x_label, y_min, y_max,
    zero_anchored, chrome, smooth, decimation, layout, heatmap_scale,
    last_value_label, legend, highlights, reference_lines, trend, points,
    empty_message, annotations,
}
graphtron::ScaleKind::{Linear, Log}
graphtron::PointMarkers::{Never, Auto, Always}
graphtron::ChartSpec::with_log_y() / .with_axis_labels(x, y) / .with_legend(cfg)
                 / .with_points(markers) / .with_reference_lines(lines)
                 / .with_trend(cfg) / .with_highlights(cfg)
graphtron::SeriesLayout::{Stacked, Grouped, StackedPercent}
graphtron::LegendConfig { show, position, format, stats }
graphtron::LegendConfig::shown(position) / ::with_stats(position)
graphtron::LegendStat::{Min, Max, Mean, Total, Last}
graphtron::LegendPosition::{Top, Bottom, Left, Right}
graphtron::LegendFormat::{NameOnly, NameAndValue, ValueOnly}
graphtron::HighlightConfig { show, mode, top_n, show_values, min_spacing_px }
graphtron::HighlightMode::{Max, Min, Extremes, Last}
graphtron::ReferenceLines { lines, per_series, show_labels }
graphtron::ReferenceLines::normal_band() / ::tail_quantiles()
graphtron::StatLine::{Mean, Median, Min, Max, Quantile(q), Sigma(n)}
graphtron::TrendConfig { kind, dashed, opacity, width, show_fit_label }
graphtron::TrendConfig::linear() / ::moving_average(window) / ::exponential(alpha)
graphtron::TrendKind::{Linear, MovingAverage { window }, Exponential { alpha }}
graphtron::ChartSpec::line(unit)      // smooth lines
graphtron::ChartSpec::area(unit)      // zero-anchored, signed stacks by default
graphtron::ChartSpec::bars(unit)      // zero-anchored, signed stacks by default
graphtron::ChartSpec::scatter(unit)
graphtron::ChartSpec::heatmap(unit)
graphtron::ChartSpec::ohlc(unit)
graphtron::ChartSpec::step(unit)      // step-after, no smoothing
graphtron::ChartSpec::histogram(bucket_unit) // Linear bucket x axis; counts use Short
graphtron::ChartSpec::histogram_with_units(bucket_unit, count_unit)
graphtron::ChartSpec::hbar(unit)      // Linear x axis, category rows
graphtron::ChartSpec::band(unit)      // percentile/envelope data
graphtron::ChartSpec::state_timeline()
graphtron::ChartSpec::sparkline()     // chrome-less line; auto highlights off
graphtron::ChartSpec::flat(unit)      // no smoothing (pair with Aesthetic::clean())

// Data — struct-of-arrays; xs ascending epoch-ms, NaN in ys marks a gap
graphtron::ChartData::{Lines, Areas, Bars, Scatter, Heatmap, Ohlc, Step, Histogram, HBar, StateTimeline, Band}
graphtron::ChartData::from_point_kind(kind, series) -> Option<ChartData>
graphtron::ChartData::validate() -> Result<(), Vec<DataIssue>>
graphtron::ChartData::validate_with(&spec) -> Result<(), Vec<DataIssue>>
graphtron::SeriesData { name, xs, ys, color: Option<u32> }
graphtron::OhlcSeriesData { name, ticks: Vec<OhlcTick>, color }
graphtron::HistogramSeries { name, buckets, counts, color, cumulative }
graphtron::HBarSeries { name, categories, values, color }
graphtron::StateTimelineSeries { name, segments: Vec<StateSegment> }
graphtron::BandSeries { name, xs, center, lower, upper, color }
graphtron::series_color(&series, index) -> u32   // explicit color or palette pick
graphtron::median_gap(&values) -> Option<f64>    // robust spacing (bar/candle width)

// Colors
graphtron::Rgba::hex(0xRRGGBB) / ::hex_a(0xRRGGBB, alpha) / .to_css()

// Units — value formatting for axes/tooltips
graphtron::Unit::{Short, Percent01, Percent, Bytes, BytesPerSec, Seconds, Millis, None}
unit.format(value) -> String       // "1.23k", "12.3%", "1.5 KiB", "5ms", …

// Scales / ticks / layout (geometry core)
graphtron::LinearScale::new(d0, d1, r0, r1)  // domain↔pixel; .to_px / .from_px
graphtron::ChartLayout                        // plot rect + x/y scales; .ts_at(px)
layout.y_px(value) / .y_at(px) / .y_domain() / .y_edge_px(v) / .y_baseline_px()
graphtron::ChartLayout::compute_scaled(.., y_kind)  // log-aware layout
graphtron::resolve_x_domain(&x_axis, &data, from_ms, to_ms) -> (f64, f64)
graphtron::data_y_domain(&data, x0, x1, y_min, y_max, zero_anchored, layout) -> (f64, f64)
graphtron::data_y_domain_scaled(.., layout, y_kind) -> (f64, f64)
graphtron::log_ticks(min, max, max_ticks) / graphtron::nice_log_domain(min, max)

// Smooth curves (geometry core)
graphtron::line_path(&pixel_points, smooth) -> Vec<PathOp>  // gap-safe polyline/curve
graphtron::monotone_cubic(&points, &gaps) -> Vec<PathOp>    // D3-class cubic Hermite
graphtron::catmull_rom(&points, tension) -> Vec<PathOp>
graphtron::PathOp::{MoveTo, LineTo, CurveTo}

// Easing & animation (geometry core)
graphtron::ease_out_cubic / ease_in_out_cubic / ease_out_expo / linear
graphtron::Animation::new(from, to, start_ms, duration_ms) / .value_at(now) / .is_done(now)

// Web layer (feature = "web")
graphtron::CanvasSurface::new(canvas) -> Option<CanvasSurface>
surface.sync_size(css_w, css_h) -> bool   // DPR-correct, compare-before-set
graphtron::draw(&surface, &spec, &data, from_ms, to_ms, &opts) -> ChartLayout
graphtron::draw_overlay(&surface, &layout, &data, &cursor, hover_info, selection, &annotations, &opts)
graphtron::CursorOverlay { hover, frozen, unit, hover_y_px }
graphtron::CursorOverlay::at(hover, frozen, unit).with_pointer_row(Some(y_px))
graphtron::RenderOptions { theme, aesthetic, axis_font, overlay }
graphtron::OverlayOptions { show_vertical_guide, show_horizontal_guides, show_axis_values,
                        snap_to_data, max_horizontal_guides, show_tooltip,
                        tooltip_max_values, top_value_first, show_cursor_value }

// Statistics (geometry core)
graphtron::Summary::of_slice(&values) -> Option<Summary>  // count/min/max/mean/sum/stddev
graphtron::quantile(&values, q) / graphtron::quantile_sorted(&sorted, q) / graphtron::median(&values)
graphtron::sorted_finite(&values) -> Vec<f64>
graphtron::simple_moving_average(&ys, window) / graphtron::exponential_moving_average(&ys, alpha)
graphtron::linear_fit(&xs, &ys) -> Option<LinearFit>   // .slope .intercept .r2 .count .at(x)

// Palette
graphtron::palette_color(explicit, index) / series_color / band_series_color
             / hbar_series_color / histogram_series_color / ohlc_series_color

// Hover hit-testing
graphtron::hit_test(&layout, &series, x_px) -> Option<HoverInfo>
graphtron::hit_test_scatter(&layout, &series, mouse_x, mouse_y, radius) -> Option<HoverInfo>
graphtron::hit_test_chart(&layout, &spec, &data, mouse_x, mouse_y) -> Option<HoverInfo>
graphtron::legend_entries(&series, hover_info) -> Vec<LegendEntry>
graphtron::annotate_delta(&mut hover, &frozen, unit)  // Δ rows vs the frozen cursor

// Interaction state machine (pure — pixels in, semantic actions out)
graphtron::InteractState                       // .on_mouse_down/move/up/leave,
                                            // .on_touch_start/move/end, .on_key
graphtron::Action::{Hover, ClearHover, ToggleFreeze, ZoomTo, ResetZoom,
                SelectionChanged, ClearFreeze, PanBy}
graphtron::Key::{ArrowLeft, ArrowRight, ArrowUp, ArrowDown, Home, End, Plus, Minus, Escape}
graphtron::wheel_zoom(from_ms, to_ms, anchor_ms, delta_y) -> (f64, f64)
graphtron::wheel_zoom_with_min_span(from, to, anchor, delta_y, min_span) -> (f64, f64)
```

For bounded interaction in a custom renderer, construct
`ZoomBounds::new(data_start, data_end, minimum_span)` and apply its `clamp`
method to zoom/pan targets before updating the viewport or starting an
animation. `None` selects a stable minimum of 1% of the full data extent;
the maximum is always the full extent. `GraphtronChart` applies these limits
automatically and exposes `min_zoom_span` for an application-specific minimum.

## Usage

### 1. Build data and a spec

```rust
use graphtron::{ChartData, ChartSpec, SeriesData, Unit};

let cpu = SeriesData {
    name: "core 0".into(),
    xs: timestamps_ms,        // Vec<f64>, ascending
    ys: values,               // Vec<f64>, NaN = gap
    color: None,              // None = palette by series index
};
let data = ChartData::Lines(vec![cpu]);
let spec = ChartSpec::line(Unit::Percent);
```

### 2. Draw onto a surface (web feature)

`draw` returns the `ChartLayout` — keep it; pointer handlers and
`draw_overlay` need it.

```rust
let opts = graphtron::RenderOptions::default();   // cyberpunk + neon
let mut surface = graphtron::CanvasSurface::new(canvas_element).unwrap();
surface.sync_size(css_width, css_height);  // call on mount and on resize
let layout = graphtron::draw(&surface, &spec, &data, from_ms, to_ms, &opts);
```

### 3. Overlay: cursor + hover feedback

On a *separate* (stacked) canvas, redraw only the overlay as the cursor moves:

```rust
let pointer_y_px = mouse_y_px; // row/cell-oriented kinds and the free readout
let cursor = graphtron::CursorOverlay::at(Some(ts), None, spec.unit)
    .with_pointer_row(Some(pointer_y_px));
let hover_info = cursor.hover
    .and_then(|ts| graphtron::hit_test_chart(
        &layout,
        &spec,
        &data,
        layout.x_scale.to_px(ts),
        pointer_y_px,
    ));
graphtron::draw_overlay(&overlay_surface, &layout, &data, &cursor,
                    hover_info.as_ref(), /*selection*/ None, &spec.annotations, &opts);
```

### 4. Pointer interaction

Feed element-relative pixel coordinates into `InteractState`; it returns
semantic `Action`s to apply to your time range / cursor state:

```rust
let mut interact = graphtron::InteractState::default();

// in mousemove:
for action in interact.on_mouse_move(x_px, &layout) {
    match action {
        graphtron::Action::Hover(ts)         => set_hover(ts as i64),
        graphtron::Action::SelectionChanged  => request_overlay_redraw(),
        _ => {}
    }
}
// in mouseup:
for action in interact.on_mouse_up(x_px, &layout, graphtron::DragMode::Zoom) {
    match action {
        graphtron::Action::ToggleFreeze(ts)            => toggle_freeze(ts as i64),
        graphtron::Action::ZoomTo { from_ms, to_ms }   => set_range(from_ms as i64, to_ms as i64),
        _ => {}
    }
}
```

### Touch and Dioxus integration

`InteractState` has mouse-shaped touch methods (`on_touch_start`,
`on_touch_move`, `on_touch_end`) and `on_pinch_with_layout` for two-touch zoom
anchored in the actual plot rectangle. Keyboard navigation emits the same
semantic `Action`s as pointer input. For linear or category axes, use
`wheel_zoom_with_min_span` and `on_mouse_up_with_min_span` instead of the
millisecond-oriented defaults.

The crate deliberately does not own Dioxus signals, effects, or DOM events.
The Dioxus integration in `crates/graphtron-demo` demonstrates the intended shape:
keep the layout in component state, render data and overlay canvases
independently, translate Dioxus mouse/touch/keyboard events into
`InteractState` calls, and apply the returned actions to application state.

### Dense data

For result sets larger than ~2 points per pixel, `decimate::min_max` reduces a
series to first/min/max/last per pixel column so spikes survive at any zoom.
`draw` applies the selected decimator to line, step, and grouped/overlaid area
series and scatter after slicing to the visible x domain plus one neighbor on each side;
off-screen history does not distort LTTB selection. Query layers should still
aim aggregation at roughly `width × 2` points so decimation is mostly a no-op.
`DecimationKind::Lttb(n)` is available for shape-preserving downsampling.
Bands and stacks use joint column selection to keep their related boundaries
aligned. Gaps remain explicit; pathological inputs with many distinct gaps
can therefore exceed an ordinary per-column point budget.

Bar spacing uses at most 128 evenly sampled gaps. This keeps overlay work
bounded; normalize irregular bucket grids upstream when exact bucket widths
matter. Histograms retain their explicit edges.

For cumulative histogram hover, build `hit::prepare_histogram_hover` when data
changes and pass the result to `hit::hit_test_chart_with_cache`. Rebuild it
whenever counts or series order change. `graphtron-dioxus` handles this preparation
automatically. The uncached hit-test API remains available for simple callers.

Tooltips reserve a separate value column and truncate long names. Canvas
callers can use `tooltip::tooltip_page` to build an inspector for omitted rows;
the Dioxus adapter provides a paged inspector for frozen cursors.

## Module map

| Module | Contents |
|--------|----------|
| `spec` | `ChartSpec`, `ChartKind`, `ChartData`, axes, layouts, annotations, canvas legend/highlight config |
| `series` | Point, OHLC, histogram, HBar, state-timeline, and band data; palette; `median_gap` |
| `color` | `Rgba` owned color type |
| `units` | `Unit` + `format` (SI, IEC bytes, %, durations) |
| `scale` | `LinearScale` (domain↔pixel, invert, `nice`), `ScaleKind` (linear/log transform) |
| `ticks` | 1-2-5 linear ticks; log ticks + `nice_log_domain`; time-aware ticks; `category_ticks` |
| `layout` | `ChartLayout`, `Rect`, `y_domain`, `data_y_domain`, `resolve_x_domain` |
| `decimate` | `min_max` (M4) and `lttb` reductions |
| `curve` | `line_path`, `monotone_cubic`, `catmull_rom`, `PathOp` |
| `stats` | `Summary` (Welford), quantiles, moving averages, `linear_fit` |
| `easing` | `ease_out_cubic`, `ease_in_out_cubic`, `ease_out_expo`, `linear` |
| `anim` | `Animation` state machine for smooth transitions |
| `hit` | Unified `hit_test_chart`, specialized hit tests, `nearest_index`, `HoverInfo`, `legend_entries` |
| `interact` | `InteractState` state machine, `Action`, `Key`, touch/keyboard, `wheel_zoom` |
| `canvas` *(web)* | `CanvasSurface` (DPR-aware sizing, clear), `crisp`, `crisp_rect` |
| `validation` | `DataIssue`, data-shape/order/chart-invariant validation |
| `draw` *(web)* | `draw`, `draw_overlay`, `CursorOverlay`, `Theme`, `Aesthetic`, `RenderOptions`, `OverlayOptions` |

## Testing

The geometry core runs as ordinary host tests:

```bash
cargo test -p graphtron --all-features --locked
```

Type-check the web layer against the WASM target:

```bash
cargo check -p graphtron --features web --target wasm32-unknown-unknown
```

The Canvas2D regression suite uses deterministic WASM fixtures and Playwright.
It checks actual pixels, text bounds, missing intervals, malformed inputs,
and visible-window drawing budgets at DPR 1, 1.25, 1.5, 2, and 3. Install
Playwright/Chromium separately from the Rust workspace, and use a
`wasm-bindgen` CLI version matching the version in `Cargo.lock`:

```bash
cargo build -p graphtron --example browser_fixture --features web \
  --target wasm32-unknown-unknown --locked
mkdir -p target/graphtron-browser
wasm-bindgen target/wasm32-unknown-unknown/debug/examples/browser_fixture.wasm \
  --target web --out-dir target/graphtron-browser
node crates/graphtron/tests/browser.cjs
```

Set `NODE_PATH` and `PLAYWRIGHT_BROWSERS_PATH` if the browser tools are installed
outside the repository. Results and screenshots go to `target/graphtron-browser`.
The script returns a nonzero exit status when an assertion fails. It uses a
temporary local server and synthetic data, with no platform services required.

## Performance notes

- **Scatter/point plots** use bounded two-dimensional screen bins, preserving
  the interior of clouds with repeated x values. Line-oriented M4 reduction
  would keep extremes and erase that distribution. Hover still reports the
  nearest original observation, not a synthetic aggregate. `SeriesData` x
  values must be sorted, including for scatter plots.
- **State timelines** binary-slice the visible interval before drawing.
  Drawing, cell highlight, and hit testing share inset geometry; ordinary
  cells have equal two-pixel internal gaps, with smaller gaps for tiny cells.
- **Partition widgets** in `graphtron-dioxus` prepare geometry on data/size changes.
  Hover scans prepared shapes and the overlay paints just the selected shape.
  Framework integrations can call `partition::geometry` and retain its result
  for `partition::hit_test`; generic `hit_test_chart` remains a convenient
  uncached fallback.

- **Two-canvas split** — cursor sync repaints only overlays; data geometry is
  retained by the data layer. Use the prepared histogram cache to avoid
  cumulative prefix scans in custom overlay integrations.
- **Pixel-quantize cursor writes** — compare against the last value before
  writing the shared cursor signal to avoid sub-pixel redraw storms.
- **Data effects must not read the cursor signal**, or hover will trigger full
  data redraws.
- **`CanvasSurface::sync_size` is compare-before-set** — resizing clears the
  canvas, so it no-ops when the CSS size and DPR are unchanged.
- **`Aesthetic::clean()` disables glow/vignette/gradients** and
  `ChartSpec::flat()` disables smoothing — combine them for dense data or
  performance-constrained scenarios.

## Known limits

- Treemap data is a flat list of weighted leaves, laid out by balanced binary
  subdivision. Nested hierarchy, drill-down, and group headers are not yet
  modeled. Pie and treemap weights must be finite and non-negative; zero
  weights occupy no area. Validation reports invalid weights and rendering
  skips them. Large finite weights are normalized before summing.
- Host maps use equal square tiles and a fixed 0–100 utilization color scale.
  Supply explicit colors for other metrics or health/status categories.
  Non-finite host metrics remain visible as gray tiles with “No data” hover.

- The canvas legend is presentational. It does not provide hit targets or
  series toggles; applications that need those should render a DOM legend and
  manage visibility themselves (`graphtron-dioxus` already does).
- Stacked bar/area callouts describe the **column total** at the stack top,
  not any single series' contribution. `StackedPercent` gets none, because
  every normalized column tops out at 100%. Histogram callouts retain their
  raw/cumulative bucket value for the label but are positioned at the signed
  accumulated stack top.
- Reference lines and trend overlays apply to kinds with a continuous x and a
  comparable y — point series, band centers, and OHLC closes. Bucket- and
  row-indexed kinds (histogram, HBar, state timeline) get neither.
- Stacked bars, stacked areas, and histogram stack modes are index/bucket
  aligned compositions. Input with unrelated x positions is rendered
  defensively, but should be normalized by the caller for meaningful stacks.
- Dense heatmaps retain fractional row bands. When several rows share one
  device pixel, their colors cannot be visually distinguished individually;
  increase the chart height to inspect individual rows. Hover uses the same
  logical row partition.
- Validation reports malformed data, but rendering remains best-effort and
  uses safe array intersections so a dashboard can still paint valid portions.

## See also

- [`crates/graphtron-demo`](../graphtron-demo) — runnable Dioxus gallery of chart
  types with shared cursor + zoom (`cd crates/graphtron-demo && dx serve`).
- [`frontend/obs/src/components/chart.rs`](../../frontend/obs/src/components/chart.rs) —
  the scope-integrated Dioxus wrapper used by the observability console.
