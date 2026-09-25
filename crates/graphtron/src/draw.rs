use crate::canvas::{CanvasSurface, crisp_rect_at_dpr, crisp_stroke_geometry};
use crate::color::{HeatmapColorScale, Rgba, heatmap_color_for_value};
use crate::curve::{PathOp, line_path};
use crate::hit::HoverInfo;
use crate::layout::{ChartLayout, Rect, data_y_domain_scaled, resolve_x_domain};
use crate::scale::ScaleKind;
use crate::series::{
    HBarSeries, HistogramSeries, OhlcSeriesData, SeriesData, css_color, glow_color, median_gap,
    rgb_to_components, series_color,
};
use crate::spec::{
    Annotation, ChartData, ChartKind, ChartSpec, DecimationKind, HighlightMode, LegendFormat,
    LegendPosition, LegendStat, PointMarkers, SeriesLayout, XAxisKind,
};
use crate::ticks::{category_ticks, format_ts_full, linear_ticks, log_ticks, time_ticks};
use crate::tooltip::draw_tooltip_limited;
use crate::units::Unit;
use wasm_bindgen::JsCast;

/// Colors and per-element styling. Owned values ([`Rgba`]) so themes can be
/// constructed at runtime (user-chosen accents, CSS-variable-driven palettes).
/// Effect knobs (glow, vignette on/off, gradients) live on [`Aesthetic`].
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Tooltip corner radius; zero gives a square, flat tooltip.
    pub tooltip_radius: f64,
    pub dark: bool,
    pub bg: Rgba,
    pub grid: Rgba,
    pub text: Rgba,
    pub accent: u32,
    /// Positive/up value color (OHLC gains, positive deltas).
    pub positive: Rgba,
    /// Negative/down value color (OHLC losses, negative deltas).
    pub negative: Rgba,
    pub hover_line: Rgba,
    pub frozen_line: Rgba,
    pub selection_fill: Rgba,
    pub selection_border: Rgba,
    pub bar_hover_band: Rgba,
    pub bar_frozen_band: Rgba,
    /// Vignette strength in [0, 1]; drawn only when `Aesthetic::vignette`.
    pub vignette: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Self::cyberpunk_dark()
    }
}

impl Theme {
    /// Black-background RRDtool palette inspired by the Charles gallery graph.
    pub fn rrdtool_dark() -> Self {
        Self {
            dark: true,
            bg: Rgba::hex(0x000000),
            grid: Rgba::hex(0x333333),
            text: Rgba::hex(0xffffff),
            accent: 0x00ccff,
            hover_line: Rgba::hex_a(0xffffff, 0.5),
            frozen_line: Rgba::hex(0x00ccff),
            selection_fill: Rgba::hex_a(0x00ccff, 0.15),
            selection_border: Rgba::hex(0x00ccff),
            ..Self::rrdtool()
        }
    }
    /// Classic RRDtool colors. Use `RenderOptions::rrdtool()` for its chrome.
    pub fn rrdtool() -> Self {
        Self {
            tooltip_radius: 0.0,
            bg: Rgba::hex(0xf2f2f2),
            grid: Rgba::hex_a(0x8f8f8f, 0.75),
            text: Rgba::hex(0x000000),
            accent: 0x801f1f,
            hover_line: Rgba::hex_a(0x202020, 0.4),
            frozen_line: Rgba::hex(0x801f1f),
            selection_fill: Rgba::hex_a(0x801f1f, 0.10),
            selection_border: Rgba::hex(0x801f1f),
            ..Self::light()
        }
    }
    pub fn cyberpunk_dark() -> Self {
        Self {
            dark: true,
            tooltip_radius: 4.0,
            bg: Rgba::hex(0x0a0e17),
            grid: Rgba::hex_a(0x00FFFF, 0.06),
            // R-2: #8899aa on #0a0e17 is ~5.5:1 contrast (the old #5a6a7a was
            // below the 4.5:1 AA floor).
            text: Rgba::hex(0x8899aa),
            accent: 0x00FFFF,
            positive: Rgba::hex(0x00d084),
            negative: Rgba::hex(0xff4d6d),
            hover_line: Rgba::hex_a(0xFFFFFF, 0.30),
            frozen_line: Rgba::hex(0x00FFFF),
            selection_fill: Rgba::hex_a(0x00FFFF, 0.08),
            selection_border: Rgba::hex_a(0x00FFFF, 0.50),
            bar_hover_band: Rgba::hex_a(0xFFFFFF, 0.08),
            bar_frozen_band: Rgba::hex_a(0x00FFFF, 0.15),
            vignette: 0.15,
        }
    }

    pub fn dark() -> Self {
        Self {
            tooltip_radius: 4.0,
            dark: true,
            bg: Rgba::hex(0x111217),
            grid: Rgba::hex_a(0xFFFFFF, 0.06),
            text: Rgba::hex(0x9aa0a6),
            accent: 0x5794F2,
            positive: Rgba::hex(0x2ecc71),
            negative: Rgba::hex(0xff5c5c),
            hover_line: Rgba::hex_a(0xFFFFFF, 0.35),
            frozen_line: Rgba::hex(0x5794f2),
            selection_fill: Rgba::hex_a(0x5794F2, 0.15),
            selection_border: Rgba::hex_a(0x5794F2, 0.6),
            bar_hover_band: Rgba::hex_a(0xFFFFFF, 0.10),
            bar_frozen_band: Rgba::hex_a(0x5794F2, 0.20),
            vignette: 0.0,
        }
    }

    pub fn light() -> Self {
        Self {
            tooltip_radius: 4.0,
            dark: false,
            bg: Rgba::hex(0xffffff),
            grid: Rgba::hex_a(0x000000, 0.08),
            text: Rgba::hex(0x5f6368),
            accent: 0x5794F2,
            positive: Rgba::hex(0x18864b),
            negative: Rgba::hex(0xc62828),
            hover_line: Rgba::hex_a(0x000000, 0.35),
            frozen_line: Rgba::hex(0x5794f2),
            selection_fill: Rgba::hex_a(0x5794F2, 0.10),
            selection_border: Rgba::hex_a(0x5794F2, 0.5),
            bar_hover_band: Rgba::hex_a(0x000000, 0.06),
            bar_frozen_band: Rgba::hex_a(0x5794F2, 0.15),
            vignette: 0.0,
        }
    }

    /// Clean, high-contrast dark theme for use outside the cyberpunk console.
    /// Pair with a glow-free [`Aesthetic`].
    pub fn professional() -> Self {
        Self {
            tooltip_radius: 4.0,
            dark: true,
            bg: Rgba::hex(0x16171d),
            grid: Rgba::hex_a(0xFFFFFF, 0.08),
            text: Rgba::hex(0xc8cdd4),
            accent: 0x5794F2,
            positive: Rgba::hex(0x2ecc71),
            negative: Rgba::hex(0xff5c5c),
            hover_line: Rgba::hex_a(0xFFFFFF, 0.40),
            frozen_line: Rgba::hex(0x5794f2),
            selection_fill: Rgba::hex_a(0x5794F2, 0.15),
            selection_border: Rgba::hex_a(0x5794F2, 0.6),
            bar_hover_band: Rgba::hex_a(0xFFFFFF, 0.10),
            bar_frozen_band: Rgba::hex_a(0x5794F2, 0.20),
            vignette: 0.0,
        }
    }
}

/// Effect knobs, separated from [`Theme`] colors: the same palette can render
/// neon (glow + vignette + gradient fills) or flat/fast.
#[derive(Debug, Clone, PartialEq)]
pub struct Aesthetic {
    pub glow: bool,
    pub glow_intensity: f64,
    pub vignette: bool,
    pub gradient_fills: bool,
    /// Allow shadowBlur under axis text. Off by default: glow under labels
    /// destroys legibility, so text is always drawn with shadows cleared
    /// unless this is explicitly set.
    pub glow_text: bool,
}

impl Default for Aesthetic {
    fn default() -> Self {
        Self::neon()
    }
}

impl Aesthetic {
    /// The cyberpunk-console look: glow, vignette, gradient fills.
    pub fn neon() -> Self {
        Self {
            glow: true,
            glow_intensity: 0.6,
            vignette: true,
            gradient_fills: true,
            glow_text: false,
        }
    }

    /// Soft professional look: no glow or vignette, gradients kept.
    pub fn soft() -> Self {
        Self {
            glow: false,
            glow_intensity: 0.0,
            vignette: false,
            gradient_fills: true,
            glow_text: false,
        }
    }

    /// Fastest path: no glow, no vignette, flat fills.
    pub fn clean() -> Self {
        Self {
            glow: false,
            glow_intensity: 0.0,
            vignette: false,
            gradient_fills: false,
            glow_text: false,
        }
    }
}

/// Consolidated rendering options: theme colors, effect knobs, fonts.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    /// Opt-in classic RRDtool frame, grid and statistics table for time series.
    pub rrdtool: Option<crate::rrd::RrdOptions>,
    pub theme: Theme,
    pub aesthetic: Aesthetic,
    pub axis_font: String,
    /// Cursor, guide-line, axis-readout, and tooltip behavior.
    pub overlay: OverlayOptions,
}

/// Interaction-overlay rendering controls.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayOptions {
    pub show_vertical_guide: bool,
    /// Draw horizontal guides from hovered values back to the y axis.
    pub show_horizontal_guides: bool,
    /// Draw compact x/y value badges at the axes.
    pub show_axis_values: bool,
    /// Snap the hover guide to the selected data point/bucket.
    pub snap_to_data: bool,
    /// Maximum simultaneous horizontal guides. The highlighted top value is
    /// always considered first.
    pub max_horizontal_guides: usize,
    pub show_tooltip: bool,
    /// Maximum series rows before the tooltip collapses the remainder.
    pub tooltip_max_values: usize,
    /// Put the automatically highlighted largest hovered value first.
    pub top_value_first: bool,
    /// Draw a free horizontal crosshair at the pointer row with the y value
    /// under the cursor, in addition to the data-snapped guides. Requires
    /// [`CursorOverlay::hover_y_px`].
    pub show_cursor_value: bool,
}

impl Default for OverlayOptions {
    fn default() -> Self {
        Self {
            show_vertical_guide: true,
            show_horizontal_guides: true,
            show_axis_values: true,
            snap_to_data: true,
            max_horizontal_guides: 1,
            show_tooltip: true,
            tooltip_max_values: 12,
            top_value_first: true,
            show_cursor_value: true,
        }
    }
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            rrdtool: None,
            theme: Theme::cyberpunk_dark(),
            aesthetic: Aesthetic::neon(),
            axis_font: AXIS_FONT.to_string(),
            overlay: OverlayOptions::default(),
        }
    }
}

impl From<bool> for RenderOptions {
    fn from(dark: bool) -> Self {
        if dark {
            Self::default()
        } else {
            Self {
                theme: Theme::light(),
                aesthetic: Aesthetic::soft(),
                ..Default::default()
            }
        }
    }
}

impl RenderOptions {
    pub fn rrdtool_dark() -> Self {
        Self {
            theme: Theme::rrdtool_dark(),
            ..Self::rrdtool()
        }
    }
    pub fn rrdtool() -> Self {
        Self {
            rrdtool: Some(crate::rrd::RrdOptions::default()),
            theme: Theme::rrdtool(),
            aesthetic: Aesthetic::clean(),
            axis_font: crate::rrd::AXIS_FONT.into(),
            ..Default::default()
        }
    }
    pub fn professional_dark() -> Self {
        Self {
            theme: Theme::professional(),
            aesthetic: Aesthetic::soft(),
            ..Default::default()
        }
    }

    pub fn professional_light() -> Self {
        Self {
            theme: Theme::light(),
            aesthetic: Aesthetic::soft(),
            ..Default::default()
        }
    }
}

const AXIS_FONT: &str = "10px 'JetBrains Mono', monospace";

/// Minimum pixel spacing between adjacent axis labels before thinning (R-4).
const MIN_LABEL_SPACING_PX: f64 = 16.0;

fn crisp_for(surface: &CanvasSurface, value: f64, width: f64) -> f64 {
    crisp_stroke_geometry(value, surface.dpr, width).0
}

fn crisp_width(surface: &CanvasSurface, width: f64) -> f64 {
    crisp_stroke_geometry(0.0, surface.dpr, width).1
}

/// Keep every k-th tick so adjacent kept ticks are at least `min_px` apart.
/// Assumes roughly even spacing (true for linear/category ticks).
fn thin_by_spacing<T>(ticks: Vec<T>, to_px: impl Fn(&T) -> f64, min_px: f64) -> Vec<T> {
    if ticks.len() < 2 {
        return ticks;
    }
    let spacing = (to_px(&ticks[1]) - to_px(&ticks[0])).abs();
    if spacing >= min_px || spacing <= 0.0 {
        return ticks;
    }
    let k = (min_px / spacing).ceil() as usize;
    ticks
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % k == 0)
        .map(|(_, t)| t)
        .collect()
}

/// Greedy collision filter for unevenly-spaced ticks (especially log axes).
/// Unlike `thin_by_spacing`, this checks every retained pair in pixel space.
fn thin_by_pixel_distance<T>(ticks: Vec<T>, to_px: impl Fn(&T) -> f64, min_px: f64) -> Vec<T> {
    let mut out = Vec::with_capacity(ticks.len());
    let mut last = f64::NEG_INFINITY;
    for tick in ticks {
        let px = to_px(&tick);
        if !px.is_finite() {
            continue;
        }
        if out.is_empty() || (px - last).abs() >= min_px {
            last = px;
            out.push(tick);
        }
    }
    out
}

fn thin_x_labels(
    ctx: &web_sys::CanvasRenderingContext2d,
    labels: Vec<(f64, String)>,
) -> Vec<(f64, String)> {
    let mut out = Vec::with_capacity(labels.len());
    let mut right = f64::NEG_INFINITY;
    for (x, label) in labels {
        let width = ctx.measure_text(&label).map_or(0.0, |m| m.width());
        let left = x - width / 2.0;
        let end = x + width / 2.0;
        if out.is_empty() || left >= right + 4.0 {
            right = end;
            out.push((x, label));
        }
    }
    out
}

struct DrawConfig {
    smooth: bool,
    glow: bool,
    glow_intensity: f64,
    gradient_fills: bool,
    decimation: DecimationKind,
    points: PointMarkers,
}

#[derive(Debug, Clone)]
struct CanvasLegendEntry {
    name: String,
    color: u32,
    value: Option<String>,
    /// Preformatted `"min 3ms"`-style summaries, in configured order.
    stats: Vec<String>,
}

/// Format the configured summary statistics over a series' finite values.
/// Non-numeric series (state timelines) pass an empty iterator and get none.
fn summarize(values: impl Iterator<Item = f64>, stats: &[LegendStat], unit: Unit) -> Vec<String> {
    if stats.is_empty() {
        return vec![];
    }
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut total = 0.0;
    let mut count = 0usize;
    let mut last = f64::NAN;
    for value in values.filter(|value| value.is_finite()) {
        min = min.min(value);
        max = max.max(value);
        total += value;
        last = value;
        count += 1;
    }
    if count == 0 {
        return vec![];
    }
    stats
        .iter()
        .map(|stat| {
            let value = match stat {
                LegendStat::Min => min,
                LegendStat::Max => max,
                LegendStat::Mean => total / count as f64,
                LegendStat::Total => total,
                LegendStat::Last => last,
            };
            format!("{} {}", stat.label(), unit.format(value))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Default)]
struct LegendReservation {
    top: f64,
    bottom: f64,
    left: f64,
    right: f64,
}

impl DrawConfig {
    fn new(spec: &ChartSpec, aesthetic: &Aesthetic) -> Self {
        Self {
            smooth: spec.smooth,
            glow: aesthetic.glow,
            glow_intensity: aesthetic.glow_intensity,
            gradient_fills: aesthetic.gradient_fills,
            decimation: spec.decimation,
            points: spec.points,
        }
    }
}

/// Dot each drawn sample on a line or step chart, so a sparse series reads as
/// measurements rather than as an interpolation. Non-finite entries are gaps
/// and get no marker.
fn draw_point_markers(
    ctx: &web_sys::CanvasRenderingContext2d,
    pixel_points: &[(f64, f64)],
    color: u32,
    config: &DrawConfig,
) {
    let drawn = pixel_points
        .iter()
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .count();
    if !config.points.enabled_for(drawn) {
        return;
    }
    let points: Vec<(f64, f64)> = pixel_points
        .iter()
        .copied()
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();
    let colors = SeriesColors::new(color);
    batch_glow_dots(
        ctx,
        &points,
        &colors.core,
        2.5,
        config
            .glow
            .then_some((colors.glow_shadow.as_str(), 6.0 * config.glow_intensity)),
        None,
    );
}

/// Per-series CSS strings computed once per frame instead of per point (P-2).
struct SeriesColors {
    core: String,
    glow_shadow: String,
}

impl SeriesColors {
    fn new(color: u32) -> Self {
        let (r, g, b) = rgb_to_components(color);
        Self {
            core: css_color(color),
            glow_shadow: format!("rgba({r},{g},{b},0.6)"),
        }
    }
}

/// Draw a set of dots in two batched passes: one glowing halo pass (shadow
/// state set once, all circles in a single path, one fill) and one bright
/// core pass. Replaces per-point shadow set/reset — the dominant Canvas2D
/// state-churn cost for scatter/step charts (P-1).
fn batch_glow_dots(
    ctx: &web_sys::CanvasRenderingContext2d,
    points: &[(f64, f64)],
    halo_css: &str,
    halo_radius: f64,
    glow: Option<(&str, f64)>,
    core: Option<(&str, f64)>,
) {
    if points.is_empty() {
        return;
    }
    if let Some((shadow_css, blur)) = glow {
        ctx.set_shadow_color(shadow_css);
        ctx.set_shadow_blur(blur);
    }
    ctx.set_fill_style_str(halo_css);
    ctx.begin_path();
    for (x, y) in points {
        ctx.move_to(x + halo_radius, *y);
        let _ = ctx.arc(*x, *y, halo_radius, 0.0, std::f64::consts::TAU);
    }
    ctx.fill();
    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);

    if let Some((core_css, core_radius)) = core {
        ctx.set_fill_style_str(core_css);
        ctx.begin_path();
        for (x, y) in points {
            ctx.move_to(x + core_radius, *y);
            let _ = ctx.arc(*x, *y, core_radius, 0.0, std::f64::consts::TAU);
        }
        ctx.fill();
    }
}

fn make_linear_gradient(
    ctx: &web_sys::CanvasRenderingContext2d,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) -> web_sys::CanvasGradient {
    ctx.create_linear_gradient(x0, y0, x1, y1)
}

fn make_radial_gradient(
    ctx: &web_sys::CanvasRenderingContext2d,
    x0: f64,
    y0: f64,
    r0: f64,
    x1: f64,
    y1: f64,
    r1: f64,
) -> web_sys::CanvasGradient {
    ctx.create_radial_gradient(x0, y0, r0, x1, y1, r1).unwrap()
}

#[allow(deprecated)]
fn set_fill_gradient(ctx: &web_sys::CanvasRenderingContext2d, grad: &web_sys::CanvasGradient) {
    ctx.set_fill_style(grad.unchecked_ref());
}

/// Decimate a series according to the configured DecimationKind.
pub(crate) fn decimate_series(
    xs: &[f64],
    ys: &[f64],
    x_scale: &crate::scale::LinearScale,
    px_width: f64,
    kind: DecimationKind,
    y_kind: ScaleKind,
) -> Vec<(f64, f64)> {
    let n = xs.len().min(ys.len());
    if n == 0 {
        return vec![];
    }
    // Both decimators must see only the viewport. Feeding all history to
    // LTTB makes a zoom select triangles from off-screen data; feeding it to
    // M4 wastes work on bins which cannot be painted. Keep one neighbor on
    // either side so a segment crossing the plot edge remains continuous.
    let xs = &xs[..n];
    let ys = &ys[..n];
    let (start, end) = if xs.len() < 2 || !sorted_sample_hint(xs) {
        (0, n)
    } else {
        (
            xs.partition_point(|x| *x < x_scale.d0).saturating_sub(1),
            xs.partition_point(|x| *x <= x_scale.d1)
                .saturating_add(1)
                .min(n),
        )
    };
    let (xs, ys) = if start < end {
        (&xs[start..end], &ys[start..end])
    } else {
        (&xs[0..0], &ys[0..0])
    };
    // A non-positive value is not representable on a log axis. Turn it into
    // an explicit gap before reduction; otherwise a reducer may discard the
    // invalid sample and make the renderer bridge across it.
    let safe_ys: Option<Vec<f64>> = (y_kind == ScaleKind::Log).then(|| {
        ys.iter()
            .map(|y| {
                if *y > 0.0 && y.is_finite() {
                    *y
                } else {
                    f64::NAN
                }
            })
            .collect()
    });
    let ys = safe_ys.as_deref().unwrap_or(ys);
    match kind {
        DecimationKind::M4 => crate::decimate::min_max(xs, ys, x_scale, px_width),
        DecimationKind::Lttb(threshold) => crate::decimate::lttb(xs, ys, threshold),
    }
}

