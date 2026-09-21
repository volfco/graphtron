use crate::scale::LinearScale;

type Point = (f64, f64);
type ColumnAccumulator = (Point, Point, Point, Point);

/// Min-max (M4-style) decimation: for each pixel column keep first, min,
/// max, and last sample so strokes preserve spikes at any zoom level.
pub fn min_max(xs: &[f64], ys: &[f64], x_scale: &LinearScale, px_width: f64) -> Vec<(f64, f64)> {
    let n = xs.len().min(ys.len());
    if n == 0 {
        return vec![];
    }
    if n as f64 <= px_width * 2.0 {
        return xs[..n]
            .iter()
            .copied()
            .zip(ys[..n].iter().copied())
            .collect();
    }

    let mut out: Vec<Point> = Vec::with_capacity((px_width as usize + 1) * 4);
    let mut col = f64::NEG_INFINITY;
    let mut acc: Option<ColumnAccumulator> = None;
    let mut gap_pending = false;

    let flush = |acc: &mut Option<ColumnAccumulator>, out: &mut Vec<Point>| {
        if let Some((first, min, max, last)) = acc.take() {
            let middle = if min.0 <= max.0 {
                [min, max]
            } else {
                [max, min]
            };
            for point in [first, middle[0], middle[1], last] {
                if out.last().copied() != Some(point) {
                    out.push(point);
                }
            }
        }
    };

    for i in 0..n {
        let (x, y) = (xs[i], ys[i]);
        if !x.is_finite() || !y.is_finite() {
            flush(&mut acc, &mut out);
            gap_pending = true;
            continue;
        }
        if gap_pending {
            out.push((x, f64::NAN));
            gap_pending = false;
        }
        let c = x_scale.to_px(x).floor();
        // A gap flushes the previous run but the next sample may land in the
        // same projected column. The missing accumulator must still start a
        // fresh finite run there.
        if c != col || acc.is_none() {
            flush(&mut acc, &mut out);
            col = c;
            acc = Some(((x, y), (x, y), (x, y), (x, y)));
        } else if let Some((_, min, max, last)) = &mut acc {
            if y < min.1 {
                *min = (x, y);
            }
            if y > max.1 {
                *max = (x, y);
            }
            *last = (x, y);
        }
    }
    flush(&mut acc, &mut out);
    out
}

