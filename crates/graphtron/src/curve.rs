/// Monotone cubic Hermite interpolation for smooth line rendering.
///
/// Given a series of pixel-space points, produces cubic bezier control points
/// that preserve monotonicity in x (no horizontal overshoot) and avoid
/// unnatural wiggles. This is the standard approach for high-quality line
/// charts (used by D3, Highcharts, and Grafana).
///
/// A path operation in pixel space, consumed by Canvas2D draw calls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathOp {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    /// Cubic bezier: (cp1x, cp1y, cp2x, cp2y, x, y).
    CurveTo(f64, f64, f64, f64, f64, f64),
}

/// Compute monotone cubic Hermite control points for a sequence of points.
///
/// Returns a list of `PathOp` that the caller feeds into Canvas2D:
/// ```ignore
/// for op in &ops {
///     match op {
///         PathOp::MoveTo(x, y) => ctx.move_to(*x, *y),
///         PathOp::LineTo(x, y) => ctx.line_to(*x, *y),
///         PathOp::CurveTo(c1x, c1y, c2x, c2y, x, y) => {
///             ctx.bezier_curve_to(*c1x, *c1y, *c2x, *c2y, *x, *y);
///         }
///     }
/// }
/// ```
///
/// `gap_indices` is a set of indices where NaN gaps occur — the curve breaks
/// at these points and resumes with a `MoveTo`.
pub fn monotone_cubic(points: &[(f64, f64)], gap_indices: &[usize]) -> Vec<PathOp> {
    let n = points.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![PathOp::MoveTo(points[0].0, points[0].1)];
    }

    // Build gap set for fast lookup.
    let gaps: std::collections::HashSet<usize> = gap_indices.iter().copied().collect();

    // Secant slopes in x. Tangents are defined from these interval slopes;
    // using a raw three-point x difference lets a large neighboring gap pull
    // a control point outside the segment currently being rendered.
    let mut dx = Vec::with_capacity(n - 1);
    let mut slopes = Vec::with_capacity(n - 1);
    for i in 0..n - 1 {
        let h = points[i + 1].0 - points[i].0;
        dx.push(h);
        slopes.push(if h != 0.0 && h.is_finite() {
            (points[i + 1].1 - points[i].1) / h
        } else {
            0.0
        });
    }

    // Fritsch-Carlson/Hyman monotone tangents. A weighted harmonic mean at an
    // interior point is zero across a slope sign change, which prevents y
    // overshoot while retaining smoothness on monotone runs.
    let mut ty = vec![0.0; n];
    if let Some(first) = slopes.first().copied() {
        ty[0] = first;
    }
    if let Some(last) = slopes.last().copied() {
        ty[n - 1] = last;
    }
    for i in 1..n - 1 {
        let left = slopes[i - 1];
        let right = slopes[i];
        ty[i] = if left == 0.0
            || right == 0.0
            || !left.is_finite()
            || !right.is_finite()
            || left.signum() != right.signum()
        {
            0.0
        } else {
            2.0 * left * right / (left + right)
        };
    }

    // Enforce the per-interval monotone tangent bound. This also handles
    // unequal x gaps, where a global tangent clamp is insufficient.
    for i in 0..n - 1 {
        let slope = slopes[i];
        if slope == 0.0 || !slope.is_finite() {
            ty[i] = 0.0;
            ty[i + 1] = 0.0;
            continue;
        }
        let a = ty[i] / slope;
        let b = ty[i + 1] / slope;
        if a < 0.0 || b < 0.0 || !a.is_finite() || !b.is_finite() {
            ty[i] = 0.0;
            ty[i + 1] = 0.0;
        } else {
            let norm = a * a + b * b;
            if norm > 9.0 {
                let scale = 3.0 / norm.sqrt();
                ty[i] = scale * a * slope;
                ty[i + 1] = scale * b * slope;
            }
        }
    }

    // Emit path ops.
    let mut ops = Vec::with_capacity(n * 2);
    ops.push(PathOp::MoveTo(points[0].0, points[0].1));

    for i in 0..n - 1 {
        // Break at gaps.
        if gaps.contains(&i) || gaps.contains(&(i + 1)) {
            ops.push(PathOp::MoveTo(points[i + 1].0, points[i + 1].1));
            continue;
        }

        let (x0, y0) = points[i];
        let (x1, y1) = points[i + 1];
        let h = dx[i];

        if h == 0.0 || !h.is_finite() {
            // Vertical segment or coincident points — just line to.
            ops.push(PathOp::LineTo(x1, y1));
            continue;
        }

        // Keep x controls local to this interval. These one-third controls
        // are monotone in x even when neighboring intervals are wildly
        // unequal; y tangents carry the smoothing independently.
        let cp1x = x0 + h / 3.0;
        let cp1y = y0 + ty[i] * h / 3.0;
        let cp2x = x1 - h / 3.0;
        let cp2y = y1 - ty[i + 1] * h / 3.0;

        ops.push(PathOp::CurveTo(cp1x, cp1y, cp2x, cp2y, x1, y1));
    }

    ops
}

