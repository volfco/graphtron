use crate::spec::ChartData;
use std::fmt;

/// A precise, non-panicking data-contract violation returned by
/// [`ChartData::validate`](crate::spec::ChartData::validate).
#[derive(Debug, Clone, PartialEq)]
pub struct DataIssue {
    /// Zero-based series/row index within the [`ChartData`] variant.
    pub series: usize,
    /// Point, bucket, tick, category, or segment index when applicable.
    pub index: Option<usize>,
    pub kind: DataIssueKind,
}

/// Kinds of malformed input that can make chart geometry ambiguous.
#[derive(Debug, Clone, PartialEq)]
pub enum DataIssueKind {
    NegativeWeight,
    LengthMismatch {
        left: &'static str,
        left_len: usize,
        right: &'static str,
        right_len: usize,
    },
    NonFiniteX,
    DecreasingX,
    NonFiniteValue {
        field: &'static str,
    },
    InvalidOhlcRange,
    HistogramShape {
        buckets: usize,
        counts: usize,
    },
    /// Stacked histograms require one common bucket grid so each column has a
    /// well-defined cumulative geometry.
    HistogramGridMismatch {
        left: usize,
        right: usize,
    },
    NonIncreasingBucket,
    InvertedBand,
    InvalidSegmentRange,
    OverlappingSegments,
    /// A value a logarithmic axis cannot place. Rendering gaps these points
    /// rather than guessing, so a panel can silently lose most of its data
    /// unless the axis kind and the data agree.
    NonPositiveOnLogAxis {
        value: f64,
        /// How many values in this series the axis will drop.
        count: usize,
    },
}

impl fmt::Display for DataIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "series {}", self.series)?;
        if let Some(index) = self.index {
            write!(f, ", item {index}")?;
        }
        write!(f, ": ")?;
        match &self.kind {
            DataIssueKind::NegativeWeight => write!(f, "area weight must be non-negative"),
            DataIssueKind::LengthMismatch {
                left,
                left_len,
                right,
                right_len,
            } => write!(
                f,
                "{left} has length {left_len}, but {right} has length {right_len}"
            ),
            DataIssueKind::NonFiniteX => write!(f, "x coordinate is not finite"),
            DataIssueKind::DecreasingX => write!(f, "x coordinates are not sorted ascending"),
            DataIssueKind::NonFiniteValue { field } => write!(f, "{field} is not finite"),
            DataIssueKind::InvalidOhlcRange => {
                write!(f, "OHLC low/high do not enclose open and close")
            }
            DataIssueKind::HistogramShape { buckets, counts } => write!(
                f,
                "histogram requires buckets.len() == counts.len() + 1 ({buckets} vs {counts})"
            ),
            DataIssueKind::HistogramGridMismatch { left, right } => write!(
                f,
                "stacked histograms require a shared bucket grid ({left} vs {right} edges)"
            ),
            DataIssueKind::NonIncreasingBucket => {
                write!(f, "histogram bucket boundaries must strictly increase")
            }
            DataIssueKind::InvertedBand => write!(f, "band lower value exceeds upper value"),
            DataIssueKind::InvalidSegmentRange => {
                write!(f, "state segment end must be greater than its start")
            }
            DataIssueKind::OverlappingSegments => {
                write!(f, "state segments overlap or are not sorted")
            }
            DataIssueKind::NonPositiveOnLogAxis { value, count } => write!(
                f,
                "{value} cannot be placed on a logarithmic y axis \
                 ({count} value(s) in this series will be drawn as gaps)"
            ),
        }
    }
}

impl std::error::Error for DataIssue {}

