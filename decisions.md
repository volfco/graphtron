# Decisions

## 2026-09-25 — Gate Pages on workspace verification

- **Reference:** CI/demo cleanup task, this commit.
- **Decision:** Run formatting, Clippy, host tests, all three WASM checks, and pinned Playwright/Chromium browser suites before the Pages build.
- **Why:** The deployment depends on the verified workspace and the browser suites are the only coverage for actual Canvas/Dioxus behavior.
- **Impact:** Pull requests and relevant master pushes run verification; Pages build/deploy jobs run only for master pushes after both Rust/WASM and browser verification pass.

## 2026-09-25 — Implement the full review set

- **Source:** User request to record and implement all findings from the in-depth review.
- **Decision:** Fix all recorded correctness, lifecycle, performance, validation, cleanup, formatting, and CI findings in this change.
- **Why:** The user selected the broadest scope option; the review identified reachable failure modes rather than speculative style preferences.
- **Impact:** Changes span the core geometry crate, Dioxus adapter, demo, formatting, CI, and finding/decision records. Existing public contracts will be preserved except for making invalid inputs fail earlier, rendering explicit singleton geometry, and preventing invalid RGBA literals.
- **Implementation:** `8ad0551` (`fix: complete repository review remediation`).
