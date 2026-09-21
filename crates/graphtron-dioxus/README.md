# graphtron-dioxus

`GraphtronChart` also serves as the Pie Chart, Point Plot, Scatter Plot, Treemap,
and Host Map widget: select `ChartData::Pie`, `Points`, `Scatter`, `Treemap`,
or `HostMap` respectively. The gallery includes all five. Pie/treemap/host-map
items use `graphtron::PartitionItem { name, value, color }`; use the matching
`ChartSpec::pie`, `point`, `scatter`, `treemap`, or `host_map` preset.
For numeric scatter x values, set `x_axis: XAxisKind::Linear { min: None, max: None }`.
Legends can toggle individual slices/tiles and preserve their original colors.
Partition shapes are prepared when data or layout changes and reused on hover.

Hover readouts show the faint signed difference between the cursor's y value
and the nearest plotted y value. Candlestick guides connect open/close bounds
to the y axis, with fainter high/low wick guides. Timelines highlight the
hovered cell and use equal horizontal and vertical gaps.

Each chart supports one x and one y axis. Independent secondary x/y axes and
per-series axis assignments are not supported.

The Dioxus widget layer for the [`graphtron`](../graphtron) renderer. `graphtron` turns
*(data + viewport) → pixels* and *pointer input → semantic actions*; this crate
supplies the one component every consumer used to re-implement by hand
(~300 lines each in `obs` and `graphtron-demo`): canvases, sizing, interaction,
overlay sync.

## What `GraphtronChart` owns

- **Two stacked canvases** — data layer (redraws on data/range/theme/size) and
  overlay layer (redraws at cursor rate), created and DPR-synced internally.
- **Container-resize reflow** — via Dioxus's ResizeObserver-backed `onresize`;
  charts follow their container. A window-resize listener additionally
  re-triggers drawing so DPR changes re-render at the new density.
- **Interaction dispatch** — pointer, touch, and pen hover/freeze crosshairs,
  drag-zoom (shift-drag pans; override with `drag_mode`), wheel zoom anchored
  at the pointer, double-click reset, arrow/± navigation, Home/End reset, and
  Escape clears a frozen cursor. Pointer capture keeps touch/pen drags active
  after leaving the canvas. Category axes remain hover/freeze-only until the
  core renderer supports category viewports.
  The adapter uses Graphtron's cached chart hit testing, so tooltips include OHLC details,
  histogram buckets, bands, heatmap rows, horizontal bars, and state segments.
- **Frozen-cursor detail inspector** — paged values and detail rows remain
  accessible when a compact canvas tooltip omits series. Histogram cumulative
  prefixes are prepared when data changes, and overlays borrow the data.
- **Bounded zoom/pan** — wheel, drag (including touch/pen), keyboard and
  built-in resets stay within the full loaded data extent. The default
  minimum viewport is 1% of that extent (100× maximum magnification).
  Set `min_zoom_span: 300_000.0` for a five-minute time-axis minimum, or use
  x-axis units for numeric charts. Invalid overrides use the default;
  overrides larger than the data span disable zoom-in. Extents are cached
  per data update and include hidden series. Empty/single-x data cannot zoom.
  App-written range/domain signals and custom `on_reset` callbacks remain
  app-controlled; a shared range is constrained by the chart being operated.
- **Optional zoom animation** (`animate`) and **cursor smoothing**
  (`smooth_cursor`), both rAF-coalesced.
- **Optional DOM legend** (`legend`) with click-to-toggle series visibility;
  palette colors stay stable when series are hidden. `spec.legend.show` also
  enables it and supplies the DOM placement and text format. The legacy
  `legend: true` prop remains a name-only legend below the chart.
- **Accessible chart region** — concise chart summary and interaction
  instructions for screen readers, keyboard focus, decorative canvases hidden
  from the accessibility tree, and keyboard-operable legend buttons. The text
  alternative summarizes series and mark counts rather than emitting a DOM
  node for every data point.

## What stays caller-owned

Cursor and viewport are **signals the caller may own** — pass the same
`Signal<Cursor>` / `Signal<(i64, i64)>` to several time charts and they sync
(the obs console pattern). For fractional linear axes, use
`Signal<(f64, f64)>` through `domain`; it takes precedence over the legacy
time-only `range`. Omit them and the chart runs self-contained with an autofit
domain and a local cursor.

## Embed

```rust
use dioxus::prelude::*;
use graphtron::{ChartData, ChartSpec, Unit};
use graphtron_dioxus::{Cursor, GraphtronChart};

#[component]
fn Panel(data: ReadSignal<ChartData>) -> Element {
    rsx! {
        GraphtronChart {
            spec: ChartSpec::line(Unit::Percent),
            data,
            animate: true,
            legend: true,
        }
    }
}
```

Synced panels:

```rust
let range = use_signal(|| (from_ms, to_ms));
let cursor = use_signal(Cursor::default);
rsx! {
    GraphtronChart { spec: spec_a, data: data_a, range, cursor }
    GraphtronChart { spec: spec_b, data: data_b, range, cursor }
}
```

Scope-integrated adapter (map app state ↔ signals, hand everything else to the
component): see `frontend/obs/src/components/chart.rs` (~80 lines).

## Props

| Prop | Default | Purpose |
|------|---------|---------|
| `spec` | — | `graphtron::ChartSpec` (axes, units, smoothing, chrome) |
| `data` | — | `ReadSignal<ChartData>` — kind travels with the data |
| `range` | autofit | `Signal<(i64, i64)>` visible range (epoch ms) |
| `domain` | autofit | precise `Signal<(f64, f64)>` viewport for linear/time axes; overrides `range` |
| `cursor` | local | `Signal<Cursor>` for cross-panel sync |
| `options` | cyberpunk+neon | `graphtron::RenderOptions` (theme, aesthetic, font) |
| `height` | `220.0` | container height in px |
| `on_zoom` | — | fired with the new absolute range after zoom/pan |
| `on_domain_change` | — | fired with the precise `f64` viewport after zoom/pan |
| `on_reset` | — | fired on double-click/Home (else `default_range`/autofit) |
| `default_range` | — | reset target when `on_reset` is absent |
| `default_domain` | — | precise reset target, before `default_range` |
| `interactive` | `true` | enable pointer/keyboard interaction |
| `animate` | `false` | animate zoom/pan transitions |
| `smooth_cursor` | `false` | ease the hover crosshair toward the pointer |
| `legend` | `false` | render the legacy name-only DOM legend below the chart; `spec.legend.show` controls show/position/format |
| `drag_mode` | shift-toggle | fix drag to `Zoom` or `Pan` |
| `freeze_delta` | `true` | while the cursor is frozen, annotate the live hover with each series' signed `Δ` against the frozen point |
| `annotations` | `[]` | additional `graphtron::Annotation` overlay markers; merged with `spec.annotations` (exact duplicates removed) |
| `class` | `graphtron-chart` | CSS class on the container |

## Verify

```bash
cargo check -p graphtron-dioxus --target wasm32-unknown-unknown
cargo test -p graphtron-dioxus
```