/// Render a chart: *(surface, spec, data, viewport, options) → pixels*.
///
/// The kind is carried by `data` ([`ChartData`]), so a kind can never be
/// paired with the wrong series type. `from_ms`/`to_ms` drive the x domain
/// for `XAxisKind::Time`; other axis kinds resolve their domain from the spec
/// or the data.
pub fn draw(
    surface: &CanvasSurface,
    spec: &ChartSpec,
    data: &ChartData,
    from_ms: f64,
    to_ms: f64,
    opts: &RenderOptions,
) -> ChartLayout {
    if let Some(style) = &opts.rrdtool
        && crate::rrd::supports(data)
    {
        return crate::rrd::draw(surface, spec, data, from_ms, to_ms, opts, style);
    }
    let mut partition_spec;
    let spec = if crate::partition::items(data).is_some() {
        partition_spec = spec.clone();
        partition_spec.chrome = false;
        partition_spec.x_axis = crate::XAxisKind::Linear {
            min: Some(0.0),
            max: Some(1.0),
        };
        partition_spec.y_label = None;
        partition_spec.x_label = None;
        &partition_spec
    } else {
        spec
    };
    let theme = &opts.theme;

    // Auto-degrade expensive effects when the workload is heavy: shadowBlur
    // cost scales with the DPR-scaled backing store, so dense data on a
    // high-DPR panel silently stalls frames otherwise (P-5).
    let heavy = (data_point_count(data) as f64) * surface.dpr * surface.dpr > 500_000.0;
    let effective_aesthetic;
    let aesthetic = if heavy && (opts.aesthetic.glow || opts.aesthetic.vignette) {
        effective_aesthetic = Aesthetic {
            glow: false,
            vignette: false,
            ..opts.aesthetic.clone()
        };
        &effective_aesthetic
    } else {
        &opts.aesthetic
    };

    let config = DrawConfig::new(spec, aesthetic);
    let legend_entries = canvas_legend_entries(data, spec.unit, &spec.legend.stats);
    let legend_reservation =
        legend_reservation(surface, spec, data, &legend_entries, &opts.axis_font);
    let axis_label_reservation = axis_label_reservation(surface, spec, data, &opts.axis_font);
    let (x0, x1) = resolve_x_domain(&spec.x_axis, data, from_ms, to_ms);
    let zero_anchor =
        spec.zero_anchored || matches!(data, ChartData::Bars(_) | ChartData::Areas(_));
    let (y_min, y_max) = data_y_domain_scaled(
        data,
        x0,
        x1,
        spec.y_min,
        spec.y_max,
        zero_anchor,
        spec.layout,
        spec.y_scale,
    );
    let (ml, mr, mt, mb) = if spec.chrome {
        (
            crate::layout::MARGIN_LEFT + legend_reservation.left + axis_label_reservation.left,
            crate::layout::MARGIN_RIGHT + legend_reservation.right + axis_label_reservation.right,
            crate::layout::MARGIN_TOP + legend_reservation.top,
            crate::layout::MARGIN_BOTTOM
                + legend_reservation.bottom
                + axis_label_reservation.bottom,
        )
    } else {
        (
            1.0 + legend_reservation.left + axis_label_reservation.left,
            1.0 + legend_reservation.right + axis_label_reservation.right,
            1.0 + legend_reservation.top,
            1.0 + legend_reservation.bottom,
        )
    };
    let layout = ChartLayout::compute_scaled(
        surface.css_w,
        surface.css_h,
        x0,
        x1,
        y_min,
        y_max,
        ml,
        mr,
        mt,
        mb,
        spec.y_scale,
    );

    surface.clear();
    surface.ctx.set_fill_style_str(&theme.bg.to_css());
    surface
        .ctx
        .fill_rect(0.0, 0.0, surface.css_w, surface.css_h);
    if spec.chrome {
        draw_grid(
            surface,
            &layout,
            spec,
            data,
            theme,
            aesthetic,
            &opts.axis_font,
        );
    }
    // Resolved once: the band pass draws behind the series and the line pass
    // on top of it, from the same statistics.
    let reference_stats = if spec.reference_lines.is_empty() {
        vec![]
    } else {
        let samples = reference_samples(&layout, data);
        resolve_reference_stats(spec, &samples, theme.accent)
    };
    draw_reference_bands(surface, &layout, &reference_stats);

    match data {
        ChartData::Lines(series) => {
            for (i, s) in series.iter().enumerate() {
                stroke_series(surface, &layout, s, series_color(s, i), false, &config);
            }
        }
        ChartData::Areas(series) => match spec.layout {
            SeriesLayout::Grouped => {
                for (i, s) in series.iter().enumerate() {
                    stroke_series(surface, &layout, s, series_color(s, i), true, &config);
                }
            }
            SeriesLayout::Stacked => draw_stacked_areas(surface, &layout, series, &config, false),
            SeriesLayout::StackedPercent => {
                draw_stacked_areas(surface, &layout, series, &config, true)
            }
        },
        ChartData::Bars(series) => match spec.layout {
            SeriesLayout::Grouped => draw_grouped_bars(surface, &layout, series, &config),
            SeriesLayout::Stacked => draw_stacked_bars(surface, &layout, series, &config),
            SeriesLayout::StackedPercent => {
                let normalized = normalize_percent(series);
                draw_stacked_bars(surface, &layout, &normalized, &config)
            }
        },
        ChartData::Points(series) | ChartData::Scatter(series) => {
            for (i, s) in series.iter().enumerate() {
                draw_scatter(surface, &layout, s, series_color(s, i), &config);
            }
        }
        ChartData::Heatmap(series) => draw_heatmap(
            surface,
            &layout,
            series,
            spec.heatmap_scale,
            spec.chrome,
            theme,
            &opts.axis_font,
        ),
        ChartData::Ohlc(series) => draw_ohlc(surface, &layout, series, theme),
        ChartData::Step(series) => {
            for (i, s) in series.iter().enumerate() {
                draw_step(surface, &layout, s, series_color(s, i), &config);
            }
        }
        ChartData::Histogram(series) => {
            draw_histogram(surface, &layout, series, spec.layout, &config)
        }
        ChartData::HBar(series) => {
            draw_hbar(surface, &layout, series, &config, theme, &opts.axis_font);
            if spec.last_value_label {
                draw_hbar_value_labels(surface, &layout, series, spec.unit, &opts.axis_font, theme);
            }
        }
        ChartData::StateTimeline(series) => {
            draw_state_timeline(surface, &layout, series, theme, &opts.axis_font)
        }
        ChartData::Band(series) => draw_band(surface, &layout, series, &config),
        ChartData::Pie(items) | ChartData::Treemap(items) | ChartData::HostMap(items) => {
            for (i, shape) in crate::partition::geometry(data.kind(), items, layout.plot) {
                crate::partition::paint(
                    surface,
                    shape,
                    &css_color(crate::partition::item_color(data.kind(), items, i)),
                );
                if let crate::partition::Shape::Tile(rect) = shape {
                    let label = &items[i].name;
                    let ctx = &surface.ctx;
                    ctx.set_font(&opts.axis_font);
                    if rect.h >= 18.0
                        && ctx
                            .measure_text(label)
                            .is_ok_and(|m| m.width() + 10.0 <= rect.w)
                    {
                        let color = crate::partition::item_color(data.kind(), items, i);
                        let (r, g, b) = rgb_to_components(color);
                        ctx.set_fill_style_str(
                            if 299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b) > 150000
                            {
                                "#111111"
                            } else {
                                "#ffffff"
                            },
                        );
                        ctx.set_text_align("center");
                        ctx.set_text_baseline("middle");
                        let _ = ctx.fill_text(label, rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
                    }
                }
            }
        }
    }

    if data.is_empty()
        && let Some(message) = &spec.empty_message
    {
        draw_empty_state(surface, &layout, message, theme);
    }
    // After the series, so a label is never painted over by its own stroke.
    if spec.last_value_label && spec.chrome {
        let values = latest_visible_values(&layout, data);
        draw_last_value_labels(surface, &layout, &values, spec.unit, &opts.axis_font);
    }
    draw_trend_overlays(surface, &layout, spec, data, opts);
    draw_reference_lines(surface, &layout, spec, &reference_stats, opts);
    draw_axis_titles(surface, &layout, spec, theme, &opts.axis_font);
    draw_highlights(surface, &layout, spec, data, opts);
    draw_canvas_legend(
        surface,
        &layout,
        spec,
        data,
        &legend_entries,
        legend_reservation,
        opts,
    );

    if aesthetic.vignette && theme.vignette > 0.0 && spec.chrome {
        draw_vignette(surface, &layout, theme.vignette);
    }

    layout
}

/// Total drawable point count, for the automatic effect downgrade.
fn data_point_count(data: &ChartData) -> usize {
    match data {
        ChartData::Ohlc(s) => s.iter().map(|x| x.ticks.len()).sum(),
        ChartData::Histogram(s) => s.iter().map(|x| x.counts.len()).sum(),
        ChartData::HBar(s) => s.iter().map(|x| x.values.len()).sum(),
        ChartData::StateTimeline(s) => s.iter().map(|x| x.segments.len()).sum(),
        ChartData::Band(s) => s.iter().map(|x| x.xs.len() * 3).sum(),
        _ => data.point_series().iter().map(|x| x.ys.len()).sum(),
    }
}

fn canvas_legend_entries(
    data: &ChartData,
    unit: Unit,
    stats: &[LegendStat],
) -> Vec<CanvasLegendEntry> {
    if let Some(items) = crate::partition::items(data) {
        return items
            .iter()
            .enumerate()
            .map(|(i, item)| CanvasLegendEntry {
                name: item.name.clone(),
                color: crate::partition::item_color(data.kind(), items, i),
                value: Some(unit.format(item.value)),
                stats: vec![],
            })
            .collect();
    }
    let point = |series: &[SeriesData]| {
        series
            .iter()
            .enumerate()
            .map(|(index, series)| CanvasLegendEntry {
                name: series.name.clone(),
                color: series_color(series, index),
                value: series
                    .ys
                    .iter()
                    .rev()
                    .copied()
                    .find(|value| value.is_finite())
                    .map(|value| unit.format(value)),
                stats: summarize(series.ys.iter().copied(), stats, unit),
            })
            .collect()
    };
    match data {
        ChartData::Lines(series)
        | ChartData::Areas(series)
        | ChartData::Bars(series)
        | ChartData::Points(series)
        | ChartData::Scatter(series)
        | ChartData::Heatmap(series)
        | ChartData::Step(series) => point(series),
        ChartData::Ohlc(series) => series
            .iter()
            .enumerate()
            .map(|(index, series)| CanvasLegendEntry {
                name: series.name.clone(),
                color: crate::series::palette_color(series.color, index),
                value: series
                    .ticks
                    .iter()
                    .rev()
                    .find(|tick| tick.close.is_finite())
                    .map(|tick| unit.format(tick.close)),
                stats: summarize(series.ticks.iter().map(|tick| tick.close), stats, unit),
            })
            .collect(),
        ChartData::Histogram(series) => series
            .iter()
            .enumerate()
            .map(|(index, series)| CanvasLegendEntry {
                name: series.name.clone(),
                color: crate::series::palette_color(series.color, index),
                value: if series.cumulative {
                    series
                        .plot_counts()
                        .into_iter()
                        .rev()
                        .find(|value| value.is_finite())
                        .map(|value| unit.format(value))
                } else {
                    Some(
                        unit.format(
                            series
                                .counts
                                .iter()
                                .copied()
                                .filter(|value| value.is_finite())
                                .sum(),
                        ),
                    )
                },
                stats: summarize(series.plot_counts().into_iter(), stats, unit),
            })
            .collect(),
        ChartData::HBar(series) => series
            .iter()
            .enumerate()
            .map(|(index, series)| CanvasLegendEntry {
                name: series.name.clone(),
                color: crate::series::palette_color(series.color, index),
                value: Some(
                    unit.format(
                        series
                            .values
                            .iter()
                            .copied()
                            .filter(|value| value.is_finite())
                            .sum(),
                    ),
                ),
                stats: summarize(series.values.iter().copied(), stats, unit),
            })
            .collect(),
        ChartData::Pie(_) | ChartData::Treemap(_) | ChartData::HostMap(_) => vec![],
        ChartData::StateTimeline(series) => series
            .iter()
            .enumerate()
            .map(|(index, series)| {
                let latest = series
                    .segments
                    .iter()
                    .filter(|segment| segment.end_ms.is_finite())
                    .max_by(|a, b| {
                        a.end_ms
                            .partial_cmp(&b.end_ms)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                CanvasLegendEntry {
                    name: series.name.clone(),
                    color: crate::series::palette_color(latest.map(|segment| segment.color), index),
                    value: latest.map(|segment| segment.label.clone()),
                    // State labels are categorical: no numeric summary exists.
                    stats: vec![],
                }
            })
            .collect(),
        ChartData::Band(series) => series
            .iter()
            .enumerate()
            .map(|(index, series)| CanvasLegendEntry {
                name: series.name.clone(),
                color: crate::series::palette_color(series.color, index),
                value: series
                    .center
                    .iter()
                    .rev()
                    .copied()
                    .find(|value| value.is_finite())
                    .map(|value| unit.format(value)),
                stats: summarize(series.center.iter().copied(), stats, unit),
            })
            .collect(),
    }
}

fn legend_label(entry: &CanvasLegendEntry, format: LegendFormat) -> String {
    let head = match format {
        LegendFormat::NameOnly => entry.name.clone(),
        LegendFormat::ValueOnly => entry.value.clone().unwrap_or_else(|| "—".into()),
        LegendFormat::NameAndValue => match &entry.value {
            Some(value) => format!("{}: {value}", entry.name),
            None => entry.name.clone(),
        },
    };
    if entry.stats.is_empty() {
        head
    } else {
        format!("{head}  ({})", entry.stats.join("  "))
    }
}

fn legend_reservation(
    surface: &CanvasSurface,
    spec: &ChartSpec,
    data: &ChartData,
    entries: &[CanvasLegendEntry],
    axis_font: &str,
) -> LegendReservation {
    if !spec.legend.show {
        return LegendReservation::default();
    }
    // A heatmap's key is its color ramp, not a list of row swatches, and it
    // needs a fixed band rather than one sized by label widths.
    if matches!(data, ChartData::Heatmap(_)) {
        if data.is_empty() {
            return LegendReservation::default();
        }
        return match spec.legend.position {
            LegendPosition::Top => LegendReservation {
                top: COLOR_BAR_BAND,
                ..Default::default()
            },
            LegendPosition::Bottom => LegendReservation {
                bottom: COLOR_BAR_BAND,
                ..Default::default()
            },
            LegendPosition::Left => LegendReservation {
                left: COLOR_BAR_WIDTH,
                ..Default::default()
            },
            LegendPosition::Right => LegendReservation {
                right: COLOR_BAR_WIDTH,
                ..Default::default()
            },
        };
    }
    if entries.is_empty() {
        return LegendReservation::default();
    }
    let ctx = &surface.ctx;
    ctx.save();
    ctx.set_font(axis_font);
    let widths: Vec<f64> = entries
        .iter()
        .map(|entry| {
            ctx.measure_text(&legend_label(entry, spec.legend.format))
                .map_or(40.0, |m| m.width())
                + 18.0
        })
        .collect();
    ctx.restore();
    const LINE_H: f64 = 16.0;
    match spec.legend.position {
        LegendPosition::Top | LegendPosition::Bottom => {
            let available =
                (surface.css_w - crate::layout::MARGIN_LEFT - crate::layout::MARGIN_RIGHT).max(1.0);
            let mut rows = 1usize;
            let mut used = 0.0;
            for width in widths {
                if used > 0.0 && used + width > available {
                    rows += 1;
                    used = 0.0;
                }
                used += width;
            }
            // Keep chrome predictable when a chart has dozens of series. The
            // painter emits a `+N more` marker once this bounded band fills.
            let height = rows.min(4) as f64 * LINE_H + 4.0;
            match spec.legend.position {
                LegendPosition::Top => LegendReservation {
                    top: height,
                    ..Default::default()
                },
                LegendPosition::Bottom => LegendReservation {
                    bottom: height,
                    ..Default::default()
                },
                _ => unreachable!(),
            }
        }
        LegendPosition::Left | LegendPosition::Right => {
            let width = widths.into_iter().fold(40.0, f64::max).min(180.0) + 4.0;
            match spec.legend.position {
                LegendPosition::Left => LegendReservation {
                    left: width,
                    ..Default::default()
                },
                LegendPosition::Right => LegendReservation {
                    right: width,
                    ..Default::default()
                },
                _ => unreachable!(),
            }
        }
    }
}

/// Reserve only the amount by which row labels or end labels exceed the
/// standard axis gutters. This keeps categorical HBar/state labels and the
/// opt-in latest line values inside the canvas without widening ordinary
/// time-series plots.
fn axis_label_reservation(
    surface: &CanvasSurface,
    spec: &ChartSpec,
    data: &ChartData,
    axis_font: &str,
) -> LegendReservation {
    if !spec.chrome {
        return LegendReservation::default();
    }
    let ctx = &surface.ctx;
    ctx.save();
    ctx.set_font(axis_font);
    let label_width = |label: &str| ctx.measure_text(label).map_or(0.0, |m| m.width());
    let mut left_needed = match data {
        ChartData::HBar(series) => series
            .iter()
            .flat_map(|series| series.categories.iter())
            .map(|label| label_width(label) + 10.0)
            .fold(0.0, f64::max),
        ChartData::Heatmap(series) => series
            .iter()
            .map(|series| label_width(&series.name) + 10.0)
            .fold(0.0, f64::max),
        ChartData::StateTimeline(series) => series
            .iter()
            .map(|series| label_width(&series.name) + 10.0)
            .fold(0.0, f64::max),
        _ => 0.0,
    };
    if uses_numeric_y_ticks(data) {
        let numeric_width = numeric_axis_label_width(ctx, data, spec.unit, spec.y_min, spec.y_max);
        left_needed = left_needed.max(numeric_width + 6.0);
    }
    // HBar labels sit inside the plot, so only the right-edge kinds widen the
    // gutter. Measure against the whole series, not the visible window: the
    // reservation is computed before the layout that would define one.
    let right_needed = if spec.last_value_label && !matches!(data, ChartData::HBar(_)) {
        last_values_for_reservation(data)
            .into_iter()
            .map(|value| label_width(&spec.unit.format(value)) + 6.0)
            .fold(0.0, f64::max)
    } else {
        0.0
    };
    ctx.restore();
    LegendReservation {
        left: (left_needed - crate::layout::MARGIN_LEFT).max(0.0) + y_title_band(spec),
        right: (right_needed - crate::layout::MARGIN_RIGHT).max(0.0),
        bottom: x_title_band(spec),
        ..Default::default()
    }
}

fn numeric_axis_label_width(
    ctx: &web_sys::CanvasRenderingContext2d,
    data: &ChartData,
    unit: Unit,
    y_min: Option<f64>,
    y_max: Option<f64>,
) -> f64 {
    let mut max_width: f64 = 0.0;
    let mut measure = |value: f64| {
        if value.is_finite() {
            max_width = max_width.max(
                ctx.measure_text(&unit.format(value))
                    .map_or(0.0, |m| m.width()),
            );
        }
    };
    if let Some(value) = y_min {
        measure(value);
    }
    if let Some(value) = y_max {
        measure(value);
    }
    match data {
        ChartData::Band(rows) => {
            for row in rows {
                row.lower
                    .iter()
                    .chain(&row.upper)
                    .copied()
                    .for_each(&mut measure);
            }
        }
        ChartData::Ohlc(rows) => {
            for row in rows {
                for tick in &row.ticks {
                    measure(tick.low);
                    measure(tick.high);
                }
            }
        }
        ChartData::Histogram(rows) => {
            for row in rows {
                row.counts.iter().copied().for_each(&mut measure);
            }
        }
        _ => {
            for row in data.point_series() {
                row.ys.iter().copied().for_each(&mut measure);
            }
        }
    }
    max_width
}

/// Last finite value of each series, for sizing the right-edge gutter before
/// a layout exists to define the visible window.
fn last_values_for_reservation(data: &ChartData) -> Vec<f64> {
    let last = |ys: &[f64]| ys.iter().rev().copied().find(|value| value.is_finite());
    match data {
        ChartData::Band(series) => series
            .iter()
            .filter_map(|series| last(&series.center))
            .collect(),
        ChartData::Ohlc(series) => series
            .iter()
            .filter_map(|series| {
                series
                    .ticks
                    .iter()
                    .rev()
                    .find(|tick| tick.close.is_finite())
                    .map(|tick| tick.close)
            })
            .collect(),
        _ => data
            .point_series()
            .iter()
            .filter_map(|series| last(&series.ys))
            .collect(),
    }
}

/// Width reserved outside the y gutter for a rotated y-axis title.
fn y_title_band(spec: &ChartSpec) -> f64 {
    if spec.chrome && spec.y_label.is_some() {
        AXIS_TITLE_BAND
    } else {
        0.0
    }
}

/// Height reserved under the x tick labels for an x-axis title.
fn x_title_band(spec: &ChartSpec) -> f64 {
    if spec.chrome && spec.x_label.is_some() {
        AXIS_TITLE_BAND
    } else {
        0.0
    }
}

/// Axis titles sit in their own band outside every other axis decoration.
const AXIS_TITLE_BAND: f64 = 14.0;

/// Draw the optional axis titles in the outermost reserved bands: the y title
/// rotated along the left edge, the x title centered under the tick labels.
fn draw_axis_titles(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    spec: &ChartSpec,
    theme: &Theme,
    axis_font: &str,
) {
    if !spec.chrome || (spec.y_label.is_none() && spec.x_label.is_none()) {
        return;
    }
    let ctx = &surface.ctx;
    ctx.save();
    ctx.set_font(axis_font);
    ctx.set_fill_style_str(&theme.text.to_css());
    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);
    ctx.set_text_align("center");
    if let Some(label) = &spec.y_label {
        ctx.set_text_baseline("top");
        ctx.save();
        let _ = ctx.translate(2.0, layout.plot.y + layout.plot.h / 2.0);
        let _ = ctx.rotate(-std::f64::consts::FRAC_PI_2);
        let _ = ctx.fill_text(label, 0.0, 0.0);
        ctx.restore();
    }
    if let Some(label) = &spec.x_label {
        ctx.set_text_baseline("bottom");
        let _ = ctx.fill_text(
            label,
            layout.plot.x + layout.plot.w / 2.0,
            surface.css_h - 2.0,
        );
    }
    ctx.restore();
}

/// Centered message for a chart whose data holds nothing drawable — an empty
/// panel that says nothing is indistinguishable from a broken one.
fn draw_empty_state(surface: &CanvasSurface, layout: &ChartLayout, message: &str, theme: &Theme) {
    let ctx = &surface.ctx;
    ctx.save();
    ctx.set_font("12px 'JetBrains Mono', monospace");
    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    ctx.set_fill_style_str(&theme.text.to_css());
    let _ = ctx.fill_text(
        message,
        layout.plot.x + layout.plot.w / 2.0,
        layout.plot.y + layout.plot.h / 2.0,
    );
    ctx.restore();
}

/// Height of a horizontal heatmap color bar band (swatch + value labels).
const COLOR_BAR_BAND: f64 = 26.0;
/// Width of a vertical heatmap color bar band.
const COLOR_BAR_WIDTH: f64 = 58.0;
/// Gradient stops used to approximate a heatmap ramp on the canvas.
const COLOR_BAR_STOPS: usize = 12;

/// A heatmap's legend is its color ramp with the value domain on it.
///
/// The per-row palette swatches an ordinary legend draws say nothing about a
/// heatmap: cells are colored by *value* through `HeatmapColorScale`, not by
/// series index. Without this key the panel cannot be read quantitatively at
/// all.
fn draw_heatmap_color_bar(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    spec: &ChartSpec,
    reservation: LegendReservation,
    opts: &RenderOptions,
) {
    let (axis_font, theme) = (opts.axis_font.as_str(), &opts.theme);
    let (lo, hi) = layout.y_domain();
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return;
    }
    let ctx = &surface.ctx;
    let plot = layout.plot;
    let base_bottom = if spec.chrome {
        crate::layout::MARGIN_BOTTOM
    } else {
        1.0
    };
    let base_right = if spec.chrome {
        crate::layout::MARGIN_RIGHT
    } else {
        1.0
    };
    let horizontal = matches!(
        spec.legend.position,
        LegendPosition::Top | LegendPosition::Bottom
    );
    // The bar occupies its reserved band; labels sit just outside the swatch.
    let bar = match spec.legend.position {
        LegendPosition::Top => Rect {
            x: plot.x,
            y: plot.y - reservation.top + 2.0,
            w: plot.w,
            h: 8.0,
        },
        LegendPosition::Bottom => Rect {
            x: plot.x,
            y: plot.y + plot.h + base_bottom + 4.0,
            w: plot.w,
            h: 8.0,
        },
        LegendPosition::Left => Rect {
            x: plot.x - reservation.left + 4.0,
            y: plot.y,
            w: 8.0,
            h: plot.h,
        },
        LegendPosition::Right => Rect {
            x: plot.x + plot.w + base_right + 4.0,
            y: plot.y,
            w: 8.0,
            h: plot.h,
        },
    };
    if bar.w <= 0.0 || bar.h <= 0.0 {
        return;
    }
    // `createLinearGradient` interpolates in sRGB, which is not how the
    // perceptual scales are defined, so sample the real scale into stops.
    let (x0, y0, x1, y1) = if horizontal {
        (bar.x, bar.y, bar.x + bar.w, bar.y)
    } else {
        // Vertical bars run low at the bottom, matching the y axis.
        (bar.x, bar.y + bar.h, bar.x, bar.y)
    };
    let gradient = make_linear_gradient(ctx, x0, y0, x1, y1);
    for step in 0..=COLOR_BAR_STOPS {
        let t = step as f64 / COLOR_BAR_STOPS as f64;
        let color = heatmap_color_for_value(spec.heatmap_scale, lo + (hi - lo) * t, lo, hi);
        if gradient.add_color_stop(t as f32, &color.to_css()).is_err() {
            return;
        }
    }
    ctx.save();
    set_fill_gradient(ctx, &gradient);
    ctx.fill_rect(bar.x, bar.y, bar.w, bar.h);
    ctx.set_font(axis_font);
    ctx.set_fill_style_str(&theme.text.to_css());
    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);
    if horizontal {
        ctx.set_text_baseline("top");
        ctx.set_text_align("left");
        let _ = ctx.fill_text(&spec.unit.format(lo), bar.x, bar.y + bar.h + 2.0);
        ctx.set_text_align("center");
        let _ = ctx.fill_text(
            &spec.unit.format((lo + hi) / 2.0),
            bar.x + bar.w / 2.0,
            bar.y + bar.h + 2.0,
        );
        ctx.set_text_align("right");
        let _ = ctx.fill_text(&spec.unit.format(hi), bar.x + bar.w, bar.y + bar.h + 2.0);
    } else {
        ctx.set_text_align("left");
        ctx.set_text_baseline("top");
        let _ = ctx.fill_text(&spec.unit.format(hi), bar.x + bar.w + 4.0, bar.y);
        ctx.set_text_baseline("bottom");
        let _ = ctx.fill_text(&spec.unit.format(lo), bar.x + bar.w + 4.0, bar.y + bar.h);
    }
    ctx.restore();
}

