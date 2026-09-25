//! Deterministic fixtures for the Canvas2D browser regression suite.
use graphtron::*;
use wasm_bindgen::prelude::*;
fn s(xs: Vec<f64>, ys: Vec<f64>) -> SeriesData {
    SeriesData {
        name: "sample".into(),
        xs,
        ys,
        color: Some(0xff0000),
    }
}
#[wasm_bindgen]
pub fn render(
    canvas: web_sys::HtmlCanvasElement,
    case: &str,
    w: f64,
    h: f64,
    n: usize,
) -> Vec<f64> {
    let full = case.ends_with("_full");
    let case = case.strip_suffix("_full").unwrap_or(case);
    let mut surface = CanvasSurface::new(canvas).unwrap();
    surface.sync_size(w, h);
    let mut spec = ChartSpec::line(Unit::None);
    spec.chrome = false;
    spec.smooth = false;
    spec.points = PointMarkers::Never;
    spec.highlights.show = false;
    spec.y_min = Some(0.0);
    spec.y_max = Some(10.0);
    let mut from = 0.0;
    let mut to = 100.0;
    let data = match case {
        "pie" | "treemap" | "hosts" => {
            let items = [10.0, 30.0, 60.0]
                .into_iter()
                .enumerate()
                .map(|(i, value)| PartitionItem {
                    name: format!("node-{i}"),
                    value,
                    color: None,
                })
                .collect();
            match case {
                "pie" => ChartData::Pie(items),
                "treemap" => ChartData::Treemap(items),
                _ => ChartData::HostMap(items),
            }
        }
        "point" => ChartData::Points(vec![s(vec![10.0, 50.0, 90.0], vec![2.0, 4.0, 6.0])]),
        "scatter_cloud" => ChartData::Scatter(vec![s(
            vec![50.0; 1000],
            (0..1000).map(|i| i as f64 / 100.0).collect(),
        )]),
        "grid_only" => {
            spec.chrome = true;
            spec.empty_message = None;
            spec.x_axis = XAxisKind::Linear {
                min: None,
                max: None,
            };
            ChartData::Lines(vec![])
        }
        "hist_ragged" => {
            spec.layout = SeriesLayout::Stacked;
            spec.x_axis = XAxisKind::Linear {
                min: None,
                max: None,
            };
            ChartData::Histogram(vec![
                HistogramSeries {
                    name: "short".into(),
                    buckets: vec![0., 1.],
                    counts: vec![1.],
                    ..Default::default()
                },
                HistogramSeries {
                    name: "long".into(),
                    buckets: vec![0., 1., 2.],
                    counts: vec![1., 2.],
                    ..Default::default()
                },
            ])
        }
        "tiny_labels" => {
            spec.chrome = true;
            spec.last_value_label = true;
            ChartData::Lines(vec![s(vec![10., 90.], vec![2., 2.])])
        }
        "decorations" => {
            spec.chrome = true;
            spec.last_value_label = true;
            spec.x_label = Some("time".into());
            spec.y_label = Some("value".into());
            spec.legend = LegendConfig::shown(LegendPosition::Bottom);
            ChartData::Lines(
                (0..n)
                    .map(|i| {
                        let mut v = s(vec![10., 90.], vec![2. + i as f64 * 0.01; 2]);
                        v.name = format!("series-{i}");
                        v
                    })
                    .collect(),
            )
        }
        "hist" => {
            spec = ChartSpec::histogram(Unit::None);
            spec.highlights.show = false;
            ChartData::Histogram(vec![HistogramSeries {
                buckets: vec![0., 1., 2.],
                counts: vec![2., 3.],
                ..Default::default()
            }])
        }
        "hbar" => {
            spec = ChartSpec::hbar(Unit::None);
            spec.highlights.show = false;
            ChartData::HBar(vec![HBarSeries {
                categories: vec!["one".into(), "two".into()],
                values: vec![-2., 3.],
                ..Default::default()
            }])
        }
        "ohlc" => ChartData::Ohlc(vec![OhlcSeriesData {
            ticks: vec![
                OhlcTick {
                    ts: 20.,
                    open: 2.,
                    high: 8.,
                    low: 1.,
                    close: 5.,
                },
                OhlcTick {
                    ts: 80.,
                    open: 5.,
                    high: 9.,
                    low: 2.,
                    close: 3.,
                },
            ],
            ..Default::default()
        }]),
        "state" => ChartData::StateTimeline(vec![StateTimelineSeries {
            name: "state".into(),
            segments: vec![
                StateSegment {
                    start_ms: 0.,
                    end_ms: 50.,
                    label: "up".into(),
                    color: 0x008800,
                },
                StateSegment {
                    start_ms: 50.,
                    end_ms: 100.,
                    label: "down".into(),
                    color: 0xff0000,
                },
            ],
        }]),
        "band" => ChartData::Band(vec![BandSeries {
            xs: vec![10., 50., 90.],
            center: vec![3.; 3],
            lower: vec![1.; 3],
            upper: vec![5.; 3],
            ..Default::default()
        }]),
        "area_markers" => {
            spec.points = PointMarkers::Always;
            spec.layout = SeriesLayout::Grouped;
            ChartData::Areas(vec![s(vec![10., 50., 90.], vec![2., 2., 2.])])
        }
        "area" => {
            spec.layout = SeriesLayout::Grouped;
            ChartData::Areas(vec![s(vec![10., 50., 90.], vec![2., 2., 2.])])
        }
        "line" => ChartData::Lines(vec![s(vec![10., 50., 90.], vec![2., 2., 2.])]),
        "singleton" => ChartData::Lines(vec![s(vec![50.], vec![5.])]),
        "band_short" => ChartData::Band(vec![BandSeries {
            name: "band".into(),
            xs: vec![10., 90.],
            center: vec![5.],
            lower: vec![2., 2.],
            upper: vec![8., 8.],
            color: None,
        }]),
        "heat" => ChartData::Heatmap(
            (0..n)
                .map(|i| {
                    let mut v = s(vec![50.], vec![if i < n / 2 { 0. } else { 10. }]);
                    v.name = format!("row{i}");
                    v
                })
                .collect(),
        ),
        "logstep" => {
            spec.y_scale = ScaleKind::Log;
            spec.y_min = Some(1.);
            spec.y_max = Some(100.);
            ChartData::Step(vec![s(vec![10., 30., 70., 90.], vec![10., 0., 0., 10.])])
        }
        "axis" => {
            spec.chrome = true;
            spec.y_min = Some(100000000.);
            spec.y_max = Some(200000000.);
            ChartData::Lines(vec![s(vec![10., 90.], vec![100000000., 200000000.])])
        }
        "logaxis" => {
            spec.chrome = true;
            spec.y_scale = ScaleKind::Log;
            spec.y_min = Some(1.);
            spec.y_max = Some(1000.);
            ChartData::Lines(vec![s(vec![10., 90.], vec![1., 1000.])])
        }
        "scatter" | "stack" | "denseband" | "denseline" => {
            from = if full { 0. } else { (n / 2) as f64 };
            to = if full { n as f64 } else { from + 100. };
            let xs: Vec<_> = (0..n).map(|i| i as f64).collect();
            let ys = vec![2.; n];
            match case {
                "scatter" => ChartData::Scatter(vec![s(xs, ys)]),
                "stack" => ChartData::Areas(vec![s(xs.clone(), ys.clone()), s(xs, ys)]),
                "denseband" => ChartData::Band(vec![BandSeries {
                    name: "band".into(),
                    xs,
                    center: ys,
                    lower: vec![1.; n],
                    upper: vec![3.; n],
                    color: None,
                }]),
                _ => ChartData::Lines(vec![s(xs, ys)]),
            }
        }
        _ => ChartData::Lines(vec![]),
    };
    let opts = RenderOptions {
        theme: Theme::light(),
        aesthetic: Aesthetic::clean(),
        ..Default::default()
    };
    let layout = draw(&surface, &spec, &data, from, to, &opts);
    let info = hit_test_chart(
        &layout,
        &spec,
        &data,
        layout.plot.x + layout.plot.w / 2.,
        layout.plot.y + layout.plot.h * 0.75,
    );
    let mut result = vec![
        layout.plot.x,
        layout.plot.y,
        layout.plot.w,
        layout.plot.h,
        layout.y_domain().0,
        layout.y_domain().1,
    ];
    if let Some(info) = info
        && let Some(value) = info.values.first()
    {
        result.extend([value.series as f64, value.y, value.px, value.py]);
    }
    result
}

