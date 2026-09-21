mod data;
mod rrd_page;

use data::{Dataset, generate};
use dioxus::prelude::*;
use graphtron::{
    ChartData, ChartSpec, HighlightConfig, HighlightMode, LegendConfig, LegendPosition,
    ReferenceLines, SeriesData, TrendConfig, Unit,
};
use graphtron_dioxus::{Cursor, GraphtronChart};

fn main() {
    dioxus::launch(App);
}

const STYLE: &str = r#"
    * { box-sizing: border-box; }
    body { margin: 0; background: #0a0e17; color: #d7dbe0;
           font-family: 'Inter', system-ui, sans-serif; }
    .wrap { max-width: 1100px; margin: 0 auto; padding: 24px; }
    h1 { font-size: 20px; margin: 0 0 4px; }
    h2 { font-size: 13px; text-transform: uppercase; letter-spacing: .05em;
         color: #5a6a7a; margin: 28px 0 10px; }
    .sub { color: #5a6a7a; font-size: 13px; margin: 0 0 8px; }
    .card { background: #111822; border: 1px solid #1e2a36; border-radius: 8px;
            padding: 12px; margin-bottom: 16px; }
    .card .title { font-weight: 600; font-size: 13px; margin-bottom: 8px; color: #00FFFF; }
    .card .hint { font-size: 11px; color: #4a5568; margin-bottom: 8px; }
    .grid { display: grid; gap: 16px; }
    .grid-3 { grid-template-columns: repeat(3, 1fr); }
    .grid-2 { grid-template-columns: repeat(2, 1fr); }
    @media (max-width: 720px) {
        .grid-2, .grid-3 { grid-template-columns: 1fr; }
        .wrap { padding: 12px; }
    }
    .density-controls { display: flex; flex-wrap: wrap; gap: 12px;
                        align-items: center; margin-bottom: 12px; }
    .hint { font-size: 12px; color: #4a5568; }
    .graphtron-legend { color: #5a6a7a; margin-top: 6px; }
    .stat { font-size: 34px; font-weight: 700; color: #00FFFF; }
    .gauge-track { height: 22px; background: #1e2a36; border-radius: 4px; overflow: hidden; }
    .gauge-fill { height: 100%; background: linear-gradient(90deg, #00FFFF, #FF00FF); }
    .row-between { display: flex; justify-content: space-between;
                   align-items: baseline; font-size: 12px; color: #5a6a7a; }
    .key { font-size: 11px; color: #5a6a7a; margin: 4px 0; }
    kbd { background: #1e2a36; padding: 2px 6px; border-radius: 3px;
          font-family: 'JetBrains Mono', monospace; font-size: 11px; }
"#;

#[component]
fn App() -> Element {
    let rrd_page = use_hook(|| {
        #[cfg(target_arch = "wasm32")]
        {
            let location = js_sys::Reflect::get(&js_sys::global(), &"location".into()).ok();
            location
                .and_then(|l| js_sys::Reflect::get(&l, &"pathname".into()).ok())
                .and_then(|p| p.as_string())
                .is_some_and(|p| p.trim_end_matches('/').ends_with("/rrdtool"))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            false
        }
    });
    let dataset = use_hook(generate);
    if rrd_page {
        return rsx! { rrd_page::RrdPage {} };
    }

    rsx! {
        style { {STYLE} }
        div { class: "wrap",
            h1 { "graphtron chart gallery" }
            p { a { href: "rrdtool", style: "color: #8ac7ff", "RRDtool style — static and interactive examples →" } }
            p { class: "sub",
                "Pure-Rust Canvas2D renderer, embedded via the <GraphtronChart> component. "
                "Shared cursor + zoom across the first row. \u{00B7} ",
                span { class: "key", kbd { "drag" } " to zoom \u{00B7} "
                    kbd { "Shift+drag" } " to pan \u{00B7} " kbd { "click" } " to freeze \u{00B7} "
                    kbd { "dblclick" } " to reset \u{00B7} "
                    kbd { "legend" } " to toggle series" }
            }
            SharedRow { dataset: dataset.clone() }
            NewChartTypesRow { dataset: dataset.clone() }
            DistributionWidgets {}
            AnalysisRow { dataset: dataset.clone() }
            StatisticsRow { dataset: dataset.clone() }
            DensityRow {}
            InspectionRow {}
            CompactRow { dataset }
        }
    }
}

/// Three charts sharing one cursor signal and one range signal — cross-panel
/// sync is just passing the same signals to each `GraphtronChart`.
#[component]
fn SharedRow(dataset: Dataset) -> Element {
    let default_range = (dataset.from_ms, dataset.to_ms);
    let range = use_signal(|| default_range);
    let cursor = use_signal(Cursor::default);

    let line = use_signal(|| ChartData::Lines(dataset.line.clone()));
    let area = use_signal(|| ChartData::Areas(dataset.area.clone()));
    let bars = use_signal(|| ChartData::Bars(dataset.bars.clone()));
    let annotations = dataset.annotations.clone();

    rsx! {
        h2 { "Shared cursor + zoom (click legend items to toggle)" }
        div { style: "display: grid; gap: 16px; margin-bottom: 4px;",
            div { class: "card",
                div { class: "title", "Multi-series line \u{2014} CPU per core (%)" }
                GraphtronChart {
                    spec: ChartSpec::line(Unit::Percent),
                    data: ReadSignal::from(line),
                    range, cursor, default_range,
                    annotations,
                    animate: true,
                    smooth_cursor: true,
                    legend: true,
                }
            }
            div { class: "card",
                div { class: "title", "Area \u{2014} memory utilization" }
                GraphtronChart {
                    spec: ChartSpec::area(Unit::Percent01),
                    data: ReadSignal::from(area),
                    range, cursor, default_range,
                    animate: true,
                    smooth_cursor: true,
                    legend: true,
                }
            }
            div { class: "card",
                div { class: "title", "Stacked bars \u{2014} log volume by severity" }
                GraphtronChart {
                    spec: ChartSpec::bars(Unit::Short),
                    data: ReadSignal::from(bars),
                    range, cursor, default_range,
                    height: 200.0,
                    legend: true,
                }
            }
        }
    }
}

#[component]
fn NewChartTypesRow(dataset: Dataset) -> Element {
    let ohlc = use_signal(|| ChartData::Ohlc(dataset.ohlc.clone()));
    let step = use_signal(|| ChartData::Step(dataset.step.clone()));
    let histogram = use_signal(|| ChartData::Histogram(dataset.histogram.clone()));
    let hbar = use_signal(|| ChartData::HBar(dataset.hbar.clone()));
    let band = use_signal(|| ChartData::Band(dataset.band.clone()));
    let states = use_signal(|| ChartData::StateTimeline(dataset.states.clone()));
    let heatmap = use_signal(|| ChartData::Heatmap(dataset.heatmap.clone()));

    rsx! {
        h2 { "More chart types" }
        div { class: "grid grid-2",
            div { class: "card",
                div { class: "title", "Band \u{2014} latency percentile envelope" }
                p { class: "card hint", "p50 center line with a shaded p10\u{2013}p90 band." }
                GraphtronChart {
                    spec: ChartSpec::band(Unit::Millis),
                    data: ReadSignal::from(band),
                }
            }
            div { class: "card",
                div { class: "title", "State timeline \u{2014} service health" }
                p { class: "card hint", "Colored segments per row: up / degraded / down." }
                GraphtronChart {
                    spec: ChartSpec::state_timeline(),
                    data: ReadSignal::from(states),
                }
            }
            div { class: "card",
                div { class: "title", "OHLC / Candlestick \u{2014} WYVRN price" }
                p { class: "card hint", "Green = close >= open, red = close < open. Range autofits to the data." }
                GraphtronChart {
                    spec: ChartSpec::ohlc(Unit::Short),
                    data: ReadSignal::from(ohlc),
                }
            }
            div { class: "card",
                div { class: "title", "Step chart \u{2014} deployment phases" }
                p { class: "card hint", "Horizontal-then-vertical step-after lines; dots appear when sparse." }
                GraphtronChart {
                    spec: ChartSpec::step(Unit::Short),
                    data: ReadSignal::from(step),
                }
            }
            div { class: "card",
                div { class: "title", "Histogram \u{2014} latency distribution" }
                p { class: "card hint", "Linear value x-axis derived from the bucket edges." }
                GraphtronChart {
                    spec: ChartSpec::histogram(Unit::Millis),
                    data: ReadSignal::from(histogram),
                    interactive: false,
                }
            }
            div { class: "card",
                div { class: "title", "Horizontal bars \u{2014} resource usage" }
                p { class: "card hint", "Value x-axis, per-category labels, and end-of-bar values." }
                GraphtronChart {
                    spec: ChartSpec {
                        last_value_label: true,
                        ..ChartSpec::hbar(Unit::Percent)
                    },
                    data: ReadSignal::from(hbar),
                    interactive: false,
                }
            }
            div { class: "card",
                div { class: "title", "Heatmap \u{2014} request rate per host" }
                p { class: "card hint",
                    "Cells are colored by value, so the legend is the color ramp with its domain on it."
                }
                GraphtronChart {
                    spec: ChartSpec {
                        legend: LegendConfig::shown(LegendPosition::Bottom),
                        ..ChartSpec::heatmap(Unit::Short)
                    },
                    data: ReadSignal::from(heatmap),
                }
            }
        }
    }
}

/// Axes and legends that do the reading for you: a log y axis for data
/// spanning decades, titled axes, and a legend that summarizes each series.
#[component]
fn DistributionWidgets() -> Element {
    let items = || {
        [42.0, 28.0, 18.0, 12.0]
            .into_iter()
            .enumerate()
            .map(|(i, value)| graphtron::PartitionItem {
                name: format!("Service {}", i + 1),
                value,
                color: None,
            })
            .collect()
    };
    let pie = use_signal(|| ChartData::Pie(items()));
    let treemap = use_signal(|| ChartData::Treemap(items()));
    let hosts = use_signal(|| {
        ChartData::HostMap(
            (0..48)
                .map(|i| graphtron::PartitionItem {
                    name: format!("host-{i:02}"),
                    value: ((i * 37) % 101) as f64,
                    color: None,
                })
                .collect(),
        )
    });
    let points = use_signal(|| {
        ChartData::Points(vec![SeriesData {
            name: "Samples".into(),
            xs: (0..30).map(|i| i as f64 * 1000.0).collect(),
            ys: (0..30)
                .map(|i| 40.0 + (i as f64 * 0.7).sin() * 20.0)
                .collect(),
            color: None,
        }])
    });
    let scatter = use_signal(|| {
        ChartData::Scatter(vec![SeriesData {
            name: "Requests".into(),
            xs: (0..120).map(|i| i as f64).collect(),
            ys: (0..120)
                .map(|i| i as f64 * 0.6 + ((i * 37) % 23) as f64)
                .collect(),
            color: None,
        }])
    });
    rsx! {
        h2 { "Distribution and infrastructure widgets" }
        div { class: "grid grid-2",
            div { class: "card",
                div { class: "title", "Pie Chart — allocation" }
                GraphtronChart { spec: ChartSpec::pie(Unit::Percent), data: ReadSignal::from(pie), legend: true }
            }
            div { class: "card",
                div { class: "title", "Treemap Widget — allocation" }
                GraphtronChart { spec: ChartSpec::treemap(Unit::Percent), data: ReadSignal::from(treemap), legend: true }
            }
            div { class: "card",
                div { class: "title", "Host Map Widget — CPU utilization" }
                p { class: "hint", "One tile per host; color represents 0–100% utilization. Hover for host details." }
                GraphtronChart { spec: ChartSpec::host_map(Unit::Percent), data: ReadSignal::from(hosts) }
            }
            div { class: "card",
                div { class: "title", "Point Plot — discrete samples" }
                GraphtronChart { spec: ChartSpec::point(Unit::Short), data: ReadSignal::from(points) }
            }
            div { class: "card",
                div { class: "title", "Scatter Plot Widget — load vs latency" }
                GraphtronChart {
                    spec: ChartSpec { x_axis: graphtron::XAxisKind::Linear { min: None, max: None },
                        x_label: Some("Load".into()), y_label: Some("Latency".into()),
                        ..ChartSpec::scatter(Unit::Millis) },
                    data: ReadSignal::from(scatter),
                }
            }
        }
    }
}

#[component]
fn AnalysisRow(dataset: Dataset) -> Element {
    let decades = use_signal(|| ChartData::Lines(dataset.decades.clone()));
    let linear = use_signal(|| ChartData::Lines(dataset.decades.clone()));

    rsx! {
        h2 { "Log axis, axis titles, and a stats legend" }
        div { class: "grid grid-2",
            div { class: "card",
                div { class: "title", "Log y axis \u{2014} latency percentiles" }
                p { class: "card hint",
                    "p50, p99, and p999 are three decades apart. Freeze the cursor with a click, then hover elsewhere to read each series' delta."
                }
                GraphtronChart {
                    spec: ChartSpec::line(Unit::Millis)
                        .with_log_y()
                        .with_axis_labels(Some("time"), Some("latency"))
                        .with_legend(LegendConfig::with_stats(LegendPosition::Top)),
                    data: ReadSignal::from(decades),
                    height: 260.0,
                }
            }
            div { class: "card",
                div { class: "title", "The same data on a linear y axis" }
                p { class: "card hint",
                    "p999 sets the domain, so p50 and p99 collapse onto the floor."
                }
                GraphtronChart {
                    spec: ChartSpec::line(Unit::Millis)
                        .with_axis_labels(Some("time"), Some("latency"))
                        .with_legend(LegendConfig::with_stats(LegendPosition::Top)),
                    data: ReadSignal::from(linear),
                    height: 260.0,
                }
            }
        }
    }
}

/// Statistics the chart works out for you: reference lines computed from the
/// visible window, a fitted trend, and callouts that name the extremes.
#[component]
fn StatisticsRow(dataset: Dataset) -> Element {
    let noisy = use_signal(|| ChartData::Lines(dataset.line.clone()));
    let trending = use_signal(|| ChartData::Lines(dataset.line.clone()));
    let extremes = use_signal(|| ChartData::Lines(dataset.line.clone()));

    rsx! {
        h2 { "Statistics: reference lines, trends, and extreme callouts" }
        div { class: "grid grid-3",
            div { class: "card",
                div { class: "title", "Mean and \u{00B1}1\u{03C3}" }
                p { class: "card hint",
                    "Recomputed from the visible window \u{2014} zoom and the band re-answers for the range on screen."
                }
                GraphtronChart {
                    spec: ChartSpec::line(Unit::Percent)
                        .with_reference_lines(ReferenceLines::normal_band()),
                    data: ReadSignal::from(noisy),
                }
            }
            div { class: "card",
                div { class: "title", "Fitted trend" }
                p { class: "card hint",
                    "A least-squares fit labeled with its per-hour rate and r\u{00B2}."
                }
                GraphtronChart {
                    spec: ChartSpec::line(Unit::Percent).with_trend(TrendConfig::linear()),
                    data: ReadSignal::from(trending),
                }
            }
            div { class: "card",
                div { class: "title", "Peak and trough callouts" }
                p { class: "card hint",
                    "HighlightMode::Extremes marks both ends of the visible range."
                }
                GraphtronChart {
                    spec: ChartSpec::line(Unit::Percent).with_highlights(HighlightConfig {
                        mode: HighlightMode::Extremes,
                        top_n: 2,
                        ..Default::default()
                    }),
                    data: ReadSignal::from(extremes),
                }
            }
        }
    }
}

#[component]
fn CompactRow(dataset: Dataset) -> Element {
    let spark_a = use_signal(|| ChartData::Lines(vec![dataset.line[0].clone()]));
    let spark_b = use_signal(|| ChartData::Lines(vec![dataset.area[0].clone()]));

    let cpu_last = last_value(&dataset.line[0]).unwrap_or(0.0);
    let mem_last = last_value(&dataset.area[0]).unwrap_or(0.0);

    rsx! {
        h2 { "Compact panels" }
        div { class: "grid grid-3",
            div { class: "card",
                div { class: "title", "Sparkline" }
                p { class: "hint", "Chrome-less line with neon glow, autofit range." }
                GraphtronChart {
                    spec: ChartSpec::sparkline(),
                    data: ReadSignal::from(spark_a),
                    interactive: false,
                    height: 60.0,
                }
                GraphtronChart {
                    spec: ChartSpec::sparkline(),
                    data: ReadSignal::from(spark_b),
                    interactive: false,
                    height: 60.0,
                }
            }
            div { class: "card",
                div { class: "title", "Stat" }
                p { class: "hint", "Single latest value (app-rendered)." }
                div { class: "stat", "{Unit::Percent.format(cpu_last)}" }
                div { class: "hint", "core 0 CPU, most recent sample" }
            }
            div { class: "card",
                div { class: "title", "Gauge" }
                p { class: "hint", "Latest value against a 0-100% range." }
                Gauge { value: mem_last * 100.0, min: 0.0, max: 100.0 }
            }
        }
    }
}

#[component]
fn Gauge(value: f64, min: f64, max: f64) -> Element {
    let pct = ((value - min) / (max - min) * 100.0).clamp(0.0, 100.0);
    rsx! {
        div { class: "row-between",
            span { "{Unit::Percent.format(min)}" }
            span { style: "color: #d7dbe0; font-weight: 600;", "{Unit::Percent.format(value)}" }
            span { "{Unit::Percent.format(max)}" }
        }
        div { class: "gauge-track",
            div { class: "gauge-fill", style: "width: {pct}%;" }
        }
    }
}

/// A fixed synthetic signal with isolated spikes and explicit missing intervals.
/// Switch geometry and zoom without regenerating random data between frames.
#[component]
fn InspectionRow() -> Element {
    let data = use_signal(|| {
        ChartData::Lines(
            (0..40)
                .map(|index| SeriesData {
                    name: format!("service-{index:02} / long metric name"),
                    xs: vec![0.0, 50.0, 100.0],
                    ys: vec![index as f64 + 1.0; 3],
                    color: None,
                })
                .collect(),
        )
    });
    let mut spec = ChartSpec::flat(Unit::None);
    spec.x_axis = graphtron::XAxisKind::Linear {
        min: None,
        max: None,
    };
    spec.highlights.show = false;
    rsx! {
        h2 { "Many-series inspection" }
        div { class: "card inspection-card",
            p { class: "hint", "Click to freeze a sample and inspect all 40 series, including values omitted from the compact tooltip. Escape clears the reference." }
            GraphtronChart {
                spec,
                data: ReadSignal::from(data),
                default_domain: (0.0, 100.0),
                height: 180.0,
                options: graphtron::RenderOptions::professional_dark(),
            }
        }
    }
}

#[component]
fn DensityRow() -> Element {
    let mut count = use_signal(|| 100_000usize);
    let mut kind = use_signal(|| "line".to_string());
    let mut domain = use_signal(|| (0.0, 99_999.0));
    let data = use_memo(move || {
        let n = count();
        let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let ys: Vec<f64> = (0..n)
            .map(|i| {
                if i % 10_000 >= 9_900 {
                    f64::NAN
                } else if i % 1_009 == 0 {
                    90.0
                } else {
                    30.0 + (i as f64 / 37.0).sin() * 15.0
                }
            })
            .collect();
        let series = SeriesData {
            name: "observations".into(),
            xs: xs.clone(),
            ys: ys.clone(),
            color: Some(0x5794f2),
        };
        match kind().as_str() {
            "scatter" => ChartData::Scatter(vec![series]),
            "area" => ChartData::Areas(vec![
                series.clone(),
                SeriesData {
                    name: "second series".into(),
                    ys: ys.iter().map(|y| y * 0.3).collect(),
                    color: Some(0x73bf69),
                    ..series
                },
            ]),
            "band" => ChartData::Band(vec![graphtron::BandSeries {
                name: "envelope".into(),
                xs,
                center: ys.clone(),
                lower: ys.iter().map(|y| y - 5.0).collect(),
                upper: ys.iter().map(|y| y + 5.0).collect(),
                color: Some(0x5794f2),
            }]),
            _ => ChartData::Lines(vec![series]),
        }
    });
    let mut spec = ChartSpec::flat(Unit::Short);
    spec.x_axis = graphtron::XAxisKind::Linear {
        min: None,
        max: None,
    };
    spec.points = graphtron::PointMarkers::Never;
    spec.highlights.show = false;
    rsx! {
        h2 { "Dense data inspection" }
        div { class: "card",
            div { class: "density-controls",
                label { "Samples "
                    select {
                        value: "{count}",
                        onchange: move |event| {
                            if let Ok(n) = event.value().parse::<usize>() {
                                count.set(n);
                                domain.set((0.0, n.saturating_sub(1) as f64));
                            }
                        },
                        option { value: "1000", "1,000" }
                        option { value: "100000", "100,000" }
                        option { value: "1000000", "1,000,000" }
                    }
                }
                label { "Chart "
                    select { value: "{kind}", onchange: move |event| kind.set(event.value()),
                        option { value: "line", "Line" }
                        option { value: "area", "Stacked area" }
                        option { value: "band", "Band" }
                        option { value: "scatter", "Scatter" }
                    }
                }
                button { onclick: move |_| {
                    let middle = count() as f64 / 2.0;
                    domain.set((middle, middle + 100.0));
                }, "Inspect 101 samples" }
                button { onclick: move |_| domain.set((0.0, count() as f64 - 1.0)), "Show all" }
            }
            p { class: "hint", "Zoom into isolated spikes and missing intervals; switch chart types to compare the same observations." }
            GraphtronChart {
                spec,
                data: ReadSignal::from(data),
                domain,
                default_domain: (0.0, count() as f64 - 1.0),
                options: graphtron::RenderOptions::professional_dark(),
                height: 240.0,
                legend: true,
            }
        }
    }
}

fn last_value(s: &SeriesData) -> Option<f64> {
    s.ys.iter().rev().find(|y| y.is_finite()).copied()
}