/// Build the stroke path for a polyline whose `y` values may contain
/// `f64::NAN` gap markers.
///
/// The points are split into contiguous runs of finite samples (breaking at
/// every NaN), and each run is rendered independently — so a gap never bridges
/// into a continuous line. When `smooth`, runs of ≥3 points use monotone-cubic
/// interpolation; shorter runs (and every run when `!smooth`) are straight
/// segments.
///
/// This supersedes calling [`monotone_cubic`] with a separate `gap_indices`
/// list: because each run is smoothed on its own contiguous slice, the point
/// and index spaces always align (previously, filtering NaNs out of the point
/// list while deriving gap indices from the *unfiltered* list mismatched the
/// two and bridged gaps under `smooth`).
pub fn line_path(points: &[(f64, f64)], smooth: bool) -> Vec<PathOp> {
    let n = points.len();
    let mut ops = Vec::with_capacity(n + 4);
    let mut i = 0;
    while i < n {
        // Skip gap markers, then take the next contiguous finite run.
        while i < n && !points[i].1.is_finite() {
            i += 1;
        }
        let start = i;
        while i < n && points[i].1.is_finite() {
            i += 1;
        }
        let run = &points[start..i];
        if run.is_empty() {
            continue;
        }
        if smooth && run.len() >= 3 {
            // monotone_cubic emits its own leading MoveTo for the run.
            ops.extend(monotone_cubic(run, &[]));
        } else {
            for (k, p) in run.iter().enumerate() {
                if k == 0 {
                    ops.push(PathOp::MoveTo(p.0, p.1));
                } else {
                    ops.push(PathOp::LineTo(p.0, p.1));
                }
            }
        }
    }
    ops
}

