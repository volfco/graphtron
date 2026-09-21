pub mod anim;
pub mod color;
pub mod curve;
pub mod decimate;
pub mod easing;
pub mod hit;
pub mod interact;
pub mod layout;
pub mod partition;
pub mod scale;
pub mod series;
pub mod spec;
pub mod stats;
pub mod ticks;
pub mod tooltip;
pub mod units;
pub mod validation;

#[cfg(feature = "web")]
pub mod canvas;
#[cfg(feature = "web")]
pub mod draw;
#[cfg(feature = "web")]
pub mod rrd;

pub use anim::{Animation, animate_range};
pub use color::{HeatmapColorScale, Rgba};
pub use curve::{PathOp, catmull_rom, line_path, monotone_cubic};
pub use easing::{ease_in_out_cubic, ease_out_cubic, ease_out_expo, linear};
pub use hit::{
    HistogramHoverCache, HoverInfo, HoverValue, LegendEntry, annotate_delta, hit_test,
    hit_test_chart, hit_test_chart_with_cache, hit_test_ohlc, hit_test_scatter, legend_entries,
    prepare_histogram_hover,
};
pub use interact::{
    Action, DragMode, DragState, InteractState, Key, PinchState, ZoomBounds, wheel_zoom,
    wheel_zoom_with_min_span,
};
pub use layout::{
    ChartLayout, Rect, data_y_domain, data_y_domain_scaled, resolve_x_domain, y_domain,
};
pub use partition::PartitionItem;
pub use scale::{LOG_EPSILON, LinearScale, ScaleKind};
pub use series::{
    BandSeries, HBarSeries, HistogramSeries, OhlcSeriesData, OhlcTick, PALETTE, SeriesData,
    StateSegment, StateTimelineSeries, band_series_color, css_color, hbar_series_color,
    histogram_series_color, median_gap, ohlc_series_color, palette_color, series_color,
};
pub use spec::{
    Annotation, ChartData, ChartKind, ChartSpec, DecimationKind, HighlightConfig, HighlightMode,
    LegendConfig, LegendFormat, LegendPosition, LegendStat, PointMarkers, ReferenceLines,
    SeriesLayout, StatLine, TrendConfig, TrendKind, XAxisKind,
};
pub use stats::{
    LinearFit, Summary, exponential_moving_average, linear_fit, median, quantile, quantile_sorted,
    simple_moving_average, sorted_finite,
};
pub use ticks::{category_ticks, format_ts_full, linear_ticks, log_ticks, nice_log_domain};
pub use tooltip::tooltip_page;
pub use units::Unit;
pub use validation::{DataIssue, DataIssueKind};

#[cfg(feature = "web")]
pub use canvas::{CanvasSurface, crisp_rect_at_dpr, crisp_stroke_geometry};
#[cfg(feature = "web")]
pub use draw::{
    Aesthetic, CursorOverlay, OverlayOptions, RenderOptions, Theme, draw, draw_overlay,
};
#[cfg(feature = "web")]
pub use tooltip::{draw_tooltip, draw_tooltip_limited};