fn draw_canvas_legend(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    spec: &ChartSpec,
    data: &ChartData,
    entries: &[CanvasLegendEntry],
    reservation: LegendReservation,
    opts: &RenderOptions,
) {
    let (axis_font, theme) = (opts.axis_font.as_str(), &opts.theme);
    if !spec.legend.show {
        return;
    }
    if let ChartData::Heatmap(_) = data {
        if !data.is_empty() {
            draw_heatmap_color_bar(surface, layout, spec, reservation, opts);
        }
        return;
    }
    if entries.is_empty() {
        return;
    }
    let ctx = &surface.ctx;
    const LINE_H: f64 = 16.0;
    ctx.save();
    // Keep a long legend inside the canvas; its reservation and this clipping
    // share the same measured layout band.
    ctx.begin_path();
    ctx.rect(0.0, 0.0, surface.css_w.max(1.0), surface.css_h.max(1.0));
    ctx.clip();
    ctx.set_font(axis_font);
    ctx.set_text_baseline("middle");
    ctx.set_fill_style_str(&theme.text.to_css());
    let base_bottom = if spec.chrome {
        crate::layout::MARGIN_BOTTOM
    } else {
        1.0
    };
    let base_right = if spec.chrome {
        crate::layout::MARGIN_RIGHT
    } else {
        1.0
    };
    let mut x;
    let mut y;
    let horizontal = matches!(
        spec.legend.position,
        LegendPosition::Top | LegendPosition::Bottom
    );
    match spec.legend.position {
        LegendPosition::Top => {
            x = layout.plot.x;
            y = layout.plot.y - reservation.top + LINE_H / 2.0 + 2.0;
        }
        LegendPosition::Bottom => {
            x = layout.plot.x;
            y = layout.plot.y + layout.plot.h + base_bottom + LINE_H / 2.0 + 2.0;
        }
        LegendPosition::Left => {
            x = layout.plot.x - reservation.left + 2.0;
            y = layout.plot.y + LINE_H / 2.0;
        }
        LegendPosition::Right => {
            x = layout.plot.x + layout.plot.w + base_right + 2.0;
            y = layout.plot.y + LINE_H / 2.0;
        }
    }
    let right = layout.plot.x + layout.plot.w;
    let mut omitted = 0usize;
    for (entry_index, entry) in entries.iter().enumerate() {
        let label = legend_label(entry, spec.legend.format);
        let text_w = ctx.measure_text(&label).map_or(40.0, |m| m.width());
        if horizontal && x > layout.plot.x && x + 12.0 + text_w > right {
            x = layout.plot.x;
            y += LINE_H;
        }
        if y - LINE_H / 2.0 < 0.0 || y + LINE_H / 2.0 > surface.css_h {
            omitted = entries.len().saturating_sub(entry_index);
            break;
        }
        ctx.set_fill_style_str(&css_color(entry.color));
        ctx.fill_rect(x, y - 4.0, 8.0, 8.0);
        ctx.set_fill_style_str(&theme.text.to_css());
        ctx.set_text_align("left");
        let _ = ctx.fill_text(&label, x + 12.0, y);
        if horizontal {
            x += 18.0 + text_w;
        } else {
            y += LINE_H;
        }
    }
    if omitted > 0 {
        ctx.set_text_align("left");
        ctx.set_fill_style_str(&theme.text.to_css());
        let label = format!("+{omitted} more");
        // `fill_text` uses a baseline; leave room for the font descent so the
        // overflow marker itself remains inside short canvases.
        let y = (surface.css_h - 5.0).max(1.0);
        let width = ctx.measure_text(&label).map_or(0.0, |m| m.width());
        let x = layout
            .plot
            .x
            .clamp(1.0, (surface.css_w - width - 1.0).max(1.0));
        let _ = ctx.fill_text(&label, x, y);
    }
    ctx.restore();
}

#[derive(Debug, Clone, Copy)]
struct HighlightCandidate {
    value: f64,
    px: f64,
    py: f64,
    color: u32,
}

/// Choose the callouts to draw: rank in the configured direction, then
/// enforce horizontal spacing so a cluster of peaks yields one label rather
/// than a pile of overprinted ones.
fn select_highlights(
    layout: &ChartLayout,
    spec: &ChartSpec,
    data: &ChartData,
) -> Vec<HighlightCandidate> {
    let config = spec.highlights;
    // Keep selection bounded even for million-point telemetry panels. The
    // spacing pass needs a little surplus because nearby winners may be
    // rejected, but never needs every visible point.
    let budget = |n: usize| n.saturating_mul(32).max(n).max(1);
    let ranked = |top_n: usize, invert: bool| {
        let mut candidates = highlight_candidates(layout, data, spec.layout, budget(top_n), invert);
        candidates.sort_by(|a, b| {
            let (a, b) = if invert {
                (a.value, b.value)
            } else {
                (b.value, a.value)
            };
            a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates
    };
    let candidates = match config.mode {
        HighlightMode::Max => ranked(config.top_n, false),
        HighlightMode::Min => ranked(config.top_n, true),
        HighlightMode::Extremes => {
            // Split the budget, peaks first: with a single callout allowed,
            // the peak is the one a dashboard is read for.
            let peaks = config.top_n.div_ceil(2);
            let troughs = config.top_n - peaks;
            let mut merged = ranked(peaks, false);
            merged.truncate(peaks);
            if troughs > 0 {
                let mut low = ranked(troughs, true);
                low.truncate(troughs);
                merged.extend(low);
            }
            merged
        }
        HighlightMode::Last => last_value_candidates(layout, data),
    };
    let mut selected: Vec<HighlightCandidate> = Vec::with_capacity(config.top_n);
    for candidate in candidates {
        if !candidate.value.is_finite() || !layout.plot.contains(candidate.px, candidate.py) {
            continue;
        }
        if selected
            .iter()
            .all(|kept| (kept.px - candidate.px).abs() >= config.min_spacing_px.max(0.0))
        {
            selected.push(candidate);
            if selected.len() == config.top_n {
                break;
            }
        }
    }
    selected
}

/// The most recent visible value of each series — the "where does it stand
/// now" callout, which is a different question from "where did it peak".
fn last_value_candidates(layout: &ChartLayout, data: &ChartData) -> Vec<HighlightCandidate> {
    let last_of = |xs: &[f64], ys: &[f64], color: u32| -> Option<HighlightCandidate> {
        let n = xs.len().min(ys.len());
        let (start, end) = visible_point_bounds(&xs[..n], &layout.x_scale);
        xs[start..end]
            .iter()
            .zip(&ys[start..end])
            .rev()
            .find(|(x, y)| {
                x.is_finite()
                    && y.is_finite()
                    && **x >= layout.x_scale.d0
                    && **x <= layout.x_scale.d1
            })
            .map(|(x, y)| HighlightCandidate {
                value: *y,
                px: layout.x_scale.to_px(*x),
                py: layout.y_px(*y),
                color,
            })
    };
    match data {
        ChartData::Lines(series)
        | ChartData::Areas(series)
        | ChartData::Bars(series)
        | ChartData::Points(series)
        | ChartData::Scatter(series)
        | ChartData::Step(series) => series
            .iter()
            .enumerate()
            .filter_map(|(index, s)| last_of(&s.xs, &s.ys, series_color(s, index)))
            .collect(),
        ChartData::Band(series) => series
            .iter()
            .enumerate()
            .filter_map(|(index, s)| {
                last_of(&s.xs, &s.center, crate::series::band_series_color(s, index))
            })
            .collect(),
        ChartData::Ohlc(series) => series
            .iter()
            .enumerate()
            .filter_map(|(index, s)| {
                let xs: Vec<f64> = s.ticks.iter().map(|tick| tick.ts).collect();
                let ys: Vec<f64> = s.ticks.iter().map(|tick| tick.close).collect();
                last_of(&xs, &ys, crate::series::ohlc_series_color(s, index))
            })
            .collect(),
        // Bucket- and row-indexed kinds have no "latest" along x.
        _ => vec![],
    }
}

fn draw_highlights(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    spec: &ChartSpec,
    data: &ChartData,
    opts: &RenderOptions,
) {
    let config = spec.highlights;
    if !config.show || config.top_n == 0 {
        return;
    }
    let selected = select_highlights(layout, spec, data);
    if selected.is_empty() {
        return;
    }
    let ctx = &surface.ctx;
    ctx.save();
    ctx.set_font(&opts.axis_font);
    ctx.set_text_baseline("bottom");
    let mut label_rows: Vec<f64> = Vec::new();
    for candidate in selected {
        let (r, g, b) = rgb_to_components(candidate.color);
        ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.95)"));
        ctx.set_shadow_color(&format!("rgba({r},{g},{b},0.65)"));
        ctx.set_shadow_blur(6.0);
        ctx.begin_path();
        let _ = ctx.arc(candidate.px, candidate.py, 4.0, 0.0, std::f64::consts::TAU);
        ctx.fill();
        ctx.set_shadow_color("transparent");
        ctx.set_shadow_blur(0.0);
        ctx.set_fill_style_str("rgba(255,255,255,0.92)");
        ctx.begin_path();
        let _ = ctx.arc(candidate.px, candidate.py, 1.5, 0.0, std::f64::consts::TAU);
        ctx.fill();
        if config.show_values {
            let label = spec.unit.format(candidate.value);
            let right = layout.plot.x + layout.plot.w - 2.0;
            let x = (candidate.px + 6.0).min(right).max(layout.plot.x + 2.0);
            let min_y = layout.plot.y + 10.0;
            let max_y = layout.plot.y + layout.plot.h - 2.0;
            let mut y = if max_y >= min_y {
                (candidate.py - 6.0).clamp(min_y, max_y)
            } else {
                layout.plot.y + layout.plot.h / 2.0
            };
            if let Some(previous) = label_rows.last().copied()
                && (y - previous).abs() < 10.0
            {
                y = (previous + 10.0).min(max_y.max(min_y));
            }
            label_rows.push(y);
            ctx.set_fill_style_str(&opts.theme.text.to_css());
            ctx.set_text_align("left");
            let _ = ctx.fill_text(&label, x, y);
        }
    }
    ctx.restore();
}

fn highlight_candidates(
    layout: &ChartLayout,
    data: &ChartData,
    series_layout: SeriesLayout,
    limit: usize,
    invert: bool,
) -> Vec<HighlightCandidate> {
    let mut pool = CandidatePool::new(limit, invert);
    // A stacked column's visible top is its accumulated total, not any one
    // series' y, so stacks get their own candidate pass.
    if let ChartData::Bars(series) | ChartData::Areas(series) = data
        && !matches!(series_layout, SeriesLayout::Grouped)
    {
        push_stacked_column_candidates(layout, &mut pool, series, series_layout);
        return pool.into_vec();
    }
    let mut push_point = |x: f64, y: f64, color: u32| {
        if x.is_finite() && y.is_finite() {
            pool.push(
                layout,
                HighlightCandidate {
                    value: y,
                    px: layout.x_scale.to_px(x),
                    py: layout.y_px(y),
                    color,
                },
            );
        }
    };
    match data {
        ChartData::Lines(series)
        | ChartData::Areas(series)
        | ChartData::Bars(series)
        | ChartData::Points(series)
        | ChartData::Scatter(series)
        | ChartData::Step(series) => {
            for (index, series) in series.iter().enumerate() {
                let color = series_color(series, index);
                let n = series.xs.len().min(series.ys.len());
                let (start, end) = visible_point_bounds(&series.xs[..n], &layout.x_scale);
                for (x, y) in series.xs[start..end].iter().zip(&series.ys[start..end]) {
                    push_point(*x, *y, color);
                }
            }
        }
        ChartData::Heatmap(series) => {
            let row_h = layout.plot.h / series.len().max(1) as f64;
            for (row, series) in series.iter().enumerate() {
                let py = layout.plot.y + (row as f64 + 0.5) * row_h;
                let n = series.xs.len().min(series.ys.len());
                let (start, end) = visible_point_bounds(&series.xs[..n], &layout.x_scale);
                for (x, value) in series.xs[start..end].iter().zip(&series.ys[start..end]) {
                    if x.is_finite() && value.is_finite() {
                        pool.push(
                            layout,
                            HighlightCandidate {
                                value: *value,
                                px: layout.x_scale.to_px(*x),
                                py,
                                color: series_color(series, row),
                            },
                        );
                    }
                }
            }
        }
        ChartData::Ohlc(series) => {
            for (index, series) in series.iter().enumerate() {
                let color = crate::series::palette_color(series.color, index);
                let (start, end) = visible_tick_bounds(&series.ticks, &layout.x_scale);
                for tick in &series.ticks[start..end] {
                    push_point(tick.ts, tick.close, color);
                }
            }
        }
        ChartData::Histogram(series) => {
            let counts: Vec<Vec<f64>> = series.iter().map(HistogramSeries::plot_counts).collect();
            if matches!(series_layout, SeriesLayout::Grouped) {
                for (index, series) in series.iter().enumerate() {
                    let color = histogram_color(series, index);
                    for (bucket, count) in counts[index].iter().enumerate().take(
                        counts[index]
                            .len()
                            .min(series.buckets.len().saturating_sub(1)),
                    ) {
                        push_point(
                            (series.buckets[bucket] + series.buckets[bucket + 1]) / 2.0,
                            *count,
                            color,
                        );
                    }
                }
            } else {
                let buckets = series
                    .iter()
                    .zip(&counts)
                    .map(|(series, counts)| {
                        counts.len().min(series.buckets.len().saturating_sub(1))
                    })
                    .max()
                    .unwrap_or(0);
                for (bucket, _) in std::iter::repeat_n((), buckets).enumerate() {
                    let Some((x0, x1)) = series.iter().find_map(|series| {
                        if bucket + 1 < series.buckets.len() {
                            Some((series.buckets[bucket], series.buckets[bucket + 1]))
                        } else {
                            None
                        }
                    }) else {
                        continue;
                    };
                    if !x0.is_finite() || !x1.is_finite() || x0 == x1 {
                        continue;
                    }
                    let (positive_total, negative_total) = counts
                        .iter()
                        .filter_map(|counts| counts.get(bucket).copied())
                        .filter(|value| value.is_finite())
                        .fold((0.0, 0.0), |(positive, negative), value| {
                            if value >= 0.0 {
                                (positive + value, negative)
                            } else {
                                (positive, negative + value.abs())
                            }
                        });
                    let mut positive = 0.0;
                    let mut negative = 0.0;
                    for (index, series) in series.iter().enumerate() {
                        let Some(raw) = counts[index]
                            .get(bucket)
                            .copied()
                            .filter(|value| value.is_finite())
                        else {
                            continue;
                        };
                        let drawn = if matches!(series_layout, SeriesLayout::StackedPercent) {
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
                        pool.push(
                            layout,
                            HighlightCandidate {
                                // Preserve the bucket's semantic value in the
                                // label while positioning at its rendered top.
                                value: raw,
                                px: (layout.x_scale.to_px(x0) + layout.x_scale.to_px(x1)) / 2.0,
                                py: layout.y_px(top),
                                color: histogram_color(series, index),
                            },
                        );
                    }
                }
            }
        }
        ChartData::HBar(series) => {
            let rows = series
                .iter()
                .map(|series| series.categories.len().min(series.values.len()))
                .max()
                .unwrap_or(0);
            let row_h = layout.plot.h / rows.max(1) as f64;
            let group_h = row_h * 0.8;
            let sub_h = group_h / series_count(series.len());
            for (index, series) in series.iter().enumerate() {
                let color = crate::series::palette_color(series.color, index);
                for (row, value) in series
                    .values
                    .iter()
                    .take(series.categories.len())
                    .enumerate()
                {
                    if value.is_finite() {
                        pool.push(
                            layout,
                            HighlightCandidate {
                                value: *value,
                                px: layout.x_scale.to_px(*value),
                                py: layout.plot.y
                                    + row as f64 * row_h
                                    + (row_h - group_h) / 2.0
                                    + (index as f64 + 0.5) * sub_h,
                                color,
                            },
                        );
                    }
                }
            }
        }
        ChartData::Pie(_)
        | ChartData::Treemap(_)
        | ChartData::HostMap(_)
        | ChartData::StateTimeline(_) => {}
        ChartData::Band(series) => {
            for (index, series) in series.iter().enumerate() {
                let color = crate::series::palette_color(series.color, index);
                let n = series.xs.len().min(series.center.len());
                let (start, end) = visible_point_bounds(&series.xs[..n], &layout.x_scale);
                for (x, center) in series.xs[start..end].iter().zip(&series.center[start..end]) {
                    push_point(*x, *center, color);
                }
            }
        }
    }
    pool.into_vec()
}

/// Candidates for a signed stack: one per column, positioned at the
/// accumulated top the eye actually reads, and valued at that total.
///
/// `StackedPercent` is excluded — every column of a normalized stack tops out
/// at 100%, so "which column is largest" is not a question it can answer.
fn push_stacked_column_candidates(
    layout: &ChartLayout,
    pool: &mut CandidatePool,
    series: &[SeriesData],
    series_layout: SeriesLayout,
) {
    if matches!(series_layout, SeriesLayout::StackedPercent) {
        return;
    }
    let Some(columns) = series.iter().map(|s| s.xs.len().min(s.ys.len())).max() else {
        return;
    };
    for column in 0..columns {
        let Some(x) = series
            .iter()
            .find_map(|s| s.xs.get(column).copied().filter(|x| x.is_finite()))
        else {
            continue;
        };
        // Positives and negatives stack onto separate baselines, exactly as
        // `draw_bar_column` and `draw_stacked_areas` accumulate them.
        let mut positive = 0.0f64;
        let mut negative = 0.0f64;
        let mut positive_color = None;
        let mut negative_color = None;
        for (index, s) in series.iter().enumerate() {
            let Some(value) = s.ys.get(column).copied().filter(|v| v.is_finite()) else {
                continue;
            };
            if value >= 0.0 {
                positive += value;
                positive_color = Some(series_color(s, index));
            } else {
                negative += value;
                negative_color = Some(series_color(s, index));
            }
        }
        let px = layout.x_scale.to_px(x);
        // The topmost contributing series owns the callout color, because
        // that is the band the marker visually sits on.
        for (total, color) in [(positive, positive_color), (negative, negative_color)] {
            let Some(color) = color else { continue };
            if total == 0.0 {
                continue;
            }
            pool.push(
                layout,
                HighlightCandidate {
                    value: total,
                    px,
                    py: layout.y_px(total),
                    color,
                },
            );
        }
    }
}

/// Visible x range plus one neighbor on either side. Sorted finite input takes
/// the O(log n) fast path; malformed/unsorted input deliberately falls back to
/// the complete safe intersection rather than trusting binary search.
fn visible_point_bounds(xs: &[f64], x_scale: &crate::scale::LinearScale) -> (usize, usize) {
    if xs.len() < 2 || !sorted_sample_hint(xs) {
        return (0, xs.len());
    }
    let start = xs.partition_point(|x| *x < x_scale.d0).saturating_sub(1);
    let end = xs
        .partition_point(|x| *x <= x_scale.d1)
        .saturating_add(1)
        .min(xs.len());
    (start.min(end), end)
}

/// Sortedness is validated at ingestion. Keep a tiny defensive sample here
/// for callers that construct data directly, without turning every pointer or
/// redraw into an O(history) scan.
fn sorted_sample_hint(xs: &[f64]) -> bool {
    if xs.len() < 64 {
        return xs.iter().all(|x| x.is_finite()) && xs.windows(2).all(|pair| pair[0] <= pair[1]);
    }
    let mut previous = None;
    for index in (0..xs.len()).step_by((xs.len() / 32).max(1)) {
        let value = xs[index];
        if !value.is_finite() || previous.is_some_and(|old| old > value) {
            return false;
        }
        previous = Some(value);
    }
    xs.last().is_some_and(|value| value.is_finite())
}

fn visible_tick_bounds(
    ticks: &[crate::series::OhlcTick],
    x_scale: &crate::scale::LinearScale,
) -> (usize, usize) {
    if ticks.len() < 2 || !sorted_tick_sample_hint(ticks) {
        return (0, ticks.len());
    }
    let start = ticks
        .partition_point(|tick| tick.ts < x_scale.d0)
        .saturating_sub(1);
    let end = ticks
        .partition_point(|tick| tick.ts <= x_scale.d1)
        .saturating_add(1)
        .min(ticks.len());
    (start.min(end), end)
}

fn sorted_tick_sample_hint(ticks: &[crate::series::OhlcTick]) -> bool {
    if ticks.len() < 64 {
        return ticks.iter().all(|tick| tick.ts.is_finite())
            && ticks.windows(2).all(|pair| pair[0].ts <= pair[1].ts);
    }
    let mut previous = None;
    for index in (0..ticks.len()).step_by((ticks.len() / 32).max(1)) {
        let value = ticks[index].ts;
        if !value.is_finite() || previous.is_some_and(|old| old > value) {
            return false;
        }
        previous = Some(value);
    }
    ticks.last().is_some_and(|tick| tick.ts.is_finite())
}

fn visible_stack_indices(
    xs: &[f64],
    series: &[SeriesData],
    layout: &ChartLayout,
    percent: bool,
    log_gaps: bool,
) -> Vec<usize> {
    let n = xs.len();
    let (start, end) = visible_point_bounds(xs, &layout.x_scale);
    let mut out = Vec::with_capacity(layout.plot.w.max(1.0) as usize * 4 + 2);
    let mut col = f64::NAN;
    let mut first = None;
    let mut last = None;
    let mut boundaries: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let mut boundary_values: Vec<(f64, f64)> = Vec::new();
    let mut separators = Vec::new();
    let flush = |out: &mut Vec<usize>,
                 first: &mut Option<usize>,
                 last: &mut Option<usize>,
                 boundaries: &mut Vec<(Option<usize>, Option<usize>)>,
                 boundary_values: &mut Vec<(f64, f64)>,
                 separators: &mut Vec<usize>| {
        let mut selected = vec![*first, *last];
        selected.extend(boundaries.iter().flat_map(|(min, max)| [*min, *max]));
        selected.extend(separators.iter().copied().map(Some));
        let mut selected = selected.into_iter().flatten().collect::<Vec<_>>();
        selected.sort_unstable();
        selected.dedup();
        out.extend(selected);
        *first = None;
        *last = None;
        boundaries.clear();
        boundary_values.clear();
        separators.clear();
    };
    for (index, &x) in xs.iter().enumerate().take(end.min(n)).skip(start.min(end)) {
        if !x.is_finite() {
            flush(
                &mut out,
                &mut first,
                &mut last,
                &mut boundaries,
                &mut boundary_values,
                &mut separators,
            );
            col = f64::NAN;
            continue;
        }
        let projected = layout.x_scale.to_px(xs[index]).floor();
        if projected != col {
            flush(
                &mut out,
                &mut first,
                &mut last,
                &mut boundaries,
                &mut boundary_values,
                &mut separators,
            );
            col = projected;
            first = Some(index);
        }
        // Retain constituent gaps so each stack boundary breaks at missing
        // measurements instead of being joined to the next representative.
        if series.iter().any(|row| {
            row.ys.get(index).is_none_or(|value| {
                !value.is_finite() || (log_gaps && layout.y_kind == ScaleKind::Log && *value <= 0.0)
            })
        }) && !separators.contains(&index)
        {
            separators.push(index);
        }
        let (positive_total, negative_total) = series.iter().fold((0.0, 0.0), |(p, n), row| {
            match row.ys.get(index).copied().filter(|v| v.is_finite()) {
                Some(value) if value >= 0.0 => (p + value, n),
                Some(value) => (p, n + value.abs()),
                None => (p, n),
            }
        });
        let mut positive = 0.0;
        let mut negative = 0.0;
        for (series_index, row) in series.iter().enumerate() {
            let Some(mut value) = row.ys.get(index).copied().filter(|v| v.is_finite()) else {
                continue;
            };
            if percent {
                value = if value >= 0.0 && positive_total > 0.0 {
                    value / positive_total * 100.0
                } else if value < 0.0 && negative_total > 0.0 {
                    value / negative_total * 100.0
                } else {
                    0.0
                };
            }
            let boundary = if value >= 0.0 {
                positive += value;
                positive
            } else {
                negative += value;
                negative
            };
            let slot = series_index * 2 + usize::from(value < 0.0);
            if boundaries.len() < series.len() * 2 {
                boundaries.resize(series.len() * 2, (None, None));
                boundary_values.resize(series.len() * 2, (f64::INFINITY, f64::NEG_INFINITY));
            }
            let (min_index, max_index) = &mut boundaries[slot];
            let (min_value, max_value) = &mut boundary_values[slot];
            if boundary < *min_value {
                *min_value = boundary;
                *min_index = Some(index);
            }
            if boundary > *max_value {
                *max_value = boundary;
                *max_index = Some(index);
            }
        }
        last = Some(index);
    }
    flush(
        &mut out,
        &mut first,
        &mut last,
        &mut boundaries,
        &mut boundary_values,
        &mut separators,
    );
    // Gap separators and extrema can be collected through different paths;
    // keep the source order required by the path builder after every flush.
    out.sort_unstable();
    out.dedup();
    out
}

/// Bounded retention of the best `limit` highlight candidates.
///
/// Selection has to stay bounded: a million-point telemetry panel would
/// otherwise materialize a million candidates to choose one callout from.
struct CandidatePool {
    out: Vec<HighlightCandidate>,
    limit: usize,
    /// Rank by smallest value instead of largest ([`HighlightMode::Min`]).
    invert: bool,
}

impl CandidatePool {
    fn new(limit: usize, invert: bool) -> Self {
        Self {
            out: Vec::with_capacity(limit.min(1024)),
            limit,
            invert,
        }
    }

    /// Whether `a` outranks `b` in this pool's ranking direction.
    fn outranks(&self, a: f64, b: f64) -> bool {
        if self.invert { a < b } else { a > b }
    }

    /// Offer a candidate. Non-finite and off-plot candidates are dropped
    /// before selection, so an off-screen maximum cannot displace the visible
    /// value the callout is actually for.
    fn push(&mut self, layout: &ChartLayout, candidate: HighlightCandidate) {
        if candidate.value.is_finite()
            && candidate.px.is_finite()
            && candidate.py.is_finite()
            && layout.plot.contains(candidate.px, candidate.py)
        {
            self.push_ranked(candidate);
        }
    }

    fn push_ranked(&mut self, candidate: HighlightCandidate) {
        if self.limit == 0 {
            return;
        }
        if self.out.len() < self.limit {
            self.out.push(candidate);
            return;
        }
        let Some((weakest_index, weakest_value)) = self
            .out
            .iter()
            .enumerate()
            .map(|(index, kept)| (index, kept.value))
            .reduce(|weakest, kept| {
                if self.outranks(weakest.1, kept.1) {
                    kept
                } else {
                    weakest
                }
            })
        else {
            return;
        };
        if self.outranks(candidate.value, weakest_value) {
            self.out[weakest_index] = candidate;
        }
    }

    fn into_vec(self) -> Vec<HighlightCandidate> {
        self.out
    }
}

/// Copies of the series with positive and negative values independently
/// normalized in each column. A diverging percent stack must end at +100 and
/// -100, not let positives cancel negatives before calculating percentages.
fn normalize_percent(series: &[SeriesData]) -> Vec<SeriesData> {
    let n = series.iter().map(|s| s.ys.len()).max().unwrap_or(0);
    let totals: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            series
                .iter()
                .filter_map(|s| s.ys.get(i))
                .filter(|y| y.is_finite())
                .fold((0.0, 0.0), |(positive, negative), value| {
                    if *value >= 0.0 {
                        (positive + *value, negative)
                    } else {
                        (positive, negative + value.abs())
                    }
                })
        })
        .collect();
    series
        .iter()
        .map(|s| SeriesData {
            ys: s
                .ys
                .iter()
                .enumerate()
                .map(|(i, y)| {
                    if !y.is_finite() {
                        return *y;
                    }
                    let (positive, negative) = totals[i];
                    if *y >= 0.0 && positive > 0.0 {
                        *y / positive * 100.0
                    } else if *y < 0.0 && negative > 0.0 {
                        *y / negative * 100.0
                    } else {
                        0.0
                    }
                })
                .collect(),
            ..s.clone()
        })
        .collect()
}

