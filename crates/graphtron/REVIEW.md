# Renderer review — 2026-09-06

Reviewed the graphtron draw/layout/hit-test paths, their validation contracts,
and integration through graphtron-dioxus and the demo gallery.

## Findings addressed

- Dense scatter used a line reducer (M4). This erased the interior of clouds
  at repeated x coordinates. Scatter and point plots now keep one original
  observation per bounded screen-space bin and filter invalid/offscreen
  coordinates before emitting canvas commands. Sorted-x viewport selection
  avoids traversing offscreen history; the existing sorted-x input contract
  still applies.
- State timelines had unequal horizontal/vertical gaps and no cell highlight.
  Shared inset rectangles now drive rendering, hit tests, and highlighting.
  Gutters do not select cells. Valid ordered timelines are binary-sliced to
  the visible interval before rendering.
- Candlestick hover only traced the close. Open/close now have stronger
  y-axis guides; high/low use fainter guides and badges. Coincident prices
  are deduplicated, preferring body guides.
- Cursor and snapped y guides lacked a numeric difference. The overlay now
  displays a faint signed delta in data units, converting both pixel rows
  through the axis mapping (including logarithmic axes and stacked values).
- Tooltip detail width omitted its indentation, truncating short detail rows
  such as pie percentages. Measurement and drawing now agree on that inset.

## New chart data and widgets

`Points`, `Pie`, `Treemap`, and `HostMap` join `ChartData`; existing `Scatter`
is exposed in the gallery as a numeric correlation widget. All run through
`GraphtronChart`, including legends and accessibility summaries. Pie, treemap,
and host map share tested geometry with their hit tests. The Dioxus adapter
prepares partition geometry on data/size changes; pointer events scan that
geometry and overlay highlights reuse the exact selected shape.

Pie/treemap normalize finite positive weights before summing to avoid
overflow; validation diagnoses invalid weights. Treemaps currently represent
flat weighted leaves, with bounded-depth binary subdivision. Host tiles have
equal areas, with explicit status colors or a default 0–100 metric scale.

## Axis validation

One independent x axis and one independent y axis are supported. Linear/log
y transforms, time/linear/category x mapping, separate x/y units, and precise
fractional numeric viewport handling have regression coverage. `ChartLayout`
contains just `x_scale`, `y_scale`, and `y_kind`; `ChartSpec` has no secondary
axis configuration or per-series axis assignment. Dual independent x or y
axes are therefore unsupported, not implied by axis titles or units.

## Verification

- 230 Rust tests pass across graphtron and graphtron-dioxus, including partition
  proportions, overflow, invalid weights, narrow geometry, hit-test agreement,
  timeline gutters, and point plots.
- WASM compilation and the Dioxus gallery build pass; Clippy passes without
  warnings for graphtron and graphtron-dioxus, including test/example targets.
- 147 Canvas2D browser fixtures pass across DPR 1, 1.25, 1.5, 2, and 3.
  Assertions cover delta text, candle guide positions/opacities, cell hover
  paint, cloud interiors, finite canvas coordinates, and rendering command
  budgets for up to one million historical samples.
- Live gallery browser checks cover new widget hover, million-point type
  switching, frozen inspection pagination, keyboard Escape, and mobile layout.

These checks establish geometry/interaction correctness and bounded canvas
work for the exercised workloads; they are not a hardware-independent frame
rate guarantee. Partition hover remains O(n) in item count, and very dense
tiles cannot be read individually without more screen space. Nested treemap
hierarchies and secondary axes remain documented limitations.
