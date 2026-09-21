//! Reusable Dioxus chart component for the `graphtron` renderer.
//!
//! `graphtron` itself is a renderer — *(data + viewport) → pixels* — with no
//! framework opinion. This crate is the missing widget layer: [`GraphtronChart`]
//! owns the two stacked canvases, DPR-aware sizing, container-resize reflow,
//! interaction dispatch, overlay sync, optional zoom animation and cursor
//! smoothing, and an optional legend, so an app renders an interactive chart
//! in a few lines:
//!
//! ```ignore
//! use graphtron::{ChartData, ChartSpec, Unit};
//! use graphtron_dioxus::{Cursor, GraphtronChart};
//!
//! #[component]
//! fn App() -> Element {
//!     let data = use_signal(|| ChartData::Lines(my_series()));
//!     let range = use_signal(|| (start_ms, end_ms));
//!     let cursor = use_signal(Cursor::default);
//!     rsx! {
//!         GraphtronChart {
//!             spec: ChartSpec::line(Unit::BytesPerSec),
//!             data,
//!             range,
//!             cursor,
//!             animate: true,
//!             legend: true,
//!         }
//!     }
//! }
//! ```
//!
//! Cursor and range stay **caller-owned signals**, so syncing many panels is
//! just passing the same signals to each chart.

mod chart;
mod cursor;
mod legend;

pub use chart::GraphtronChart;
pub use cursor::Cursor;
pub use legend::Legend;

// Re-export the renderer so consumers need only one dependency.
pub use graphtron;