/// Latest finite value of each line, drawn at the right plot edge in the
/// series color (R-5; opt-in via `ChartSpec::last_value_label`).
/// The latest visible value of each series, with its color, for the
/// right-edge value labels. Kinds whose x is not a continuous timeline
/// (histogram buckets, HBar rows, state timelines) have no "latest".
fn latest_visible_values(layout: &ChartLayout, data: &ChartData) -> Vec<(u32, f64)> {
    let latest = |xs: &[f64], ys: &[f64], color: u32| -> Option<(u32, f64)> {
        let n = xs.len().min(ys.len());
        xs[..n]
            .iter()
            .zip(&ys[..n])
            .rev()
            .find(|(x, y)| y.is_finite() && **x <= layout.x_scale.d1 && **x >= layout.x_scale.d0)
            .map(|(_, y)| (color, *y))
    };
    match data {
        ChartData::Lines(series)
        | ChartData::Areas(series)
        | ChartData::Bars(series)
        | ChartData::Points(series)
        | ChartData::Scatter(series)
        | ChartData::Step(series) => series
            .iter()
            .enumerate()
            .filter_map(|(index, s)| latest(&s.xs, &s.ys, series_color(s, index)))
            .collect(),
        ChartData::Band(series) => series
            .iter()
            .enumerate()
            .filter_map(|(index, s)| {
                latest(&s.xs, &s.center, crate::series::band_series_color(s, index))
            })
            .collect(),
        ChartData::Ohlc(series) => series
            .iter()
            .enumerate()
            .filter_map(|(index, s)| {
                s.ticks
                    .iter()
                    .rev()
                    .find(|tick| {
                        tick.close.is_finite()
                            && tick.ts <= layout.x_scale.d1
                            && tick.ts >= layout.x_scale.d0
                    })
                    .map(|tick| (crate::series::ohlc_series_color(s, index), tick.close))
            })
            .collect(),
        _ => vec![],
    }
}

/// Each series' latest value, printed just past the right edge in its own
/// color — the "what is it now" readout that otherwise costs a hover.
fn draw_last_value_labels(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    values: &[(u32, f64)],
    unit: Unit,
    axis_font: &str,
) {
    let ctx = &surface.ctx;
    let plot = layout.plot;
    ctx.save();
    ctx.set_font(axis_font);
    ctx.set_text_align("left");
    ctx.set_text_baseline("middle");
    let mut rows: Vec<(f64, u32, f64)> = values
        .iter()
        .filter_map(|(color, value)| {
            let py = layout.y_px(*value);
            py.is_finite().then_some((py, *color, *value))
        })
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut previous = f64::NEG_INFINITY;
    let min_y = plot.y + 5.0;
    let max_y = plot.y + plot.h - 5.0;
    for (raw_py, color, value) in rows {
        let py = if max_y >= min_y {
            raw_py.clamp(min_y, max_y).max(previous + 10.0).min(max_y)
        } else {
            plot.y + plot.h / 2.0
        };
        previous = py;
        ctx.set_fill_style_str(&css_color(color));
        let _ = ctx.fill_text(&unit.format(value), plot.x + plot.w + 3.0, py);
    }
    ctx.restore();
}

/// Value labels at the end of each horizontal bar. A ranked list is read for
/// its numbers, and pixel lengths do not supply them.
fn draw_hbar_value_labels(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[HBarSeries],
    unit: Unit,
    axis_font: &str,
    theme: &Theme,
) {
    let count = series
        .iter()
        .map(|s| s.categories.len().min(s.values.len()))
        .max()
        .unwrap_or(0);
    if count == 0 {
        return;
    }
    let ctx = &surface.ctx;
    let plot = layout.plot;
    let row_h = plot.h / count as f64;
    let group_h = row_h * 0.8;
    let sub_h = group_h / series_count(series.len());
    // Too little room per bar and the labels would overlap their neighbors.
    if sub_h < 9.0 {
        return;
    }
    ctx.save();
    ctx.set_font(axis_font);
    ctx.set_text_baseline("middle");
    ctx.set_fill_style_str(&theme.text.to_css());
    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);
    for (si, s) in series.iter().enumerate() {
        let n = s.categories.len().min(s.values.len());
        for ci in 0..n {
            let value = s.values[ci];
            if !value.is_finite() {
                continue;
            }
            let cy =
                plot.y + ci as f64 * row_h + (row_h - group_h) / 2.0 + (si as f64 + 0.5) * sub_h;
            let end_px = layout.x_scale.to_px(value);
            // Labels sit outside the bar end, flipping inward at the plot
            // edge so a full-width bar keeps its number visible.
            let (x, align) = if value >= 0.0 {
                if end_px + 4.0 < plot.x + plot.w - 24.0 {
                    (end_px + 4.0, "left")
                } else {
                    (end_px - 4.0, "right")
                }
            } else if end_px - 4.0 > plot.x + 24.0 {
                (end_px - 4.0, "right")
            } else {
                (end_px + 4.0, "left")
            };
            ctx.set_text_align(align);
            let _ = ctx.fill_text(&unit.format(value), x, cy);
        }
    }
    ctx.restore();
}

fn draw_grid(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    spec: &ChartSpec,
    data: &ChartData,
    theme: &Theme,
    aesthetic: &Aesthetic,
    axis_font: &str,
) {
    let ctx = &surface.ctx;
    let plot = layout.plot;

    ctx.set_font(axis_font);
    ctx.set_line_width(crisp_width(surface, 1.0));
    let text_css = theme.text.to_css();
    let grid_css = theme.grid.to_css();

    // Gridline pass (optionally glowing), then a label pass with shadows
    // cleared — shadowBlur under text destroys legibility (R-2).
    if aesthetic.glow {
        let glow = glow_color(theme.accent, 0.15);
        ctx.set_shadow_color(&glow);
        ctx.set_shadow_blur(2.0);
    }
    ctx.set_stroke_style_str(&grid_css);

    // HBar's and StateTimeline's cross axes are categorical (labels drawn by
    // their draw functions), so linear y ticks would be meaningless there.
    let linear_y_ticks = uses_numeric_y_ticks(data);
    let y_ticks = if linear_y_ticks {
        let (lo, hi) = layout.y_domain();
        let ticks = match spec.y_scale {
            ScaleKind::Linear => linear_ticks(lo, hi, 5),
            // A log axis is uneven, so pixel-spacing thinning would drop the
            // wrong ticks; `log_ticks` already bounds its own density.
            ScaleKind::Log => log_ticks(lo, hi, 6),
        };
        // Thin ticks whose pixel spacing would collide (R-4).
        thin_by_pixel_distance(ticks, |t| layout.y_px(*t), MIN_LABEL_SPACING_PX)
    } else {
        vec![]
    };
    for t in &y_ticks {
        let y = crisp_for(surface, layout.y_px(*t), 1.0);
        if y < plot.y - 0.5 || y > plot.y + plot.h + 0.5 {
            continue;
        }
        ctx.begin_path();
        ctx.move_to(plot.x, y);
        ctx.line_to(plot.x + plot.w, y);
        ctx.stroke();
    }

    // X gridlines + collect label positions per axis kind.
    let mut x_labels: Vec<(f64, String)> = vec![];
    match &spec.x_axis {
        XAxisKind::Time => {
            let ticks = time_ticks(
                layout.x_scale.d0 as i64,
                layout.x_scale.d1 as i64,
                (plot.w / 80.0).max(2.0) as usize,
            );
            for t in ticks {
                let x = crisp_for(surface, layout.x_scale.to_px(t.ms as f64), 1.0);
                ctx.begin_path();
                ctx.move_to(x, plot.y);
                ctx.line_to(x, plot.y + plot.h);
                ctx.stroke();
                x_labels.push((x, t.label));
            }
        }
        XAxisKind::Linear { .. } => {
            let ticks = linear_ticks(
                layout.x_scale.d0,
                layout.x_scale.d1,
                (plot.w / 80.0).max(2.0) as usize,
            );
            for t in ticks {
                let x = crisp_for(surface, layout.x_scale.to_px(t), 1.0);
                if x < plot.x - 0.5 || x > plot.x + plot.w + 0.5 {
                    continue;
                }
                ctx.begin_path();
                ctx.move_to(x, plot.y);
                ctx.line_to(x, plot.y + plot.h);
                ctx.stroke();
                x_labels.push((x, spec.x_unit.format(t)));
            }
        }
        XAxisKind::Category { labels } => {
            // Band-centered labels, no gridlines through the bands.
            for (pos, label) in category_ticks(labels) {
                x_labels.push((layout.x_scale.to_px(pos), label));
            }
            // Category labels can be arbitrarily dense — thin to readable
            // spacing rather than overlapping (R-4).
            x_labels = thin_by_spacing(x_labels, |(x, _)| *x, 40.0);
        }
    }

    x_labels = thin_x_labels(ctx, x_labels);

    // Label pass: shadows off unless explicitly requested.
    if !aesthetic.glow_text {
        ctx.set_shadow_color("transparent");
        ctx.set_shadow_blur(0.0);
    }
    ctx.set_fill_style_str(&text_css);
    if linear_y_ticks {
        ctx.set_text_align("right");
        ctx.set_text_baseline("middle");
        for t in &y_ticks {
            let y = crisp_for(surface, layout.y_px(*t), 1.0);
            if y < plot.y - 0.5 || y > plot.y + plot.h + 0.5 {
                continue;
            }
            let _ = ctx.fill_text(&spec.unit.format(*t), plot.x - 6.0, y);
        }
    }
    ctx.set_text_baseline("top");
    for (x, label) in &x_labels {
        let width = ctx.measure_text(label).map_or(0.0, |m| m.width());
        let align = if *x - width / 2.0 < 0.0 {
            "left"
        } else if *x + width / 2.0 > surface.css_w {
            "right"
        } else {
            "center"
        };
        ctx.set_text_align(align);
        let _ = ctx.fill_text(label, *x, plot.y + plot.h + 6.0);
    }

    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);
}

fn uses_numeric_y_ticks(data: &ChartData) -> bool {
    !matches!(
        data,
        ChartData::HBar(_) | ChartData::Heatmap(_) | ChartData::StateTimeline(_)
    )
}

fn stroke_series(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &SeriesData,
    color: u32,
    fill: bool,
    config: &DrawConfig,
) {
    let ctx = &surface.ctx;
    let decimated = decimate_series(
        &series.xs,
        &series.ys,
        &layout.x_scale,
        layout.plot.w,
        config.decimation,
        layout.y_kind,
    );
    if decimated.is_empty() {
        return;
    }

    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();

    let baseline = layout.y_baseline_px();
    let (r, g, b) = rgb_to_components(color);

    // Map to pixel space, preserving NaN gap markers so the path splits at gaps
    // instead of bridging across them. `line_path` smooths each contiguous
    // finite run independently, keeping point/index spaces aligned. (C1)
    let pixel_points: Vec<(f64, f64)> = decimated
        .iter()
        .map(|(x, y)| {
            let px = layout.x_scale.to_px(*x);
            let py = if y.is_finite() {
                layout.y_px(*y)
            } else {
                f64::NAN
            };
            (px, py)
        })
        .collect();

    let ops = line_path(&pixel_points, config.smooth);
    if ops.is_empty() {
        // A singleton has no line path, but it is still a valid observation.
        // Draw the same default marker as a sparse series so one-point lines
        // and areas do not silently render blank.
        draw_point_markers(ctx, &pixel_points, color, config);
        ctx.restore();
        return;
    }

    if config.glow {
        let glow_s = format!("rgba({r},{g},{b},0.35)");
        let glow_shadow = format!("rgba({r},{g},{b},0.6)");
        ctx.set_stroke_style_str(&glow_s);
        ctx.set_line_width(crisp_width(surface, 4.0));
        ctx.set_line_join("round");
        ctx.set_line_cap("round");
        ctx.set_shadow_color(&glow_shadow);
        ctx.set_shadow_blur(8.0 * config.glow_intensity);
        ctx.set_shadow_offset_x(0.0);
        ctx.set_shadow_offset_y(0.0);
        ctx.begin_path();
        replay_path(ctx, &ops);
        ctx.stroke();

        let glow_wide = format!("rgba({r},{g},{b},0.12)");
        ctx.set_stroke_style_str(&glow_wide);
        ctx.set_line_width(crisp_width(surface, 8.0));
        ctx.set_shadow_blur(16.0 * config.glow_intensity);
        ctx.begin_path();
        replay_path(ctx, &ops);
        ctx.stroke();
    }

    let core_color = css_color(color);
    ctx.set_stroke_style_str(&core_color);
    ctx.set_line_width(crisp_width(surface, 1.5));
    ctx.set_line_join("round");
    ctx.set_line_cap("round");
    if config.glow {
        let gc = format!("rgba({r},{g},{b},0.4)");
        ctx.set_shadow_color(&gc);
        ctx.set_shadow_blur(3.0 * config.glow_intensity);
    } else {
        ctx.set_shadow_color("transparent");
        ctx.set_shadow_blur(0.0);
    }
    ctx.begin_path();
    replay_path(ctx, &ops);
    ctx.stroke();

    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);

    draw_point_markers(ctx, &pixel_points, color, config);

    if fill && let Some(fx) = pixel_points.iter().find(|p| p.1.is_finite()).map(|p| p.0) {
        if config.gradient_fills {
            // Vertical gradient (x0 == x1), so `fx` only anchors the gradient line.
            let grad = make_linear_gradient(ctx, fx, layout.plot.y, fx, baseline);
            let c1 = format!("rgba({r},{g},{b},0.30)");
            let c2 = format!("rgba({r},{g},{b},0.10)");
            let c3 = format!("rgba({r},{g},{b},0.02)");
            grad.add_color_stop(0.0, &c1).unwrap();
            grad.add_color_stop(0.6, &c2).unwrap();
            grad.add_color_stop(1.0, &c3).unwrap();
            set_fill_gradient(ctx, &grad);
        } else {
            ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.15)"));
        }
        let fill_ops = build_fill_path_with_curve(&decimated, layout, baseline, config.smooth);
        ctx.begin_path();
        replay_path(ctx, &fill_ops);
        ctx.fill();
    }

    ctx.restore();
}

fn replay_path(ctx: &web_sys::CanvasRenderingContext2d, ops: &[PathOp]) {
    for op in ops {
        match op {
            PathOp::MoveTo(x, y) => ctx.move_to(*x, *y),
            PathOp::LineTo(x, y) => ctx.line_to(*x, *y),
            PathOp::CurveTo(c1x, c1y, c2x, c2y, x, y) => {
                ctx.bezier_curve_to(*c1x, *c1y, *c2x, *c2y, *x, *y);
            }
        }
    }
}

fn build_fill_path(decimated: &[(f64, f64)], layout: &ChartLayout, baseline: f64) -> Vec<PathOp> {
    let mut ops = Vec::with_capacity(decimated.len() + 4);
    let mut first_x = None;
    let mut last_x = 0.0;
    for (x, y) in decimated {
        if !y.is_finite() {
            if let Some(first_x) = first_x.take() {
                ops.push(PathOp::LineTo(last_x, baseline));
                ops.push(PathOp::LineTo(first_x, baseline));
            }
            continue;
        }
        let px = layout.x_scale.to_px(*x);
        let py = layout.y_px(*y);
        if first_x.is_none() {
            ops.push(PathOp::MoveTo(px, py));
            first_x = Some(px);
        } else {
            ops.push(PathOp::LineTo(px, py));
        }
        last_x = px;
    }
    if let Some(first_x) = first_x {
        ops.push(PathOp::LineTo(last_x, baseline));
        ops.push(PathOp::LineTo(first_x, baseline));
    }
    ops
}

