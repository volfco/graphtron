//! Classic RRDtool presentation, implemented in Canvas2D, not an RRD parser.
//! Reference: upstream rrd_graph.c (graph_col, axis_paint, text_prop).
use crate::*;

pub const AXIS_FONT: &str = "9px 'DejaVu Sans Mono', 'Liberation Mono', monospace";
pub const LEGEND_FONT: &str = "10px 'DejaVu Sans Mono', 'Liberation Mono', monospace";

#[derive(Debug, Clone, PartialEq)]
pub struct RrdOptions {
    pub title: String,
    pub show_legend: bool,
    /// AREA charts may include LINE overlays, e.g. outbound network traffic.
    pub line_series: Vec<usize>,
    /// RRDtool defaults to staircase traces. Explicitly opt into slope mode.
    pub slope_mode: bool,
    /// Small vertical provenance mark; deliberately does not claim RRDtool output.
    pub watermark: Option<String>,
}

impl Default for RrdOptions {
    fn default() -> Self {
        Self {
            title: String::new(),
            show_legend: true,
            line_series: vec![],
            slope_mode: false,
            watermark: Some("GRAPHTRON / RRD STYLE".into()),
        }
    }
}

pub fn supports(data: &ChartData) -> bool {
    matches!(
        data,
        ChartData::Lines(_) | ChartData::Areas(_) | ChartData::Step(_)
    )
}

/// Decimal SI notation with fixed precision, matching GPRINT-style columns.
pub fn number(value: f64) -> String {
    if !value.is_finite() {
        return "nan".into();
    }
    let (scale, suffix) = if value.abs() >= 1e9 {
        (1e9, "G")
    } else if value.abs() >= 1e6 {
        (1e6, "M")
    } else if value.abs() >= 1e3 {
        (1e3, "k")
    } else {
        (1.0, "")
    };
    format!("{:.2}{suffix}", value / scale)
}

fn stacked(data: &ChartData, spec: &ChartSpec) -> bool {
    matches!(data, ChartData::Areas(_)) && spec.layout != SeriesLayout::Grouped
}

/// Raw contribution, baseline and stack top at a source sample. Missing
/// values are gaps; opposite signs stack on opposite sides of zero.
fn values(
    data: &ChartData,
    spec: &ChartSpec,
    style: &RrdOptions,
    si: usize,
    index: usize,
) -> Option<(f64, f64, f64)> {
    let series = data.point_series();
    let s = series.get(si)?;
    let raw = *s.ys.get(index)?;
    let x = *s.xs.get(index)?;
    if !raw.is_finite() || (spec.y_scale == ScaleKind::Log && raw <= 0.0) {
        return None;
    }
    if !stacked(data, spec) || style.line_series.contains(&si) {
        return Some((raw, 0.0, raw));
    }
    let mut base = 0.0;
    let mut total = 0.0;
    for (j, other) in series.iter().enumerate() {
        if style.line_series.contains(&j) || other.xs.get(index) != Some(&x) {
            continue;
        }
        if let Some(v) = other
            .ys
            .get(index)
            .filter(|v| v.is_finite() && (**v >= 0.0) == (raw >= 0.0))
        {
            total = crate::series::finite_add(total, v.abs());
            if j < si {
                base = crate::series::finite_add(base, *v);
            }
        }
    }
    let factor = if spec.layout == SeriesLayout::StackedPercent && total > 0.0 {
        100.0 / total
    } else {
        1.0
    };
    Some((
        raw,
        base * factor,
        crate::series::finite_add(base * factor, raw * factor),
    ))
}

