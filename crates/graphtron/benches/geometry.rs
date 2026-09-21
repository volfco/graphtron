//! Criterion benches for the geometry core hot paths (T-3). The LTTB sizes
//! double as a regression guard for the O(n) fix — the pre-fix tail-averaging
//! implementation was O(n·threshold) and blows up visibly at 1M points.
//!
//! Run: `cargo bench -p graphtron`

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use graphtron::hit::{hit_test_chart_with_cache, prepare_histogram_hover};
use graphtron::{
    ChartData, ChartSpec, HistogramSeries, LinearScale, SeriesData, Unit, decimate, hit_test_chart,
    monotone_cubic, y_domain,
};

fn series(n: usize) -> (Vec<f64>, Vec<f64>) {
    let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let ys: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.01).sin() * 100.0).collect();
    (xs, ys)
}

fn bench_decimate(c: &mut Criterion) {
    let mut g = c.benchmark_group("decimate");
    for n in [1_000usize, 10_000, 100_000, 1_000_000] {
        let (xs, ys) = series(n);
        let scale = LinearScale::new(0.0, n as f64, 0.0, 800.0);
        g.bench_function(format!("min_max/{n}"), |b| {
            b.iter(|| decimate::min_max(black_box(&xs), black_box(&ys), &scale, 800.0))
        });
        g.bench_function(format!("lttb/{n}"), |b| {
            b.iter(|| decimate::lttb(black_box(&xs), black_box(&ys), 1_600))
        });
    }
    g.finish();
}

fn bench_curve(c: &mut Criterion) {
    let mut g = c.benchmark_group("curve");
    for n in [100usize, 1_000, 10_000] {
        let points: Vec<(f64, f64)> = (0..n)
            .map(|i| (i as f64, ((i as f64) * 0.05).sin() * 50.0))
            .collect();
        g.bench_function(format!("monotone_cubic/{n}"), |b| {
            b.iter(|| monotone_cubic(black_box(&points), &[]))
        });
    }
    g.finish();
}

fn bench_y_domain(c: &mut Criterion) {
    let mut g = c.benchmark_group("layout");
    for n in [1_000usize, 100_000] {
        let (xs, ys) = series(n);
        let s = SeriesData {
            name: "s".into(),
            xs,
            ys,
            color: None,
        };
        let series = vec![s];
        g.bench_function(format!("y_domain/{n}"), |b| {
            b.iter(|| y_domain(black_box(&series), 0.0, n as f64, None, None, false))
        });
    }
    g.finish();
}

fn bench_hover_bars(c: &mut Criterion) {
    let mut g = c.benchmark_group("hover/bars_fixed_viewport");
    let spec = ChartSpec::bars(Unit::None);
    for history in [1_000usize, 10_000, 100_000] {
        let from = history as f64 / 2.0;
        let layout = graphtron::ChartLayout::compute(
            800.0,
            240.0,
            from,
            from + 100.0,
            -100.0,
            history as f64 * 100.0,
        );
        let series: Vec<SeriesData> = (0..32)
            .map(|si| SeriesData {
                name: format!("series-{si}"),
                xs: (0..history).map(|i| i as f64).collect(),
                ys: (0..history).map(|i| ((i + si) as f64).sin()).collect(),
                color: None,
            })
            .collect();
        let data = ChartData::Bars(series);
        let mouse_x = layout.x_scale.to_px(from + 50.0);
        g.bench_function(format!("{history}"), |b| {
            b.iter(|| {
                let live = hit_test_chart(
                    black_box(&layout),
                    black_box(&spec),
                    black_box(&data),
                    black_box(mouse_x),
                    black_box(layout.plot.y),
                );
                let frozen = hit_test_chart(
                    black_box(&layout),
                    black_box(&spec),
                    black_box(&data),
                    black_box(mouse_x + 1.0),
                    black_box(layout.plot.y),
                );
                black_box((live, frozen))
            })
        });
    }
    g.finish();
}

fn bench_hover_cumulative_histograms(c: &mut Criterion) {
    let mut g = c.benchmark_group("hover/cumulative_hist_fixed_viewport");
    let mut spec = ChartSpec::histogram(Unit::None);
    spec.layout = graphtron::SeriesLayout::Stacked;
    for history in [1_000usize, 10_000, 100_000] {
        let from = history as f64 / 2.0;
        let layout = graphtron::ChartLayout::compute(
            800.0,
            240.0,
            from,
            from + 100.0,
            -100.0,
            history as f64 * 100.0,
        );
        let series: Vec<HistogramSeries> = (0..32)
            .map(|si| HistogramSeries {
                name: format!("hist-{si}"),
                buckets: (0..=history).map(|i| i as f64).collect(),
                counts: (0..history).map(|i| ((i + si) % 7) as f64).collect(),
                color: None,
                cumulative: true,
            })
            .collect();
        let data = ChartData::Histogram(series.clone());
        let cache = prepare_histogram_hover(&series);
        let mouse_x = layout.x_scale.to_px(from + 50.0);
        g.bench_function(format!("uncached/{history}"), |b| {
            b.iter(|| {
                let live = hit_test_chart(
                    black_box(&layout),
                    black_box(&spec),
                    black_box(&data),
                    black_box(mouse_x),
                    black_box(layout.plot.y),
                );
                let frozen = hit_test_chart(
                    black_box(&layout),
                    black_box(&spec),
                    black_box(&data),
                    black_box(mouse_x + 1.0),
                    black_box(layout.plot.y),
                );
                black_box((live, frozen))
            })
        });
        g.bench_function(format!("prepared_live_and_frozen/{history}"), |b| {
            b.iter(|| {
                let live = hit_test_chart_with_cache(
                    black_box(&layout),
                    black_box(&spec),
                    black_box(&data),
                    black_box(mouse_x),
                    black_box(layout.plot.y),
                    Some(black_box(&cache)),
                );
                let frozen = hit_test_chart_with_cache(
                    black_box(&layout),
                    black_box(&spec),
                    black_box(&data),
                    black_box(mouse_x + 1.0),
                    black_box(layout.plot.y),
                    Some(black_box(&cache)),
                );
                black_box((live, frozen))
            })
        });
    }
    g.finish();
}

criterion_group!(
    benches,
    bench_decimate,
    bench_curve,
    bench_y_domain,
    bench_hover_bars,
    bench_hover_cumulative_histograms
);
criterion_main!(benches);