/// Build an area boundary from the same curve used by the visible stroke.
/// The legacy straight-line helper remains useful to callers/tests that need
/// an intentionally unsmoothed polygon, while normal rendering uses this
/// variant so a smoothed edge cannot leave a conspicuous gap from its fill.
fn build_fill_path_with_curve(
    decimated: &[(f64, f64)],
    layout: &ChartLayout,
    baseline: f64,
    smooth: bool,
) -> Vec<PathOp> {
    if !smooth {
        return build_fill_path(decimated, layout, baseline);
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    while start < decimated.len() {
        while start < decimated.len() && !decimated[start].1.is_finite() {
            start += 1;
        }
        let end = (start..decimated.len())
            .find(|&index| !decimated[index].1.is_finite())
            .unwrap_or(decimated.len());
        if end > start {
            let points: Vec<(f64, f64)> = decimated[start..end]
                .iter()
                .map(|(x, y)| (layout.x_scale.to_px(*x), layout.y_px(*y)))
                .filter(|(x, y)| x.is_finite() && y.is_finite())
                .collect();
            if !points.is_empty() {
                let first_x = points[0].0;
                let last_x = points.last().map(|point| point.0).unwrap_or(first_x);
                out.extend(line_path(&points, true));
                out.push(PathOp::LineTo(last_x, baseline));
                out.push(PathOp::LineTo(first_x, baseline));
            }
        }
        start = end.saturating_add(1);
    }
    out
}

fn scatter_points(layout: &ChartLayout, series: &SeriesData) -> Vec<(f64, f64)> {
    let plot = layout.plot;
    if !plot.w.is_finite() || !plot.h.is_finite() || plot.w <= 0.0 || plot.h <= 0.0 {
        return vec![];
    }
    let n = series.xs.len().min(series.ys.len());
    let (start, end) = visible_point_bounds(&series.xs[..n], &layout.x_scale);
    let cell = 4.0_f64
        .max((plot.w * plot.h / 4096.0).sqrt())
        .max(plot.w.max(plot.h) / 4096.0);
    let cols = (plot.w / cell).ceil() as usize + 1;
    let rows = (plot.h / cell).ceil() as usize + 1;
    let mut occupied = vec![false; cols * rows];
    let mut points = Vec::with_capacity((end - start).min(occupied.len()));
    for (x, y) in series.xs[start..end].iter().zip(&series.ys[start..end]) {
        let px = layout.x_scale.to_px(*x);
        let py = layout.y_px(*y);
        if !px.is_finite() || !py.is_finite() || !plot.contains(px, py) {
            continue;
        }
        let index = ((py - plot.y) / cell) as usize * cols + ((px - plot.x) / cell) as usize;
        if let Some(slot) = occupied.get_mut(index)
            && !*slot
        {
            *slot = true;
            points.push((px, py));
        }
    }
    points
}

fn draw_scatter(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &SeriesData,
    color: u32,
    config: &DrawConfig,
) {
    let ctx = &surface.ctx;
    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();

    // Two-dimensional occupancy preserves a point cloud's interior. M4 is
    // appropriate for lines but discards most observations at repeated x.
    let points = scatter_points(layout, series);

    let colors = SeriesColors::new(color);
    let glow = config
        .glow
        .then_some((colors.glow_shadow.as_str(), 10.0 * config.glow_intensity));
    batch_glow_dots(
        ctx,
        &points,
        &colors.core,
        3.0,
        glow,
        Some(("rgba(255,255,255,0.8)", 1.0)),
    );

    ctx.restore();
}

fn draw_heatmap(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[SeriesData],
    scale: HeatmapColorScale,
    chrome: bool,
    theme: &Theme,
    axis_font: &str,
) {
    let ctx = &surface.ctx;
    if series.is_empty() {
        return;
    }
    let cell_h = crate::hit::row_height(layout, series.len()).unwrap_or(0.0);
    if chrome {
        let row_labels = thin_by_spacing(
            series
                .iter()
                .enumerate()
                .filter(|(_, series)| !series.name.is_empty())
                .map(|(row, series)| {
                    (
                        layout.plot.y + (row as f64 + 0.5) * cell_h,
                        series.name.clone(),
                    )
                })
                .collect(),
            |(y, _)| *y,
            MIN_LABEL_SPACING_PX,
        );
        let ctx = &surface.ctx;
        ctx.save();
        ctx.set_font(axis_font);
        ctx.set_fill_style_str(&theme.text.to_css());
        ctx.set_text_align("right");
        ctx.set_text_baseline("middle");
        for (y, label) in row_labels {
            let _ = ctx.fill_text(&label, layout.plot.x - 6.0, y);
        }
        ctx.restore();
    }
    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();
    // Cell shading normalizes against the value domain in data space; a log
    // y scale stores log10 bounds, so read the domain rather than `y_scale`.
    let (heat_lo, heat_hi) = layout.y_domain();
    let mut xs: Vec<f64> = series
        .iter()
        .flat_map(|row| {
            let n = row.xs.len().min(row.ys.len());
            let (start, end) = visible_point_bounds(&row.xs[..n], &layout.x_scale);
            row.xs[start..end].iter().copied()
        })
        .filter(|x| x.is_finite())
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs.dedup_by(|a, b| *a == *b);
    let cell_w = crate::series::sampled_median_gap(&xs)
        .map(|step| {
            (layout.x_scale.to_px(layout.x_scale.d0 + step)
                - layout.x_scale.to_px(layout.x_scale.d0))
            .abs()
        })
        // A one-column heatmap is still useful (for example a current-state
        // matrix); use the plot width instead of silently drawing nothing.
        .unwrap_or(layout.plot.w)
        .clamp(0.0, layout.plot.w.max(0.0));
    for (si, s) in series.iter().enumerate() {
        let n = s.xs.len().min(s.ys.len());
        let (start, end) = visible_point_bounds(&s.xs[..n], &layout.x_scale);
        for (x, y) in s.xs[start..end].iter().zip(&s.ys[start..end]) {
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            ctx.set_fill_style_str(&heatmap_color_for_value(scale, *y, heat_lo, heat_hi).to_css());
            let px = layout.x_scale.to_px(*x);
            // Keep the continuous row partition when rows are denser than a
            // CSS pixel. Device snapping may collapse a subpixel cell to zero
            // width; skipping that cell preserves shared boundaries instead
            // of expanding it and overlapping its neighbor.
            let (rx, _, rw, _) =
                crisp_rect_at_dpr(px - cell_w / 2.0, 0.0, cell_w, 1.0, surface.dpr);
            if rw <= 0.0 {
                continue;
            }
            let ry = layout.plot.y + si as f64 * cell_h;
            ctx.fill_rect(rx, ry, rw, cell_h);
        }
    }
    ctx.restore();
}

/// Heatmap row geometry is deliberately shared with hit testing. Rows may be
/// thinner than a device pixel for dense matrices; clamping their height to a
/// pixel makes the last rows unreachable and disagrees with the displayed
/// data. Callers should use the same continuous partition for drawing and hit.
fn draw_vignette(surface: &CanvasSurface, layout: &ChartLayout, strength: f64) {
    let ctx = &surface.ctx;
    let plot = layout.plot;
    let cx = plot.x + plot.w / 2.0;
    let cy = plot.y + plot.h / 2.0;
    let radius = (plot.w.max(plot.h)) / 2.0;
    let grad = make_radial_gradient(ctx, cx, cy, radius * 0.5, cx, cy, radius);
    let alpha = (strength * 0.4).clamp(0.0, 0.4);
    grad.add_color_stop(0.0, "rgba(0,0,0,0)").unwrap();
    let edge = format!("rgba(0,0,0,{alpha})");
    grad.add_color_stop(1.0, &edge).unwrap();
    set_fill_gradient(ctx, &grad);
    ctx.fill_rect(plot.x, plot.y, plot.w, plot.h);
}

// --- OHLC / Candlestick ---

/// Aggregate OHLC ticks to at most one candle per pixel column: first open,
/// max high, min low, last close (P-3). Passthrough when already sparse.
fn decimate_ohlc(
    ticks: &[crate::series::OhlcTick],
    x_scale: &crate::scale::LinearScale,
    px_width: f64,
) -> Vec<crate::series::OhlcTick> {
    if (ticks.len() as f64) <= px_width {
        return ticks.to_vec();
    }
    let mut out: Vec<crate::series::OhlcTick> = Vec::with_capacity(px_width as usize + 1);
    let mut col = f64::NEG_INFINITY;
    for t in ticks {
        let c = x_scale.to_px(t.ts).floor();
        if c != col {
            col = c;
            out.push(*t);
        } else if let Some(agg) = out.last_mut() {
            agg.high = agg.high.max(t.high);
            agg.low = agg.low.min(t.low);
            agg.close = t.close;
        }
    }
    out
}

fn is_drawable_ohlc_tick(tick: &crate::series::OhlcTick) -> bool {
    tick.ts.is_finite()
        && tick.open.is_finite()
        && tick.high.is_finite()
        && tick.low.is_finite()
        && tick.close.is_finite()
        && tick.low <= tick.high
        && tick.open >= tick.low
        && tick.open <= tick.high
        && tick.close >= tick.low
        && tick.close <= tick.high
}

fn draw_ohlc(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[OhlcSeriesData],
    theme: &Theme,
) {
    let ctx = &surface.ctx;
    if series.is_empty() {
        return;
    }
    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();

    for s in series.iter() {
        let up_color = theme.positive.to_css();
        let down_color = theme.negative.to_css();

        let drawable_ticks: Vec<_> = s
            .ticks
            .iter()
            .copied()
            .filter(is_drawable_ohlc_tick)
            .collect();
        let (start, end) = visible_tick_bounds(&drawable_ticks, &layout.x_scale);
        let ticks = decimate_ohlc(&drawable_ticks[start..end], &layout.x_scale, layout.plot.w);

        // Candle width from the real median tick spacing (C4), not a hardcoded
        // one-minute interval — otherwise any non-1m interval renders candles
        // overlapping or hairline-thin.
        let spacing = {
            let ts: Vec<f64> = ticks.iter().map(|t| t.ts).collect();
            median_gap(&ts).unwrap_or(60_000.0)
        };
        let candle_w = ((layout.x_scale.to_px(layout.x_scale.d0 + spacing)
            - layout.x_scale.to_px(layout.x_scale.d0))
            * 0.6)
            .max(2.0);

        for t in &ticks {
            if t.ts < layout.x_scale.d0 || t.ts > layout.x_scale.d1 {
                continue;
            }
            let cx = layout.x_scale.to_px(t.ts);
            let high_y = layout.y_px(t.high);
            let low_y = layout.y_px(t.low);
            let open_y = layout.y_px(t.open);
            let close_y = layout.y_px(t.close);
            let is_up = t.close >= t.open;
            let candle_color = if is_up { &up_color } else { &down_color };

            // High-low stem
            ctx.set_stroke_style_str(candle_color);
            ctx.set_line_width(crisp_width(surface, 1.0));
            ctx.begin_path();
            ctx.move_to(cx, high_y);
            ctx.line_to(cx, low_y);
            ctx.stroke();

            // Candle body
            let body_top = open_y.min(close_y);
            let body_bot = open_y.max(close_y);
            let body_h = (body_bot - body_top).max(1.0);
            ctx.set_fill_style_str(candle_color);
            let (rx, ry, rw, rh) =
                crisp_rect_at_dpr(cx - candle_w / 2.0, body_top, candle_w, body_h, surface.dpr);
            ctx.fill_rect(rx, ry, rw, rh);
        }
    }

    ctx.restore();
}

// --- Step chart ---

fn draw_step(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &SeriesData,
    color: u32,
    config: &DrawConfig,
) {
    let ctx = &surface.ctx;
    let decimated = decimate_series(
        &series.xs,
        &series.ys,
        &layout.x_scale,
        layout.plot.w,
        config.decimation,
        layout.y_kind,
    );
    if decimated.is_empty() {
        return;
    }

    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();

    // Step: horizontal-then-vertical (step-after)
    let core_color = css_color(color);
    ctx.set_stroke_style_str(&core_color);
    ctx.set_line_width(crisp_width(surface, 1.5));
    ctx.set_line_join("miter");
    ctx.set_line_cap("butt");

    ctx.begin_path();
    let mut first = true;
    let mut last_y = 0.0;
    for (x, y) in &decimated {
        if !y.is_finite() {
            first = true;
            continue;
        }
        let px = layout.x_scale.to_px(*x);
        let py = layout.y_px(*y);
        if first {
            ctx.move_to(px, py);
            first = false;
        } else {
            ctx.line_to(px, last_y);
            ctx.line_to(px, py);
        }
        last_y = py;
    }
    ctx.stroke();

    // Dots at each step point. Steps carry more meaning per sample than a
    // line, so `Auto` keeps the more permissive density gate of roughly one
    // dot per 4px of plot width rather than the shared point-count rule; an
    // explicit `Never`/`Always` still wins. (C12)
    let show_dots = match config.points {
        PointMarkers::Never => false,
        PointMarkers::Always => true,
        PointMarkers::Auto => decimated.len() < (layout.plot.w / 4.0) as usize,
    };
    if show_dots {
        let dot_points: Vec<(f64, f64)> = decimated
            .iter()
            .filter(|(_, y)| y.is_finite())
            .map(|(x, y)| (layout.x_scale.to_px(*x), layout.y_px(*y)))
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect();
        batch_glow_dots(ctx, &dot_points, &core_color, 2.0, None, None);
    }

    ctx.restore();
}

// --- Histogram ---

fn draw_histogram(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[HistogramSeries],
    mode: SeriesLayout,
    config: &DrawConfig,
) {
    let ctx = &surface.ctx;
    if series.is_empty() {
        return;
    }
    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();

    let series_len = series.len();
    let counts: Vec<Vec<f64>> = series.iter().map(HistogramSeries::plot_counts).collect();
    let bins = series
        .iter()
        .zip(&counts)
        .map(|(series, counts)| counts.len().min(series.buckets.len().saturating_sub(1)))
        .max()
        .unwrap_or(0);
    match mode {
        SeriesLayout::Grouped => {
            for (si, series) in series.iter().enumerate() {
                let series_counts = &counts[si];
                let n = series_counts
                    .len()
                    .min(series.buckets.len().saturating_sub(1));
                for (bucket, value) in series_counts.iter().copied().enumerate().take(n) {
                    if !value.is_finite() || value == 0.0 {
                        continue;
                    }
                    let x0 = layout.x_scale.to_px(series.buckets[bucket]);
                    let x1 = layout.x_scale.to_px(series.buckets[bucket + 1]);
                    let width = (x1 - x0).abs();
                    if width <= 0.0 {
                        continue;
                    }
                    let sub_width = width / series_count(series_len);
                    let left = x0.min(x1) + si as f64 * sub_width + 0.5;
                    draw_histogram_rect(
                        ctx,
                        layout,
                        left,
                        (sub_width - 1.0).max(1.0),
                        0.0,
                        value,
                        histogram_color(series, si),
                        config,
                        surface.dpr,
                    );
                }
            }
        }
        SeriesLayout::Stacked | SeriesLayout::StackedPercent => {
            for (bucket, _) in std::iter::repeat_n((), bins).enumerate() {
                let Some((edge_series, x0, x1)) =
                    series.iter().enumerate().find_map(|(si, series)| {
                        if bucket + 1 < series.buckets.len() {
                            Some((si, series.buckets[bucket], series.buckets[bucket + 1]))
                        } else {
                            None
                        }
                    })
                else {
                    continue;
                };
                if !x0.is_finite() || !x1.is_finite() || x0 == x1 {
                    continue;
                }
                let (positive_total, negative_total) = counts
                    .iter()
                    .filter_map(|counts| counts.get(bucket).copied())
                    .filter(|v| v.is_finite())
                    .fold((0.0, 0.0), |(positive, negative), value| {
                        if value >= 0.0 {
                            (positive + value, negative)
                        } else {
                            (positive, negative + value.abs())
                        }
                    });
                let mut positive = 0.0;
                let mut negative = 0.0;
                for (si, series) in series.iter().enumerate() {
                    let Some(mut value) = counts[si].get(bucket).copied().filter(|v| v.is_finite())
                    else {
                        continue;
                    };
                    if matches!(mode, SeriesLayout::StackedPercent) {
                        value = if value >= 0.0 && positive_total > 0.0 {
                            value / positive_total * 100.0
                        } else if value < 0.0 && negative_total > 0.0 {
                            value / negative_total * 100.0
                        } else {
                            0.0
                        };
                    }
                    let (base, top) = if value >= 0.0 {
                        let base = positive;
                        positive += value;
                        (base, positive)
                    } else {
                        let base = negative;
                        negative += value;
                        (base, negative)
                    };
                    draw_histogram_rect(
                        ctx,
                        layout,
                        layout.x_scale.to_px(x0.min(x1)) + 0.5,
                        ((layout.x_scale.to_px(x1) - layout.x_scale.to_px(x0)).abs() - 1.0)
                            .max(1.0),
                        base,
                        top,
                        histogram_color(series, si),
                        config,
                        surface.dpr,
                    );
                }
                let _ = edge_series; // documents that bucket geometry comes from one coherent edge pair.
            }
        }
    }

    ctx.restore();
}

fn series_count(count: usize) -> f64 {
    count.max(1) as f64
}

fn histogram_color(series: &HistogramSeries, index: usize) -> u32 {
    crate::series::palette_color(series.color, index)
}

#[allow(clippy::too_many_arguments)] // Geometry values are intentionally explicit at call sites.
fn draw_histogram_rect(
    ctx: &web_sys::CanvasRenderingContext2d,
    layout: &ChartLayout,
    x: f64,
    width: f64,
    base: f64,
    top: f64,
    color: u32,
    config: &DrawConfig,
    dpr: f64,
) {
    if base == top || !base.is_finite() || !top.is_finite() {
        return;
    }
    let (r, g, b) = rgb_to_components(color);
    let base_px = layout.y_edge_px(base);
    let top_px = layout.y_edge_px(top);
    let y = base_px.min(top_px);
    let h = (base_px - top_px).abs().max(1.0);
    if config.glow {
        ctx.set_shadow_color(&format!("rgba({r},{g},{b},0.3)"));
        ctx.set_shadow_blur(4.0 * config.glow_intensity);
    }
    if config.gradient_fills {
        let grad = make_linear_gradient(ctx, x, y, x, y + h);
        grad.add_color_stop(0.0, &format!("rgba({r},{g},{b},0.90)"))
            .unwrap();
        grad.add_color_stop(1.0, &format!("rgba({r},{g},{b},0.40)"))
            .unwrap();
        set_fill_gradient(ctx, &grad);
    } else {
        ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.85)"));
    }
    let (rx, ry, rw, rh) = crisp_rect_at_dpr(x, y, width, h, dpr);
    ctx.fill_rect(rx, ry, rw, rh);
    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);
}

// --- Horizontal Bar ---

fn draw_hbar(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[HBarSeries],
    config: &DrawConfig,
    theme: &Theme,
    axis_font: &str,
) {
    let ctx = &surface.ctx;
    if series.is_empty() {
        return;
    }
    ctx.save();

    let count = series
        .iter()
        .map(|s| s.categories.len().min(s.values.len()))
        .max()
        .unwrap_or(0);
    if count == 0 {
        ctx.restore();
        return;
    }

    let row_h = layout.plot.h / count as f64;
    let group_h = row_h * 0.8;
    let sub_h = group_h / series_count(series.len());
    let zero_px = layout.x_scale.to_px(0.0);

    // Draw category labels on the left
    ctx.set_font(axis_font);
    ctx.set_text_align("right");
    ctx.set_text_baseline("middle");
    ctx.set_fill_style_str(&theme.text.to_css());
    for row in 0..count {
        if let Some(label) = series.iter().find_map(|series| series.categories.get(row)) {
            let cy = layout.plot.y + row as f64 * row_h + row_h / 2.0;
            let _ = ctx.fill_text(label, layout.plot.x - 4.0, cy);
        }
    }

    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();

    for si in (0..series.len()).rev() {
        let s = &series[si];
        let color = crate::series::palette_color(s.color, si);
        let (r, g, b) = rgb_to_components(color);
        if config.glow {
            let gc = format!("rgba({r},{g},{b},0.4)");
            ctx.set_shadow_color(&gc);
            ctx.set_shadow_blur(4.0);
        }

        let n = s.categories.len().min(s.values.len());
        for ci in 0..n {
            let val = s.values[ci];
            if !val.is_finite() {
                continue;
            }
            let cy = layout.plot.y
                + ci as f64 * row_h
                + (row_h - group_h) / 2.0
                + (si as f64 + 0.5) * sub_h;
            let val_px = layout.x_scale.to_px(val);
            let w = (val_px - zero_px).abs().max(1.0);
            let x = if val >= 0.0 { zero_px } else { val_px };
            let bar_top = cy - sub_h / 2.0;

            if config.gradient_fills {
                let c1 = format!("rgba({r},{g},{b},0.95)");
                let c2 = format!("rgba({r},{g},{b},0.55)");
                let grad = make_linear_gradient(ctx, x, 0.0, x + w, 0.0);
                grad.add_color_stop(0.0, &c1).unwrap();
                grad.add_color_stop(1.0, &c2).unwrap();
                set_fill_gradient(ctx, &grad);
            } else {
                ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.85)"));
            }
            let (rx, ry, rw, rh) = crisp_rect_at_dpr(x, bar_top, w, sub_h.max(1.0), surface.dpr);
            ctx.fill_rect(rx, ry, rw, rh);

            ctx.set_shadow_color("transparent");
            ctx.set_shadow_blur(0.0);
        }
    }

    ctx.restore();
    ctx.restore();
}

// --- Stacked bars ---

fn draw_stacked_bars(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[SeriesData],
    config: &DrawConfig,
) {
    let ctx = &surface.ctx;
    let Some(first) = series.iter().find(|s| s.xs.len() > 1) else {
        if let Some(s) = series.first()
            && s.xs.len() == 1
        {
            draw_bar_column(surface, layout, series, 0, layout.plot.w.min(24.0), config);
        }
        return;
    };
    let visible = visible_stack_indices(&first.xs, series, layout, false, false);
    if visible.is_empty() {
        return;
    }
    let step = crate::series::sampled_median_gap(&first.xs).unwrap_or(1.0);
    let bar_w = (layout.x_scale.to_px(step) - layout.x_scale.to_px(0.0) - 1.0).max(1.0);
    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();
    for bi in visible {
        draw_bar_column(surface, layout, series, bi, bar_w, config);
    }
    ctx.restore();
}

fn draw_bar_column(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[SeriesData],
    bucket: usize,
    bar_w: f64,
    config: &DrawConfig,
) {
    let ctx = &surface.ctx;
    let mut positive = 0.0;
    let mut negative = 0.0;
    for (si, s) in series.iter().enumerate() {
        let (Some(x), Some(y)) = (s.xs.get(bucket), s.ys.get(bucket)) else {
            continue;
        };
        if !y.is_finite() || *y == 0.0 {
            continue;
        }
        let px = layout.x_scale.to_px(*x);
        let (base, top) = if *y >= 0.0 {
            let base = positive;
            positive += *y;
            (base, positive)
        } else {
            let base = negative;
            negative += *y;
            (base, negative)
        };
        let base_px = layout.y_edge_px(base);
        let top_px = layout.y_edge_px(top);
        let bar_top = base_px.min(top_px);
        let h = (base_px - top_px).abs().max(1.0);
        let color = series_color(s, si);
        let (r, g, b) = rgb_to_components(color);
        if config.glow {
            let gc = format!("rgba({r},{g},{b},0.4)");
            ctx.set_shadow_color(&gc);
            ctx.set_shadow_blur(6.0 * config.glow_intensity);
        }
        if config.gradient_fills {
            let grad = make_linear_gradient(ctx, px, bar_top, px, bar_top + h);
            let c1 = format!("rgba({r},{g},{b},0.95)");
            let c2 = format!("rgba({r},{g},{b},0.65)");
            grad.add_color_stop(0.0, &c1).unwrap();
            grad.add_color_stop(1.0, &c2).unwrap();
            set_fill_gradient(ctx, &grad);
        } else {
            ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.85)"));
        }
        let (rx, ry, rw, rh) = crisp_rect_at_dpr(px - bar_w / 2.0, bar_top, bar_w, h, surface.dpr);
        ctx.fill_rect(rx, ry, rw, rh);
        ctx.set_shadow_color("transparent");
        ctx.set_shadow_blur(0.0);
    }
}

// --- Band (percentile envelope) ---

/// Return a bounded set of jointly sampled band points in the visible window.
/// Keeping the envelope extremes together is essential: independently
/// reducing lower/center/upper can manufacture a band that never existed.
fn visible_band_indices(s: &crate::series::BandSeries, layout: &ChartLayout) -> Vec<usize> {
    let n = s.xs.len().min(s.lower.len()).min(s.upper.len());
    if n == 0 {
        return vec![];
    }
    let (start, end) = visible_point_bounds(&s.xs[..n], &layout.x_scale);
    let start = start.min(end);
    if end.saturating_sub(start) as f64 <= layout.plot.w.max(1.0) * 2.0 {
        return (start..end).collect();
    }
    let mut kept = Vec::with_capacity(layout.plot.w as usize * 4 + 4);
    let mut bucket = f64::NAN;
    let mut first = None;
    let mut lower_min = None;
    let mut lower_max = None;
    let mut upper_min = None;
    let mut upper_max = None;
    let mut center_low = None;
    let mut center_high = None;
    let mut last = None;
    let flush = |kept: &mut Vec<usize>,
                 first: &mut Option<usize>,
                 lower_min: &mut Option<usize>,
                 lower_max: &mut Option<usize>,
                 upper_min: &mut Option<usize>,
                 upper_max: &mut Option<usize>,
                 center_low: &mut Option<usize>,
                 center_high: &mut Option<usize>,
                 last: &mut Option<usize>| {
        let mut selected: Vec<usize> = [
            *first,
            *lower_min,
            *lower_max,
            *upper_min,
            *upper_max,
            *center_low,
            *center_high,
            *last,
        ]
        .into_iter()
        .flatten()
        .collect();
        selected.sort_unstable();
        selected.dedup();
        for index in selected {
            if kept.last().copied() != Some(index) {
                kept.push(index);
            }
        }
        *first = None;
        *lower_min = None;
        *lower_max = None;
        *upper_min = None;
        *upper_max = None;
        *center_low = None;
        *center_high = None;
        *last = None;
    };
    for index in start..end {
        let valid = s.xs[index].is_finite()
            && s.lower[index].is_finite()
            && s.upper[index].is_finite()
            && layout.y_px(s.lower[index]).is_finite()
            && layout.y_px(s.upper[index]).is_finite();
        if !valid {
            flush(
                &mut kept,
                &mut first,
                &mut lower_min,
                &mut lower_max,
                &mut upper_min,
                &mut upper_max,
                &mut center_low,
                &mut center_high,
                &mut last,
            );
            // Retain a separator so each finite run remains disconnected.
            if kept.last().copied() != Some(index) {
                kept.push(index);
            }
            bucket = f64::NAN;
            continue;
        }
        let col = layout.x_scale.to_px(s.xs[index]).floor();
        if col != bucket {
            flush(
                &mut kept,
                &mut first,
                &mut lower_min,
                &mut lower_max,
                &mut upper_min,
                &mut upper_max,
                &mut center_low,
                &mut center_high,
                &mut last,
            );
            bucket = col;
            first = Some(index);
        }
        lower_min = match lower_min {
            Some(old) if s.lower[old] <= s.lower[index] => Some(old),
            _ => Some(index),
        };
        lower_max = match lower_max {
            Some(old) if s.lower[old] >= s.lower[index] => Some(old),
            _ => Some(index),
        };
        upper_min = match upper_min {
            Some(old) if s.upper[old] <= s.upper[index] => Some(old),
            _ => Some(index),
        };
        upper_max = match upper_max {
            Some(old) if s.upper[old] >= s.upper[index] => Some(old),
            _ => Some(index),
        };
        // Keep an explicit center separator even when the envelope remains
        // valid. The fill may continue across this point, but the center
        // stroke must retain the telemetry gap after reduction.
        if !s
            .center
            .get(index)
            .copied()
            .is_some_and(|value| value.is_finite() && layout.y_px(value).is_finite())
            && kept.last().copied() != Some(index)
        {
            kept.push(index);
        }
        if let Some(center) = s
            .center
            .get(index)
            .copied()
            .filter(|value| value.is_finite() && layout.y_px(*value).is_finite())
        {
            center_low = match center_low {
                Some(old) if s.center[old] <= center => Some(old),
                _ => Some(index),
            };
            center_high = match center_high {
                Some(old) if s.center[old] >= center => Some(old),
                _ => Some(index),
            };
        }
        last = Some(index);
    }
    flush(
        &mut kept,
        &mut first,
        &mut lower_min,
        &mut lower_max,
        &mut upper_min,
        &mut upper_max,
        &mut center_low,
        &mut center_high,
        &mut last,
    );
    // Invalid center samples are inserted as separators while envelope
    // extrema are emitted by the bucket flush. Restore source order before
    // drawing so a gap can never make the reduced path run backwards.
    kept.sort_unstable();
    kept.dedup();
    kept
}

fn draw_band(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[crate::series::BandSeries],
    config: &DrawConfig,
) {
    let ctx = &surface.ctx;
    if series.is_empty() {
        return;
    }
    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();

    for (si, s) in series.iter().enumerate() {
        let color = crate::series::palette_color(s.color, si);
        let (r, g, b) = rgb_to_components(color);
        let indices = visible_band_indices(s, layout);
        if indices.is_empty() {
            continue;
        }

        // Fill each contiguous finite run separately. Filtering invalid points
        // into one vector would join the two sides of a telemetry gap.
        ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.18)"));
        let mut start = 0usize;
        while start < indices.len() {
            while start < indices.len() {
                let i = indices[start];
                if s.xs[i].is_finite()
                    && s.lower[i].is_finite()
                    && s.upper[i].is_finite()
                    && layout.y_px(s.lower[i]).is_finite()
                    && layout.y_px(s.upper[i]).is_finite()
                {
                    break;
                }
                start += 1;
            }
            let end = (start..indices.len())
                .find(|position| {
                    let i = indices[*position];
                    !s.xs[i].is_finite()
                        || !s.lower[i].is_finite()
                        || !s.upper[i].is_finite()
                        || !layout.y_px(s.lower[i]).is_finite()
                        || !layout.y_px(s.upper[i]).is_finite()
                })
                .unwrap_or(indices.len());
            if end >= start + 2 {
                let first = indices[start];
                ctx.begin_path();
                ctx.move_to(
                    layout.x_scale.to_px(s.xs[first]),
                    layout.y_px(s.upper[first]),
                );
                for &i in &indices[start + 1..end] {
                    ctx.line_to(layout.x_scale.to_px(s.xs[i]), layout.y_px(s.upper[i]));
                }
                for index in (start..end).rev() {
                    let i = indices[index];
                    ctx.line_to(layout.x_scale.to_px(s.xs[i]), layout.y_px(s.lower[i]));
                }
                ctx.close_path();
                ctx.fill();
            }
            start = end.saturating_add(1);
        }

        // Center line on top (gap-safe, optionally smoothed).
        let center_n = s.xs.len().min(s.center.len());
        let center_px: Vec<(f64, f64)> = indices
            .iter()
            .copied()
            .filter(|&index| index < center_n)
            .map(|index| {
                let x = s.xs[index];
                let y = s.center[index];
                (
                    layout.x_scale.to_px(x),
                    if x.is_finite() && y.is_finite() && layout.y_px(y).is_finite() {
                        layout.y_px(y)
                    } else {
                        f64::NAN
                    },
                )
            })
            .collect();
        let ops = line_path(&center_px, config.smooth);
        if !ops.is_empty() {
            let colors = SeriesColors::new(color);
            if config.glow {
                ctx.set_shadow_color(&colors.glow_shadow);
                ctx.set_shadow_blur(4.0 * config.glow_intensity);
            }
            ctx.set_stroke_style_str(&colors.core);
            ctx.set_line_width(crisp_width(surface, 1.5));
            ctx.set_line_join("round");
            ctx.set_line_cap("round");
            ctx.begin_path();
            replay_path(ctx, &ops);
            ctx.stroke();
            ctx.set_shadow_color("transparent");
            ctx.set_shadow_blur(0.0);
        } else {
            // A one-sample band has no envelope polygon or center path. Its
            // center is still a visible sample and must not disappear.
            let colors = SeriesColors::new(color);
            batch_glow_dots(
                ctx,
                &center_px
                    .iter()
                    .copied()
                    .filter(|(x, y)| x.is_finite() && y.is_finite())
                    .collect::<Vec<_>>(),
                &colors.core,
                2.5,
                config
                    .glow
                    .then_some((colors.glow_shadow.as_str(), 6.0 * config.glow_intensity)),
                None,
            );
        }
    }

    ctx.restore();
}

