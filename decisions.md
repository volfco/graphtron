# Decisions

## 2026-09-25 — Gate Pages on workspace verification

- **Reference:** CI/demo cleanup task, this commit.
- **Decision:** Run formatting, Clippy, host tests, and all three WASM checks before the Pages build; keep browser scripts manual because no Node/Playwright package is pinned.
- **Why:** The deployment depends on the verified workspace, while a fake browser gate would silently depend on undeclared tooling.
- **Impact:** Pull requests and relevant master pushes run verification; Pages deploys only after it passes.

## 2026-09-25 — Implement the full review set

- **Source:** User request to record and implement all findings from the in-depth review.
- **Decision:** Fix all recorded correctness, lifecycle, performance, validation, cleanup, formatting, and CI findings in this change.
- **Why:** The user selected the broadest scope option; the review identified reachable failure modes rather than speculative style preferences.
- **Impact:** Changes span the core geometry crate, Dioxus adapter, demo, formatting, and CI. Existing public contracts will be preserved except for making invalid inputs fail earlier or render explicitly.
