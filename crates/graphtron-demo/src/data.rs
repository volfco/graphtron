use graphtron::{
    Annotation, BandSeries, HBarSeries, HistogramSeries, OhlcSeriesData, OhlcTick, SeriesData,
    StateSegment, StateTimelineSeries,
};

#[derive(Clone, PartialEq)]
pub struct Dataset {
    pub from_ms: i64,
    pub to_ms: i64,
    pub line: Vec<SeriesData>,
    pub area: Vec<SeriesData>,
    pub bars: Vec<SeriesData>,
    /// OHLC candlestick data (synthetic price).
    pub ohlc: Vec<OhlcSeriesData>,
    /// Step chart data (deployment phase states).
    pub step: Vec<SeriesData>,
    /// Histogram (latency distribution).
    pub histogram: Vec<HistogramSeries>,
    /// Horizontal bar (resource usage by category).
    pub hbar: Vec<HBarSeries>,
    /// Percentile band (latency envelope).
    pub band: Vec<BandSeries>,
    /// State timeline rows (service health over time).
    pub states: Vec<StateTimelineSeries>,
    /// Request rate per host as a value-colored matrix, one row per host.
    pub heatmap: Vec<SeriesData>,
    /// Latency percentiles spanning several decades — the case a linear y
    /// axis cannot show, since p50 flattens onto the floor next to p999.
    pub decades: Vec<SeriesData>,
    /// Annotations for the line chart.
    pub annotations: Vec<Annotation>,
}

struct Lcg(u64);

impl Lcg {
    fn next_unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64) / (1u64 << 31) as f64
    }
}

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