// --- State timeline ---

fn draw_state_timeline(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[crate::series::StateTimelineSeries],
    theme: &Theme,
    axis_font: &str,
) {
    let ctx = &surface.ctx;
    if series.is_empty() {
        return;
    }
    let plot = layout.plot;
    let row_h = plot.h / series.len() as f64;

    // Row labels sit in the left margin — draw before clipping to the plot.
    ctx.set_font(axis_font);
    ctx.set_text_align("right");
    ctx.set_text_baseline("middle");
    ctx.set_fill_style_str(&theme.text.to_css());
    for (ri, row) in series.iter().enumerate() {
        let cy = plot.y + ri as f64 * row_h + row_h / 2.0;
        let _ = ctx.fill_text(&row.name, plot.x - 6.0, cy);
    }

    ctx.save();
    ctx.begin_path();
    ctx.rect(plot.x, plot.y, plot.w, plot.h);
    ctx.clip();

    // Equal one-pixel insets on all four edges (two-pixel internal gaps).
    // No glow — state timelines are about at-a-glance discrete reading.
    for (ri, row) in series.iter().enumerate() {
        // Valid timelines are ordered and non-overlapping. Slice the viewport
        // before visiting segments, so zooming into years of history is cheap.
        let start = row
            .segments
            .partition_point(|seg| seg.end_ms < layout.x_scale.d0);
        let end = row
            .segments
            .partition_point(|seg| seg.start_ms <= layout.x_scale.d1);
        for seg in &row.segments[start.min(end)..end] {
            let Some(cell) = crate::hit::state_cell(layout, series.len(), ri, seg) else {
                continue;
            };
            let (rx, ry, rw, rh) = crisp_rect_at_dpr(cell.x, cell.y, cell.w, cell.h, surface.dpr);
            ctx.set_fill_style_str(&css_color(seg.color));
            ctx.fill_rect(rx, ry, rw, rh);
        }
    }

    ctx.restore();
}

// --- Stacked / percent areas ---

/// Stacked area fill with independent positive/negative baselines. This keeps
/// diverging data on the correct side of zero and preserves missing samples as
/// actual gaps rather than joining a polygon across absent data.
fn draw_stacked_areas(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[SeriesData],
    config: &DrawConfig,
    percent: bool,
) {
    let ctx = &surface.ctx;
    let Some(first) = series.iter().find(|s| !s.xs.is_empty()) else {
        return;
    };
    let indices = visible_stack_indices(&first.xs, series, layout, percent, true);
    if indices.is_empty() {
        return;
    }
    let xs: Vec<f64> = indices.iter().map(|&index| first.xs[index]).collect();
    let n = indices.len();

    let totals: Vec<(f64, f64)> = (0..n)
        .map(|visible_index| {
            let i = indices[visible_index];
            series
                .iter()
                .filter_map(|s| s.ys.get(i))
                .filter(|y| y.is_finite())
                .fold((0.0, 0.0), |(positive, negative), value| {
                    if *value >= 0.0 {
                        (positive + *value, negative)
                    } else {
                        (positive, negative + value.abs())
                    }
                })
        })
        .collect();

    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();

    let mut positive = vec![0.0f64; n];
    let mut negative = vec![0.0f64; n];
    for (si, s) in series.iter().enumerate() {
        let color = series_color(s, si);
        let (r, g, b) = rgb_to_components(color);

        let mut lower = vec![None; n];
        let mut upper = vec![None; n];
        for (visible_index, &source_index) in indices.iter().enumerate() {
            let Some(raw) =
                s.ys.get(source_index)
                    .copied()
                    .filter(|value| value.is_finite())
            else {
                continue;
            };
            if layout.y_kind == ScaleKind::Log && raw <= 0.0 {
                continue;
            }
            let (positive_total, negative_total) = totals[visible_index];
            let value = if percent {
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
            let (base, next) = if value >= 0.0 {
                let base = positive[visible_index];
                positive[visible_index] += value;
                (base, positive[visible_index])
            } else {
                let base = negative[visible_index];
                negative[visible_index] += value;
                (base, negative[visible_index])
            };
            lower[visible_index] = Some(base);
            upper[visible_index] = Some(next);
        }

        let alpha = if config.gradient_fills { 0.55 } else { 0.70 };
        ctx.set_fill_style_str(&format!("rgba({r},{g},{b},{alpha})"));
        draw_area_segments(ctx, layout, &xs, &lower, &upper, config.smooth && !percent);

        // Crisp top edge.
        let edge: Vec<(f64, f64)> = (0..n)
            .map(|i| {
                (
                    layout.x_scale.to_px(xs[i]),
                    upper[i]
                        .map(|value| layout.y_edge_px(value))
                        .unwrap_or(f64::NAN),
                )
            })
            .collect();
        let ops = line_path(&edge, config.smooth && !percent);
        ctx.set_stroke_style_str(&css_color(color));
        ctx.set_line_width(crisp_width(surface, 1.25));
        ctx.set_line_join("round");
        ctx.begin_path();
        replay_path(ctx, &ops);
        ctx.stroke();
        if ops.is_empty() {
            let points: Vec<(f64, f64)> = edge
                .iter()
                .copied()
                .filter(|(x, y)| x.is_finite() && y.is_finite())
                .collect();
            let colors = SeriesColors::new(color);
            batch_glow_dots(
                ctx,
                &points,
                &colors.core,
                2.5,
                config
                    .glow
                    .then_some((colors.glow_shadow.as_str(), 6.0 * config.glow_intensity)),
                None,
            );
        }
    }

    ctx.restore();
}

fn draw_area_segments(
    ctx: &web_sys::CanvasRenderingContext2d,
    layout: &ChartLayout,
    xs: &[f64],
    lower: &[Option<f64>],
    upper: &[Option<f64>],
    smooth: bool,
) {
    let n = xs.len().min(lower.len()).min(upper.len());
    let mut start = 0usize;
    while start < n {
        while start < n
            && (!xs[start].is_finite() || lower[start].is_none() || upper[start].is_none())
        {
            start += 1;
        }
        let end = (start..n)
            .find(|index| {
                !xs[*index].is_finite() || lower[*index].is_none() || upper[*index].is_none()
            })
            .unwrap_or(n);
        if end > start {
            ctx.begin_path();
            let upper_points: Vec<_> = (start..end)
                .map(|index| {
                    (
                        layout.x_scale.to_px(xs[index]),
                        layout.y_edge_px(upper[index].unwrap()),
                    )
                })
                .collect();
            let lower_points: Vec<_> = (start..end)
                .rev()
                .map(|index| {
                    (
                        layout.x_scale.to_px(xs[index]),
                        layout.y_edge_px(lower[index].unwrap()),
                    )
                })
                .collect();
            let upper_path = line_path(&upper_points, smooth);
            let lower_path = line_path(&lower_points, smooth);
            replay_path(ctx, &upper_path);
            for op in lower_path {
                match op {
                    PathOp::MoveTo(x, y) if !upper_path.is_empty() => ctx.line_to(x, y),
                    other => replay_path(ctx, std::slice::from_ref(&other)),
                }
            }
            ctx.close_path();
            ctx.fill();
        }
        start = end.saturating_add(1);
    }
}

// --- Grouped bars ---

fn draw_grouped_bars(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[SeriesData],
    config: &DrawConfig,
) {
    let ctx = &surface.ctx;
    let Some(first) = series.iter().find(|s| !s.xs.is_empty()) else {
        return;
    };
    let group_w = crate::series::sampled_median_gap(&first.xs)
        .map(|step| {
            (layout.x_scale.to_px(layout.x_scale.d0 + step)
                - layout.x_scale.to_px(layout.x_scale.d0))
            .abs()
        })
        // A single-bucket grouped chart has no spacing to infer. Center a
        // modest bucket rather than dropping all bars.
        .unwrap_or((layout.plot.w * 0.6).min(24.0))
        .max(1.0)
        - 2.0;
    let group_w = group_w.max(1.0);
    let sub_w = (group_w / series.len() as f64).max(1.0);
    let zero = layout.y_baseline_px();

    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();

    for (si, s) in series.iter().enumerate() {
        let color = series_color(s, si);
        let (r, g, b) = rgb_to_components(color);
        if config.glow {
            let gc = format!("rgba({r},{g},{b},0.4)");
            ctx.set_shadow_color(&gc);
            ctx.set_shadow_blur(6.0 * config.glow_intensity);
        }
        ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.85)"));
        let n = s.xs.len().min(s.ys.len());
        let (start, end) = visible_point_bounds(&s.xs[..n], &layout.x_scale);
        for (x, y) in s.xs[start..end].iter().zip(&s.ys[start..end]) {
            if !y.is_finite() || *y == 0.0 {
                continue;
            }
            let px = layout.x_scale.to_px(*x);
            let top = layout.y_edge_px(*y);
            let h = (zero - top).abs().max(1.0);
            let bar_x = px - group_w / 2.0 + si as f64 * sub_w;
            let (rx, ry, rw, rh) =
                crisp_rect_at_dpr(bar_x, top.min(zero), (sub_w - 1.0).max(1.0), h, surface.dpr);
            ctx.fill_rect(rx, ry, rw, rh);
        }
        ctx.set_shadow_color("transparent");
        ctx.set_shadow_blur(0.0);
    }

    ctx.restore();
}

// --- Overlay ---

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CursorOverlay {
    pub hover: Option<f64>,
    pub frozen: Option<f64>,
    pub unit: Unit,
    /// Raw pointer row in CSS pixels, when the pointer is over the chart.
    ///
    /// Drives the free horizontal crosshair and its y-axis readout: the value
    /// *under the cursor*, which is the question "how does this point compare
    /// to that gridline" actually asks. `None` draws neither.
    pub hover_y_px: Option<f64>,
}

impl CursorOverlay {
    /// A cursor with no pointer row — the shape for callers that only sync a
    /// timestamp across panels.
    pub fn at(hover: Option<f64>, frozen: Option<f64>, unit: Unit) -> Self {
        Self {
            hover,
            frozen,
            unit,
            hover_y_px: None,
        }
    }

    /// Attach the pointer row that drives the free crosshair readout.
    pub fn with_pointer_row(mut self, hover_y_px: Option<f64>) -> Self {
        self.hover_y_px = hover_y_px;
        self
    }
}

#[allow(clippy::too_many_arguments)] // Public overlay API mirrors independent interaction state.
pub fn draw_overlay(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    data: &ChartData,
    cursor: &CursorOverlay,
    hover_info: Option<&HoverInfo>,
    selection: Option<(f64, f64)>,
    annotations: &[Annotation],
    opts: &RenderOptions,
) {
    let theme = &opts.theme;
    let overlay = &opts.overlay;
    let series = data.point_series();
    let ctx = &surface.ctx;
    surface.clear();
    let plot = layout.plot;

    // Annotations apply to every chart kind, including partition overlays.
    for ann in annotations {
        draw_annotation(surface, layout, ann, theme, &opts.axis_font);
    }

    // Selection highlight
    if crate::partition::items(data).is_some() {
        if let Some(info) = hover_info
            && let Some(value) = info.values.first()
        {
            if let Some(shape) = value.shape {
                crate::partition::paint(surface, shape, &theme.hover_line.to_css());
            }
            if overlay.show_tooltip {
                draw_tooltip_limited(
                    surface,
                    layout,
                    info,
                    &[],
                    cursor.unit,
                    value.px,
                    value.py,
                    theme,
                    overlay.tooltip_max_values,
                    overlay.top_value_first,
                );
            }
        }
        return;
    }
    if let Some((x0, x1)) = selection {
        let (a, b) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let a = a.clamp(plot.x, plot.x + plot.w);
        let b = b.clamp(plot.x, plot.x + plot.w);
        ctx.set_fill_style_str(&theme.selection_fill.to_css());
        ctx.fill_rect(a, plot.y, b - a, plot.h);
        ctx.set_stroke_style_str(&theme.selection_border.to_css());
        ctx.set_line_width(crisp_width(surface, 1.0));
        let gs = glow_color(theme.accent, 0.3);
        ctx.set_shadow_color(&gs);
        ctx.set_shadow_blur(if opts.rrdtool.is_some() { 0.0 } else { 4.0 });
        ctx.stroke_rect(crisp_for(surface, a, 1.0), plot.y, b - a, plot.h);
        ctx.set_shadow_color("transparent");
        ctx.set_shadow_blur(0.0);
    }

    // Bucket-based overlay. Continue into guide/tooltip rendering: the old
    // early return made bars and heatmaps incapable of showing tooltips.
    if matches!(data.kind(), ChartKind::Bars | ChartKind::Heatmap) {
        if let Some(ts) = cursor.frozen {
            draw_bucket_band(surface, layout, series, ts, &theme.bar_frozen_band.to_css());
        }
        if let Some(ts) = cursor.hover {
            draw_bucket_band(surface, layout, series, ts, &theme.bar_hover_band.to_css());
        }
    }

    // Frozen crosshair
    if overlay.show_vertical_guide
        && let Some(ts) = cursor.frozen
    {
        let x = layout.x_scale.to_px(ts);
        if x >= plot.x && x <= plot.x + plot.w {
            let (r, g, b) = rgb_to_components(theme.accent);
            ctx.set_stroke_style_str(&theme.frozen_line.to_css());
            ctx.set_line_width(crisp_width(surface, 1.5));
            let gc = format!("rgba({r},{g},{b},0.5)");
            ctx.set_shadow_color(&gc);
            ctx.set_shadow_blur(if opts.rrdtool.is_some() { 0.0 } else { 6.0 });
            ctx.begin_path();
            ctx.move_to(crisp_for(surface, x, 1.5), plot.y);
            ctx.line_to(crisp_for(surface, x, 1.5), plot.y + plot.h);
            ctx.stroke();
            ctx.set_shadow_color("transparent");
            ctx.set_shadow_blur(0.0);
        }
    }

    // Hover crosshair
    let hover_x = cursor.hover.map(|ts| {
        if overlay.snap_to_data {
            hover_info
                .and_then(|info| {
                    info.values
                        .iter()
                        .find(|value| value.highlighted)
                        .or_else(|| info.values.first())
                })
                .map(|value| value.px)
                .unwrap_or_else(|| layout.x_scale.to_px(ts))
        } else {
            layout.x_scale.to_px(ts)
        }
    });
    if overlay.show_vertical_guide
        && let Some(x) = hover_x
        && x >= plot.x
        && x <= plot.x + plot.w
    {
        ctx.set_stroke_style_str(&theme.hover_line.to_css());
        ctx.set_line_width(crisp_width(surface, 1.0));
        ctx.begin_path();
        ctx.move_to(crisp_for(surface, x, 1.0), plot.y);
        ctx.line_to(crisp_for(surface, x, 1.0), plot.y + plot.h);
        ctx.stroke();
    }

    // Rows already claimed by a data-snapped guide badge, so the free cursor
    // readout below does not overprint one.
    let mut guide_rows: Vec<f64> = Vec::new();

    if let ChartData::StateTimeline(_) = data
        && let Some(info) = hover_info
        && let Some(value) = info.values.first()
        && let Some(crate::partition::Shape::Tile(cell)) = value.shape
    {
        let (x, y, w, h) = crisp_rect_at_dpr(cell.x, cell.y, cell.w, cell.h, surface.dpr);
        ctx.save();
        ctx.begin_path();
        ctx.rect(plot.x, plot.y, plot.w, plot.h);
        ctx.clip();
        ctx.set_fill_style_str(&theme.hover_line.to_css());
        ctx.fill_rect(x, y, w, h);
        ctx.set_stroke_style_str(&theme.text.to_css());
        ctx.set_line_width(1.0);
        ctx.stroke_rect(x, y, w, h);
        ctx.restore();
    }

    // Horizontal alignment guides. The automatically highlighted largest
    // value is considered first, then remaining values descend by magnitude.
    if overlay.show_horizontal_guides
        && overlay.max_horizontal_guides > 0
        && let Some(info) = hover_info
    {
        let mut guides: Vec<_> = info
            .values
            .iter()
            .filter(|value| layout.plot.contains(value.px, value.py))
            .collect();
        guides.sort_by(|a, b| {
            b.highlighted
                .cmp(&a.highlighted)
                .then_with(|| b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal))
        });
        for value in guides.into_iter().take(overlay.max_horizontal_guides) {
            if matches!(data, ChartData::Ohlc(_)) {
                for (index, label) in ["O", "C", "H", "L"].iter().enumerate() {
                    let Some((_, price)) = value.extra.iter().find(|(key, _)| key == label) else {
                        continue;
                    };
                    // Prefer the body when a wick or doji shares its price.
                    if ["O", "C", "H", "L"][..index].iter().any(|prior| {
                        value
                            .extra
                            .iter()
                            .any(|(key, p)| key == prior && p == price)
                    }) {
                        continue;
                    }
                    let py = layout.y_px(*price);
                    if !py.is_finite() || py < plot.y || py > plot.y + plot.h {
                        continue;
                    }
                    let alpha = if *label == "H" || *label == "L" {
                        0.22
                    } else {
                        0.55
                    };
                    let (r, g, b) = rgb_to_components(hover_value_color(value, series));
                    ctx.set_stroke_style_str(&format!("rgba({r},{g},{b},{alpha})"));
                    ctx.set_line_width(crisp_width(surface, 1.0));
                    set_line_dash_from_vec(ctx, &[3.0, 3.0]);
                    ctx.begin_path();
                    ctx.move_to(plot.x, crisp_for(surface, py, 1.0));
                    ctx.line_to(value.px, crisp_for(surface, py, 1.0));
                    ctx.stroke();
                    clear_line_dash(ctx);
                    if overlay.show_axis_values
                        && guide_rows.iter().all(|row| (row - py).abs() >= 11.0)
                    {
                        ctx.set_global_alpha(if alpha < 0.5 { 0.45 } else { 1.0 });
                        draw_y_axis_badge(
                            surface,
                            layout,
                            py,
                            &cursor.unit.format(*price),
                            hover_value_color(value, series),
                            opts,
                        );
                        ctx.set_global_alpha(1.0);
                    }
                    guide_rows.push(py);
                }
                continue;
            }
            guide_rows.push(value.py);
            let color = hover_value_color(value, series);
            let (r, g, b) = rgb_to_components(color);
            ctx.set_stroke_style_str(&format!("rgba({r},{g},{b},0.55)"));
            ctx.set_line_width(crisp_width(
                surface,
                if value.highlighted { 1.25 } else { 1.0 },
            ));
            set_line_dash_from_vec(ctx, &[3.0, 3.0]);
            ctx.begin_path();
            ctx.move_to(plot.x, crisp_for(surface, value.py, 1.0));
            ctx.line_to(
                value.px.clamp(plot.x, plot.x + plot.w),
                crisp_for(surface, value.py, 1.0),
            );
            ctx.stroke();
            clear_line_dash(ctx);
            if overlay.show_axis_values {
                // Several series can converge on one row. Keep the first
                // badge (the highlighted value is ordered first) and avoid
                // painting a stack of unreadable labels over it.
                let badge_free = guide_rows[..guide_rows.len().saturating_sub(1)]
                    .iter()
                    .all(|row| (row - value.py).abs() >= 11.0);
                if badge_free {
                    let label = overlay_y_badge_label_at(layout, data, cursor, info, value);
                    draw_y_axis_badge(surface, layout, value.py, &label, color, opts);
                }
            }
        }
    }

    // Free horizontal crosshair: the value under the pointer, traced back to
    // the y axis. Unlike the snapped guides this answers "what value is the
    // cursor sitting at", which is how a chart gets read against its grid.
    if overlay.show_cursor_value
        && uses_numeric_y_ticks(data)
        && let Some(py) = cursor.hover_y_px
        && py >= plot.y
        && py <= plot.y + plot.h
    {
        ctx.set_stroke_style_str(&theme.hover_line.to_css());
        ctx.set_line_width(crisp_width(surface, 1.0));
        set_line_dash_from_vec(ctx, &[2.0, 4.0]);
        ctx.begin_path();
        ctx.move_to(plot.x, crisp_for(surface, py, 1.0));
        ctx.line_to(plot.x + plot.w, crisp_for(surface, py, 1.0));
        ctx.stroke();
        clear_line_dash(ctx);
        if let Some(info) = hover_info
            && let Some(nearest) = info
                .values
                .iter()
                .filter(|value| plot.contains(value.px, value.py))
                .min_by(|a, b| (a.py - py).abs().total_cmp(&(b.py - py).abs()))
        {
            let delta = layout.y_at(py) - layout.y_at(nearest.py);
            if delta.is_finite() && delta != 0.0 && plot.h >= 14.0 {
                ctx.save();
                ctx.set_global_alpha(0.45);
                ctx.set_fill_style_str(&theme.text.to_css());
                ctx.set_font(&opts.axis_font);
                ctx.set_text_align("left");
                ctx.set_text_baseline("middle");
                let _ = ctx.fill_text(
                    &format!("Δ {}", cursor.unit.format(delta)),
                    plot.x + 6.0,
                    if (nearest.py - py).abs() >= 14.0 {
                        (py + nearest.py) / 2.0
                    } else {
                        (py + 12.0).clamp(plot.y + 7.0, plot.y + plot.h - 7.0)
                    },
                );
                ctx.restore();
            }
        }
        if overlay.show_axis_values && guide_rows.iter().all(|row| (row - py).abs() >= 11.0) {
            let value = layout.y_at(py);
            if value.is_finite() {
                draw_y_axis_badge(
                    surface,
                    layout,
                    py,
                    &cursor.unit.format(value),
                    theme.accent,
                    opts,
                );
            }
        }
    }

    if overlay.show_axis_values
        && let Some(x) = hover_x
    {
        let label = overlay_x_badge_label(data, cursor, hover_info);
        if let Some(label) = label {
            draw_x_axis_badge(surface, layout, x, &label, opts);
        }
    }

    // Hover dots. The current top value gets a larger ring so it remains
    // obvious even when several series converge at the cursor.
    if let Some(info) = hover_info {
        for v in &info.values {
            if !layout.plot.contains(v.px, v.py) {
                continue;
            }
            let color = hover_value_color(v, series);
            let (r, g, b) = rgb_to_components(color);
            let gc = format!("rgba({r},{g},{b},0.7)");
            ctx.set_shadow_color(&gc);
            ctx.set_shadow_blur(if opts.rrdtool.is_some() { 0.0 } else { 10.0 });
            let cs = css_color(color);
            ctx.set_fill_style_str(&cs);
            ctx.begin_path();
            let radius = if v.highlighted { 5.0 } else { 4.0 };
            let _ = ctx.arc(v.px, v.py, radius, 0.0, std::f64::consts::TAU);
            ctx.fill();
            ctx.set_shadow_color("transparent");
            ctx.set_shadow_blur(0.0);
            ctx.set_fill_style_str("rgba(255,255,255,0.9)");
            ctx.begin_path();
            let _ = ctx.arc(v.px, v.py, 1.5, 0.0, std::f64::consts::TAU);
            ctx.fill();
        }
        ctx.set_shadow_color("transparent");
        ctx.set_shadow_blur(0.0);
    }

    // Tooltip
    if overlay.show_tooltip
        && let Some(info) = hover_info
        && let Some(anchor) = info
            .values
            .iter()
            .find(|value| overlay.top_value_first && value.highlighted)
            .or_else(|| info.values.first())
    {
        draw_tooltip_limited(
            surface,
            layout,
            info,
            series,
            cursor.unit,
            anchor.px,
            anchor.py,
            theme,
            overlay.tooltip_max_values,
            overlay.top_value_first,
        );
    }
}