#[wasm_bindgen]
pub fn tooltip(canvas: web_sys::HtmlCanvasElement, w: f64, h: f64, n: usize) {
    let mut surface = CanvasSurface::new(canvas).unwrap();
    surface.sync_size(w, h);
    let layout = ChartLayout::compute(w, h, 0., 100., 0., 10.);
    let info = HoverInfo {
        ts: 50.,
        header: Some("50".into()),
        values: (0..n)
            .map(|i| {
                HoverValue::new(
                    i,
                    "a very long series label that exceeds tooltip width by a lot".into(),
                    2.,
                    200.,
                    50.,
                )
            })
            .collect(),
    };
    graphtron::draw_tooltip_limited(
        &surface,
        &layout,
        &info,
        &[],
        Unit::None,
        200.,
        50.,
        &Theme::light(),
        10,
        true,
    );
}
#[wasm_bindgen]
pub fn overlay(canvas: web_sys::HtmlCanvasElement, n: usize) {
    let mut surface = CanvasSurface::new(canvas).unwrap();
    surface.sync_size(400., 180.);
    let layout = ChartLayout::compute(400., 180., 0., n as f64, 0., 10.);
    let data = ChartData::Bars(vec![s((0..n).map(|i| i as f64).collect(), vec![2.; n])]);
    let cursor = CursorOverlay::at(Some(n as f64 / 2.), None, Unit::None);
    draw_overlay(
        &surface,
        &layout,
        &data,
        &cursor,
        None,
        None,
        &[],
        &RenderOptions::professional_light(),
    );
}