pub(crate) fn validate_chart_data(data: &ChartData) -> Vec<DataIssue> {
    let mut issues = Vec::new();
    match data {
        ChartData::Pie(items) | ChartData::Treemap(items) | ChartData::HostMap(items) => {
            for (i, item) in items.iter().enumerate() {
                if !item.value.is_finite() {
                    issues.push(DataIssue {
                        series: i,
                        index: None,
                        kind: DataIssueKind::NonFiniteValue { field: "value" },
                    });
                } else if item.value < 0.0 && !matches!(data, ChartData::HostMap(_)) {
                    issues.push(DataIssue {
                        series: i,
                        index: None,
                        kind: DataIssueKind::NegativeWeight,
                    });
                }
            }
        }
        ChartData::Lines(series)
        | ChartData::Areas(series)
        | ChartData::Bars(series)
        | ChartData::Points(series)
        | ChartData::Scatter(series)
        | ChartData::Heatmap(series)
        | ChartData::Step(series) => {
            for (si, s) in series.iter().enumerate() {
                check_lengths(&mut issues, si, "xs", s.xs.len(), "ys", s.ys.len());
                check_xs(&mut issues, si, &s.xs);
                for (index, value) in s.ys.iter().copied().enumerate() {
                    if value.is_infinite() {
                        issues.push(DataIssue {
                            series: si,
                            index: Some(index),
                            kind: DataIssueKind::NonFiniteValue { field: "y" },
                        });
                    }
                }
            }
        }
        ChartData::Ohlc(series) => {
            for (si, s) in series.iter().enumerate() {
                check_x_values(&mut issues, si, s.ticks.iter().map(|tick| tick.ts));
                for (i, tick) in s.ticks.iter().enumerate() {
                    for (field, value) in [
                        ("open", tick.open),
                        ("high", tick.high),
                        ("low", tick.low),
                        ("close", tick.close),
                    ] {
                        if !value.is_finite() {
                            issues.push(DataIssue {
                                series: si,
                                index: Some(i),
                                kind: DataIssueKind::NonFiniteValue { field },
                            });
                        }
                    }
                    if tick.low.is_finite()
                        && tick.high.is_finite()
                        && (tick.low > tick.high
                            || tick.low > tick.open
                            || tick.low > tick.close
                            || tick.high < tick.open
                            || tick.high < tick.close)
                    {
                        issues.push(DataIssue {
                            series: si,
                            index: Some(i),
                            kind: DataIssueKind::InvalidOhlcRange,
                        });
                    }
                }
            }
        }
        ChartData::Histogram(series) => {
            for (si, s) in series.iter().enumerate() {
                if s.buckets.len() != s.counts.len().saturating_add(1) {
                    issues.push(DataIssue {
                        series: si,
                        index: None,
                        kind: DataIssueKind::HistogramShape {
                            buckets: s.buckets.len(),
                            counts: s.counts.len(),
                        },
                    });
                }
                for (i, pair) in s.buckets.windows(2).enumerate() {
                    if !pair[0].is_finite() || !pair[1].is_finite() {
                        issues.push(DataIssue {
                            series: si,
                            index: Some(i),
                            kind: DataIssueKind::NonFiniteX,
                        });
                    } else if pair[1] <= pair[0] {
                        issues.push(DataIssue {
                            series: si,
                            index: Some(i + 1),
                            kind: DataIssueKind::NonIncreasingBucket,
                        });
                    }
                }
                for (i, count) in s.counts.iter().enumerate() {
                    if !count.is_finite() {
                        issues.push(DataIssue {
                            series: si,
                            index: Some(i),
                            kind: DataIssueKind::NonFiniteValue { field: "count" },
                        });
                    }
                }
            }
        }
        ChartData::HBar(series) => {
            for (si, s) in series.iter().enumerate() {
                check_lengths(
                    &mut issues,
                    si,
                    "categories",
                    s.categories.len(),
                    "values",
                    s.values.len(),
                );
                for (i, value) in s.values.iter().enumerate() {
                    if !value.is_finite() {
                        issues.push(DataIssue {
                            series: si,
                            index: Some(i),
                            kind: DataIssueKind::NonFiniteValue { field: "value" },
                        });
                    }
                }
            }
        }
        ChartData::StateTimeline(series) => {
            for (si, row) in series.iter().enumerate() {
                let mut previous_end = f64::NEG_INFINITY;
                for (i, segment) in row.segments.iter().enumerate() {
                    if !segment.start_ms.is_finite()
                        || !segment.end_ms.is_finite()
                        || segment.end_ms <= segment.start_ms
                    {
                        issues.push(DataIssue {
                            series: si,
                            index: Some(i),
                            kind: DataIssueKind::InvalidSegmentRange,
                        });
                    }
                    if segment.start_ms < previous_end {
                        issues.push(DataIssue {
                            series: si,
                            index: Some(i),
                            kind: DataIssueKind::OverlappingSegments,
                        });
                    }
                    previous_end = previous_end.max(segment.end_ms);
                }
            }
        }
        ChartData::Band(series) => {
            for (si, s) in series.iter().enumerate() {
                for (field, len) in [
                    ("center", s.center.len()),
                    ("lower", s.lower.len()),
                    ("upper", s.upper.len()),
                ] {
                    check_lengths(&mut issues, si, "xs", s.xs.len(), field, len);
                }
                check_xs(&mut issues, si, &s.xs);
                let n = s.lower.len().min(s.upper.len());
                for i in 0..n {
                    if s.lower[i].is_finite() && s.upper[i].is_finite() && s.lower[i] > s.upper[i] {
                        issues.push(DataIssue {
                            series: si,
                            index: Some(i),
                            kind: DataIssueKind::InvertedBand,
                        });
                    }
                }
            }
        }
    }
    issues
}