#[cfg(test)]
fn overlay_y_badge_label(
    data: &ChartData,
    cursor: &CursorOverlay,
    info: &HoverInfo,
    value: &crate::hit::HoverValue,
) -> String {
    match data {
        ChartData::HBar(_) => info.header.clone().unwrap_or_else(|| value.name.clone()),
        ChartData::Heatmap(_) | ChartData::StateTimeline(_) => value.name.clone(),
        _ => value
            .formatted_value
            .clone()
            .unwrap_or_else(|| cursor.unit.format(value.y)),
    }
}

fn overlay_y_badge_label_at(
    layout: &ChartLayout,
    data: &ChartData,
    cursor: &CursorOverlay,
    info: &HoverInfo,
    value: &crate::hit::HoverValue,
) -> String {
    match data {
        ChartData::HBar(_) => info.header.clone().unwrap_or_else(|| value.name.clone()),
        ChartData::Heatmap(_) | ChartData::StateTimeline(_) => value.name.clone(),
        // Numeric axis badges describe the plotted coordinate. HoverValue::y
        // may intentionally retain a raw contribution for stacked and
        // cumulative charts, while `py` is the composed point on the axis.
        _ => cursor.unit.format(layout.y_at(value.py)),
    }
}

fn overlay_x_badge_label(
    data: &ChartData,
    cursor: &CursorOverlay,
    info: Option<&HoverInfo>,
) -> Option<String> {
    if matches!(data, ChartData::HBar(_)) {
        return info.and_then(|info| {
            info.values
                .iter()
                .find(|value| value.highlighted)
                .or_else(|| info.values.first())
                .map(|value| {
                    value
                        .formatted_value
                        .clone()
                        .unwrap_or_else(|| cursor.unit.format(value.y))
                })
        });
    }
    info.and_then(|info| info.header.clone())
        .or_else(|| cursor.hover.map(|ts| format_ts_full(ts as i64)))
}

fn hover_value_color(value: &crate::hit::HoverValue, series: &[SeriesData]) -> u32 {
    value
        .color
        .or_else(|| {
            series
                .get(value.series)
                .map(|series| series_color(series, value.series))
        })
        .unwrap_or(0xffffff)
}

fn draw_y_axis_badge(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    py: f64,
    label: &str,
    color: u32,
    opts: &RenderOptions,
) {
    let ctx = &surface.ctx;
    if !py.is_finite() || label.is_empty() {
        return;
    }
    ctx.save();
    ctx.set_font(&opts.axis_font);
    let text_w = ctx.measure_text(label).map(|m| m.width()).unwrap_or(34.0);
    let h = 15.0_f64.min(surface.css_h.max(1.0));
    let w = (text_w + 8.0).min(surface.css_w.max(1.0));
    let x = (layout.plot.x - w - 2.0).clamp(1.0, (surface.css_w - w - 1.0).max(1.0));
    let y = if surface.css_h >= h + 2.0 {
        (py - h / 2.0).clamp(1.0, surface.css_h - h - 1.0)
    } else {
        0.0
    };
    ctx.set_fill_style_str(&opts.theme.bg.with_alpha(0.94).to_css());
    ctx.fill_rect(x, y, w, h);
    ctx.set_stroke_style_str(&css_color(color));
    ctx.set_line_width(crisp_width(surface, 1.0));
    ctx.stroke_rect(crisp_for(surface, x, 1.0), crisp_for(surface, y, 1.0), w, h);
    ctx.set_fill_style_str(&opts.theme.text.to_css());
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(label, x + w / 2.0, y + h / 2.0);
    ctx.restore();
}

fn draw_x_axis_badge(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    px: f64,
    label: &str,
    opts: &RenderOptions,
) {
    let ctx = &surface.ctx;
    ctx.save();
    ctx.set_font(&opts.axis_font);
    let text_w = ctx.measure_text(label).map(|m| m.width()).unwrap_or(70.0);
    let w = (text_w + 10.0).min(surface.css_w.max(1.0));
    let h = 16.0;
    let x = (px - w / 2.0).clamp(1.0, (surface.css_w - w - 1.0).max(1.0));
    let preferred_y = layout.plot.y + layout.plot.h + 3.0;
    let y = preferred_y.min((surface.css_h - h - 1.0).max(1.0));
    ctx.set_fill_style_str(&opts.theme.bg.with_alpha(0.94).to_css());
    ctx.fill_rect(x, y, w, h);
    ctx.set_stroke_style_str(&opts.theme.frozen_line.to_css());
    ctx.set_line_width(crisp_width(surface, 1.0));
    ctx.stroke_rect(crisp_for(surface, x, 1.0), crisp_for(surface, y, 1.0), w, h);
    ctx.set_fill_style_str(&opts.theme.text.to_css());
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(label, x + w / 2.0, y + h / 2.0);
    ctx.restore();
}

fn set_line_dash_from_vec(ctx: &web_sys::CanvasRenderingContext2d, dash: &[f64]) {
    let arr = js_sys::Array::new();
    for v in dash {
        arr.push(&wasm_bindgen::JsValue::from_f64(*v));
    }
    ctx.set_line_dash(&arr.into()).ok();
}

fn clear_line_dash(ctx: &web_sys::CanvasRenderingContext2d) {
    ctx.set_line_dash(&js_sys::Array::new().into()).ok();
}

fn draw_annotation(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    ann: &Annotation,
    _theme: &Theme,
    axis_font: &str,
) {
    let ctx = &surface.ctx;
    let plot = layout.plot;
    let color = css_color(ann.color);
    let (r, g, b) = rgb_to_components(ann.color);

    // Vertical time band [ts, ts2]
    if let (Some(ts), Some(ts2)) = (ann.ts, ann.ts2) {
        draw_time_band(
            surface,
            layout,
            ts.min(ts2),
            ts.max(ts2),
            color,
            (r, g, b),
            ann,
            axis_font,
        );
        return;
    }

    // Horizontal threshold / band
    if ann.ts.is_none() {
        let y0_px = layout.y_px(ann.y);
        // A log axis cannot place a non-positive threshold; drawing it at a
        // NaN row would smear the annotation across the plot.
        if !y0_px.is_finite() {
            return;
        }
        if let Some(y2) = ann.y2
            && layout.y_px(y2).is_finite()
        {
            let y1_px = layout.y_px(y2);
            let top = y0_px.min(y1_px);
            let h = (y1_px - y0_px).abs();
            ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.08)"));
            ctx.fill_rect(plot.x, top, plot.w, h);
        }

        ctx.set_stroke_style_str(&color);
        ctx.set_line_width(crisp_width(surface, 1.0));
        if let Some(dash) = &ann.dash {
            set_line_dash_from_vec(ctx, dash);
        } else {
            clear_line_dash(ctx);
        }

        ctx.begin_path();
        ctx.move_to(plot.x, crisp_for(surface, y0_px, 1.0));
        ctx.line_to(plot.x + plot.w, crisp_for(surface, y0_px, 1.0));
        ctx.stroke();
        clear_line_dash(ctx);

        if let Some(label) = &ann.label {
            ctx.set_font(axis_font);
            ctx.set_fill_style_str(&color);
            ctx.set_text_align("left");
            ctx.set_text_baseline("bottom");
            let _ = ctx.fill_text(label, plot.x + 4.0, y0_px - 2.0);
        }
    }

    // Vertical marker (single timestamp)
    if let Some(ts) = ann.ts {
        let x = layout.x_scale.to_px(ts);
        if x >= plot.x && x <= plot.x + plot.w {
            ctx.set_stroke_style_str(&color);
            ctx.set_line_width(crisp_width(surface, 1.0));
            if let Some(dash) = &ann.dash {
                set_line_dash_from_vec(ctx, dash);
            }
            ctx.begin_path();
            ctx.move_to(crisp_for(surface, x, 1.0), plot.y);
            ctx.line_to(crisp_for(surface, x, 1.0), plot.y + plot.h);
            ctx.stroke();
            clear_line_dash(ctx);

            if let Some(label) = &ann.label {
                draw_annotation_label(
                    ctx,
                    axis_font,
                    &color,
                    x,
                    plot.y - 2.0,
                    "center",
                    label,
                    ann.subtitle.as_deref(),
                );
            }
        }
    }
}

/// Draw the label (and optional dimmer subtitle line) near a vertical marker
/// or band, anchored at (x, y_baseline).
#[allow(clippy::too_many_arguments)] // A label needs its independent placement and text fields.
fn draw_annotation_label(
    ctx: &web_sys::CanvasRenderingContext2d,
    axis_font: &str,
    color: &str,
    x: f64,
    y_baseline: f64,
    align: &str,
    label: &str,
    subtitle: Option<&str>,
) {
    ctx.set_font(axis_font);
    ctx.set_fill_style_str(color);
    ctx.set_text_align(align);
    ctx.set_text_baseline("bottom");
    let _ = ctx.fill_text(label, x, y_baseline);
    if let Some(sub) = subtitle {
        ctx.set_fill_style_str("rgba(160,160,160,0.9)");
        ctx.set_text_align("left");
        let _ = ctx.fill_text(sub, x + 4.0, y_baseline + 11.0);
    }
}

/// Filled vertical band from `from` to `to` (epoch ms) across the plot height,
/// clipped to the visible window, with optional label at the top-left edge.
#[allow(clippy::too_many_arguments)] // Annotation data and resolved rendering geometry are distinct.
fn draw_time_band(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    from: f64,
    to: f64,
    color: String,
    (r, g, b): (u8, u8, u8),
    ann: &Annotation,
    axis_font: &str,
) {
    let ctx = &surface.ctx;
    let plot = layout.plot;
    let x0 = layout.x_scale.to_px(from).max(plot.x);
    let x1 = layout.x_scale.to_px(to).min(plot.x + plot.w);
    if x1 <= x0 {
        return;
    }

    ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.12)"));
    ctx.fill_rect(x0, plot.y, x1 - x0, plot.h);

    ctx.set_stroke_style_str(&color);
    ctx.set_line_width(crisp_width(surface, 1.0));
    for x in [crisp_for(surface, x0, 1.0), crisp_for(surface, x1, 1.0)] {
        ctx.begin_path();
        ctx.move_to(x, plot.y);
        ctx.line_to(x, plot.y + plot.h);
        ctx.stroke();
    }

    if let Some(label) = &ann.label {
        draw_annotation_label(
            ctx,
            axis_font,
            &color,
            x0,
            plot.y - 2.0,
            "left",
            label,
            ann.subtitle.as_deref(),
        );
    }
}

fn draw_bucket_band(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[SeriesData],
    ts: f64,
    fill: &str,
) {
    let Some((left, width)) = bucket_band(layout, series, ts) else {
        return;
    };
    let plot = layout.plot;
    let x0 = left.max(plot.x);
    let x1 = (left + width).min(plot.x + plot.w);
    if x1 <= x0 {
        return;
    }
    surface.ctx.set_fill_style_str(fill);
    surface.ctx.fill_rect(x0, plot.y, x1 - x0, plot.h);
}

fn bucket_band(layout: &ChartLayout, series: &[SeriesData], ts: f64) -> Option<(f64, f64)> {
    let xs = &series.iter().find(|s| s.xs.len() > 1)?.xs;
    let idx = crate::hit::nearest_index(xs, ts)?;
    let step = crate::series::sampled_median_gap(xs)?;
    let step_px =
        layout.x_scale.to_px(layout.x_scale.d0 + step) - layout.x_scale.to_px(layout.x_scale.d0);
    let center = layout.x_scale.to_px(xs[idx]);
    Some((center - step_px / 2.0, step_px))
}

// --- Statistical reference lines and trend overlays ---

/// Series-colored samples inside the visible x window, for statistics.
///
/// Only kinds whose y axis carries a comparable magnitude contribute: point
/// series, band centers, and OHLC closes. Row-indexed kinds (HBar, state
/// timelines) and bucket kinds (histograms) have no such axis, so they yield
/// nothing and the reference-line pass draws nothing for them.
fn reference_samples(layout: &ChartLayout, data: &ChartData) -> Vec<(u32, Vec<f64>)> {
    let visible = |xs: &[f64], values: &[f64]| -> Vec<f64> {
        let n = xs.len().min(values.len());
        let (start, end) = visible_point_bounds(&xs[..n], &layout.x_scale);
        xs[start..end]
            .iter()
            .zip(&values[start..end])
            .filter(|(x, y)| {
                x.is_finite()
                    && y.is_finite()
                    && **x >= layout.x_scale.d0
                    && **x <= layout.x_scale.d1
            })
            .map(|(_, y)| *y)
            .collect()
    };
    match data {
        ChartData::Lines(series)
        | ChartData::Areas(series)
        | ChartData::Bars(series)
        | ChartData::Points(series)
        | ChartData::Scatter(series)
        | ChartData::Step(series) => series
            .iter()
            .enumerate()
            .map(|(index, s)| (series_color(s, index), visible(&s.xs, &s.ys)))
            .collect(),
        ChartData::Band(series) => series
            .iter()
            .enumerate()
            .map(|(index, s)| {
                (
                    crate::series::band_series_color(s, index),
                    visible(&s.xs, &s.center),
                )
            })
            .collect(),
        ChartData::Ohlc(series) => series
            .iter()
            .enumerate()
            .map(|(index, s)| {
                let (start, end) = visible_tick_bounds(&s.ticks, &layout.x_scale);
                let values = s.ticks[start..end]
                    .iter()
                    .filter(|tick| {
                        tick.ts.is_finite()
                            && tick.close.is_finite()
                            && tick.ts >= layout.x_scale.d0
                            && tick.ts <= layout.x_scale.d1
                    })
                    .map(|tick| tick.close)
                    .collect();
                (crate::series::ohlc_series_color(s, index), values)
            })
            .collect(),
        _ => vec![],
    }
}

/// One reference line ready to draw.
struct ResolvedStat {
    label: String,
    value: f64,
    color: u32,
    /// Shaded extent for sigma bands, in data space.
    band: Option<(f64, f64)>,
}

/// Resolve the configured statistics against the visible samples, once per
/// frame. Both the band pass (behind the series) and the line pass (on top of
/// it) read the same result.
fn resolve_reference_stats(
    spec: &ChartSpec,
    samples: &[(u32, Vec<f64>)],
    fallback_color: u32,
) -> Vec<ResolvedStat> {
    let config = &spec.reference_lines;
    if config.is_empty() || samples.is_empty() {
        return vec![];
    }
    // Sorting is what a quantile costs; nothing else here needs it, and on a
    // dense panel that sort is the whole expense of the feature.
    let needs_sort = config.lines.iter().any(|line| {
        matches!(
            line,
            crate::spec::StatLine::Median | crate::spec::StatLine::Quantile(_)
        )
    });
    // Pooled is the default because the usual question is about the panel,
    // not about one of its ten series.
    let pooled: Vec<f64>;
    let groups: Vec<(u32, &[f64])> = if config.per_series {
        samples
            .iter()
            .map(|(color, values)| (*color, values.as_slice()))
            .collect()
    } else {
        pooled = samples
            .iter()
            .flat_map(|(_, v)| v.iter().copied())
            .collect();
        vec![(fallback_color, pooled.as_slice())]
    };
    let mut out = Vec::new();
    for (color, values) in groups {
        let Some(summary) = crate::stats::Summary::of_slice(values) else {
            continue;
        };
        // Sorted once per group, then shared by every quantile query.
        let sorted = if needs_sort {
            crate::stats::sorted_finite(values)
        } else {
            vec![]
        };
        for line in &config.lines {
            let Some(value) = line.resolve(&summary, &sorted) else {
                continue;
            };
            if !value.is_finite() {
                continue;
            }
            let band = match line {
                crate::spec::StatLine::Sigma(n) => {
                    let spread = n.abs() * summary.stddev;
                    (spread > 0.0).then_some((summary.mean - spread, summary.mean + spread))
                }
                _ => None,
            };
            out.push(ResolvedStat {
                label: line.label(),
                value,
                color,
                band,
            });
        }
    }
    // Largest first, so overlapping labels resolve downward predictably.
    out.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Shaded sigma bands, drawn *behind* the series so a wide band never hides
/// the data it describes.
fn draw_reference_bands(surface: &CanvasSurface, layout: &ChartLayout, stats: &[ResolvedStat]) {
    let ctx = &surface.ctx;
    let plot = layout.plot;
    for stat in stats {
        let Some((lo, hi)) = stat.band else { continue };
        // A log axis cannot draw a non-positive edge; collapse it onto the
        // baseline the same way bars and stacks do.
        let (top, bottom) = (layout.y_edge_px(hi), layout.y_edge_px(lo));
        if !top.is_finite() || !bottom.is_finite() {
            continue;
        }
        let top = top.clamp(plot.y, plot.y + plot.h);
        let bottom = bottom.clamp(plot.y, plot.y + plot.h);
        if (bottom - top).abs() < 0.5 {
            continue;
        }
        let (r, g, b) = rgb_to_components(stat.color);
        ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.07)"));
        ctx.fill_rect(plot.x, top.min(bottom), plot.w, (bottom - top).abs());
    }
}

/// Horizontal reference lines with right-edge labels, drawn on top of the
/// series (thin and dashed, so they read as annotation rather than data).
fn draw_reference_lines(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    spec: &ChartSpec,
    stats: &[ResolvedStat],
    opts: &RenderOptions,
) {
    if stats.is_empty() {
        return;
    }
    let ctx = &surface.ctx;
    let plot = layout.plot;
    ctx.save();
    ctx.set_font(&opts.axis_font);
    ctx.set_text_align("right");
    ctx.set_text_baseline("bottom");
    let mut label_rows: Vec<f64> = Vec::new();
    for stat in stats {
        let py = layout.y_px(stat.value);
        if !py.is_finite() || py < plot.y || py > plot.y + plot.h {
            continue;
        }
        let (r, g, b) = rgb_to_components(stat.color);
        ctx.set_stroke_style_str(&format!("rgba({r},{g},{b},0.55)"));
        ctx.set_line_width(crisp_width(surface, 1.0));
        set_line_dash_from_vec(ctx, &[6.0, 4.0]);
        ctx.begin_path();
        ctx.move_to(plot.x, crisp_for(surface, py, 1.0));
        ctx.line_to(plot.x + plot.w, crisp_for(surface, py, 1.0));
        ctx.stroke();
        clear_line_dash(ctx);
        if !spec.reference_lines.show_labels {
            continue;
        }
        // One label per row: two statistics that land on the same pixel would
        // otherwise overprint into an unreadable smear.
        if label_rows.iter().any(|row| (row - py).abs() < 11.0) {
            continue;
        }
        label_rows.push(py);
        let label = format!("{} {}", stat.label, spec.unit.format(stat.value));
        ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.95)"));
        let _ = ctx.fill_text(&label, plot.x + plot.w - 3.0, (py - 2.0).max(plot.y + 9.0));
    }
    ctx.restore();
}

/// The fitted y values for one series' visible window, paired with their x.
///
/// Linear fits return the two endpoints of the fitted line across the visible
/// domain; moving averages return one point per sample of a slice that is
/// deliberately wider than the viewport, so panning does not change the
/// smoothed values that stay on screen.
fn trend_points(
    layout: &ChartLayout,
    kind: crate::spec::TrendKind,
    xs: &[f64],
    ys: &[f64],
) -> (Vec<(f64, f64)>, Option<crate::stats::LinearFit>) {
    let n = xs.len().min(ys.len());
    let (start, end) = visible_point_bounds(&xs[..n], &layout.x_scale);
    if start >= end {
        return (vec![], None);
    }
    match kind {
        crate::spec::TrendKind::Linear => {
            let Some(fit) = crate::stats::linear_fit(&xs[start..end], &ys[start..end]) else {
                return (vec![], None);
            };
            let (x0, x1) = (layout.x_scale.d0, layout.x_scale.d1);
            (vec![(x0, fit.at(x0)), (x1, fit.at(x1))], Some(fit))
        }
        crate::spec::TrendKind::MovingAverage { window } => {
            // A centered window needs half a window of context on each side,
            // or the first and last on-screen points would be averaged over a
            // truncated window and shift as the viewport moves.
            let (start, end) = widen(start, end, window / 2, n);
            let smoothed = crate::stats::simple_moving_average(&ys[start..end], window);
            (xs[start..end].iter().copied().zip(smoothed).collect(), None)
        }
        crate::spec::TrendKind::Exponential { alpha } => {
            // An EWMA is causal, so it needs warm-up history rather than
            // symmetric context. Three time constants puts the seeding error
            // under 5% by the time the visible window starts.
            let warmup = if alpha.is_finite() && alpha > 0.0 {
                (3.0 / alpha).ceil().min(4096.0) as usize
            } else {
                0
            };
            let (start, end) = widen(start, end, warmup, n);
            let smoothed = crate::stats::exponential_moving_average(&ys[start..end], alpha);
            (xs[start..end].iter().copied().zip(smoothed).collect(), None)
        }
    }
}

/// Grow `start..end` by `pad` samples on each side, clamped to `0..len`.
fn widen(start: usize, end: usize, pad: usize, len: usize) -> (usize, usize) {
    (start.saturating_sub(pad), end.saturating_add(pad).min(len))
}

/// Stroke one fitted trend over a series.
fn stroke_trend(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    spec: &ChartSpec,
    color: u32,
    xs: &[f64],
    ys: &[f64],
    opts: &RenderOptions,
) {
    let Some(kind) = spec.trend.kind else { return };
    let (points, fit) = trend_points(layout, kind, xs, ys);
    if points.len() < 2 {
        return;
    }
    // Smoothed overlays inherit the chart's decimation budget: a per-sample
    // moving average over a dense series has as many points as the series.
    let decimated = if points.len() as f64 > layout.plot.w * 2.0 {
        let (tx, ty): (Vec<f64>, Vec<f64>) = points.iter().copied().unzip();
        decimate_series(
            &tx,
            &ty,
            &layout.x_scale,
            layout.plot.w,
            spec.decimation,
            layout.y_kind,
        )
    } else {
        points
    };
    let pixel_points: Vec<(f64, f64)> = decimated
        .iter()
        .map(|(x, y)| {
            let py = if y.is_finite() {
                layout.y_px(*y)
            } else {
                f64::NAN
            };
            (layout.x_scale.to_px(*x), py)
        })
        .collect();
    let ops = line_path(&pixel_points, false);
    if ops.is_empty() {
        return;
    }
    let ctx = &surface.ctx;
    ctx.save();
    ctx.begin_path();
    ctx.rect(layout.plot.x, layout.plot.y, layout.plot.w, layout.plot.h);
    ctx.clip();
    let (r, g, b) = rgb_to_components(color);
    let opacity = spec.trend.opacity.clamp(0.0, 1.0);
    ctx.set_stroke_style_str(&format!("rgba({r},{g},{b},{opacity})"));
    ctx.set_line_width(crisp_width(surface, spec.trend.width.max(0.5)));
    ctx.set_line_join("round");
    ctx.set_line_cap("round");
    if spec.trend.dashed {
        set_line_dash_from_vec(ctx, &[7.0, 5.0]);
    }
    ctx.begin_path();
    replay_path(ctx, &ops);
    ctx.stroke();
    clear_line_dash(ctx);
    ctx.restore();

    if spec.trend.show_fit_label
        && let Some(fit) = fit
        && let Some((_, last_py)) = pixel_points.last().copied()
        && last_py.is_finite()
    {
        let label = trend_fit_label(spec, &fit);
        let ctx = &surface.ctx;
        ctx.save();
        ctx.set_font(&opts.axis_font);
        ctx.set_text_align("right");
        ctx.set_text_baseline("bottom");
        ctx.set_fill_style_str(&format!("rgba({r},{g},{b},0.95)"));
        let y = last_py.clamp(layout.plot.y + 10.0, layout.plot.y + layout.plot.h - 2.0);
        let _ = ctx.fill_text(&label, layout.plot.x + layout.plot.w - 3.0, y - 2.0);
        ctx.restore();
    }
}

