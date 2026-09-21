use dioxus::prelude::*;
use graphtron::{ChartData, ChartSpec, RenderOptions, SeriesData, SeriesLayout, Unit};
use graphtron_dioxus::{Cursor, GraphtronChart};

const START: i64 = 1_783_036_800_000; // 2026-07-03 00:00 UTC
const END: i64 = START + 86_400_000;
const CSS: &str = r#"
* { box-sizing: border-box; }
body { margin: 0; background: #fff; color: #252525; font-family: system-ui, sans-serif; }
.rrd-page { max-width: 824px; margin: 0 auto; padding: 28px 24px 56px; }
.rrd-page a { color: #783333; }
.rrd-page h1 { font-size: 26px; margin: 24px 0 10px; font-weight: 650; }
.rrd-page h2 { font-size: 20px; margin: 36px 0 8px; border-top: 1px solid #ccc; padding-top: 22px; }
.rrd-page p { font-size: 14px; line-height: 1.65; color: #555; }
.rrd-page code { font-size: 12px; }
.rrd-scroll { overflow-x: auto; margin: 18px 0 8px; padding-bottom: 4px; }
.rrd-image { width: 760px; }
.rrd-page figure { margin: 24px 0; }
.rrd-page figcaption { font-size: 12px; color: #666; margin-top: 8px; }
.rrd-controls { display: flex; flex-wrap: wrap; gap: 8px; margin: 12px 0; }
.rrd-page button { font: 12px 'DejaVu Sans Mono', monospace; padding: 5px 9px; background: #f2f2f2;
    color: #222; border: 1px solid #aaa; border-radius: 0; cursor: pointer; }
.rrd-page button:hover { background: #e3e3e3; }
.rrd-page button:focus-visible, .rrd-page a:focus-visible { outline: 2px solid #801f1f; outline-offset: 3px; }
.rrd-page .graphtron-legend { font: 11px 'DejaVu Sans Mono', monospace; color: #333; }
.rrd-page footer { border-top: 1px solid #ccc; margin-top: 32px; padding-top: 12px; }
body:has(.rrd-dark) { background: #101010; color: #eee; color-scheme: dark; }
.rrd-dark p, .rrd-dark figcaption, .rrd-dark .graphtron-legend { color: #bbb; }
.rrd-dark a { color: #66ddff; }
.rrd-dark button { background: #222; color: #eee; border-color: #666; }
.rrd-dark button:hover { background: #333; }
.rrd-dark h2, .rrd-dark footer { border-color: #555; }
.rrd-dark button:focus-visible, .rrd-dark a:focus-visible { outline-color: #00ccff; }
@media(max-width: 600px) { .rrd-page { padding: 16px 12px 36px; } }
"#;

fn datasets(kind: usize) -> Vec<SeriesData> {
    let names: &[(&str, u32)] = match kind {
        0 => &[("Inbound", 0x00cc00), ("Outbound", 0x0000ff)],
        1 => &[
            ("User", 0x00cc00),
            ("System", 0x0000ff),
            ("I/O wait", 0xffcc00),
        ],
        _ => &[
            ("1 minute", 0x00b000),
            ("5 minutes", 0x0000ff),
            ("15 minutes", 0xff0000),
        ],
    };
    names
        .iter()
        .enumerate()
        .map(|(si, &(name, color))| {
            let xs: Vec<_> = (0..=288).map(|i| (START + i * 300_000) as f64).collect();
            let mut rng = 0x5eed_u32.wrapping_add(si as u32 * 7919);
            let ys = (0..=288)
                .map(|i| {
                    let t = i as f64 / 288.0;
                    let day = ((t - 0.25) * std::f64::consts::TAU).sin().max(0.0);
                    rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let noise = (rng as f64 / u32::MAX as f64 - 0.5).abs();
                    let pulse = if (165..178).contains(&i) { 1.0 } else { 0.0 };
                    match kind {
                        0 => {
                            if si == 0 {
                                (0.6 + day * 5.2 + noise * 2.0 + pulse * 2.0) * 1e6
                            } else {
                                (0.25 + day * 1.7 + noise * 0.8) * 1e6
                            }
                        }
                        1 => match si {
                            0 => 8.0 + day * 32.0 + noise * 12.0,
                            1 => 4.0 + day * 11.0 + noise * 6.0,
                            _ => 1.0 + noise * 4.0 + pulse * 13.0,
                        },
                        _ => {
                            0.12 + day * (1.5 - si as f64 * 0.2)
                                + noise * (0.7 / (si + 1) as f64)
                                + pulse * (0.7 / (si + 1) as f64)
                        }
                    }
                })
                .collect();
            SeriesData {
                name: name.into(),
                color: Some(color),
                xs,
                ys,
            }
        })
        .collect()
}

fn spec(kind: usize) -> ChartSpec {
    ChartSpec {
        unit: if kind == 1 {
            Unit::Percent
        } else {
            Unit::Short
        },
        y_label: Some(
            match kind {
                0 => "bits per second",
                1 => "CPU utilization (%)",
                _ => "processes in run queue",
            }
            .into(),
        ),
        zero_anchored: true,
        y_min: Some(0.0),
        y_max: Some(match kind {
            0 => 10e6,
            1 => 100.0,
            _ => 3.0,
        }),
        layout: if kind == 1 {
            SeriesLayout::Stacked
        } else {
            SeriesLayout::Grouped
        },
        ..ChartSpec::default()
    }
}

fn options(kind: usize, dark: bool) -> RenderOptions {
    let mut options = if dark {
        RenderOptions::rrdtool_dark()
    } else {
        RenderOptions::rrdtool()
    };
    let style = options.rrdtool.as_mut().unwrap();
    style.title = match kind {
        0 => "router01 / eth0 — Daily traffic",
        1 => "server01 — CPU usage",
        _ => "server01 — Load average",
    }
    .into();
    if kind == 0 {
        style.line_series = vec![1];
    }
    options
}

#[component]
fn Example(
    kind: usize,
    interactive: bool,
    dark: bool,
    range: Signal<(i64, i64)>,
    cursor: Signal<Cursor>,
) -> Element {
    let data = use_memo(use_reactive((&kind, &dark), move |(kind, dark)| {
        let mut series = datasets(kind);
        if dark {
            // The reference's white/cyan/blue spectrum, kept explicit so
            // application-supplied series colors are never silently changed.
            let colors = if kind == 1 {
                [0x0066ff, 0x00ccff, 0xccffff]
            } else {
                [0x00ccff, 0xffffff, 0x3366ff]
            };
            for (i, s) in series.iter_mut().enumerate() {
                s.color = Some(colors[i]);
            }
        }
        if kind == 2 {
            ChartData::Lines(series)
        } else {
            ChartData::Areas(series)
        }
    }));
    let class = if interactive {
        "rrd-interactive"
    } else {
        "rrd-static"
    };
    let id = format!("{class}-{kind}");
    rsx! {
        figure { class, id: id.clone(),
            div { class: "rrd-scroll",
                div { class: "rrd-image",
                    GraphtronChart { spec: spec(kind), data: ReadSignal::from(data), options: options(kind, dark),
                        height: if kind == 0 { 286.0 } else { 300.0 }, interactive,
                        range, cursor, default_range: (START, END), legend: interactive,
                    }
                }
            }
            figcaption {
                match kind {
                    0 => if dark { "Cyan AREA for inbound traffic, white LINE for outbound; decimal SI statistics." } else { "Green AREA for inbound traffic, blue LINE for outbound; decimal SI statistics." },
                    1 => "Opaque stacked AREA traces for user, system and I/O wait, on a fixed 0–100% scale.",
                    _ => "Three one-pixel LINE traces for the 1, 5 and 15 minute load averages."
                }
            }
            button { onclick: move |_| {
                let _ = document::eval(&format!(
                    "const canvas=document.getElementById('{id}').querySelector('canvas'); const a=document.createElement('a'); a.href=canvas.toDataURL('image/png'); a.download='graphtron-rrd-{kind}.png'; a.click();"
                ));
            }, "Download PNG" }
        }
    }
}

#[component]
pub fn RrdPage() -> Element {
    let static_range = use_signal(|| (START, END));
    let static_cursor = use_signal(Cursor::default);
    let mut range = use_signal(|| (START, END));
    let mut cursor = use_signal(Cursor::default);
    let mut dark = use_signal(|| false);
    rsx! {
        style { {CSS} }
        main { class: if dark() { "rrd-page rrd-dark" } else { "rrd-page" },
            nav { a { href: "/", "← Graphtron chart gallery" } }
            h1 { "RRDtool, in the browser" }
            button { r#type: "button", aria_pressed: dark().to_string(), onclick: move |_| { let next = !dark(); dark.set(next); }, "Dark mode" }
            p { "Dark mode takes its black plotting paper, white labels and blue/cyan spectrum from the ", a { href: "https://oss.oetiker.ch/rrdtool/gallery/charles.png", "Charles RRDtool gallery graph" }, "." }
            p { "The familiar monitoring graph: a gray bevel, white plotting paper, fine dashed grids, tiny monospace type, solid areas and a printed statistics table. One optional Graphtron theme, shown two ways." }
            h2 { "1. Static graphs" }
            p { "Fixed 24-hour snapshots, like the images on a traditional monitoring page. These canvases have no hover, zoom, focus target or animated decoration. Both sets use the same deterministic five-minute samples, from 3 July 2026 (UTC)." }
            for kind in 0..3 { Example { key: "static-{kind}", kind, dark: dark(), interactive: false, range: static_range, cursor: static_cursor } }
            h2 { "2. Interactive graphs" }
            p { "The same graphics with shared crosshairs and time range. Hover to inspect, click to freeze, drag to zoom, Shift-drag to pan, or scroll to zoom. Double-click resets. The legend buttons toggle series; the printed statistics recalculate for the visible window." }
            div { class: "rrd-controls",
                button { onclick: move |_| range.set((END - 21_600_000, END)), "Last 6 hours" }
                button { onclick: move |_| { range.set((START, END)); cursor.set(Cursor::default()); }, "Reset 24 hours" }
            }
            output { class: "rrd-window", style: "font: 11px monospace",
                "{graphtron::format_ts_full(range().0)} — {graphtron::format_ts_full(range().1)} (UTC)"
            }
            for kind in 0..3 { Example { key: "interactive-{kind}", kind, dark: dark(), interactive: true, range, cursor } }
            p { "Keyboard: focus a chart, use arrows to pan, +/− to zoom, Home to reset, and Escape to clear the frozen cursor. On small screens, scroll each graph horizontally to preserve its image proportions." }
            footer {
                p { "Rendered by Graphtron, not generated by RRDtool. The theme follows the ",
                    a { href: "https://github.com/oetiker/rrdtool-1.x/blob/master/src/rrd_graph.c", "upstream default colors and graph furniture" },
                    ". Browser font rasterization and tick selection can differ from Cairo/Pango. This is a visual theme, not an RRD file reader or command interpreter." }
                p { code { "RenderOptions::rrdtool()" } " · " code { "options.rrdtool.as_mut().unwrap().title = …" } }
            }
        }
    }
}
