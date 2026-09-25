# Review Findings

Review head: `aaae4ba` (`ci: add rrdtool/index.html copy for pages static routing`)

Status values: `planned`, `fixed`, `deferred`, `verified`.

## High priority

- [ ] `time_ticks` overflows `t += step` and can emit an unbounded number of ticks for large `i64` ranges. `crates/graphtron/src/ticks.rs:195-227`.
- [ ] Fragmented LTTB allocates per-run budgets with repeated full-run scans, creating quadratic work. `crates/graphtron/src/decimate.rs:163-181`.
- [ ] Point-series validation accepts infinite y values while rendering drops them. `crates/graphtron/src/validation.rs:133-138`.
- [ ] Log-axis band validation checks only `lower`, not `center` or `upper`. `crates/graphtron/src/validation.rs:395-397`.
- [ ] Stacked point-series data with different x grids passes validation but is composed incorrectly. `crates/graphtron/src/rrd.rs:62-91`, `crates/graphtron/src/layout.rs`.
- [ ] Dioxus resize listeners are not removed on unmount. `crates/graphtron-dioxus/src/chart.rs:270-281`.
- [ ] Frozen charts retain stale pointer coordinates after pointer leave. `crates/graphtron-dioxus/src/chart.rs:72-76,801-819`.
- [ ] Pinch-to-zoom exists in the core but is unreachable from the Dioxus chart. `crates/graphtron/src/interact.rs`, `crates/graphtron-dioxus/src/chart.rs:707-766`.
- [ ] Cumulative histogram totals can overflow to infinity and hide valid finite data. `crates/graphtron/src/series.rs:376-418`.

## Medium priority

- [ ] Singleton line/area/band data is considered non-empty but renders no default geometry. `crates/graphtron/src/spec.rs:127-146`, `crates/graphtron/src/draw.rs`.
- [ ] Zero-only histograms are considered non-empty but render blank. `crates/graphtron/src/spec.rs:149-159`, `crates/graphtron/src/draw.rs`.
- [ ] Statistics overflow for finite extreme values: Welford, moving average, and linear-fit `r2`. `crates/graphtron/src/stats.rs`.
- [ ] Extreme finite linear domains collapse to `(0, 1)`. `crates/graphtron/src/layout.rs`.
- [ ] Positive subnormal log bounds produce an unusable log scale. `crates/graphtron/src/layout.rs`, `crates/graphtron/src/ticks.rs`.
- [ ] Zero and negative animation durations produce inconsistent state. `crates/graphtron/src/anim.rs:69-88`.
- [ ] `nearest_index` returns the first sample for non-finite targets. `crates/graphtron/src/hit.rs:1021-1031`.
- [ ] Fractional CSS sizes disagree with the rounded DPR backing store. `crates/graphtron/src/canvas.rs:35-48`.
- [ ] Short plots can paint tooltip text outside the tooltip background. `crates/graphtron/src/tooltip.rs`.
- [ ] Hidden-series indices are not reconciled when data changes. `crates/graphtron-dioxus/src/chart.rs`.
- [ ] `css_color` emits invalid CSS for oversized colors. `crates/graphtron/src/series.rs`.
- [ ] Partition overlays return before drawing annotations. `crates/graphtron/src/draw.rs:4248-4289`.
- [ ] Animated redraws perform avoidable full-history work. `crates/graphtron-dioxus/src/chart.rs`, `crates/graphtron/src/draw.rs`.
- [ ] `Rgba` permits invalid alpha values. `crates/graphtron/src/color.rs`.
- [ ] `Animation` equality compares easing only at `0.5`. `crates/graphtron/src/anim.rs:16-26`.
- [ ] OHLC/log validation creates avoidable full-series allocations. `crates/graphtron/src/validation.rs`.
- [ ] Browser CI does not run tests, clippy, formatting, WASM checks, or browser suites. `.github/workflows/gh-pages.yml`.
- [ ] `graphtron-demo/src/chart.rs` is an unreachable duplicate adapter. `crates/graphtron-demo/src/chart.rs`.
- [ ] Existing rustfmt differences remain in three tracked files.