/// `"+1.2k/h  r² 0.93"` — the direction, its per-hour rate, and how much of
/// the variance the straight line actually explains.
///
/// Slope is reported per hour on a time axis (a per-millisecond rate is
/// unreadable) and per x-unit otherwise.
fn trend_fit_label(spec: &ChartSpec, fit: &crate::stats::LinearFit) -> String {
    let (rate, suffix) = match spec.x_axis {
        XAxisKind::Time => (fit.slope * 3_600_000.0, "/h"),
        _ => (fit.slope, ""),
    };
    let sign = if rate > 0.0 { "+" } else { "" };
    format!("{sign}{}{suffix}  r² {:.2}", spec.unit.format(rate), fit.r2)
}

/// Draw the configured trend overlay for every series that has one.
fn draw_trend_overlays(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    spec: &ChartSpec,
    data: &ChartData,
    opts: &RenderOptions,
) {
    if !spec.trend.is_enabled() {
        return;
    }
    match data {
        ChartData::Lines(series)
        | ChartData::Areas(series)
        | ChartData::Bars(series)
        | ChartData::Points(series)
        | ChartData::Scatter(series)
        | ChartData::Step(series) => {
            for (index, s) in series.iter().enumerate() {
                stroke_trend(
                    surface,
                    layout,
                    spec,
                    series_color(s, index),
                    &s.xs,
                    &s.ys,
                    opts,
                );
            }
        }
        ChartData::Band(series) => {
            for (index, s) in series.iter().enumerate() {
                stroke_trend(
                    surface,
                    layout,
                    spec,
                    crate::series::band_series_color(s, index),
                    &s.xs,
                    &s.center,
                    opts,
                );
            }
        }
        ChartData::Ohlc(series) => {
            for (index, s) in series.iter().enumerate() {
                let xs: Vec<f64> = s.ticks.iter().map(|tick| tick.ts).collect();
                let ys: Vec<f64> = s.ticks.iter().map(|tick| tick.close).collect();
                stroke_trend(
                    surface,
                    layout,
                    spec,
                    crate::series::ohlc_series_color(s, index),
                    &xs,
                    &ys,
                    opts,
                );
            }
        }
        // Bucket- and row-indexed kinds have no continuous x to fit against.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{HighlightConfig, ReferenceLines, StatLine, TrendKind};

    #[test]
    fn latest_values_come_from_the_visible_window_across_kinds() {
        let layout = ChartLayout::compute(400.0, 120.0, 0.0, 2.0, 0.0, 12.0);
        // 3.0 sits outside the 0..=2 window, so 2.0's value wins.
        let series = SeriesData {
            name: "a".into(),
            xs: vec![0.0, 1.0, 2.0, 3.0],
            ys: vec![1.0, 2.0, 5.0, 9.0],
            color: Some(0x445566),
        };
        assert_eq!(
            latest_visible_values(&layout, &ChartData::Lines(vec![series.clone()])),
            vec![(0x445566, 5.0)]
        );
        assert_eq!(
            latest_visible_values(&layout, &ChartData::Step(vec![series])),
            vec![(0x445566, 5.0)]
        );
        let band = crate::series::BandSeries {
            name: "p50".into(),
            xs: vec![0.0, 1.0, 2.0],
            center: vec![1.0, 2.0, 4.0],
            lower: vec![0.0, 1.0, 3.0],
            upper: vec![2.0, 3.0, 5.0],
            color: Some(0x778899),
        };
        assert_eq!(
            latest_visible_values(&layout, &ChartData::Band(vec![band])),
            vec![(0x778899, 4.0)]
        );
        // Row- and bucket-indexed kinds have no "latest" along x.
        assert!(latest_visible_values(&layout, &ChartData::HBar(vec![])).is_empty());
    }

    #[test]
    fn a_trailing_gap_does_not_erase_the_latest_value() {
        let layout = ChartLayout::compute(400.0, 120.0, 0.0, 3.0, 0.0, 12.0);
        let series = SeriesData {
            name: "a".into(),
            xs: vec![0.0, 1.0, 2.0, 3.0],
            ys: vec![1.0, 7.0, f64::NAN, f64::NAN],
            color: Some(0x010203),
        };
        assert_eq!(
            latest_visible_values(&layout, &ChartData::Lines(vec![series])),
            vec![(0x010203, 7.0)]
        );
    }

    #[test]
    fn hbar_value_labels_stay_inside_the_plot_so_the_gutter_is_not_widened() {
        // The reservation pass sizes the right gutter for right-edge labels
        // only; HBar prints inside its own bars.
        let hbar = ChartData::HBar(vec![HBarSeries {
            name: "cpu".into(),
            categories: vec!["a".into()],
            values: vec![42.0],
            color: None,
        }]);
        assert!(last_values_for_reservation(&hbar).is_empty());

        let lines = ChartData::Lines(vec![SeriesData {
            name: "a".into(),
            xs: vec![0.0, 1.0],
            ys: vec![1.0, f64::NAN],
            color: None,
        }]);
        assert_eq!(last_values_for_reservation(&lines), vec![1.0]);
    }

    fn ramp(name: &str, ys: Vec<f64>) -> SeriesData {
        SeriesData {
            name: name.into(),
            xs: (0..ys.len()).map(|i| i as f64).collect(),
            ys,
            color: Some(0x112233),
        }
    }

    #[test]
    fn stacked_columns_are_called_out_at_their_accumulated_top() {
        let layout = ChartLayout::compute(200.0, 120.0, 0.0, 3.0, 0.0, 12.0);
        let data = ChartData::Bars(vec![
            ramp("a", vec![1.0, 2.0, 3.0]),
            ramp("b", vec![1.0, 5.0, 1.0]),
        ]);
        let candidates = highlight_candidates(&layout, &data, SeriesLayout::Stacked, 8, false);
        // Column 1 stacks to 7, which is the value a reader sees at its top —
        // not either series' raw 2.0 or 5.0.
        let peak = candidates
            .iter()
            .max_by(|a, b| a.value.partial_cmp(&b.value).unwrap())
            .unwrap();
        assert_eq!(peak.value, 7.0);
        assert_eq!(peak.py, layout.y_px(7.0));
    }

    #[test]
    fn signed_stacks_call_out_both_baselines_separately() {
        let layout = ChartLayout::compute(200.0, 120.0, 0.0, 1.0, -8.0, 8.0);
        let data = ChartData::Bars(vec![
            ramp("up", vec![3.0, 2.0]),
            ramp("down", vec![-4.0, -1.0]),
        ]);
        let candidates = highlight_candidates(&layout, &data, SeriesLayout::Stacked, 8, false);
        assert!(candidates.iter().any(|c| c.value == 3.0));
        assert!(candidates.iter().any(|c| c.value == -4.0));
    }

    #[test]
    fn a_normalized_stack_declines_callouts_because_every_column_tops_at_100() {
        let layout = ChartLayout::compute(200.0, 120.0, 0.0, 2.0, 0.0, 100.0);
        let data = ChartData::Bars(vec![ramp("a", vec![1.0, 2.0, 3.0])]);
        assert!(
            highlight_candidates(&layout, &data, SeriesLayout::StackedPercent, 8, false).is_empty()
        );
    }

    #[test]
    fn highlight_modes_pick_peaks_troughs_extremes_and_latest() {
        let layout = ChartLayout::compute(400.0, 120.0, 0.0, 4.0, 0.0, 12.0);
        let data = ChartData::Lines(vec![ramp("a", vec![5.0, 10.0, 1.0, 6.0, 4.0])]);
        let with_mode = |mode: HighlightMode, top_n: usize| {
            let spec = ChartSpec {
                highlights: HighlightConfig {
                    mode,
                    top_n,
                    min_spacing_px: 0.0,
                    ..Default::default()
                },
                ..ChartSpec::line(Unit::None)
            };
            select_highlights(&layout, &spec, &data)
                .iter()
                .map(|c| c.value)
                .collect::<Vec<_>>()
        };
        assert_eq!(with_mode(HighlightMode::Max, 1), vec![10.0]);
        assert_eq!(with_mode(HighlightMode::Min, 1), vec![1.0]);
        // A single-callout budget still goes to the peak.
        assert_eq!(with_mode(HighlightMode::Extremes, 1), vec![10.0]);
        assert_eq!(with_mode(HighlightMode::Extremes, 2), vec![10.0, 1.0]);
        assert_eq!(with_mode(HighlightMode::Last, 1), vec![4.0]);
    }

    #[test]
    fn callout_spacing_collapses_a_cluster_of_neighboring_peaks() {
        let layout = ChartLayout::compute(400.0, 120.0, 0.0, 4.0, 0.0, 12.0);
        let data = ChartData::Lines(vec![ramp("a", vec![9.0, 10.0, 9.5, 1.0, 2.0])]);
        let spec = ChartSpec {
            highlights: HighlightConfig {
                top_n: 3,
                min_spacing_px: 200.0,
                ..Default::default()
            },
            ..ChartSpec::line(Unit::None)
        };
        let selected = select_highlights(&layout, &spec, &data);
        // The 9.0/10.0/9.5 cluster collapses to its single peak; the distant
        // trough is far enough away to still earn its own callout.
        let values: Vec<f64> = selected.iter().map(|c| c.value).collect();
        assert_eq!(values, vec![10.0, 2.0]);
    }

    #[test]
    fn reference_lines_pool_across_series_unless_asked_per_series() {
        let layout = ChartLayout::compute(400.0, 120.0, 0.0, 2.0, 0.0, 12.0);
        let data = ChartData::Lines(vec![
            ramp("a", vec![0.0, 0.0, 0.0]),
            ramp("b", vec![10.0, 10.0, 10.0]),
        ]);
        let pooled = ChartSpec {
            reference_lines: ReferenceLines {
                lines: vec![StatLine::Mean],
                per_series: false,
                show_labels: true,
            },
            ..ChartSpec::line(Unit::None)
        };
        let samples = reference_samples(&layout, &data);
        let stats = resolve_reference_stats(&pooled, &samples, 0);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].value, 5.0);

        let per_series = ChartSpec {
            reference_lines: ReferenceLines {
                per_series: true,
                ..pooled.reference_lines.clone()
            },
            ..pooled
        };
        let stats = resolve_reference_stats(&per_series, &samples, 0);
        // One mean per series, largest first so labels resolve downward.
        assert_eq!(
            stats.iter().map(|stat| stat.value).collect::<Vec<_>>(),
            vec![10.0, 0.0]
        );
    }

    #[test]
    fn reference_samples_are_limited_to_the_visible_window() {
        // Domain 1..=2 of a 0..=4 series: statistics must describe what is
        // on screen, not the whole dataset.
        let layout = ChartLayout::compute(400.0, 120.0, 1.0, 2.0, 0.0, 100.0);
        let data = ChartData::Lines(vec![ramp("a", vec![0.0, 10.0, 20.0, 90.0, 90.0])]);
        let samples = reference_samples(&layout, &data);
        assert_eq!(samples[0].1, vec![10.0, 20.0]);
    }

    #[test]
    fn a_sigma_line_carries_the_band_it_shades() {
        let layout = ChartLayout::compute(400.0, 120.0, 0.0, 3.0, 0.0, 12.0);
        let data = ChartData::Lines(vec![ramp("a", vec![2.0, 4.0, 4.0, 6.0])]);
        let spec = ChartSpec {
            reference_lines: ReferenceLines {
                lines: vec![StatLine::Sigma(1.0)],
                per_series: false,
                show_labels: true,
            },
            ..ChartSpec::line(Unit::None)
        };
        let samples = reference_samples(&layout, &data);
        let stats = resolve_reference_stats(&spec, &samples, 0);
        let (lo, hi) = stats[0].band.unwrap();
        // mean 4, population sigma sqrt(2).
        assert!((hi - (4.0 + 2.0f64.sqrt())).abs() < 1e-9);
        assert!((lo - (4.0 - 2.0f64.sqrt())).abs() < 1e-9);
        assert_eq!(stats[0].label, "±1σ");
    }

    #[test]
    fn a_linear_trend_spans_the_visible_domain_not_the_sampled_one() {
        let layout = ChartLayout::compute(400.0, 120.0, 1.0, 3.0, 0.0, 12.0);
        let data = ramp("a", vec![0.0, 2.0, 4.0, 6.0, 8.0]);
        let (points, fit) = trend_points(&layout, TrendKind::Linear, &data.xs, &data.ys);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].0, 1.0);
        assert_eq!(points[1].0, 3.0);
        let fit = fit.unwrap();
        assert!((fit.slope - 2.0).abs() < 1e-9);
        assert!((points[0].1 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_moving_average_trend_keeps_one_point_per_visible_sample() {
        let layout = ChartLayout::compute(400.0, 120.0, 0.0, 4.0, 0.0, 12.0);
        let data = ramp("a", vec![1.0, 5.0, 1.0, 5.0, 1.0]);
        let (points, fit) = trend_points(
            &layout,
            TrendKind::MovingAverage { window: 3 },
            &data.xs,
            &data.ys,
        );
        assert_eq!(points.len(), 5);
        assert!(fit.is_none());
        // The smoothed middle sits between the alternating extremes.
        assert!((points[2].1 - 11.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_moving_average_does_not_change_as_the_viewport_pans() {
        // The same sample must smooth to the same value whether it sits at
        // the edge of the window or well inside it.
        let data = ramp("a", (0..40).map(|i| (i % 5) as f64).collect());
        let wide = ChartLayout::compute(400.0, 120.0, 0.0, 39.0, 0.0, 12.0);
        let narrow = ChartLayout::compute(400.0, 120.0, 20.0, 30.0, 0.0, 12.0);
        let kind = TrendKind::MovingAverage { window: 5 };
        let value_at = |layout: &ChartLayout, x: f64| {
            let (points, _) = trend_points(layout, kind, &data.xs, &data.ys);
            points
                .iter()
                .find(|(px, _)| *px == x)
                .map(|(_, y)| *y)
                .unwrap()
        };
        assert_eq!(value_at(&wide, 25.0), value_at(&narrow, 25.0));
        // The first fully on-screen sample of the narrow view too.
        assert_eq!(value_at(&wide, 21.0), value_at(&narrow, 21.0));
    }

    #[test]
    fn a_trend_label_reports_a_time_axis_rate_per_hour() {
        let spec = ChartSpec::line(Unit::None);
        let fit = crate::stats::LinearFit {
            // +1 unit per second.
            slope: 1.0 / 1000.0,
            intercept: 0.0,
            r2: 0.9312,
            count: 10,
        };
        assert_eq!(trend_fit_label(&spec, &fit), "+3600/h  r² 0.93");

        let linear_axis = ChartSpec {
            x_axis: XAxisKind::Linear {
                min: None,
                max: None,
            },
            ..ChartSpec::line(Unit::None)
        };
        assert_eq!(trend_fit_label(&linear_axis, &fit), "+0  r² 0.93");
    }

    #[test]
    fn bucket_and_row_indexed_kinds_contribute_no_statistics() {
        let layout = ChartLayout::compute(400.0, 120.0, 0.0, 4.0, 0.0, 12.0);
        assert!(reference_samples(&layout, &ChartData::HBar(vec![])).is_empty());
        assert!(reference_samples(&layout, &ChartData::StateTimeline(vec![])).is_empty());
        assert!(reference_samples(&layout, &ChartData::Histogram(vec![])).is_empty());
    }

    #[test]
    fn fill_path_closes_each_finite_run_at_the_baseline() {
        let layout = ChartLayout::compute(100.0, 80.0, 0.0, 4.0, 0.0, 5.0);
        let ops = build_fill_path(
            &[
                (0.0, 1.0),
                (1.0, 2.0),
                (2.0, f64::NAN),
                (3.0, 3.0),
                (4.0, 4.0),
            ],
            &layout,
            71.0,
        );
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op, PathOp::MoveTo(..)))
                .count(),
            2
        );
        assert_eq!(
            ops.windows(2)
                .filter(|pair| {
                    matches!(pair, [PathOp::LineTo(_, y0), PathOp::LineTo(_, y1)] if *y0 == 71.0 && *y1 == 71.0)
                })
                .count(),
            2
        );
    }

    #[test]
    fn bounded_highlights_ignore_offscreen_maxima_before_selection() {
        let layout = ChartLayout::compute(100.0, 80.0, 0.0, 10.0, 0.0, 10.0);
        let mut pool = CandidatePool::new(1, false);
        pool.push(
            &layout,
            HighlightCandidate {
                value: 100.0,
                px: layout.plot.x - 1.0,
                py: layout.plot.y + 10.0,
                color: 0,
            },
        );
        pool.push(
            &layout,
            HighlightCandidate {
                value: 5.0,
                px: layout.plot.x + 10.0,
                py: layout.plot.y + 10.0,
                color: 0,
            },
        );
        let candidates = pool.into_vec();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].value, 5.0);
    }

    #[test]
    fn an_inverted_pool_retains_the_smallest_values() {
        let layout = ChartLayout::compute(100.0, 80.0, 0.0, 10.0, 0.0, 10.0);
        let mut pool = CandidatePool::new(1, true);
        for value in [5.0, 1.0, 9.0] {
            pool.push(
                &layout,
                HighlightCandidate {
                    value,
                    px: layout.plot.x + 10.0,
                    py: layout.plot.y + 10.0,
                    color: 0,
                },
            );
        }
        assert_eq!(pool.into_vec()[0].value, 1.0);
    }

    #[test]
    fn highlight_bounds_use_visible_sorted_window_and_fallback_for_unsorted_xs() {
        let layout = ChartLayout::compute(100.0, 80.0, 20.0, 30.0, 0.0, 1.0);
        let xs: Vec<f64> = (0..=100).map(f64::from).collect();
        assert_eq!(visible_point_bounds(&xs, &layout.x_scale), (19, 32));
        assert_eq!(
            visible_point_bounds(&[0.0, 10.0, 5.0], &layout.x_scale),
            (0, 3)
        );
    }

    #[test]
    fn stacked_histogram_highlights_keep_raw_values_at_stack_tops() {
        let layout = ChartLayout::compute(160.0, 100.0, 0.0, 1.0, -10.0, 10.0);
        let data = ChartData::Histogram(vec![
            HistogramSeries {
                name: "positive-a".into(),
                buckets: vec![0.0, 1.0],
                counts: vec![2.0],
                color: Some(0x111111),
                cumulative: false,
            },
            HistogramSeries {
                name: "positive-b".into(),
                buckets: vec![0.0, 1.0],
                counts: vec![3.0],
                color: Some(0x222222),
                cumulative: false,
            },
            HistogramSeries {
                name: "negative".into(),
                buckets: vec![0.0, 1.0],
                counts: vec![-4.0],
                color: Some(0x333333),
                cumulative: false,
            },
        ]);
        let candidates = highlight_candidates(&layout, &data, SeriesLayout::Stacked, 8, false);
        let positive_b = candidates
            .iter()
            .find(|candidate| candidate.value == 3.0)
            .unwrap();
        let negative = candidates
            .iter()
            .find(|candidate| candidate.value == -4.0)
            .unwrap();
        assert_eq!(positive_b.py, layout.y_px(5.0));
        assert_eq!(negative.py, layout.y_px(-4.0));
    }

    #[test]
    fn row_oriented_charts_use_row_labels_not_numeric_y_ticks_or_badges() {
        assert!(!uses_numeric_y_ticks(&ChartData::Heatmap(vec![])));
        assert!(!uses_numeric_y_ticks(&ChartData::HBar(vec![])));
        assert!(!uses_numeric_y_ticks(&ChartData::StateTimeline(vec![])));

        let cursor = CursorOverlay::at(Some(42.0), None, Unit::Short);
        let info = HoverInfo {
            ts: 42.0,
            header: Some("database".into()),
            values: vec![crate::hit::HoverValue::new(
                0,
                "requests".into(),
                42.0,
                0.0,
                0.0,
            )],
        };
        assert_eq!(
            overlay_y_badge_label(&ChartData::HBar(vec![]), &cursor, &info, &info.values[0]),
            "database"
        );
        assert_eq!(
            overlay_x_badge_label(&ChartData::HBar(vec![]), &cursor, Some(&info)),
            Some(cursor.unit.format(42.0))
        );
        assert_eq!(
            overlay_y_badge_label(&ChartData::Heatmap(vec![]), &cursor, &info, &info.values[0]),
            "requests"
        );
    }

    #[test]
    fn ohlc_draw_predicate_rejects_nonfinite_and_inverted_ticks() {
        let valid = crate::series::OhlcTick {
            ts: 1.0,
            open: 11.0,
            high: 12.0,
            low: 10.0,
            close: 10.5,
        };
        assert!(is_drawable_ohlc_tick(&valid));
        assert!(!is_drawable_ohlc_tick(&crate::series::OhlcTick {
            high: 9.0,
            low: 10.0,
            ..valid
        }));
        assert!(!is_drawable_ohlc_tick(&crate::series::OhlcTick {
            close: f64::NAN,
            ..valid
        }));
    }

    #[test]
    fn legend_summary_reports_min_max_mean_over_finite_values() {
        let stats = summarize(
            [2.0, f64::NAN, 4.0, 12.0].into_iter(),
            &[
                LegendStat::Min,
                LegendStat::Max,
                LegendStat::Mean,
                LegendStat::Total,
            ],
            Unit::None,
        );
        assert_eq!(stats, vec!["min 2", "max 12", "mean 6", "total 18"]);
    }

    #[test]
    fn legend_summary_is_empty_without_finite_values_or_stats() {
        assert!(summarize([1.0].into_iter(), &[], Unit::None).is_empty());
        assert!(
            summarize(
                [f64::NAN, f64::INFINITY].into_iter(),
                &[LegendStat::Mean],
                Unit::None
            )
            .is_empty()
        );
    }

    #[test]
    fn legend_label_appends_statistics_to_the_configured_head() {
        let entry = CanvasLegendEntry {
            name: "p99".into(),
            color: 0,
            value: Some("12ms".into()),
            stats: vec!["min 2ms".into(), "max 30ms".into()],
        };
        assert_eq!(
            legend_label(&entry, LegendFormat::NameAndValue),
            "p99: 12ms  (min 2ms  max 30ms)"
        );
        assert_eq!(
            legend_label(&entry, LegendFormat::NameOnly),
            "p99  (min 2ms  max 30ms)"
        );

        let bare = CanvasLegendEntry {
            stats: vec![],
            ..entry
        };
        assert_eq!(legend_label(&bare, LegendFormat::NameAndValue), "p99: 12ms");
    }

    #[test]
    fn axis_title_bands_are_reserved_only_when_titled_and_chromed() {
        let titled = ChartSpec::line(Unit::None).with_axis_labels(Some("time"), Some("requests/s"));
        assert_eq!(y_title_band(&titled), AXIS_TITLE_BAND);
        assert_eq!(x_title_band(&titled), AXIS_TITLE_BAND);

        let untitled = ChartSpec::line(Unit::None);
        assert_eq!(y_title_band(&untitled), 0.0);
        assert_eq!(x_title_band(&untitled), 0.0);

        // A sparkline has no room for chrome, so titles reserve nothing.
        let sparkline = ChartSpec::sparkline().with_axis_labels(Some("t"), Some("v"));
        assert_eq!(y_title_band(&sparkline), 0.0);
        assert_eq!(x_title_band(&sparkline), 0.0);
    }

    #[test]
    fn dense_band_reduction_is_sorted_and_keeps_center_extrema() {
        let layout = ChartLayout::compute(80.0, 100.0, 0.0, 1_000.0, 0.0, 20.0);
        let mut center = vec![1.0; 1_001];
        center[500] = 19.0;
        center[600] = f64::NAN;
        let mut lower = vec![0.0; 1_001];
        lower[450] = 10.0;
        let mut upper = vec![20.0; 1_001];
        upper[700] = 0.0;
        let band = crate::series::BandSeries {
            name: "band".into(),
            xs: (0..=1_000).map(|i| i as f64).collect(),
            center,
            lower,
            upper,
            color: None,
        };
        let indices = visible_band_indices(&band, &layout);
        assert!(indices.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(indices.contains(&500));
        assert!(indices.contains(&450));
        assert!(indices.contains(&600));
        assert!(indices.contains(&700));
    }

    #[test]
    fn dense_stack_reduction_keeps_composed_boundary_extrema() {
        let layout = ChartLayout::compute(80.0, 100.0, 0.0, 1_000.0, 0.0, 20.0);
        let xs: Vec<f64> = (0..=1_000).map(|i| i as f64).collect();
        let mut first = vec![1.0; 1_001];
        let mut second = vec![9.0; 1_001];
        first[500] = 9.0;
        second[500] = 1.0;
        first[600] = f64::NAN;
        let series = vec![
            SeriesData {
                name: "a".into(),
                xs: xs.clone(),
                ys: first,
                color: None,
            },
            SeriesData {
                name: "b".into(),
                xs,
                ys: second,
                color: None,
            },
        ];
        let indices = visible_stack_indices(&series[0].xs, &series, &layout, false, false);
        assert!(indices.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(indices.contains(&500));
        assert!(indices.contains(&600));
    }
}
