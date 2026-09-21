# graphtron — follow-ups

Remaining work moved from `docs/graphtron-code-review-2026-09-05.md`. These items
describe limits and design work that remain after the completed review fixes.

## Validation gaps and current limits

- [ ] Exercise Firefox, Safari, physical touch devices, and a full accessibility
      audit. The added Bazel gallery target also still needs a build check.
- [ ] Decide how dense heatmaps should behave when multiple logical rows map to
      one device pixel. Fractional row bands are currently retained, but the
      rows cannot be individually distinguished without increasing the height.
- [ ] Replace or make configurable the bounded 128-gap spacing estimate used by
      large irregular bar grids when exact bucket widths matter. Explicit
      histogram edges remain the preferred exact representation.
- [ ] Revisit continuous viewport extents. The current conservative behavior
      includes neighboring endpoints to avoid clipping smoothed curves; add
      coverage for whether this is the desired trade-off for every interpolation
      mode.

## Prepared scene and analytical interaction

- [ ] Introduce validated/prepared data with stable series IDs, sortedness
      metadata, shared storage, spacing, cumulative values, and optional extent
      indexes. Perform ingestion work when data changes instead of repeatedly
      validating ordering or recomputing statistics during hover and animation.
- [ ] Produce one visible scene containing composed coordinates, bucket/row
      bounds, source-sample identities, gap boundaries, and measured
      decorations. Use that scene for both drawing and hit-testing, while
      keeping raw values distinct from aggregate/plotted values.
- [ ] Use reducers appropriate to each geometry: extrema-preserving time-series
      columns; jointly reduced lower/center/upper bands and stacks; OHLC candles
      with matching hover summaries; and two-dimensional density/binning for
      scatter. M4's first/min/max/last selection is an envelope summary, not a
      faithful dense-scatter distribution.
- [ ] Make analytical inspection usable with many series: pointer-nearest
      selection, stable colors, isolate/mute controls, searchable or virtualized
      legends, an expanded value table, and keyboard-accessible detail
      navigation. Do not make smaller series inaccessible by truncating a
      tooltip to only the largest values. Expose semantic targets and labels in
      the renderer while adapters provide accessible DOM controls.
- [ ] Separate heatmap row geometry from its color scale and color domain. Rows
      are categorical, but the y scale currently doubles as the heatmap value
      domain; distinguish a logarithmic color mapping from a logarithmic
      coordinate axis so free y-cursor readouts describe the pointed row.
- [ ] Make density policy and styling explicit: plot-relative marker density,
      configurable series stroke/marker sizes, palette selection independent of
      theme, formatter precision derived from tick spacing, and UTC
      labeling/timezone policy. Offer a restrained analytical preset with a
      defined visual budget for annotations and effects instead of combining
      smoothing, neon effects, and automatic peak labels by default.
- [ ] In `graphtron-dioxus`, use immutable shared data for overlay reads and route
      non-time keyboard zoom through `on_key_with_min_span`. The handler at
      `../graphtron-dioxus/src/chart.rs` currently calls the millisecond-default
      method for linear domains, so `+` cannot zoom into an ordinary `0..10`
      value range.

## Acceptance gates

- [ ] **Geometry agreement:** drawn primitives and hover agree on source
      identity, sample time, aggregate value, and row/bucket bounds.
- [ ] **Browser rendering:** use deterministic fonts, themes, data, and time;
      add pixel/screenshot comparisons for every chart kind at several DPRs and
      sizes.
- [ ] **Density:** add fixed-viewport tests with 1k, 100k, and 1M history,
      many-series tests, and bounded submitted primitives and allocations.
- [ ] **Interaction:** cover pointer, frozen, and synced cursors; drag, pan,
      and zoom; touch cancellation; keyboard input; and fractional linear
      domains.
- [ ] **Robustness:** cover empty and mismatched arrays, non-finite and extreme
      values, zero/negative dimensions, one-point data, tiny plots, and
      hidden/resized canvases.
- [ ] **Usability:** ensure values are not clipped, legends are bounded, axis
      labels are readable, omitted series remain inspectable, and adapter
      accessibility checks pass.
- [ ] Choose target devices and measurable release-build budgets for data draws
      and overlays before tuning effects. Measure CPU preparation, Canvas
      submission, rasterization, and allocation separately; existing geometry
      benchmarks do not exercise the browser pipeline or pointer-rate work.