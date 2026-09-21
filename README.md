# graphtron

Pure-Rust Canvas2D chart rendering. Series are stroked directly onto a
`<canvas>` via `web-sys` — no JavaScript charting library — which keeps WASM
bundles small and gives full control over dense, information-rich rendering
and cross-chart cursor sync.

Extracted from the Wyvrn platform monorepo, where it renders the `obs`
observability console.

## Crates

| Crate | What it is |
|-------|------------|
| [`graphtron`](crates/graphtron) | The renderer. Geometry core (scales, ticks, decimation, hit-testing, interaction state machine) always compiles; the Canvas2D draw pipeline sits behind the `web` feature. No framework opinion. |
| [`graphtron-dioxus`](crates/graphtron-dioxus) | The widget layer: a `GraphtronChart` Dioxus component, legend, and shared `Cursor` signal. WASM only. |
| [`graphtron-demo`](crates/graphtron-demo) | Standalone Dioxus gallery rendering every chart type from synthetic telemetry. Exists to exercise the other two. |

`graphtron` alone has one dependency (`serde`); the `web` feature adds
`web-sys` / `js-sys` / `wasm-bindgen`. The zero-extra-dependency geometry core
is a deliberate design constraint.

## Build and test

Host tests cover the geometry core:

```bash
cargo test -p graphtron --all-features --locked
```

Type-check the web layers against WASM:

```bash
cargo check -p graphtron --features web --target wasm32-unknown-unknown
cargo check -p graphtron-dioxus --target wasm32-unknown-unknown
cargo check -p graphtron-demo --target wasm32-unknown-unknown
```

Run the demo gallery:

```bash
cd crates/graphtron-demo && dx serve --platform web
```

The Canvas2D pixel-regression suites need Playwright and a `wasm-bindgen` CLI
matching `Cargo.lock`; see each crate's README for the exact invocations.

## Consuming it

Both library crates are unpublished, so depend on them by git revision:

```toml
[dependencies]
graphtron = { git = "https://github.com/volfco/graphtron", rev = "<sha>", features = ["web"] }
graphtron-dioxus = { git = "https://github.com/volfco/graphtron", rev = "<sha>", default-features = false }
```

`graphtron-dioxus` wants `default-features = false` when the consuming app
already enables `dioxus/web` itself — its `web` feature adds nothing else.