/// Staircase-aware hover for this presentation. No data cloning or full scan.
pub fn hit_test(
    layout: &ChartLayout,
    spec: &ChartSpec,
    data: &ChartData,
    style: &RrdOptions,
    x: f64,
) -> Option<HoverInfo> {
    if !supports(data) {
        return None;
    }
    let ts = layout.ts_at(x);
    let mut rows = vec![];
    for (si, series) in data.point_series().iter().enumerate() {
        let n = series.xs.len().min(series.ys.len());
        let xs = &series.xs[..n];
        if xs.last().is_none_or(|last| ts > *last) {
            continue;
        }
        let Some(i) = xs.partition_point(|t| *t <= ts).checked_sub(1) else {
            continue;
        };
        let Some((mut raw, _, mut top)) = values(data, spec, style, si, i) else {
            continue;
        };
        if style.slope_mode
            && let Some(&next_x) = xs.get(i + 1)
            && next_x > xs[i]
        {
            let Some((next_raw, _, next_top)) = values(data, spec, style, si, i + 1) else {
                continue;
            };
            let t = (ts - xs[i]) / (next_x - xs[i]);
            raw += (next_raw - raw) * t;
            // Interpolate in plotted space for a logarithmic y axis.
            top = layout.y_at(layout.y_px(top) + (layout.y_px(next_top) - layout.y_px(top)) * t);
            if !stacked(data, spec) || style.line_series.contains(&si) {
                raw = top;
            }
        }
        let py = layout.y_px(top);
        if layout.plot.contains(x, py) {
            rows.push(
                HoverValue::new(si, series.name.clone(), raw, x, py)
                    .with_color(series_color(series, si)),
            );
        }
    }
    (!rows.is_empty()).then(|| HoverInfo {
        ts,
        header: Some(format_ts_full(ts as i64)),
        values: rows,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    surface: &CanvasSurface,
    spec: &ChartSpec,
    data: &ChartData,
    from: f64,
    to: f64,
    opts: &RenderOptions,
    style: &RrdOptions,
) -> ChartLayout {
    let (from, to) = resolve_x_domain(&spec.x_axis, data, from, to);
    let (lo, hi) = domain(spec, data, style, from, to);
    let series = data.point_series();
    let legend_rows = if style.show_legend {
        series.len().min(8) + 1
    } else {
        0
    };
    let layout = ChartLayout::compute_scaled(
        surface.css_w,
        surface.css_h,
        from,
        to,
        lo,
        hi,
        68.0,
        26.0,
        if style.title.is_empty() { 16.0 } else { 34.0 },
        30.0 + legend_rows as f64 * 14.0,
        spec.y_scale,
    );
    let ctx = &surface.ctx;
    surface.clear();
    ctx.save();
    ctx.set_shadow_blur(0.0);
    ctx.set_line_dash(&js_sys::Array::new()).ok();
    ctx.set_fill_style_str(&opts.theme.bg.to_css());
    ctx.fill_rect(0.0, 0.0, surface.css_w, surface.css_h);
    // Classic two-pixel bevel around the entire generated image.
    ctx.set_fill_style_str(if opts.theme.dark {
        "#cccccc"
    } else {
        "#cfcfcf"
    });
    ctx.fill_rect(0.0, 0.0, surface.css_w, 2.0);
    ctx.fill_rect(0.0, 0.0, 2.0, surface.css_h);
    ctx.set_fill_style_str(if opts.theme.dark {
        "#777777"
    } else {
        "#9e9e9e"
    });
    ctx.fill_rect(0.0, surface.css_h - 2.0, surface.css_w, 2.0);
    ctx.fill_rect(surface.css_w - 2.0, 0.0, 2.0, surface.css_h);
    let p = layout.plot;
    ctx.set_fill_style_str(if opts.theme.dark {
        "#000000"
    } else {
        "#ffffff"
    });
    ctx.fill_rect(p.x, p.y, p.w, p.h);
    ctx.save();
    ctx.begin_path();
    ctx.rect(p.x, p.y, p.w, p.h);
    ctx.clip();
    for (si, s) in series.iter().enumerate() {
        paint_series(surface, &layout, spec, data, style, si, s);
    }
    ctx.restore();
    grid(surface, &layout, spec, opts);
    ctx.set_fill_style_str(&opts.theme.text.to_css());
    ctx.set_font("bold 12px 'DejaVu Sans Mono', 'Liberation Mono', monospace");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(&style.title, surface.css_w / 2.0, 17.0);
    if let Some(label) = &spec.y_label {
        ctx.save();
        ctx.translate(15.0, p.y + p.h / 2.0).ok();
        ctx.rotate(-std::f64::consts::FRAC_PI_2).ok();
        ctx.set_font(LEGEND_FONT);
        let _ = ctx.fill_text(label, 0.0, 0.0);
        ctx.restore();
    }
    if let Some(mark) = &style.watermark {
        ctx.save();
        ctx.translate(surface.css_w - 10.0, p.y + p.h / 2.0).ok();
        ctx.rotate(-std::f64::consts::FRAC_PI_2).ok();
        ctx.set_font("7px monospace");
        ctx.set_fill_style_str("#777777");
        let _ = ctx.fill_text(mark, 0.0, 0.0);
        ctx.restore();
    }
    if style.show_legend {
        legend(surface, &layout, series, spec.unit, opts);
    }
    if data.is_empty() {
        ctx.set_font(LEGEND_FONT);
        let _ = ctx.fill_text("No data", p.x + p.w / 2.0, p.y + p.h / 2.0);
    }
    ctx.restore();
    layout
}

/// Include the sample owning the left edge of a staircase viewport; a zoom
/// strictly between observations must not autofit to an imaginary slope.
fn domain(
    spec: &ChartSpec,
    data: &ChartData,
    style: &RrdOptions,
    from: f64,
    to: f64,
) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for (si, s) in data.point_series().iter().enumerate() {
        let n = s.xs.len().min(s.ys.len());
        let start = s.xs[..n].partition_point(|x| *x < from).saturating_sub(1);
        let end = s.xs[..n]
            .partition_point(|x| *x <= to)
            .saturating_add(usize::from(style.slope_mode))
            .min(n);
        for i in start.min(end)..end {
            if let Some((_, base, top)) = values(data, spec, style, si, i) {
                let bottom =
                    if matches!(data, ChartData::Areas(_)) && !style.line_series.contains(&si) {
                        base
                    } else {
                        top
                    };
                for value in [bottom, top] {
                    if value.is_finite() && (spec.y_scale != ScaleKind::Log || value > 0.0) {
                        lo = lo.min(value);
                        hi = hi.max(value);
                    }
                }
            }
        }
    }
    if spec.y_scale == ScaleKind::Linear {
        crate::layout::finish_domain(
            lo,
            hi,
            spec.y_min,
            spec.y_max,
            spec.zero_anchored,
            lo >= 0.0,
        )
    } else {
        if lo.is_finite() && hi.is_finite() {
            lo /= 10f64.powf(0.2);
            hi *= 10f64.powf(0.2);
        } else {
            lo = 1.0;
            hi = 10.0;
        }
        lo = spec
            .y_min
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(lo);
        hi = spec
            .y_max
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(hi);
        if lo < hi { (lo, hi) } else { (hi / 10.0, hi) }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_series(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    spec: &ChartSpec,
    data: &ChartData,
    style: &RrdOptions,
    si: usize,
    s: &SeriesData,
) {
    let n = s.xs.len().min(s.ys.len());
    let start = s.xs[..n]
        .partition_point(|x| *x < layout.x_scale.d0)
        .saturating_sub(1);
    let end = s.xs[..n]
        .partition_point(|x| *x <= layout.x_scale.d1)
        .saturating_add(1)
        .min(n);
    let xs = &s.xs[start.min(end)..end];
    let vals: Vec<_> = (start.min(end)..end)
        .map(|i| values(data, spec, style, si, i))
        .collect();
    let tops: Vec<_> = vals
        .iter()
        .map(|v| v.map_or(f64::NAN, |(_, _, top)| top))
        .collect();
    let bases: Vec<_> = vals
        .iter()
        .map(|v| v.map_or(f64::NAN, |(_, base, _)| base))
        .collect();
    // Preserve extrema of both edges, reducing canvas work to plot resolution.
    let mut selected = crate::decimate::min_max(xs, &tops, &layout.x_scale, layout.plot.w);
    selected.extend(crate::decimate::min_max(
        xs,
        &bases,
        &layout.x_scale,
        layout.plot.w,
    ));
    let mut indices: Vec<_> = selected
        .iter()
        .filter_map(|(x, _)| xs.binary_search_by(|v| v.total_cmp(x)).ok())
        .collect();
    indices.sort_unstable();
    indices.dedup();
    let ctx = &surface.ctx;
    let color = css_color(series_color(s, si));
    ctx.set_fill_style_str(&color);
    ctx.set_stroke_style_str(&color);
    ctx.set_line_width(1.0);
    ctx.set_line_join("miter");
    ctx.set_line_cap("butt");
    let fill = matches!(data, ChartData::Areas(_)) && !style.line_series.contains(&si);
    ctx.begin_path();
    let mut previous: Option<(f64, f64, f64)> = None;
    let mut drawable = 0usize;
    let mut singleton = None;
    for i in indices {
        let Some((_, base, top)) = vals[i] else {
            // A step sample owns the interval up to the next timestamp,
            // including when that next sample is missing.
            if !style.slope_mode
                && let Some((px, py, pb)) = previous
            {
                let x = layout.x_scale.to_px(xs[i]);
                if x.is_finite() {
                    ctx.move_to(px, if fill { pb } else { py });
                    if fill {
                        ctx.line_to(px, py);
                    }
                    ctx.line_to(x, py);
                    if fill {
                        ctx.line_to(x, pb);
                        ctx.close_path();
                    }
                }
            }
            previous = None;
            continue;
        };
        let (x, y, bottom) = (
            layout.x_scale.to_px(xs[i]),
            layout.y_px(top),
            layout.y_edge_px(base),
        );
        if !x.is_finite() || !y.is_finite() || !bottom.is_finite() {
            previous = None;
            continue;
        }
        drawable += 1;
        singleton = Some((x, y));
        if let Some((px, py, pb)) = previous {
            if fill {
                ctx.move_to(px, pb);
                ctx.line_to(px, py);
                ctx.line_to(x, if style.slope_mode { y } else { py });
                ctx.line_to(x, if style.slope_mode { bottom } else { pb });
                ctx.close_path();
            } else {
                ctx.move_to(px, py);
                if !style.slope_mode {
                    ctx.line_to(x, py);
                }
                ctx.line_to(x, y);
            }
        }
        previous = Some((x, y, bottom));
    }
    if drawable == 1
        && let Some((x, y)) = singleton
    {
        ctx.begin_path();
        let _ = ctx.arc(x, y, 1.5, 0.0, std::f64::consts::TAU);
        ctx.fill();
        return;
    }
    if fill {
        ctx.fill();
    } else {
        ctx.stroke();
    }
}

fn line(ctx: &web_sys::CanvasRenderingContext2d, a: (f64, f64), b: (f64, f64)) {
    ctx.begin_path();
    ctx.move_to(a.0, a.1);
    ctx.line_to(b.0, b.1);
    ctx.stroke();
}

fn time_labels(from: f64, to: f64, width: f64) -> Vec<(f64, String)> {
    let span = to - from;
    if !span.is_finite() || span <= 0.0 {
        return vec![];
    }
    if span > 7.0 * 86_400_000.0 {
        return crate::ticks::time_ticks(
            from as i64,
            to as i64,
            (width / 85.0).clamp(2.0, 32.0) as usize,
        )
        .into_iter()
        .map(|t| (t.ms as f64, t.label))
        .collect();
    }
    let desired = span / (width / 75.0).clamp(2.0, 32.0);
    let step = [
        1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0, 7200.0, 10800.0,
        21600.0, 43200.0, 86400.0, 172800.0,
    ]
    .into_iter()
    .map(|s| s * 1000.0)
    .find(|s| *s >= desired)
    .unwrap_or(172800000.0);
    let first = (from / step).ceil() * step;
    (0..=32)
        .map(|i| first + i as f64 * step)
        .take_while(|t| *t <= to)
        .map(|t| {
            let full = format_ts_full(t as i64);
            let label = if span <= 3_600_000.0 {
                full[6..14].to_string()
            } else if span <= 2.0 * 86_400_000.0 {
                full[6..11].to_string()
            } else {
                full[..11].to_string()
            };
            (t, label)
        })
        .collect()
}

fn grid(surface: &CanvasSurface, layout: &ChartLayout, spec: &ChartSpec, opts: &RenderOptions) {
    let ctx = &surface.ctx;
    let p = layout.plot;
    let ys = if spec.y_scale == ScaleKind::Log {
        log_ticks(layout.y_domain().0, layout.y_domain().1, 6)
    } else {
        linear_ticks(layout.y_domain().0, layout.y_domain().1, 5)
    };
    let xs: Vec<_> = match &spec.x_axis {
        XAxisKind::Time => time_labels(layout.x_scale.d0, layout.x_scale.d1, p.w),
        _ => linear_ticks(layout.x_scale.d0, layout.x_scale.d1, 6)
            .into_iter()
            .map(|x| (x, number(x)))
            .collect(),
    };
    let dash = js_sys::Array::new();
    dash.push(&1.0.into());
    dash.push(&1.0.into());
    ctx.set_line_dash(&dash).ok();
    ctx.set_line_width(0.5);
    ctx.set_stroke_style_str(&opts.theme.grid.to_css());
    // Four subdivisions between labeled major ticks, all bounded by tick budget.
    for pair in ys.windows(2) {
        for j in 1..5 {
            let y = layout.y_px(pair[0])
                + (layout.y_px(pair[1]) - layout.y_px(pair[0])) * j as f64 / 5.0;
            line(ctx, (p.x, y), (p.x + p.w, y));
        }
    }
    for pair in xs.windows(2) {
        for j in 1..4 {
            let x = layout
                .x_scale
                .to_px(pair[0].0 + (pair[1].0 - pair[0].0) * j as f64 / 4.0);
            line(ctx, (x, p.y), (x, p.y + p.h));
        }
    }
    ctx.set_stroke_style_str(if opts.theme.dark {
        "#663333"
    } else {
        "rgba(222,79,79,0.6)"
    });
    ctx.set_line_width(0.8);
    ctx.set_fill_style_str(&opts.theme.text.to_css());
    ctx.set_font(&opts.axis_font);
    ctx.set_text_align("right");
    ctx.set_text_baseline("middle");
    for y in ys {
        let py = layout.y_px(y);
        line(ctx, (p.x - 2.0, py), (p.x + p.w, py));
        let value = if spec.unit == Unit::Percent01 {
            y * 100.0
        } else {
            y
        };
        let _ = ctx.fill_text(&number(value), p.x - 7.0, py);
    }
    ctx.set_text_align("center");
    ctx.set_text_baseline("top");
    for (x, text) in xs {
        let px = layout.x_scale.to_px(x);
        line(ctx, (px, p.y), (px, p.y + p.h + 3.0));
        let half = ctx.measure_text(&text).map_or(0.0, |m| m.width() / 2.0);
        let tx = if surface.css_w >= 2.0 * (half + 3.0) {
            px.clamp(half + 3.0, surface.css_w - half - 3.0)
        } else {
            surface.css_w / 2.0
        };
        let _ = ctx.fill_text(&text, tx, p.y + p.h + 5.0);
    }
    ctx.set_line_dash(&js_sys::Array::new()).ok();
    ctx.set_line_width(1.0);
    ctx.set_stroke_style_str(if opts.theme.dark {
        "#555555"
    } else {
        "#1f1f1f"
    });
    line(ctx, (p.x - 4.0, p.y + p.h), (p.x + p.w + 4.0, p.y + p.h));
    line(ctx, (p.x, p.y + p.h + 4.0), (p.x, p.y - 4.0));
    ctx.set_fill_style_str("#801f1f");
    for points in [
        [
            (p.x - 3.0, p.y - 2.0),
            (p.x + 3.0, p.y - 2.0),
            (p.x, p.y - 7.0),
        ],
        [
            (p.x + p.w + 2.0, p.y + p.h - 3.0),
            (p.x + p.w + 2.0, p.y + p.h + 3.0),
            (p.x + p.w + 7.0, p.y + p.h),
        ],
    ] {
        ctx.begin_path();
        ctx.move_to(points[0].0, points[0].1);
        ctx.line_to(points[1].0, points[1].1);
        ctx.line_to(points[2].0, points[2].1);
        ctx.close_path();
        ctx.fill();
    }
}

fn legend(
    surface: &CanvasSurface,
    layout: &ChartLayout,
    series: &[SeriesData],
    unit: Unit,
    opts: &RenderOptions,
) {
    let ctx = &surface.ctx;
    let p = layout.plot;
    let y = p.y + p.h + 29.0;
    ctx.set_font(LEGEND_FONT);
    ctx.set_fill_style_str(&opts.theme.text.to_css());
    ctx.set_text_baseline("middle");
    let cols = [
        p.x + p.w * 0.43,
        p.x + p.w * 0.62,
        p.x + p.w * 0.81,
        p.x + p.w,
    ];
    ctx.set_text_align("right");
    for (x, label) in cols
        .iter()
        .zip(["Current", "Average", "Maximum", "Minimum"])
    {
        let _ = ctx.fill_text(label, *x, y);
    }
    for (i, s) in series.iter().take(8).enumerate() {
        let n = s.xs.len().min(s.ys.len());
        let start = s.xs[..n].partition_point(|x| *x < layout.x_scale.d0);
        let end = s.xs[..n].partition_point(|x| *x <= layout.x_scale.d1);
        let ys = &s.ys[start.min(end)..end];
        let summary = Summary::of_slice(ys);
        let current = ys.last().copied().unwrap_or(f64::NAN);
        let stats = [
            current,
            summary.as_ref().map_or(f64::NAN, |s| s.mean),
            summary.as_ref().map_or(f64::NAN, |s| s.max),
            summary.as_ref().map_or(f64::NAN, |s| s.min),
        ];
        let row_y = y + 14.0 * (i + 1) as f64;
        ctx.set_fill_style_str(&css_color(series_color(s, i)));
        ctx.fill_rect(p.x, row_y - 4.0, 9.0, 9.0);
        ctx.set_stroke_style_str("#000000");
        ctx.set_line_width(0.5);
        ctx.stroke_rect(p.x, row_y - 4.0, 9.0, 9.0);
        ctx.set_fill_style_str(&opts.theme.text.to_css());
        ctx.set_text_align("left");
        let _ = ctx.fill_text(&s.name, p.x + 14.0, row_y);
        ctx.set_text_align("right");
        for (x, value) in cols.iter().zip(stats) {
            let value = if unit == Unit::Percent01 {
                value * 100.0
            } else {
                value
            };
            let _ = ctx.fill_text(&number(value), *x, row_y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dark_preset_preserves_classic_geometry_and_flat_effects() {
        let light = RenderOptions::rrdtool();
        let dark = RenderOptions::rrdtool_dark();
        assert!(dark.theme.dark);
        assert_eq!(dark.theme.bg, Rgba::hex(0));
        assert_eq!(dark.theme.text, Rgba::hex(0xffffff));
        assert_eq!(dark.theme.tooltip_radius, 0.0);
        assert_eq!(light.rrdtool, dark.rrdtool);
        assert_eq!(light.aesthetic, dark.aesthetic);
        assert_eq!(light.axis_font, dark.axis_font);
    }
    fn s(ys: Vec<f64>) -> SeriesData {
        SeriesData {
            name: "s".into(),
            xs: (0..ys.len()).map(|i| i as f64 * 100.0).collect(),
            ys,
            color: None,
        }
    }

    #[test]
    fn optional_preset_has_classic_palette_and_no_effects() {
        assert!(RenderOptions::default().rrdtool.is_none());
        let options = RenderOptions::rrdtool();
        assert_eq!(options.theme.bg, Rgba::hex(0xf2f2f2));
        assert!(
            !options.aesthetic.glow
                && !options.aesthetic.gradient_fills
                && !options.aesthetic.vignette
        );
        assert!(!options.rrdtool.unwrap().slope_mode);
    }

    #[test]
    fn staircase_hover_uses_preceding_sample_and_respects_gaps() {
        let data = ChartData::Lines(vec![s(vec![20.0, f64::NAN, 80.0])]);
        let spec = ChartSpec::default();
        let style = RrdOptions::default();
        let layout = ChartLayout::compute(700.0, 300.0, 0.0, 200.0, 0.0, 100.0);
        let hit = hit_test(&layout, &spec, &data, &style, layout.x_scale.to_px(90.0)).unwrap();
        assert_eq!(hit.values[0].y, 20.0);
        assert!(hit_test(&layout, &spec, &data, &style, layout.x_scale.to_px(150.0)).is_none());
    }

    #[test]
    fn zoom_between_samples_retains_plateau_in_domain() {
        let data = ChartData::Lines(vec![s(vec![80.0, 20.0])]);
        let (lo, hi) = domain(
            &ChartSpec::default(),
            &data,
            &RrdOptions::default(),
            70.0,
            90.0,
        );
        assert!(lo <= 80.0 && hi >= 80.0);
    }

    #[test]
    fn stacks_and_line_overrides_use_the_same_values_for_drawing_and_hover() {
        let data = ChartData::Areas(vec![s(vec![20.0, 30.0]), s(vec![10.0, 15.0])]);
        let spec = ChartSpec::area(Unit::Short);
        let mut style = RrdOptions::default();
        assert_eq!(values(&data, &spec, &style, 1, 0), Some((10.0, 20.0, 30.0)));
        style.line_series = vec![1];
        assert_eq!(values(&data, &spec, &style, 1, 0), Some((10.0, 0.0, 10.0)));
    }

    #[test]
    fn slope_hover_agrees_with_log_pixel_interpolation() {
        let data = ChartData::Lines(vec![s(vec![1.0, 100.0])]);
        let spec = ChartSpec::default().with_log_y();
        let style = RrdOptions {
            slope_mode: true,
            ..Default::default()
        };
        let layout = ChartLayout::compute_scaled(
            700.0,
            300.0,
            0.0,
            100.0,
            1.0,
            100.0,
            60.0,
            20.0,
            20.0,
            30.0,
            ScaleKind::Log,
        );
        let hit = hit_test(&layout, &spec, &data, &style, layout.x_scale.to_px(50.0)).unwrap();
        assert!((hit.values[0].y - 10.0).abs() < 1e-8);
    }

    #[test]
    fn extreme_stacks_saturate_without_non_finite_hover_geometry() {
        let data = ChartData::Areas(vec![s(vec![f64::MAX]), s(vec![f64::MAX])]);
        let spec = ChartSpec::area(Unit::None);
        let (_, _, top) = values(&data, &spec, &RrdOptions::default(), 1, 0).unwrap();
        assert_eq!(top, f64::MAX);
    }

    #[test]
    fn printed_numbers_use_decimal_si_and_explicit_missing_values() {
        assert_eq!(number(1_500_000.0), "1.50M");
        assert_eq!(number(f64::NAN), "nan");
    }
}
