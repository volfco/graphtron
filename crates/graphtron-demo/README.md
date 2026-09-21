# graphtron-demo

A standalone Dioxus gallery for the [`graphtron`](../graphtron) chart renderer. It
generates synthetic telemetry and renders every chart type graphtron produces.

## Run

```bash
cd crates/graphtron-demo
dx serve            # opens http://localhost:8080 by default
```

(Requires `dioxus-cli` and the `wasm32-unknown-unknown` target, same as the
other Dioxus apps in this workspace.)

The gallery is a workspace member. A bundle can also be built from this
directory with `dx build --web`, or type-checked from the repository root with
`cargo check -p graphtron-demo --target wasm32-unknown-unknown`.

## What it shows

### RRDtool style page

Open `/rrdtool` after `dx serve`, or follow the RRDtool link from the gallery.
Use **Dark mode** to switch both sets to the black, white and blue/cyan
palette inspired by the [Charles graph](https://oss.oetiker.ch/rrdtool/gallery/charles.png).
Switching themes preserves the interactive time range.
The dedicated page has two sets of traffic, stacked CPU, and load graphs:

- Static, fixed 24-hour canvases, presented like traditional generated images.
- Identically styled interactive graphs with shared cursor/range, hover
  readouts, freeze inspection, drag/pan, wheel/keyboard zoom, reset controls,
  and togglable legends. The printed statistics follow the visible window.

Both sets use the same deterministic samples. Each graph has a PNG download
button. Fixed image proportions are preserved on narrow screens with local
horizontal scrolling. The optional renderer preset is `RenderOptions::rrdtool()`;
its implementation and source references are documented in graphtron's README.

The dedicated regression script is `node crates/graphtron-demo/tests/rrd-browser.cjs`.
It verifies pixel-identical static/interactive data layers at rest, reference
palette colors, interaction isolation, zoom/freeze/legends, PNG exports, and
mobile containment at DPR 1 and 2. Screenshots go to `target/graphtron-browser/rrd-*.png`.

**Shared cursor + zoom row** — three charts driven by one cursor and one time
range, so the cross-chart sync is visible:

- **Multi-series line** — CPU per core (%).
- **Area** — memory utilization, zero-anchored fill.
- **Stacked bars** — log volume by severity (INFO/WARN/ERROR).

The chart-types row adds OHLC candles, a latency histogram, horizontal bars
with end-of-bar value labels, a state timeline, a percentile band, a step
chart, and a **heatmap** whose legend is the color ramp with its value domain
on it (a heatmap colors by value, so per-row swatches would say nothing).

Interactions are provided by `graphtron-dioxus::GraphtronChart`: hover for a synced
crosshair and chart-kind-aware tooltip, **click** to freeze the cursor,
**drag** to zoom to a window (Shift-drag pans), **scroll** to zoom around the
pointer, **double-click** to reset, and keyboard navigation with arrows,
plus/minus, Home/End reset, and Escape to clear a frozen cursor. Pointer
capture keeps touch and pen drags active after the pointer leaves the canvas.

**Analysis row** — the same latency percentiles (p50 / p99 / p999, three
decades apart) drawn twice:

- **Log y axis** (`ChartSpec::with_log_y()`) with axis titles and a stats
  legend (`LegendConfig::with_stats`) reporting min/max/mean per series.
  Click to freeze the cursor, then hover elsewhere: each series gains a signed
  `Δ` row against the frozen point.
- **The same data on a linear y axis**, where p999 owns the domain and the
  lower percentiles collapse onto the floor — the reason the log axis exists.

**Statistics row** — the numbers the chart works out for you, all recomputed
from the visible window (zoom and they re-answer):

- **Mean and ±1σ** — `ReferenceLines::normal_band()`, the band shaded behind
  the series and the lines labeled at the right edge.
- **Fitted trend** — `TrendConfig::linear()`, labeled with its per-hour rate
  and the `r²` that says how much the straight line actually explains.
- **Peak and trough callouts** — `HighlightMode::Extremes`, marking both ends
  of the visible range.

**Compact panels row:**

- **Sparkline** — `ChartSpec::sparkline()`, chrome-less line that fills its box.
- **Stat** — single latest value (app-rendered DOM, not a graphtron canvas).
- **Gauge** — latest value against a range (app-rendered DOM).

**Dense data inspection** offers 1,000, 100,000, or 1,000,000 observations with
repeatable spikes and missing intervals. Switch between line, stacked area,
band, and scatter, or select **Inspect 101 samples** to test a small viewport
against a large history. The gallery collapses its multi-column rows on
narrow screens.

## How it's wired

`src/main.rs` embeds `graphtron_dioxus::GraphtronChart` directly. It passes the shared
time range and cursor as plain `Signal`s (no app context); the adapter owns the
two canvases, resize/DPR handling, hit-testing, overlays, and interaction
dispatch. Use `GraphtronChart` as the starting point for a new Dioxus embedding.

The gallery covers line, area, stacked bars, heatmap, OHLC, histogram,
horizontal-bar, state-timeline, band, step, sparkline, stat, and gauge views.
The last two are app-rendered DOM rather than graphtron canvases.

`src/data.rs` generates the synthetic series (deterministic LCG noise, seeded
from the page-load time).

## Browser checks

After `dx build --web`, run `node crates/graphtron-demo/tests/browser.cjs` from the
repository root with Playwright and Chromium installed (the same `NODE_PATH`
and `PLAYWRIGHT_BROWSERS_PATH` setup used by Graphtron's canvas tests). The suite
switches million-point chart types, inspects all 40 series through a frozen
cursor, checks Escape, and checks the narrow-screen gallery layout.
