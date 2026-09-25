# Assumptions and Constraints

- The user wants the full review set implemented, not only the highest-priority blockers.
- `NaN` remains the supported point-series gap marker; infinities are invalid values and should be reported.
- Stacked point-series data is expected to share aligned x coordinates; mismatched grids will be rejected at the spec-aware validation boundary rather than silently interpolated.
- The repository's existing dependency and feature constraints remain; no new dependency is required.
- The project supports Rust 2024, host geometry tests, and WASM builds for web crates.
- Browser suites remain optional locally when Playwright/Chromium is unavailable; CI should run them when its environment provides the required tooling.
- Dioxus lifecycle cleanup must retain the callback object until `remove_event_listener_with_callback` runs.
- The current public `PinchState`/`on_pinch` API is intended to be reachable from `GraphtronChart`; multi-pointer tracking is therefore in scope.
- The user did not request a release or external publication; only local code, tests, and workflow files will be changed.
