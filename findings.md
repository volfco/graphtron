# Review Findings

Review head: `aaae4ba` (`ci: add rrdtool/index.html copy for pages static routing`)

Status: all listed findings are implemented. Verification evidence is recorded in `decisions.md` and the repository's Rust/WASM/CI checks.

## High priority

- [x] `time_ticks` overflows `t += step` and can emit an unbounded number of ticks for large `i64` ranges. `crates/graphtron/src/ticks.rs:195-227`.
- [x] Fragmented LTTB allocates per-run budgets with repeated full-run scans, creating quadratic work. `crates/graphtron/src/decimate.rs:163-181`.
- [x] Point-series validation accepts infinite y values while rendering drops them. `crates/graphtron/src/validation.rs:133-138`.
- [x] Log-axis band validation checks only `lower`, not `center` or `upper`. `crates/graphtron/src/validation.rs:395-397`.
- [x] Stacked point-series data with different x grids passes validation but is composed incorrectly. `crates/graphtron/src/rrd.rs:62-91`, `crates/graphtron/src/layout.rs`.
- [x] Dioxus resize listeners are not removed on unmount. `crates/graphtron-dioxus/src/chart.rs:270-281`.
- [x] Frozen charts retain stale pointer coordinates after pointer leave. `crates/graphtron-dioxus/src/chart.rs:72-76,801-819`.
- [x] Pinch-to-zoom exists in the core but is unreachable from the Dioxus chart. `crates/graphtron/src/interact.rs`, `crates/graphtron-dioxus/src/chart.rs:707-766`.
- [x] Cumulative histogram totals can overflow to infinity and hide valid finite data. `crates/graphtron/src/series.rs:376-418`.

## Medium priority

- [x] Singleton line/area/band data is considered non-empty but renders no default geometry. `crates/graphtron/src/spec.rs:127-146`, `crates/graphtron/src/draw.rs`.
- [x] Zero-only histograms are considered non-empty but render blank. `crates/graphtron/src/spec.rs:149-159`, `crates/graphtron/src/draw.rs`.
- [x] Statistics overflow for finite extreme values: Welford, moving average, and linear-fit `r2`. `crates/graphtron/src/stats.rs`.
- [x] Extreme finite linear domains collapse to `(0, 1)`. `crates/graphtron/src/layout.rs`.
- [x] Positive subnormal log bounds produce an unusable log scale. `crates/graphtron/src/layout.rs`, `crates/graphtron/src/ticks.rs`.
- [x] Zero and negative animation durations produce inconsistent state. `crates/graphtron/src/anim.rs:69-88`.
- [x] `nearest_index` returns the first sample for non-finite targets. `crates/graphtron/src/hit.rs:1021-1031`.
- [x] Fractional CSS sizes disagree with the rounded DPR backing store. `crates/graphtron/src/canvas.rs:35-48`.
- [x] Short plots can paint tooltip text outside the tooltip background. `crates/graphtron/src/tooltip.rs`.
- [x] Hidden-series indices are not reconciled when data changes. `crates/graphtron-dioxus/src/chart.rs`.
- [x] `css_color` emits invalid CSS for oversized colors. `crates/graphtron/src/series.rs`.
- [x] Partition overlays return before drawing annotations. `crates/graphtron/src/draw.rs:4248-4289`.
- [x] Animated redraws perform avoidable full-history work. `crates/graphtron-dioxus/src/chart.rs`, `crates/graphtron/src/draw.rs`.
- [x] `Rgba` permits invalid alpha values. `crates/graphtron/src/color.rs`.
- [x] `Animation` equality compares easing only at `0.5`. `crates/graphtron/src/anim.rs:16-26`.
- [x] OHLC/log validation creates avoidable full-series allocations. `crates/graphtron/src/validation.rs`.
- [x] Browser CI does not run tests, clippy, formatting, WASM checks, or browser suites. `.github/workflows/gh-pages.yml`.
- [x] `graphtron-demo/src/chart.rs` is an unreachable duplicate adapter. `crates/graphtron-demo/src/chart.rs`.
- [x] Existing rustfmt differences were resolved by running `cargo fmt --all`.

## Verification

- `cargo test --workspace --all-targets --locked` — passed, including 223 core unit tests, 9 geometry property tests, 6 partition tests, 10 regression tests, 16 Dioxus tests, and benchmark targets.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` — passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- WASM checks for graphtron, graphtron-dioxus, and graphtron-demo — passed.
- CI now runs the core, demo, and RRD browser suites with pinned Playwright/Chromium and wasm-bindgen tooling. Local browser execution remains unavailable here because Playwright is not installed.
