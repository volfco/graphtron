#[cfg(feature = "web")]
use crate::canvas::CanvasSurface;
#[cfg(feature = "web")]
use crate::draw::Theme;
#[cfg(feature = "web")]
use crate::hit::HoverInfo;
#[cfg(feature = "web")]
use crate::layout::ChartLayout;
#[cfg(feature = "web")]
use crate::series::{css_color, glow_color, series_color};
#[cfg(feature = "web")]
use crate::ticks::format_ts_full;
#[cfg(feature = "web")]
use crate::units::Unit;

// Only the web draw path reads these; gate them so host builds stay
// warning-free.
#[cfg(feature = "web")]
const TOOLTIP_FONT: &str = "11px 'JetBrains Mono', monospace";
#[cfg(feature = "web")]
const TOOLTIP_PAD: f64 = 10.0;
#[cfg(feature = "web")]
const TOOLTIP_SWATCH: f64 = 8.0;
#[cfg(feature = "web")]
const TOOLTIP_SWATCH_GAP: f64 = 6.0;
#[cfg(feature = "web")]
const TOOLTIP_LINE_H: f64 = 18.0;
#[cfg(feature = "web")]
const TOOLTIP_HEADER_H: f64 = 22.0;
#[cfg(feature = "web")]
const TOOLTIP_MIN_W: f64 = 120.0;
#[cfg(feature = "web")]
const TOOLTIP_OFFSET: f64 = 12.0;
#[cfg(feature = "web")]
const TOOLTIP_MAX_W: f64 = 280.0;

pub fn tooltip_anchor(info: &crate::hit::HoverInfo) -> Option<(f64, f64)> {
    info.values.first().map(|v| (v.px, v.py))
}

/// Return a stable page of hover rows for an inspector or accessible DOM
/// tooltip. Canvas rendering shows the first page when space is limited; the
/// caller can expose the remaining rows without re-running hit testing.
pub fn tooltip_page(
    info: &crate::hit::HoverInfo,
    offset: usize,
    max_values: usize,
) -> Option<crate::hit::HoverInfo> {
    let end = offset
        .saturating_add(max_values.max(1))
        .min(info.values.len());
    (offset < end).then(|| crate::hit::HoverInfo {
        ts: info.ts,
        header: info.header.clone(),
        values: info.values[offset..end].to_vec(),
    })
}

#[cfg(feature = "web")]
mod web {
    use super::*;

    struct TooltipMetrics {
        w: f64,
        h: f64,
        value_w: f64,
    }