/// Overlay-only fixture, allowing tests to inspect guides independently of data.
#[wasm_bindgen]
pub fn hover_fixture(canvas: web_sys::HtmlCanvasElement, case: &str) -> Vec<f64> {
    let mut surface = CanvasSurface::new(canvas).unwrap();
    surface.sync_size(400.0, 180.0);
    let layout = ChartLayout::compute(400.0, 180.0, 0.0, 100.0, 0.0, 100.0);
    let (data, spec) = match case {
        "pie_hover" => (
            ChartData::Pie(vec![PartitionItem {
                name: "Slice".into(),
                value: 1.0,
                color: None,
            }]),
            ChartSpec::pie(Unit::None),
        ),
        "state_hover" => (
            ChartData::StateTimeline(vec![StateTimelineSeries {
                name: "host".into(),
                segments: vec![StateSegment {
                    start_ms: 0.0,
                    end_ms: 100.0,
                    label: "up".into(),
                    color: 0x00ff00,
                }],
            }]),
            ChartSpec::state_timeline(),
        ),
        "ohlc_hover" => (
            ChartData::Ohlc(vec![OhlcSeriesData {
                name: "price".into(),
                ticks: vec![OhlcTick {
                    ts: 50.0,
                    open: 30.0,
                    close: 60.0,
                    low: 10.0,
                    high: 90.0,
                }],
                color: None,
            }]),
            ChartSpec::ohlc(Unit::None),
        ),
        _ => (
            ChartData::Lines(vec![s(vec![50.0], vec![40.0])]),
            ChartSpec::line(Unit::None),
        ),
    };
    let x = layout.x_scale.to_px(50.0);
    let y = layout.y_px(50.0);
    let info = hit_test_chart(&layout, &spec, &data, x, y);
    let mut opts = RenderOptions::professional_light();
    opts.overlay.show_tooltip = case == "pie_hover";
    opts.overlay.show_vertical_guide = false;
    opts.overlay.show_cursor_value = case == "delta_hover";
    let cursor = CursorOverlay::at(Some(50.0), None, Unit::None).with_pointer_row(Some(y));
    draw_overlay(
        &surface,
        &layout,
        &data,
        &cursor,
        info.as_ref(),
        None,
        &[],
        &opts,
    );
    vec![
        layout.plot.x,
        layout.plot.y,
        layout.plot.w,
        layout.plot.h,
        layout.y_px(30.0),
        layout.y_px(60.0),
        layout.y_px(10.0),
        layout.y_px(90.0),
    ]
}
