//! Property-style tests for the geometry core (T-2). Uses a seeded LCG for
//! reproducible "random" inputs — no proptest dependency, same guarantees for
//! the invariants that matter.

use graphtron::ticks::linear_ticks;
use graphtron::{
    ChartLayout, LinearScale, PathOp, ScaleKind, SeriesData, decimate, hit::nearest_index,
    monotone_cubic, y_domain,
};

struct Lcg(u64);

impl Lcg {
    fn next_unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64) / (1u64 << 31) as f64
    }

    fn in_range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_unit() * (hi - lo)
    }
}

#[test]
fn scale_roundtrips_within_tolerance() {
    let mut rng = Lcg(1);
    for _ in 0..200 {
        let d0 = rng.in_range(-1e9, 1e9);
        let d1 = d0 + rng.in_range(1.0, 1e9);
        let r0 = rng.in_range(0.0, 100.0);
        let r1 = r0 + rng.in_range(10.0, 2000.0);
        let scale = LinearScale::new(d0, d1, r0, r1);
        for _ in 0..20 {
            let x = rng.in_range(d0, d1);
            let rt = scale.from_px(scale.to_px(x));
            let tol = (d1 - d0) * 1e-9;
            assert!((rt - x).abs() <= tol.max(1e-6), "roundtrip {x} -> {rt}");
        }
    }
}

#[test]
fn min_max_never_drops_global_extremes() {
    let mut rng = Lcg(2);
    for round in 0..50 {
        let n = 500 + (round * 137) % 5000;
        let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let ys: Vec<f64> = (0..n).map(|_| rng.in_range(-100.0, 100.0)).collect();
        let global_min = ys.iter().copied().fold(f64::INFINITY, f64::min);
        let global_max = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        let scale = LinearScale::new(0.0, n as f64, 0.0, 100.0);
        let out = decimate::min_max(&xs, &ys, &scale, 100.0);
        assert!(
            out.iter().any(|(_, y)| *y == global_min),
            "round {round}: global min dropped"
        );
        assert!(
            out.iter().any(|(_, y)| *y == global_max),
            "round {round}: global max dropped"
        );
    }
}

#[test]
fn lttb_respects_threshold_and_endpoints() {
    let mut rng = Lcg(3);
    for round in 0..50 {
        let n = 100 + (round * 211) % 3000;
        let threshold = 10 + (round * 7) % 200;
        let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let ys: Vec<f64> = (0..n).map(|_| rng.in_range(0.0, 1000.0)).collect();
        let out = decimate::lttb(&xs, &ys, threshold);
        assert!(
            out.len() <= threshold.max(n),
            "round {round}: len {} > threshold {threshold}",
            out.len()
        );
        if n > threshold {
            assert!(out.len() <= threshold, "round {round}");
            assert_eq!(out.first().map(|p| p.0), Some(0.0), "first point dropped");
            assert_eq!(
                out.last().map(|p| p.0),
                Some((n - 1) as f64),
                "last point dropped"
            );
        }
        // x strictly increasing — LTTB must preserve order.
        for pair in out.windows(2) {
            assert!(pair[1].0 > pair[0].0, "round {round}: unsorted output");
        }
    }
}

#[test]
fn nearest_index_is_actually_nearest() {
    let mut rng = Lcg(4);
    for _ in 0..50 {
        let n = 100;
        let mut xs: Vec<f64> = (0..n).map(|_| rng.in_range(0.0, 10_000.0)).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for _ in 0..20 {
            let q = rng.in_range(-100.0, 10_100.0);
            let idx = nearest_index(&xs, q).unwrap();
            let best = xs
                .iter()
                .map(|x| (x - q).abs())
                .fold(f64::INFINITY, f64::min);
            assert!(
                ((xs[idx] - q).abs() - best).abs() < 1e-9,
                "nearest_index({q}) -> {idx} (dist {}), best dist {best}",
                (xs[idx] - q).abs()
            );
        }
    }
}