    impl TooltipMetrics {
        fn compute(
            info: &HoverInfo,
            indices: &[usize],
            omitted: usize,
            unit: Unit,
            ctx: &web_sys::CanvasRenderingContext2d,
            available_w: f64,
            available_h: f64,
        ) -> Self {
            ctx.set_font(TOOLTIP_FONT);

            let ts_text = info
                .header
                .clone()
                .unwrap_or_else(|| format_ts_full(info.ts as i64));
            let header_w = ctx
                .measure_text(&ts_text)
                .map(|m| m.width())
                .unwrap_or(100.0);

            let mut max_row_w: f64 = 0.0;
            let mut value_w: f64 = 0.0;
            let mut row_count = 0usize;
            for &index in indices {
                let v = &info.values[index];
                let val_text = v
                    .formatted_value
                    .clone()
                    .unwrap_or_else(|| unit.format(v.y));
                let row_text = format!("{}  {}", v.name, val_text);
                value_w = value_w.max(
                    ctx.measure_text(&val_text)
                        .map(|m| m.width())
                        .unwrap_or(40.0),
                );
                let w = ctx
                    .measure_text(&row_text)
                    .map(|m| m.width())
                    .unwrap_or(80.0);
                max_row_w = max_row_w.max(w);
                row_count += 1;
                // Extra OHLC fields
                for (label, val) in &v.extra {
                    let extra_text = format!("{}: {}", label, unit.format(*val));
                    let w = ctx
                        .measure_text(&extra_text)
                        .map(|m| m.width())
                        .unwrap_or(60.0);
                    max_row_w = max_row_w.max(w + 12.0);
                    row_count += 1;
                }
                for (label, value) in &v.extra_text {
                    let extra_text = format!("{label}: {value}");
                    let w = ctx
                        .measure_text(&extra_text)
                        .map(|m| m.width())
                        .unwrap_or(60.0);
                    max_row_w = max_row_w.max(w + 12.0);
                    row_count += 1;
                }
            }
            if omitted > 0 {
                let footer = format!("+{omitted} more");
                max_row_w = max_row_w.max(
                    ctx.measure_text(&footer)
                        .map(|metrics| metrics.width())
                        .unwrap_or(60.0),
                );
                row_count += 1;
            }

            let row_count = row_count.max(1);
            let content_w = header_w.max(max_row_w);
            let desired_w = content_w + TOOLTIP_PAD * 2.0 + TOOLTIP_SWATCH + TOOLTIP_SWATCH_GAP;
            let max_w = available_w.max(TOOLTIP_MIN_W.min(available_w.max(1.0)));
            let w = desired_w.clamp(TOOLTIP_MIN_W.min(max_w), TOOLTIP_MAX_W.min(max_w));
            let h = TOOLTIP_HEADER_H + row_count as f64 * TOOLTIP_LINE_H + TOOLTIP_PAD * 2.0;

            Self {
                w,
                h: h.min(available_h.max(1.0)),
                value_w,
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // Stable public canvas API.
    pub fn draw_tooltip(
        surface: &CanvasSurface,
        layout: &ChartLayout,
        info: &HoverInfo,
        series: &[crate::series::SeriesData],
        unit: Unit,
        cursor_px: f64,
        cursor_py: f64,
        theme: &Theme,
    ) {
        draw_tooltip_limited(
            surface,
            layout,
            info,
            series,
            unit,
            cursor_px,
            cursor_py,
            theme,
            usize::MAX,
            false,
        );
    }

    /// Draw a bounded tooltip, optionally ordering the automatically
    /// highlighted top value first. This keeps high-cardinality panels usable
    /// and prevents the tooltip from growing beyond the plot.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_tooltip_limited(
        surface: &CanvasSurface,
        layout: &ChartLayout,
        info: &HoverInfo,
        series: &[crate::series::SeriesData],
        unit: Unit,
        cursor_px: f64,
        cursor_py: f64,
        theme: &Theme,
        max_values: usize,
        top_first: bool,
    ) {
        let ctx = &surface.ctx;
        let mut indices: Vec<usize> = (0..info.values.len()).collect();
        if top_first {
            indices.sort_by(|&a, &b| {
                info.values[b]
                    .highlighted
                    .cmp(&info.values[a].highlighted)
                    .then_with(|| {
                        info.values[b]
                            .y
                            .partial_cmp(&info.values[a].y)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });
        }

        // Fit by both value count and estimated detail-line count. A single
        // detailed OHLC row is always retained even in a very short panel.
        let line_budget = ((layout.plot.h - TOOLTIP_HEADER_H - TOOLTIP_PAD * 2.0) / TOOLTIP_LINE_H)
            .floor()
            .max(1.0) as usize;
        // Always reserve the footer before selecting detail groups. Without
        // this reservation `+N more` is painted below the tooltip background.
        let total_lines: usize = info
            .values
            .iter()
            .map(|value| 1 + value.extra.len() + value.extra_text.len())
            .sum();
        let footer_reserve =
            (info.values.len() > max_values.max(1) || total_lines > line_budget) as usize;
        let content_budget = line_budget.saturating_sub(footer_reserve).max(1);
        let mut used_lines = 0usize;
        let mut visible = Vec::new();
        for index in indices {
            if visible.len() >= max_values.max(1) {
                break;
            }
            let value = &info.values[index];
            let lines = 1 + value.extra.len() + value.extra_text.len();
            if used_lines + lines > content_budget {
                if visible.is_empty() {
                    // A detailed row that cannot fit would push its extras
                    // outside the box; leave it for the inspectable footer.
                    continue;
                }
                break;
            }
            visible.push(index);
            used_lines += lines;
        }
        let omitted = info.values.len().saturating_sub(visible.len());
        let metrics = TooltipMetrics::compute(
            info,
            &visible,
            omitted,
            unit,
            ctx,
            // The tooltip is clamped to the plot, so its dimensions must fit
            // the plot as well as the backing canvas. Using canvas width alone
            // lets a narrow left axis gutter force values past the right edge.
            (surface.css_w - 4.0).min(layout.plot.w - 4.0).max(1.0),
            (surface.css_h - 4.0).min(layout.plot.h - 4.0).max(1.0),
        );

        let plot = layout.plot;
        let mut tx = cursor_px + TOOLTIP_OFFSET;
        let mut ty = cursor_py + TOOLTIP_OFFSET;

        if tx + metrics.w > plot.x + plot.w {
            tx = cursor_px - TOOLTIP_OFFSET - metrics.w;
        }
        if ty + metrics.h > plot.y + plot.h {
            ty = cursor_py - TOOLTIP_OFFSET - metrics.h;
        }
        let tx_min = plot.x + 2.0;
        let tx_max = (plot.x + plot.w - metrics.w - 2.0).max(tx_min);
        tx = tx.clamp(tx_min, tx_max);
        let ty_min = plot.y + 2.0;
        let ty_max = (plot.y + plot.h - metrics.h - 2.0).max(ty_min);
        ty = ty.clamp(ty_min, ty_max);

        let accent = theme.accent;
        let (bg, _border, text_color, glow_blur) = if theme.dark {
            ("rgba(10,14,23,0.92)", "#00FFFF", "#d7dbe0", 8.0)
        } else {
            ("rgba(255,255,255,0.95)", "#5794F2", "#1a1a1a", 4.0)
        };
        let border = css_color(theme.accent);
        let glow_blur = if theme.tooltip_radius == 0.0 {
            0.0
        } else {
            glow_blur
        };

        ctx.save();

        let gc = glow_color(accent, 0.3);
        ctx.set_shadow_color(&gc);
        ctx.set_shadow_blur(glow_blur);

        ctx.set_fill_style_str(bg);
        round_rect(ctx, tx, ty, metrics.w, metrics.h, theme.tooltip_radius);
        ctx.fill();

        ctx.set_stroke_style_str(&border);
        ctx.set_line_width(1.0);
        let bc = glow_color(accent, 0.6);
        ctx.set_shadow_color(&bc);
        ctx.set_shadow_blur(6.0);
        round_rect(ctx, tx, ty, metrics.w, metrics.h, theme.tooltip_radius);
        ctx.stroke();

        ctx.set_shadow_color("transparent");
        ctx.set_shadow_blur(0.0);

        let mut y = ty + TOOLTIP_PAD;

        ctx.set_font(TOOLTIP_FONT);
        ctx.set_fill_style_str(&border);
        ctx.set_text_align("left");
        ctx.set_text_baseline("top");
        let ts_text = info
            .header
            .clone()
            .unwrap_or_else(|| format_ts_full(info.ts as i64));
        let _ = ctx.fill_text(&ts_text, tx + TOOLTIP_PAD, y);
        y += TOOLTIP_HEADER_H;

        for index in visible {
            let v = &info.values[index];
            let color = v
                .color
                .or_else(|| series.get(v.series).map(|s| series_color(s, v.series)))
                .unwrap_or(0xffffff);

            if v.highlighted {
                ctx.set_fill_style_str(if theme.dark {
                    "rgba(255,255,255,0.06)"
                } else {
                    "rgba(0,0,0,0.05)"
                });
                ctx.fill_rect(tx + 3.0, y - 2.0, metrics.w - 6.0, TOOLTIP_LINE_H);
                ctx.set_font("bold 11px 'JetBrains Mono', monospace");
            } else {
                ctx.set_font(TOOLTIP_FONT);
            }

            let swatch_x = tx + TOOLTIP_PAD;
            ctx.set_fill_style_str(&css_color(color));
            round_rect(ctx, swatch_x, y + 2.0, TOOLTIP_SWATCH, TOOLTIP_SWATCH, 1.0);
            ctx.fill();

            ctx.set_fill_style_str(text_color);
            let val_text = v
                .formatted_value
                .clone()
                .unwrap_or_else(|| unit.format(v.y));
            let value_x = tx + metrics.w - TOOLTIP_PAD;
            let name_x = swatch_x + TOOLTIP_SWATCH + TOOLTIP_SWATCH_GAP;
            let name_max = (value_x - name_x - metrics.value_w - 8.0).max(0.0);
            let name = ellipsize(ctx, &v.name, name_max);
            let _ = ctx.fill_text(&name, name_x, y);
            ctx.set_text_align("right");
            let _ = ctx.fill_text(&val_text, value_x, y);
            ctx.set_text_align("left");

            // Render extra OHLC fields on subsequent lines
            let mut ey = y + TOOLTIP_LINE_H;
            ctx.set_font(TOOLTIP_FONT);
            for (field_name, field_val) in &v.extra {
                let extra_text = format!("{}: {}", field_name, unit.format(*field_val));
                ctx.set_fill_style_str(text_color);
                let extra_x = swatch_x + TOOLTIP_SWATCH + TOOLTIP_SWATCH_GAP + 12.0;
                let _ = ctx.fill_text(&ellipsize(ctx, &extra_text, value_x - extra_x), extra_x, ey);
                ey += TOOLTIP_LINE_H;
            }

            for (field_name, field_value) in &v.extra_text {
                let extra_text = format!("{field_name}: {field_value}");
                ctx.set_fill_style_str(text_color);
                ctx.set_font(TOOLTIP_FONT);
                let extra_x = swatch_x + TOOLTIP_SWATCH + TOOLTIP_SWATCH_GAP + 12.0;
                let _ = ctx.fill_text(&ellipsize(ctx, &extra_text, value_x - extra_x), extra_x, ey);
                ey += TOOLTIP_LINE_H;
            }

            y = ey;
        }

        if omitted > 0 {
            ctx.set_font(TOOLTIP_FONT);
            ctx.set_fill_style_str(if theme.dark {
                "rgba(215,219,224,0.65)"
            } else {
                "rgba(26,26,26,0.65)"
            });
            let _ = ctx.fill_text(
                &format!("+{omitted} more"),
                tx + TOOLTIP_PAD + TOOLTIP_SWATCH + TOOLTIP_SWATCH_GAP,
                y,
            );
        }

        ctx.restore();
    }

    fn ellipsize(ctx: &web_sys::CanvasRenderingContext2d, text: &str, max_width: f64) -> String {
        if max_width <= 0.0 {
            return String::new();
        }
        if ctx
            .measure_text(text)
            .map(|m| m.width() <= max_width)
            .unwrap_or(true)
        {
            return text.to_owned();
        }
        let suffix = "…";
        let mut end = text.len();
        while end > 0 {
            end -= 1;
            if !text.is_char_boundary(end) {
                continue;
            }
            let candidate = format!("{}{}", &text[..end], suffix);
            if ctx
                .measure_text(&candidate)
                .map(|m| m.width() <= max_width)
                .unwrap_or(false)
            {
                return candidate;
            }
        }
        suffix.to_owned()
    }

    fn round_rect(ctx: &web_sys::CanvasRenderingContext2d, x: f64, y: f64, w: f64, h: f64, r: f64) {
        ctx.begin_path();
        ctx.move_to(x + r, y);
        ctx.line_to(x + w - r, y);
        ctx.quadratic_curve_to(x + w, y, x + w, y + r);
        ctx.line_to(x + w, y + h - r);
        ctx.quadratic_curve_to(x + w, y + h, x + w - r, y + h);
        ctx.line_to(x + r, y + h);
        ctx.quadratic_curve_to(x, y + h, x, y + h - r);
        ctx.line_to(x, y + r);
        ctx.quadratic_curve_to(x, y, x + r, y);
        ctx.close_path();
    }
}

#[cfg(feature = "web")]
pub use web::{draw_tooltip, draw_tooltip_limited};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hit::HoverInfo;

    #[test]
    fn tooltip_anchor_returns_first_value() {
        let info = HoverInfo {
            ts: 1000.0,
            header: None,
            values: vec![
                crate::hit::HoverValue::new(0, "a".into(), 42.0, 100.0, 50.0),
                crate::hit::HoverValue::new(1, "b".into(), 80.0, 100.0, 30.0),
            ],
        };
        let (x, y) = tooltip_anchor(&info).unwrap();
        assert!((x - 100.0).abs() < 0.1);
        assert!((y - 50.0).abs() < 0.1);
    }

    #[test]
    fn tooltip_anchor_empty() {
        let info = HoverInfo {
            ts: 0.0,
            header: None,
            values: vec![],
        };
        assert!(tooltip_anchor(&info).is_none());
    }

    #[test]
    fn tooltip_page_exposes_rows_omitted_by_canvas_budget() {
        let info = HoverInfo {
            ts: 1000.0,
            header: Some("t".into()),
            values: (0..3)
                .map(|i| crate::hit::HoverValue::new(i, format!("s{i}"), i as f64, 0.0, 0.0))
                .collect(),
        };
        let page = tooltip_page(&info, 1, 2).unwrap();
        assert_eq!(page.header.as_deref(), Some("t"));
        assert_eq!(page.values.len(), 2);
        assert_eq!(page.values[0].name, "s1");
        assert!(tooltip_page(&info, 3, 1).is_none());
    }
}