/// Simpler smooth curve: Catmull-Rom to cubic bezier conversion.
///
/// Less mathematically rigorous than monotone Hermite but visually pleasing
/// and simpler to implement. Good for sparklines and decorative curves.
pub fn catmull_rom(points: &[(f64, f64)], tension: f64) -> Vec<PathOp> {
    let n = points.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![PathOp::MoveTo(points[0].0, points[0].1)];
    }
    if n == 2 {
        return vec![
            PathOp::MoveTo(points[0].0, points[0].1),
            PathOp::LineTo(points[1].0, points[1].1),
        ];
    }

    let tension = tension.clamp(0.0, 1.0);
    let mut ops = Vec::with_capacity(n * 2);
    ops.push(PathOp::MoveTo(points[0].0, points[0].1));

    for i in 0..n - 1 {
        let p0 = if i > 0 { points[i - 1] } else { points[i] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < n {
            points[i + 2]
        } else {
            points[i + 1]
        };

        let d1x = (p2.0 - p0.0) * tension / 3.0;
        let d1y = (p2.1 - p0.1) * tension / 3.0;
        let d2x = (p3.0 - p1.0) * tension / 3.0;
        let d2y = (p3.1 - p1.1) * tension / 3.0;

        ops.push(PathOp::CurveTo(
            p1.0 + d1x,
            p1.1 + d1y,
            p2.0 - d2x,
            p2.1 - d2y,
            p2.0,
            p2.1,
        ));
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        assert!(monotone_cubic(&[], &[]).is_empty());
    }

    #[test]
    fn single_point() {
        let ops = monotone_cubic(&[(10.0, 20.0)], &[]);
        assert_eq!(ops, vec![PathOp::MoveTo(10.0, 20.0)]);
    }

    #[test]
    fn two_points_produces_one_curve() {
        let ops = monotone_cubic(&[(0.0, 0.0), (100.0, 50.0)], &[]);
        assert!(ops.iter().any(|op| matches!(op, PathOp::CurveTo(..))));
    }

    #[test]
    fn controls_stay_inside_irregular_x_intervals() {
        let points = [(0.0, 0.0), (1.0, 1.0), (100.0, 2.0)];
        let ops = monotone_cubic(&points, &[]);
        let curves: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                PathOp::CurveTo(c1x, c1y, c2x, c2y, x, y) => Some((*c1x, *c1y, *c2x, *c2y, *x, *y)),
                _ => None,
            })
            .collect();
        assert_eq!(curves.len(), 2);
        assert!((0.0..=1.0).contains(&curves[0].0));
        assert!((0.0..=1.0).contains(&curves[0].2));
        assert!((1.0..=100.0).contains(&curves[1].0));
        assert!((1.0..=100.0).contains(&curves[1].2));
    }

    #[test]
    fn duplicate_x_values_do_not_create_non_finite_controls() {
        let ops = monotone_cubic(&[(0.0, 1.0), (0.0, 2.0), (1.0, 3.0)], &[]);
        for op in ops {
            match op {
                PathOp::MoveTo(x, y) | PathOp::LineTo(x, y) => {
                    assert!(x.is_finite() && y.is_finite())
                }
                PathOp::CurveTo(a, b, c, d, x, y) => {
                    assert!(a.is_finite() && b.is_finite() && c.is_finite() && d.is_finite());
                    assert!(x.is_finite() && y.is_finite());
                }
            }
        }
    }

    #[test]
    fn gap_breaks_curve() {
        let points = vec![(0.0, 0.0), (50.0, 25.0), (100.0, 50.0), (150.0, 75.0)];
        let gaps = vec![1]; // gap between index 1 and 2
        let ops = monotone_cubic(&points, &gaps);
        // Should have MoveTo at start, curves, then a MoveTo after the gap.
        let move_count = ops
            .iter()
            .filter(|op| matches!(op, PathOp::MoveTo(..)))
            .count();
        assert!(move_count >= 2);
    }

    fn move_count(ops: &[PathOp]) -> usize {
        ops.iter()
            .filter(|op| matches!(op, PathOp::MoveTo(..)))
            .count()
    }

    #[test]
    fn line_path_no_gaps_single_subpath() {
        let pts = vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0), (3.0, 1.0)];
        // Exactly one MoveTo → one continuous subpath.
        assert_eq!(move_count(&line_path(&pts, true)), 1);
        assert_eq!(move_count(&line_path(&pts, false)), 1);
    }

    #[test]
    fn line_path_breaks_at_interior_gap() {
        // A NaN in the middle must split into two subpaths (two MoveTos) so the
        // gap is never bridged — this is the C1 regression guard, and it must
        // hold under smooth=true (the buggy path bridged smoothed gaps).
        let pts = vec![
            (0.0, 0.0),
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, f64::NAN),
            (4.0, 2.0),
            (5.0, 1.0),
            (6.0, 0.0),
        ];
        assert_eq!(move_count(&line_path(&pts, true)), 2);
        assert_eq!(move_count(&line_path(&pts, false)), 2);
        // The bridging segment (2.0 → 4.0) must not appear as a drawn op.
        for op in line_path(&pts, false) {
            if let PathOp::LineTo(x, _) = op {
                assert_ne!(
                    x, 4.0,
                    "gap was bridged with a LineTo to the post-gap point"
                );
            }
        }
    }

    #[test]
    fn line_path_trims_leading_and_trailing_gaps() {
        let pts = vec![
            (0.0, f64::NAN),
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, f64::NAN),
        ];
        let ops = line_path(&pts, true);
        assert_eq!(move_count(&ops), 1);
        // First op moves to the first *finite* point, not the leading NaN.
        assert!(matches!(ops.first(), Some(PathOp::MoveTo(x, _)) if *x == 1.0));
    }

    #[test]
    fn line_path_all_gaps_is_empty() {
        let pts = vec![(0.0, f64::NAN), (1.0, f64::NAN)];
        assert!(line_path(&pts, true).is_empty());
    }

    #[test]
    fn catmull_rom_produces_curves() {
        let points = vec![(0.0, 0.0), (25.0, 30.0), (50.0, 10.0), (75.0, 40.0)];
        let ops = catmull_rom(&points, 0.5);
        let curve_count = ops
            .iter()
            .filter(|op| matches!(op, PathOp::CurveTo(..)))
            .count();
        assert_eq!(curve_count, 3);
    }
}