#[test]
fn y_domain_invariants() {
    let mut rng = Lcg(5);
    for _ in 0..100 {
        let n = 50;
        let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let ys: Vec<f64> = (0..n).map(|_| rng.in_range(-500.0, 500.0)).collect();
        let s = SeriesData {
            name: "s".into(),
            xs,
            ys,
            color: None,
        };
        let (lo, hi) = y_domain(std::slice::from_ref(&s), 0.0, n as f64, None, None, false);
        assert!(lo < hi, "empty-width domain");
        let (zlo, _) = y_domain(&[s], 0.0, n as f64, None, None, true);
        assert!(zlo <= 0.0, "zero-anchored domain must include 0");
    }
}

#[test]
fn linear_ticks_sorted_and_bounded() {
    let mut rng = Lcg(6);
    for _ in 0..100 {
        let min = rng.in_range(-1e6, 1e6);
        let max = min + rng.in_range(0.001, 1e6);
        let ticks = linear_ticks(min, max, 5);
        assert!(!ticks.is_empty());
        for pair in ticks.windows(2) {
            assert!(pair[1] > pair[0], "unsorted ticks");
        }
        // Ticks stay within the (tolerance-padded) range.
        let span = max - min;
        for t in &ticks {
            assert!(
                *t >= min - span * 1e-6 && *t <= max + span * 1e-6,
                "tick {t} outside [{min}, {max}]"
            );
        }
    }
}

#[test]
fn monotone_cubic_starts_and_ends_on_data() {
    let mut rng = Lcg(7);
    for _ in 0..50 {
        let n = 3 + (rng.next_unit() * 40.0) as usize;
        let points: Vec<(f64, f64)> = (0..n)
            .map(|i| (i as f64 * 10.0, rng.in_range(0.0, 100.0)))
            .collect();
        let ops = monotone_cubic(&points, &[]);
        assert!(
            matches!(ops.first(), Some(PathOp::MoveTo(x, y)) if *x == points[0].0 && *y == points[0].1)
        );
        // Every curve segment ends exactly on a data point.
        let mut ends: Vec<(f64, f64)> = vec![];
        for op in &ops {
            match op {
                PathOp::CurveTo(.., x, y) | PathOp::LineTo(x, y) => ends.push((*x, *y)),
                PathOp::MoveTo(..) => {}
            }
        }
        assert_eq!(ends.last().copied(), points.last().copied());
    }
}

#[test]
fn log_axis_roundtrips_and_preserves_order() {
    let mut rng = Lcg(11);
    for _ in 0..200 {
        // Domains spanning up to nine decades — real latency/throughput data.
        let lo = 10f64.powf(rng.in_range(-6.0, 3.0));
        let hi = lo * 10f64.powf(rng.in_range(0.5, 6.0));
        let layout = ChartLayout::compute_scaled(
            800.0,
            400.0,
            0.0,
            1000.0,
            lo,
            hi,
            50.0,
            10.0,
            10.0,
            20.0,
            ScaleKind::Log,
        );
        let (d0, d1) = layout.y_domain();
        assert!(d0 > 0.0 && d1 > d0, "{d0}..{d1}");

        let mut previous_px = f64::INFINITY;
        for step in 0..=20 {
            // Walk the domain geometrically so samples land evenly on the axis.
            let value = d0 * (d1 / d0).powf(step as f64 / 20.0);
            let px = layout.y_px(value);
            assert!(px.is_finite(), "{value} -> {px}");
            // Canvas y grows downward: larger values sit at smaller pixels.
            assert!(px <= previous_px + 1e-6, "not monotonic at {value}");
            previous_px = px;

            let roundtrip = layout.y_at(px);
            let relative = (roundtrip - value).abs() / value.max(1e-12);
            assert!(relative < 1e-6, "roundtrip {value} -> {roundtrip}");
        }
    }
}

#[test]
fn log_ticks_stay_sorted_inside_the_domain() {
    let mut rng = Lcg(13);
    for _ in 0..200 {
        let lo = 10f64.powf(rng.in_range(-6.0, 3.0));
        let hi = lo * 10f64.powf(rng.in_range(0.1, 8.0));
        let ticks = graphtron::log_ticks(lo, hi, 6);
        assert!(ticks.windows(2).all(|w| w[1] > w[0]), "{ticks:?}");
        assert!(
            ticks
                .iter()
                .all(|t| *t >= lo * (1.0 - 1e-9) && *t <= hi * (1.0 + 1e-9)),
            "{ticks:?} outside {lo}..{hi}"
        );
    }
}