/// Largest Triangle Three Buckets (LTTB) downsampling.
/// Reduces `(xs, ys)` to at most `threshold` points while preserving visual
/// shape. NaNs are treated as gaps.
pub fn lttb(xs: &[f64], ys: &[f64], threshold: usize) -> Vec<(f64, f64)> {
    let n = xs.len().min(ys.len());
    if n == 0 || threshold < 2 {
        return vec![];
    }
    if n <= threshold {
        return xs[..n]
            .iter()
            .copied()
            .zip(ys[..n].iter().copied())
            .collect();
    }

    // LTTB's triangle math is undefined across missing values. Split the
    // input into finite runs, budget samples proportionally, and preserve a
    // NaN separator between sampled runs so the renderer never bridges a gap.
    let mut runs = Vec::new();
    let mut start = None;
    for i in 0..n {
        let finite = xs[i].is_finite() && ys[i].is_finite();
        match (start, finite) {
            (None, true) => start = Some(i),
            (Some(run_start), false) => {
                runs.push((run_start, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(run_start) = start {
        runs.push((run_start, n));
    }
    if runs.is_empty() {
        return vec![];
    }
    if runs.len() == 1 {
        let (start, end) = runs[0];
        return lttb_finite(&xs[start..end], &ys[start..end], threshold);
    }

    let separators = runs.len() - 1;
    let minimum_points: usize = runs.iter().map(|(start, end)| (end - start).min(2)).sum();

    // Extremely fragmented data can contain more runs than the output budget
    // can represent. Keep evenly distributed runs (including both ends), one
    // representative each, rather than connecting them or exceeding the
    // caller's threshold.
    if minimum_points + separators > threshold {
        let keep = threshold.div_ceil(2).max(1).min(runs.len());
        let selected: Vec<usize> = if keep == 1 {
            let best = runs
                .iter()
                .enumerate()
                .max_by(|(_, (a0, a1)), (_, (b0, b1))| (a1 - a0).cmp(&(b1 - b0)))
                .map(|(index, _)| index)
                .unwrap_or(0);
            vec![best]
        } else {
            (0..keep)
                .map(|i| i * (runs.len() - 1) / (keep - 1))
                .collect()
        };
        let mut out = Vec::with_capacity(keep * 2 - 1);
        for (position, run_index) in selected.into_iter().enumerate() {
            let (start, end) = runs[run_index];
            if position > 0 {
                out.push((xs[start], f64::NAN));
            }
            let representative = (start..end)
                .max_by(|&a, &b| {
                    ys[a]
                        .abs()
                        .partial_cmp(&ys[b].abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or(start);
            out.push((xs[representative], ys[representative]));
        }
        return out;
    }

    let mut budgets: Vec<usize> = runs
        .iter()
        .map(|(start, end)| (end - start).min(2))
        .collect();
    let mut remaining = threshold - minimum_points - separators;
    while remaining > 0 {
        let Some((index, _)) = runs
            .iter()
            .enumerate()
            .filter(|(index, (start, end))| budgets[*index] < end - start)
            .max_by(|(ai, (a0, a1)), (bi, (b0, b1))| {
                let a_remaining = (a1 - a0) - budgets[*ai];
                let b_remaining = (b1 - b0) - budgets[*bi];
                a_remaining.cmp(&b_remaining)
            })
        else {
            break;
        };
        budgets[index] += 1;
        remaining -= 1;
    }

    let mut result = Vec::with_capacity(threshold);
    for (run_index, ((start, end), budget)) in runs.into_iter().zip(budgets).enumerate() {
        if run_index > 0 {
            result.push((xs[start], f64::NAN));
        }
        result.extend(lttb_finite(&xs[start..end], &ys[start..end], budget));
    }
    result
}

fn lttb_finite(xs: &[f64], ys: &[f64], threshold: usize) -> Vec<(f64, f64)> {
    let n = xs.len().min(ys.len());
    if n == 0 || threshold == 0 {
        return vec![];
    }
    if threshold == 1 {
        let index = (0..n)
            .max_by(|&a, &b| {
                ys[a]
                    .abs()
                    .partial_cmp(&ys[b].abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0);
        return vec![(xs[index], ys[index])];
    }
    if n <= threshold {
        return xs.iter().copied().zip(ys.iter().copied()).collect();
    }

    let mut result = Vec::with_capacity(threshold);
    let bucket_size = (n - 2) as f64 / (threshold - 2) as f64;

    result.push((xs[0], ys[0]));

    let mut a = 0usize;
    for i in 0..threshold - 2 {
        // The candidate bucket is [floor(i * every) + 1,
        // floor((i + 1) * every) + 1). The following bucket supplies the
        // centroid. Starting at (i + 1) skips the first interior bucket.
        let start = (i as f64 * bucket_size).floor() as usize + 1;
        let end = ((i as f64 + 1.0) * bucket_size).floor() as usize + 1;
        let end = end.min(n - 1);

        if start >= end || start >= n - 1 {
            break;
        }

        // Centroid of the *next* bucket only — [end, next_end) — not the whole
        // remaining tail. Averaging `end..n` distorts the selected point and
        // makes LTTB O(n·threshold); the next-bucket centroid is the standard
        // spec and keeps it O(n).
        let next_start = end;
        let next_end = (((i as f64 + 2.0) * bucket_size).floor() as usize + 1).min(n);
        let (avg_x, avg_y) = {
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            let mut cnt = 0usize;
            for j in next_start..next_end {
                if ys[j].is_finite() {
                    sum_x += xs[j];
                    sum_y += ys[j];
                    cnt += 1;
                }
            }
            if cnt > 0 {
                (sum_x / cnt as f64, sum_y / cnt as f64)
            } else {
                (xs[end], ys[end])
            }
        };

        let mut best_area = -1.0f64;
        let mut best_j = start;
        for j in start..end {
            if !ys[j].is_finite() || !ys[a].is_finite() {
                continue;
            }
            let area =
                ((xs[a] - avg_x) * (ys[j] - avg_y) - (xs[a] - xs[j]) * (ys[a] - avg_y)).abs() * 0.5;
            if area > best_area {
                best_area = area;
                best_j = j;
            }
        }
        result.push((xs[best_j], ys[best_j]));
        a = best_j;
    }

    result.push((xs[n - 1], ys[n - 1]));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_sparse() {
        let scale = LinearScale::new(0.0, 100.0, 0.0, 100.0);
        let xs = vec![0.0, 50.0, 100.0];
        let ys = vec![1.0, 2.0, 3.0];
        assert_eq!(
            min_max(&xs, &ys, &scale, 100.0),
            vec![(0.0, 1.0), (50.0, 2.0), (100.0, 3.0)]
        );
    }

    #[test]
    fn preserves_extremes_in_dense_data() {
        let scale = LinearScale::new(0.0, 10_000.0, 0.0, 10.0);
        let xs: Vec<f64> = (0..10_000).map(|i| i as f64).collect();
        let mut ys: Vec<f64> = xs.iter().map(|x| (x / 100.0).sin()).collect();
        ys[1234] = 99.0;
        ys[8765] = -99.0;
        let out = min_max(&xs, &ys, &scale, 10.0);
        assert!(out.len() < 100);
        assert!(out.iter().any(|(_, y)| *y == 99.0));
        assert!(out.iter().any(|(_, y)| *y == -99.0));
    }

    #[test]
    fn keeps_gap_markers() {
        let scale = LinearScale::new(0.0, 10_000.0, 0.0, 4.0);
        let xs: Vec<f64> = (0..10_000).map(|i| i as f64).collect();
        let mut ys: Vec<f64> = vec![1.0; 10_000];
        for y in ys[4000..5000].iter_mut() {
            *y = f64::NAN;
        }
        let out = min_max(&xs, &ys, &scale, 4.0);
        assert!(out.iter().any(|(_, y)| y.is_nan()));
    }

    #[test]
    fn min_max_does_not_repeat_single_column_points() {
        let scale = LinearScale::new(0.0, 100.0, 0.0, 1.0);
        let out = min_max(&[10.0, 20.0], &[1.0, 2.0], &scale, 0.25);
        assert_eq!(out, vec![(10.0, 1.0), (20.0, 2.0)]);
    }

    #[test]
    fn min_max_restarts_a_column_after_a_gap() {
        let scale = LinearScale::new(0.0, 100.0, 0.0, 1.0);
        let out = min_max(
            &[0.0, 1.0, 2.0, 3.0],
            &[1.0, f64::NAN, 99.0, 2.0],
            &scale,
            1.0,
        );
        assert!(out.iter().any(|(x, y)| *x == 2.0 && *y == 99.0));
        assert!(out.iter().any(|(x, y)| *x == 3.0 && *y == 2.0));
    }

    #[test]
    fn lttb_considers_the_first_interior_bucket() {
        let xs: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let mut ys = vec![0.0; 100];
        ys[1] = 100.0;
        let out = lttb(&xs, &ys, 10);
        assert_eq!(out.len(), 10);
        assert!(
            out.contains(&(1.0, 100.0)),
            "first bucket spike dropped: {out:?}"
        );
    }

    #[test]
    fn lttb_matches_an_independent_reference() {
        fn reference(xs: &[f64], ys: &[f64], threshold: usize) -> Vec<(f64, f64)> {
            let n = xs.len();
            let every = (n - 2) as f64 / (threshold - 2) as f64;
            let mut output = vec![(xs[0], ys[0])];
            let mut a = 0usize;
            for bucket in 0..threshold - 2 {
                let average_start = ((bucket + 1) as f64 * every).floor() as usize + 1;
                let average_end = (((bucket + 2) as f64 * every).floor() as usize + 1).min(n);
                let count = average_end.saturating_sub(average_start).max(1);
                let (avg_x, avg_y) = if average_start < average_end {
                    (
                        xs[average_start..average_end].iter().sum::<f64>() / count as f64,
                        ys[average_start..average_end].iter().sum::<f64>() / count as f64,
                    )
                } else {
                    (xs[n - 1], ys[n - 1])
                };
                let range_start = (bucket as f64 * every).floor() as usize + 1;
                let range_end = (((bucket + 1) as f64 * every).floor() as usize + 1).min(n - 1);
                let mut selected = range_start;
                let mut best = -1.0;
                for j in range_start..range_end {
                    let area = ((xs[a] - avg_x) * (ys[j] - avg_y)
                        - (xs[a] - xs[j]) * (ys[a] - avg_y))
                        .abs();
                    if area > best {
                        best = area;
                        selected = j;
                    }
                }
                output.push((xs[selected], ys[selected]));
                a = selected;
            }
            output.push((xs[n - 1], ys[n - 1]));
            output
        }

        let xs: Vec<f64> = (0..37).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs
            .iter()
            .enumerate()
            .map(|(i, x)| (x / 3.0).sin() * (i % 5 + 1) as f64)
            .collect();
        assert_eq!(lttb(&xs, &ys, 8), reference(&xs, &ys, 8));
    }

    #[test]
    fn lttb_returns_the_requested_count_for_finite_inputs() {
        for n in 3..80 {
            let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
            let ys: Vec<f64> = xs.iter().map(|x| (x * 0.37).sin()).collect();
            for threshold in 2..=n {
                assert_eq!(
                    lttb(&xs, &ys, threshold).len(),
                    threshold,
                    "n={n}, threshold={threshold}"
                );
            }
        }
    }

    #[test]
    fn lttb_reduces_large_input() {
        let xs: Vec<f64> = (0..1000).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| (x / 50.0).sin()).collect();
        let out = lttb(&xs, &ys, 50);
        assert!(out.len() <= 50);
        assert!((out[0].0 - 0.0).abs() < 1.0);
        assert!((out[out.len() - 1].0 - 999.0).abs() < 1.0);
    }

    #[test]
    fn lttb_passthrough_when_under_threshold() {
        let xs = vec![0.0, 1.0, 2.0];
        let ys = vec![10.0, 20.0, 30.0];
        let out = lttb(&xs, &ys, 10);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn lttb_empty() {
        assert!(lttb(&[], &[], 10).is_empty());
    }

    #[test]
    fn lttb_preserves_spike() {
        // Flat line with a single tall spike: the point maximizing triangle
        // area is the spike, so LTTB must retain it. (Regression guard for the
        // next-bucket-average fix — a correct local centroid still selects it.)
        let xs: Vec<f64> = (0..1000).map(|i| i as f64).collect();
        let mut ys = vec![0.0; 1000];
        ys[500] = 100.0;
        let out = lttb(&xs, &ys, 20);
        assert!(out.len() <= 20);
        assert!(
            out.iter().any(|(x, y)| *x == 500.0 && *y == 100.0),
            "LTTB dropped the spike: {out:?}"
        );
        // Endpoints are always preserved.
        assert_eq!(out.first().copied(), Some((0.0, 0.0)));
        assert_eq!(out.last().copied(), Some((999.0, 0.0)));
    }

    #[test]
    fn lttb_monotonic_ramp_stays_sorted() {
        // A plain ramp should come back sorted and within bounds — the
        // next-bucket centroid must never select out-of-range indices.
        let xs: Vec<f64> = (0..500).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.clone();
        let out = lttb(&xs, &ys, 40);
        assert!(out.len() <= 40);
        for pair in out.windows(2) {
            assert!(pair[1].0 > pair[0].0, "x not strictly increasing: {out:?}");
        }
    }

    #[test]
    fn lttb_preserves_gaps_without_exceeding_threshold() {
        let xs: Vec<f64> = (0..300).map(|i| i as f64).collect();
        let mut ys: Vec<f64> = xs.iter().map(|x| (x / 20.0).sin()).collect();
        for y in &mut ys[100..150] {
            *y = f64::NAN;
        }
        let out = lttb(&xs, &ys, 30);
        assert!(out.len() <= 30, "{} > 30: {out:?}", out.len());
        let gap = out
            .iter()
            .position(|(_, value)| value.is_nan())
            .expect("missing gap separator");
        assert!(out[..gap].iter().any(|(_, value)| value.is_finite()));
        assert!(out[gap + 1..].iter().any(|(_, value)| value.is_finite()));
    }

    #[test]
    fn lttb_fragmented_input_still_honors_budget() {
        let xs: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let ys: Vec<f64> = (0..100)
            .map(|i| if i % 2 == 0 { i as f64 } else { f64::NAN })
            .collect();
        let out = lttb(&xs, &ys, 15);
        assert!(out.len() <= 15);
        assert!(out.iter().any(|(_, value)| value.is_nan()));
    }
}
