//! Public API regressions reproduced independently during the 2026-09-05 review.
use graphtron::*;
fn s(xs: Vec<f64>, ys: Vec<f64>) -> SeriesData {
    SeriesData {
        xs,
        ys,
        ..Default::default()
    }
}
#[test]
fn m4_retains_post_gap_extrema() {
    let out = decimate::min_max(
        &[0., 1., 2., 3.],
        &[1., f64::NAN, 99., 2.],
        &LinearScale::new(0., 100., 0., 1.),
        1.,
    );
    assert!(out.iter().any(|p| p.1 == 99.), "lost spike: {out:?}");
}
#[test]
fn lttb_retains_first_bucket_spike() {
    let xs: Vec<_> = (0..100).map(|x| x as f64).collect();
    let mut ys = vec![0.; 100];
    ys[1] = 99.;
    let out = decimate::lttb(&xs, &ys, 10);
    assert!(out.iter().any(|p| p.1 == 99.), "lost spike: {out:?}");
}
#[test]
fn negative_zero_anchor() {
    let domain = y_domain(
        &[s(vec![0., 1.], vec![-10., -10.])],
        0.,
        1.,
        None,
        None,
        true,
    );
    assert!(
        domain.0 <= -10. && domain.1 >= 0.,
        "invalid zero domain: {domain:?}"
    );
}
#[test]
fn log_stack_domain() {
    let data = ChartData::Bars((0..20).map(|_| s(vec![0., 1.], vec![10., 10.])).collect());
    let (lo, hi) = data_y_domain_scaled(
        &data,
        0.,
        1.,
        None,
        None,
        true,
        SeriesLayout::Stacked,
        ScaleKind::Log,
    );
    let layout =
        ChartLayout::compute_scaled(400., 200., 0., 1., lo, hi, 0., 0., 0., 0., ScaleKind::Log);
    assert!(
        layout.y_domain().1 >= 200.,
        "stack top 200 clipped by {:?}",
        layout.y_domain()
    );
}
#[test]
fn step_hover_uses_preceding_value() {
    let layout = ChartLayout::compute(400., 200., 0., 10., 0., 100.);
    let data = ChartData::Step(vec![s(vec![0., 10.], vec![1., 100.])]);
    let hover = hit_test_chart(
        &layout,
        &ChartSpec::line(Unit::None),
        &data,
        layout.x_scale.to_px(6.),
        100.,
    )
    .unwrap();
    assert_eq!(hover.values[0].y, 1., "hover selects future step");
}
#[test]
fn log_hit_excludes_undrawable() {
    let layout = ChartLayout::compute_scaled(
        400.,
        200.,
        0.,
        10.,
        1.,
        100.,
        0.,
        0.,
        0.,
        0.,
        ScaleKind::Log,
    );
    let hover = hit_test(&layout, &[s(vec![0., 10.], vec![0., 100.])], 0.);
    assert!(hover.is_none(), "invalid hover geometry: {hover:?}");
}
#[test]
fn zoom_between_samples_keeps_crossing_segment() {
    let domain = y_domain(
        &[s(vec![0., 10.], vec![100., 200.])],
        4.,
        6.,
        None,
        None,
        false,
    );
    assert!(
        domain.0 <= 140. && domain.1 >= 160.,
        "crossing line invisible: {domain:?}"
    );
}
#[test]
fn smooth_curve_stays_inside_segment() {
    let ops = monotone_cubic(&[(0., 0.), (1., 1.), (100., 2.)], &[]);
    let PathOp::CurveTo(a, b, c, d, x, y) = ops[1] else {
        panic!()
    };
    let midx = 0.375 * a + 0.375 * c + 0.125 * x;
    let midy = 0.375 * b + 0.375 * d + 0.125 * y;
    assert!(
        (0. ..=1.).contains(&midx) && (0. ..=1.).contains(&midy),
        "curve midpoint outside first segment: ({midx}, {midy}), ops={ops:?}"
    );
}
#[test]
fn empty_histogram_hover_does_not_panic() {
    let layout = ChartLayout::compute(400., 180., 0., 100., 0., 10.);
    let data = ChartData::Histogram(vec![HistogramSeries::default()]);
    let result = hit_test_chart(
        &layout,
        &ChartSpec::histogram(Unit::None),
        &data,
        100.,
        100.,
    );
    assert!(result.is_none());
}
#[test]
fn sma_preserves_missing_sample() {
    let out = simple_moving_average(&[1., f64::NAN, 3.], 3);
    assert!(
        out[1].is_nan(),
        "smoothed across missing observation: {out:?}"
    );
}
