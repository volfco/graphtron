//! Area-proportional pie/treemap geometry and equal-sized host tiles.
//! Geometry is shared by drawing and hit testing, including gaps.
use crate::{ChartData, ChartKind, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct PartitionItem {
    pub name: String,
    /// Slice weight / rectangle area; for host maps this is the color metric.
    pub value: f64,
    /// Explicit category/status color; otherwise use the chart palette.
    pub color: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Tile(Rect),
    Slice {
        cx: f64,
        cy: f64,
        radius: f64,
        start: f64,
        end: f64,
    },
}

impl Shape {
    pub fn contains(self, x: f64, y: f64) -> bool {
        match self {
            Self::Tile(r) => r.w > 0.0 && r.h > 0.0 && r.contains(x, y),
            Self::Slice {
                cx,
                cy,
                radius,
                start,
                end,
            } => {
                let dx = x - cx;
                let dy = y - cy;
                let angle =
                    (dy.atan2(dx) + std::f64::consts::FRAC_PI_2).rem_euclid(std::f64::consts::TAU);
                dx * dx + dy * dy <= radius * radius && angle >= start && angle < end
            }
        }
    }
    pub fn center(self) -> (f64, f64) {
        match self {
            Self::Tile(r) => (r.x + r.w / 2.0, r.y + r.h / 2.0),
            Self::Slice {
                cx,
                cy,
                radius,
                start,
                end,
            } => {
                let angle = (start + end) / 2.0 - std::f64::consts::FRAC_PI_2;
                (
                    cx + angle.cos() * radius * 0.65,
                    cy + angle.sin() * radius * 0.65,
                )
            }
        }
    }
}

pub fn items(data: &ChartData) -> Option<&[PartitionItem]> {
    match data {
        ChartData::Pie(items) | ChartData::Treemap(items) | ChartData::HostMap(items) => {
            Some(items)
        }
        _ => None,
    }
}

/// Host metrics use a fixed 0–100 utilization scale by default. Set explicit
/// colors for status categories or metrics with another range.
pub fn item_color(kind: ChartKind, items: &[PartitionItem], i: usize) -> u32 {
    let item = &items[i];
    item.color.unwrap_or_else(|| {
        if kind == ChartKind::HostMap {
            if item.value.is_finite() {
                let c = crate::color::heatmap_color_for_value(
                    crate::HeatmapColorScale::Viridis,
                    item.value,
                    0.0,
                    100.0,
                );
                (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
            } else {
                0x667788
            }
        } else {
            crate::series::palette_color(None, i)
        }
    })
}

/// Hit-test prepared geometry. Prepare again only when data or plot size changes.
pub fn hit_test(
    layout: &crate::ChartLayout,
    data: &ChartData,
    shapes: &[(usize, Shape)],
    x: f64,
    y: f64,
) -> Option<crate::HoverInfo> {
    if !layout.plot.contains(x, y) {
        return None;
    }
    let items = items(data)?;
    let &(i, shape) = shapes.iter().find(|(_, shape)| shape.contains(x, y))?;
    let item = items.get(i)?;
    let mut value = crate::HoverValue::new(i, item.name.clone(), item.value, x, y)
        .with_color(item_color(data.kind(), items, i));
    value.shape = Some(shape);
    if let Shape::Slice { start, end, .. } = shape {
        value.extra_text.push((
            "Share".into(),
            format!("{:.1}%", (end - start) / std::f64::consts::TAU * 100.0),
        ));
    }
    if !item.value.is_finite() {
        value.formatted_value = Some("No data".into());
    }
    Some(crate::HoverInfo {
        ts: layout.ts_at(x),
        header: Some(item.name.clone()),
        values: vec![value],
    })
}

/// O(n) storage; pie/grid O(n), balanced binary treemap O(n log n).
/// Scale weights before summing so large finite values cannot overflow.
pub fn geometry(kind: ChartKind, items: &[PartitionItem], plot: Rect) -> Vec<(usize, Shape)> {
    if !plot.w.is_finite() || !plot.h.is_finite() || plot.w <= 0.0 || plot.h <= 0.0 {
        return vec![];
    }
    if kind == ChartKind::HostMap {
        if items.is_empty() {
            return vec![];
        }
        let cols =
            ((items.len() as f64 * plot.w / plot.h).sqrt().ceil() as usize).clamp(1, items.len());
        let rows = items.len().div_ceil(cols);
        let side = (plot.w / cols as f64).min(plot.h / rows as f64);
        return (0..items.len())
            .map(|i| {
                (
                    i,
                    Shape::Tile(inset(Rect {
                        x: plot.x + (plot.w - cols as f64 * side) / 2.0 + (i % cols) as f64 * side,
                        y: plot.y + (plot.h - rows as f64 * side) / 2.0 + (i / cols) as f64 * side,
                        w: side,
                        h: side,
                    })),
                )
            })
            .collect();
    }
    let max = items
        .iter()
        .map(|i| i.value)
        .filter(|v| v.is_finite() && *v > 0.0)
        .fold(0.0, f64::max);
    if max == 0.0 {
        return vec![];
    }
    let weights: Vec<_> = items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.value.is_finite() && i.value > 0.0)
        .map(|(i, item)| (i, item.value / max))
        .collect();
    let total: f64 = weights.iter().map(|(_, w)| w).sum();
    if kind == ChartKind::Pie {
        let mut start = 0.0;
        return weights
            .iter()
            .enumerate()
            .map(|(j, &(i, w))| {
                let end = if j + 1 == weights.len() {
                    std::f64::consts::TAU
                } else {
                    (start + w / total * std::f64::consts::TAU).min(std::f64::consts::TAU)
                };
                let shape = Shape::Slice {
                    cx: plot.x + plot.w / 2.0,
                    cy: plot.y + plot.h / 2.0,
                    radius: plot.w.min(plot.h) / 2.0,
                    start,
                    end,
                };
                start = end;
                (i, shape)
            })
            .collect();
    }
    let mut out = Vec::with_capacity(weights.len());
    split(&weights, plot, &mut out);
    out
}

pub(crate) fn inset(r: Rect) -> Rect {
    let gap = 1.0_f64.min(r.w / 4.0).min(r.h / 4.0);
    Rect {
        x: r.x + gap,
        y: r.y + gap,
        w: (r.w - 2.0 * gap).max(0.0),
        h: (r.h - 2.0 * gap).max(0.0),
    }
}

// Split by item count to bound recursion depth even for highly skewed weights.
fn split(weights: &[(usize, f64)], r: Rect, out: &mut Vec<(usize, Shape)>) {
    if weights.len() == 1 {
        out.push((weights[0].0, Shape::Tile(inset(r))));
        return;
    }
    let mid = weights.len() / 2;
    let left: f64 = weights[..mid].iter().map(|(_, w)| w).sum();
    let right: f64 = weights[mid..].iter().map(|(_, w)| w).sum();
    let fraction = if left + right > 0.0 {
        left / (left + right)
    } else {
        0.5
    };
    let (a, b) = if r.w >= r.h {
        let w = r.w * fraction;
        (
            Rect { w, ..r },
            Rect {
                x: r.x + w,
                w: r.w - w,
                ..r
            },
        )
    } else {
        let h = r.h * fraction;
        (
            Rect { h, ..r },
            Rect {
                y: r.y + h,
                h: r.h - h,
                ..r
            },
        )
    };
    split(&weights[..mid], a, out);
    split(&weights[mid..], b, out);
}

#[cfg(feature = "web")]
pub(crate) fn paint(surface: &crate::CanvasSurface, shape: Shape, color: &str) {
    let ctx = &surface.ctx;
    ctx.set_fill_style_str(color);
    match shape {
        Shape::Tile(r) => ctx.fill_rect(r.x, r.y, r.w, r.h),
        Shape::Slice {
            cx,
            cy,
            radius,
            start,
            end,
        } => {
            ctx.begin_path();
            ctx.move_to(cx, cy);
            let _ = ctx.arc(
                cx,
                cy,
                radius,
                start - std::f64::consts::FRAC_PI_2,
                end - std::f64::consts::FRAC_PI_2,
            );
            ctx.close_path();
            ctx.fill();
        }
    }
}