fn check_lengths(
    issues: &mut Vec<DataIssue>,
    series: usize,
    left: &'static str,
    left_len: usize,
    right: &'static str,
    right_len: usize,
) {
    if left_len != right_len {
        issues.push(DataIssue {
            series,
            index: None,
            kind: DataIssueKind::LengthMismatch {
                left,
                left_len,
                right,
                right_len,
            },
        });
    }
}

fn check_xs(issues: &mut Vec<DataIssue>, series: usize, xs: &[f64]) {
    check_x_values(issues, series, xs.iter().copied());
}

fn check_x_values(issues: &mut Vec<DataIssue>, series: usize, xs: impl IntoIterator<Item = f64>) {
    let mut previous: Option<f64> = None;
    for (i, x) in xs.into_iter().enumerate() {
        if !x.is_finite() {
            issues.push(DataIssue {
                series,
                index: Some(i),
                kind: DataIssueKind::NonFiniteX,
            });
        }
        if let Some(previous) = previous
            && x.is_finite()
            && previous.is_finite()
            && x < previous
        {
            issues.push(DataIssue {
                series,
                index: Some(i),
                kind: DataIssueKind::DecreasingX,
            });
        }
        if x.is_finite() {
            previous = Some(x);
        }
    }
}

/// Spec-aware validation: everything [`validate_chart_data`] reports, plus
/// issues that only exist for a particular axis configuration.
///
/// The one that matters today is a logarithmic y axis over data containing
/// zero or negative values. Rendering gaps those points by design, which is
/// correct but silent — a panel can lose most of its data without a visible
/// error. This surfaces it at the ingestion boundary instead.
pub(crate) fn validate_chart_data_for_spec(
    data: &ChartData,
    spec: &crate::spec::ChartSpec,
) -> Vec<DataIssue> {
    let mut issues = validate_chart_data(data);
    if matches!(
        spec.layout,
        crate::spec::SeriesLayout::Stacked | crate::spec::SeriesLayout::StackedPercent
    ) && let ChartData::Histogram(series) = data
        && let Some(first) = series.first()
    {
        for (si, current) in series.iter().enumerate().skip(1) {
            if current.buckets != first.buckets {
                issues.push(DataIssue {
                    series: si,
                    index: None,
                    kind: DataIssueKind::HistogramGridMismatch {
                        left: first.buckets.len(),
                        right: current.buckets.len(),
                    },
                });
            }
        }
    }
    if !matches!(spec.y_scale, crate::scale::ScaleKind::Log) {
        return issues;
    }
    // Row-indexed kinds position rows by index, so a log y axis never applies.
    if matches!(data, ChartData::HBar(_) | ChartData::StateTimeline(_)) {
        return issues;
    }
    let mut report = |series: usize, values: &mut dyn Iterator<Item = (usize, f64)>| {
        let mut first: Option<(usize, f64)> = None;
        let mut count = 0usize;
        for (index, value) in values {
            if value.is_finite() && value <= 0.0 {
                first.get_or_insert((index, value));
                count += 1;
            }
        }
        // One issue per series: a series of a million zeros is one problem,
        // not a million of them.
        if let Some((index, value)) = first {
            issues.push(DataIssue {
                series,
                index: Some(index),
                kind: DataIssueKind::NonPositiveOnLogAxis { value, count },
            });
        }
    };
    match data {
        ChartData::Ohlc(series) => {
            for (si, s) in series.iter().enumerate() {
                report(si, &mut s.ticks.iter().enumerate().map(|(i, t)| (i, t.low)));
            }
        }
        ChartData::Histogram(series) => {
            for (si, s) in series.iter().enumerate() {
                let mut total = 0.0;
                report(
                    si,
                    &mut s.counts.iter().enumerate().map(|(i, count)| {
                        if count.is_finite() {
                            total = crate::series::finite_add(total, *count);
                        }
                        (i, total)
                    }),
                );
            }
        }
        ChartData::Band(series) => {
            for (si, s) in series.iter().enumerate() {
                report(
                    si,
                    &mut s
                        .lower
                        .iter()
                        .copied()
                        .chain(s.center.iter().copied())
                        .chain(s.upper.iter().copied())
                        .enumerate(),
                );
            }
        }
        _ => {
            for (si, s) in data.point_series().iter().enumerate() {
                report(si, &mut s.ys.iter().copied().enumerate());
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::series::{HistogramSeries, SeriesData};

    #[test]
    fn reports_point_shape_and_order_without_panicking() {
        let data = ChartData::Lines(vec![SeriesData {
            name: "bad".into(),
            xs: vec![2.0, 1.0, f64::NAN],
            ys: vec![4.0],
            color: None,
        }]);
        let issues = validate_chart_data(&data);
        assert!(
            issues
                .iter()
                .any(|issue| matches!(issue.kind, DataIssueKind::LengthMismatch { .. }))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.kind == DataIssueKind::DecreasingX)
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.kind == DataIssueKind::NonFiniteX)
        );
    }

    #[test]
    fn reports_malformed_histogram_shape() {
        let data = ChartData::Histogram(vec![HistogramSeries {
            name: "bad".into(),
            buckets: vec![0.0, 2.0, 1.0],
            counts: vec![1.0],
            color: None,
            cumulative: false,
        }]);
        let issues = validate_chart_data(&data);
        assert!(
            issues
                .iter()
                .any(|issue| matches!(issue.kind, DataIssueKind::HistogramShape { .. }))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.kind == DataIssueKind::NonIncreasingBucket)
        );
    }
    #[test]
    fn reports_infinite_point_values_and_all_log_band_components() {
        let points = ChartData::Lines(vec![SeriesData {
            name: "points".into(),
            xs: vec![0.0],
            ys: vec![f64::INFINITY],
            color: None,
        }]);
        assert!(matches!(
            points.validate().unwrap_err()[0].kind,
            DataIssueKind::NonFiniteValue { field: "y" }
        ));

        let band = ChartData::Band(vec![crate::series::BandSeries {
            name: "band".into(),
            xs: vec![0.0],
            center: vec![1.0],
            lower: vec![2.0],
            upper: vec![0.0],
            color: None,
        }]);
        let issues = band
            .validate_with(&crate::spec::ChartSpec::line(crate::units::Unit::None).with_log_y())
            .unwrap_err();
        assert!(issues.iter().any(|issue| matches!(
            issue.kind,
            DataIssueKind::NonPositiveOnLogAxis { count: 1, .. }
        )));
    }

    #[test]
    fn log_axis_validation_counts_values_it_will_gap() {
        use crate::series::SeriesData;
        use crate::spec::ChartSpec;
        use crate::units::Unit;

        let data = ChartData::Lines(vec![
            SeriesData {
                name: "has-zeros".into(),
                xs: vec![0.0, 1.0, 2.0, 3.0],
                ys: vec![5.0, 0.0, -2.0, f64::NAN],
                color: None,
            },
            SeriesData {
                name: "all-positive".into(),
                xs: vec![0.0, 1.0],
                ys: vec![1.0, 2.0],
                color: None,
            },
        ]);
        let spec = ChartSpec::line(Unit::Millis).with_log_y();

        // A linear axis draws these fine, so nothing is reported.
        assert!(data.validate().is_ok());

        let issues = data.validate_with(&spec).expect_err("log axis gaps values");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].series, 0);
        assert_eq!(issues[0].index, Some(1));
        // NaN is a declared gap, not a log-axis casualty; only 0 and -2 count.
        assert_eq!(
            issues[0].kind,
            DataIssueKind::NonPositiveOnLogAxis {
                value: 0.0,
                count: 2
            }
        );
    }

    #[test]
    fn row_indexed_kinds_are_exempt_from_log_axis_validation() {
        use crate::series::HBarSeries;
        use crate::spec::ChartSpec;
        use crate::units::Unit;

        // HBar rows are positioned by index, so its y axis is never logarithmic.
        let data = ChartData::HBar(vec![HBarSeries {
            name: "budget".into(),
            categories: vec!["a".into(), "b".into()],
            values: vec![0.0, -3.0],
            color: None,
        }]);
        assert!(
            data.validate_with(&ChartSpec::hbar(Unit::None).with_log_y())
                .is_ok()
        );
    }

    #[test]
    fn stacked_histograms_report_incompatible_grids() {
        use crate::series::HistogramSeries;
        use crate::spec::ChartSpec;
        use crate::units::Unit;
        let data = ChartData::Histogram(vec![
            HistogramSeries {
                buckets: vec![0.0, 1.0, 2.0],
                counts: vec![1.0, 2.0],
                ..Default::default()
            },
            HistogramSeries {
                buckets: vec![0.0, 2.0],
                counts: vec![3.0],
                ..Default::default()
            },
        ]);
        let mut spec = ChartSpec::histogram(Unit::None);
        spec.layout = crate::spec::SeriesLayout::Stacked;
        let issues = data.validate_with(&spec).expect_err("stack grid mismatch");
        assert!(
            issues
                .iter()
                .any(|issue| matches!(issue.kind, DataIssueKind::HistogramGridMismatch { .. }))
        );
    }
}