pub fn generate() -> Dataset {
    let to_ms = now_ms();
    let from_ms = to_ms - 3_600_000;
    let mut rng = Lcg(0x9E3779B97F4A7C15);

    // Fine series: 120 points at 30s spacing.
    let n = 120usize;
    let step = (to_ms - from_ms) / n as i64;
    let xs: Vec<f64> = (0..n).map(|i| (from_ms + i as i64 * step) as f64).collect();

    // CPU per core
    let line = (0..3)
        .map(|core| {
            let phase = core as f64 * 1.7;
            let base = 40.0 + core as f64 * 10.0;
            let ys = (0..n)
                .map(|i| {
                    let t = i as f64 / n as f64 * std::f64::consts::TAU * 3.0;
                    let v = base + 25.0 * (t + phase).sin() + (rng.next_unit() - 0.5) * 14.0;
                    v.clamp(0.0, 100.0)
                })
                .collect();
            SeriesData {
                name: format!("core {core}"),
                xs: xs.clone(),
                ys,
                color: None,
            }
        })
        .collect();

    // Memory utilization
    let mut mem = 0.6_f64;
    let area_ys = (0..n)
        .map(|_| {
            mem = (mem + (rng.next_unit() - 0.5) * 0.04).clamp(0.3, 0.95);
            mem
        })
        .collect();
    let area = vec![SeriesData {
        name: "memory".into(),
        xs: xs.clone(),
        ys: area_ys,
        color: Some(0x73BF69),
    }];

    // Log volume bars
    let buckets = 30usize;
    let bstep = (to_ms - from_ms) / buckets as i64;
    let bxs: Vec<f64> = (0..buckets)
        .map(|i| (from_ms + i as i64 * bstep) as f64)
        .collect();
    let bars = [
        ("INFO", 0x5794F2u32, 60.0, 40.0),
        ("WARN", 0xF2CC0Cu32, 12.0, 10.0),
        ("ERROR", 0xF2495Cu32, 3.0, 6.0),
    ]
    .iter()
    .map(|(name, color, base, spread)| {
        let ys = (0..buckets)
            .map(|_| (base + rng.next_unit() * spread).round())
            .collect();
        SeriesData {
            name: (*name).into(),
            xs: bxs.clone(),
            ys,
            color: Some(*color),
        }
    })
    .collect();

    // OHLC: synthetic price data (60 ticks).
    let ohlc_count = 60usize;
    let ohlc_step = (to_ms - from_ms) / ohlc_count as i64;
    let mut price = 150.0;
    let ohlc = vec![OhlcSeriesData {
        name: "WYVRN".into(),
        ticks: (0..ohlc_count)
            .map(|i| {
                let ts = (from_ms + i as i64 * ohlc_step) as f64;
                let change = (rng.next_unit() - 0.5) * 6.0;
                let open = price;
                let close = (price + change).clamp(100.0, 200.0);
                let high = open.max(close) + rng.next_unit() * 3.0;
                let low = open.min(close) - rng.next_unit() * 3.0;
                price = close;
                OhlcTick {
                    ts,
                    open,
                    high,
                    low,
                    close,
                }
            })
            .collect(),
        color: None,
    }];

    // Step: deployment phase state (0/1/2).
    let step_count = 30usize;
    let step_step = (to_ms - from_ms) / step_count as i64;
    let mut phase = 0.0;
    let step_ys: Vec<f64> = (0..step_count)
        .map(|_| {
            if rng.next_unit() < 0.15 {
                phase = ((phase + 1.0_f64) % 3.0_f64).floor();
            }
            phase
        })
        .collect();
    let step_xs: Vec<f64> = (0..step_count)
        .map(|i| (from_ms + i as i64 * step_step) as f64)
        .collect();
    let step = vec![SeriesData {
        name: "deploy phase".into(),
        xs: step_xs,
        ys: step_ys,
        color: Some(0x00FF41),
    }];

    // Histogram: latency distribution (10 buckets).
    let hist_buckets: Vec<f64> = (0..11).map(|i| i as f64 * 10.0).collect();
    let mut hist_counts: Vec<f64> = (0..10)
        .map(|i| ((10 - i) as f64 * (rng.next_unit() * 5.0 + 2.0)).round())
        .collect();
    hist_counts[0] = 100.0 + rng.next_unit() * 20.0; // spike at low latency
    let histogram = vec![HistogramSeries {
        name: "latency".into(),
        buckets: hist_buckets,
        counts: hist_counts,
        color: Some(0x00BFFF),
        cumulative: false,
    }];

    // Horizontal bar: resource usage by service.
    let categories = vec![
        "api".into(),
        "worker".into(),
        "cache".into(),
        "db".into(),
        "queue".into(),
    ];
    let hbar = vec![
        HBarSeries {
            name: "CPU".into(),
            categories: categories.clone(),
            values: categories
                .iter()
                .map(|_| rng.next_unit() * 80.0 + 10.0)
                .collect(),
            color: Some(0xFF6EC7),
        },
        HBarSeries {
            name: "Memory".into(),
            categories: categories.clone(),
            values: categories
                .iter()
                .map(|_| rng.next_unit() * 60.0 + 20.0)
                .collect(),
            color: Some(0x00FFFF),
        },
    ];

    // Band: latency percentile envelope (p50 with p10-p90).
    let mut p50 = 40.0_f64;
    let mut band_center = Vec::with_capacity(n);
    let mut band_lower = Vec::with_capacity(n);
    let mut band_upper = Vec::with_capacity(n);
    for _ in 0..n {
        p50 = (p50 + (rng.next_unit() - 0.5) * 6.0).clamp(20.0, 80.0);
        band_center.push(p50);
        band_lower.push(p50 - 8.0 - rng.next_unit() * 10.0);
        band_upper.push(p50 + 12.0 + rng.next_unit() * 25.0);
    }
    let band = vec![BandSeries {
        name: "latency p50 (p10-p90)".into(),
        xs: xs.clone(),
        center: band_center,
        lower: band_lower,
        upper: band_upper,
        color: Some(0x7B68EE),
    }];

    // State timeline: per-service health segments.
    let state_palette = [
        ("up", 0x00FF41u32, 0.75),
        ("degraded", 0xFFD700u32, 0.18),
        ("down", 0xFF3366u32, 0.07),
    ];
    let states = ["api", "worker", "cache"]
        .iter()
        .map(|svc| {
            let mut segments = Vec::new();
            let mut t = from_ms as f64;
            while t < to_ms as f64 {
                let dur = (3.0 + rng.next_unit() * 12.0) * 60_000.0;
                let end = (t + dur).min(to_ms as f64);
                let roll = rng.next_unit();
                let (label, color, _) = if roll < state_palette[0].2 {
                    state_palette[0]
                } else if roll < state_palette[0].2 + state_palette[1].2 {
                    state_palette[1]
                } else {
                    state_palette[2]
                };
                segments.push(StateSegment {
                    start_ms: t,
                    end_ms: end,
                    label: label.into(),
                    color,
                });
                t = end;
            }
            StateTimelineSeries {
                name: (*svc).into(),
                segments,
            }
        })
        .collect();

    // Latency percentiles three decades apart (log-axis showcase).
    let decades = [("p50", 1.2_f64, 0x00FF41), ("p99", 45.0, 0xFFD700), ("p999", 900.0, 0xFF3366)]
        .into_iter()
        .map(|(name, base, color)| {
            let ys = (0..n)
                .map(|i| {
                    let t = i as f64 / n as f64 * std::f64::consts::TAU * 2.0;
                    // Multiplicative noise, which is what a log axis reads well.
                    base * (1.0 + 0.35 * t.sin()) * (0.75 + rng.next_unit() * 0.5)
                })
                .collect();
            SeriesData {
                name: name.into(),
                xs: xs.clone(),
                ys,
                color: Some(color),
            }
        })
        .collect();

    // Request rate per host: one heatmap row per host, colored by value.
    let heatmap = ["web-01", "web-02", "web-03", "cache-01", "db-01"]
        .into_iter()
        .enumerate()
        .map(|(row, host)| {
            let base = 200.0 + row as f64 * 120.0;
            let ys = (0..n)
                .map(|i| {
                    let t = i as f64 / n as f64 * std::f64::consts::TAU * 2.0;
                    (base * (1.0 + 0.4 * (t + row as f64).sin())
                        + (rng.next_unit() - 0.5) * 90.0)
                        .max(0.0)
                })
                .collect();
            SeriesData {
                name: host.into(),
                xs: xs.clone(),
                ys,
                color: None,
            }
        })
        .collect();

    // Annotations: threshold lines and a marker.
    let annotations = vec![
        Annotation::threshold(85.0, "critical", 0xFF3366),
        Annotation::band(50.0, 60.0, 0x00FF41),
        Annotation::marker(((from_ms + to_ms) / 2) as f64, "midpoint", 0xFFD700),
    ];

    Dataset {
        from_ms,
        to_ms,
        line,
        area,
        bars,
        ohlc,
        step,
        histogram,
        hbar,
        band,
        states,
        heatmap,
        decades,
        annotations,
    }
}
