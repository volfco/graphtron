/// Classic 1-2-5 "nice number" linear ticks covering [min, max].
pub fn linear_ticks(min: f64, max: f64, max_ticks: usize) -> Vec<f64> {
    if !min.is_finite() || !max.is_finite() || max_ticks == 0 {
        return vec![];
    }
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    if min == max {
        return vec![min];
    }
    // Tick generation is a display operation; never let an untrusted budget
    // turn into an unbounded allocation/loop.
    let budget = max_ticks.clamp(1, 1_024);
    let count = budget as f64;
    // Compute the span in a scaled coordinate when direct subtraction
    // overflows. Conversely, retain the direct subtraction for small spans so
    // that close values around a large offset are not rounded together.
    let raw_step = {
        let span = max - min;
        if span.is_finite() {
            span / count
        } else {
            let magnitude = max.abs().max(min.abs());
            let scaled_span = max / magnitude - min / magnitude;
            scaled_span * (magnitude / count)
        }
    };
    if !raw_step.is_finite() || raw_step <= 0.0 {
        return vec![min, max];
    }
    let mag = 10f64.powf(raw_step.log10().floor());
    let norm = raw_step / mag;
    let step = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    } * mag;

    let first = (min / step).floor() * step;
    if !step.is_finite() || step <= 0.0 || !first.is_finite() {
        return vec![min, max];
    }
    let mut ticks = Vec::new();
    let mut previous = f64::NEG_INFINITY;
    // A nice step needs at most roughly max_ticks + two aligned positions.
    // Indexing also makes termination independent of floating-point addition.
    for index in 0..=budget + 2 {
        let t = first + index as f64 * step;
        if !t.is_finite() || t <= previous {
            break;
        }
        if t > max {
            break;
        }
        previous = t;
        if t >= min {
            ticks.push(t);
        }
    }
    // At very large offsets the representable spacing can exceed the nice
    // step. Keep the finite endpoints rather than returning duplicate ticks
    // or silently dropping the domain.
    if ticks.len() < 2 {
        return vec![min, max];
    }
    ticks
}

/// Expand a strictly-positive domain to the enclosing 1-2-5 log boundaries,
/// so a log axis starts and ends on a readable tick instead of mid-decade.
pub fn nice_log_domain(min: f64, max: f64) -> (f64, f64) {
    if !min.is_finite() || !max.is_finite() || min <= 0.0 || max <= 0.0 {
        return (min, max);
    }
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    // Subnormal positive values have a finite logarithm, but the enclosing
    // 1/2/5 boundary can underflow to zero. Keep the public log domain above
    // the smallest representable scale instead of returning `-inf`.
    let min = min.max(crate::scale::LOG_EPSILON);
    let max = max.max(crate::scale::LOG_EPSILON);
    if min == max {
        return (min, (min * 10.0).min(f64::MAX));
    }
    (log_floor(min), log_ceil(max))
}

/// Largest 1/2/5 x 10^k boundary at or below `value` (strictly positive).
fn log_floor(value: f64) -> f64 {
    let value = value.max(crate::scale::LOG_EPSILON);
    let decade = value.log10().floor();
    let base = 10f64.powf(decade);
    let mantissa = value / base;
    let m = if mantissa >= 5.0 {
        5.0
    } else if mantissa >= 2.0 {
        2.0
    } else {
        1.0
    };
    (base * m).max(crate::scale::LOG_EPSILON)
}

/// Smallest 1/2/5 x 10^k boundary at or above `value` (strictly positive).
fn log_ceil(value: f64) -> f64 {
    let value = value.max(crate::scale::LOG_EPSILON);
    let decade = value.log10().floor();
    let base = 10f64.powf(decade);
    let mantissa = value / base;
    if mantissa <= 1.0 + 1e-12 {
        base
    } else if mantissa <= 2.0 {
        base * 2.0
    } else if mantissa <= 5.0 {
        base * 5.0
    } else {
        base * 10.0
    }
    .max(crate::scale::LOG_EPSILON)
}

/// Logarithmic ticks in **data** space covering `[min, max]` (both strictly
/// positive). Narrow ranges get 1-2-5 sub-decade ticks; wide ranges thin to
/// every k-th power of ten, so a latency axis spanning microseconds to
/// minutes stays readable rather than emitting hundreds of labels.
pub fn log_ticks(min: f64, max: f64, max_ticks: usize) -> Vec<f64> {
    if !min.is_finite() || !max.is_finite() || min <= 0.0 || max <= 0.0 || max_ticks == 0 {
        return vec![];
    }
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    if min == max {
        return vec![min];
    }
    let lo = min.log10().floor() as i32;
    let hi = max.log10().ceil() as i32;
    let decades = (hi - lo).max(1);
    let lo_bound = min * (1.0 - 1e-9);
    let hi_bound = max * (1.0 + 1e-9);
    let mut ticks = Vec::new();

    if decades <= 3 {
        for decade in lo..=hi {
            let base = 10f64.powi(decade);
            for mantissa in [1.0, 2.0, 5.0] {
                let value = base * mantissa;
                if value >= lo_bound && value <= hi_bound {
                    ticks.push(value);
                }
            }
        }
    } else {
        // One tick per k decades, where k keeps the count under the budget.
        let step = ((decades as f64) / max_ticks.max(1) as f64).ceil().max(1.0) as i32;
        let mut decade = lo;
        while decade <= hi {
            let value = 10f64.powi(decade);
            if value >= lo_bound && value <= hi_bound {
                ticks.push(value);
            }
            decade += step;
        }
    }

    // A sub-decade window ("1.4 to 3.1") can yield too few log boundaries to
    // read an axis from; a linear ladder describes it better.
    if ticks.len() < 3 && decades <= 1 {
        let linear = linear_ticks(min, max, max_ticks);
        if linear.len() > ticks.len() {
            return linear;
        }
    }
    ticks
}

/// Time-axis tick step ladder, in milliseconds.
const TIME_STEPS_MS: &[i64] = &[
    1_000,
    5_000,
    15_000,
    30_000,
    60_000,
    5 * 60_000,
    15 * 60_000,
    30 * 60_000,
    3_600_000,
    3 * 3_600_000,
    6 * 3_600_000,
    12 * 3_600_000,
    86_400_000,
    7 * 86_400_000,
    30 * 86_400_000,
];

#[derive(Debug, Clone, PartialEq)]
pub struct TimeTick {
    pub ms: i64,
    pub label: String,
}

/// Time-aware ticks: picks a step from the ladder, aligns the first tick to
/// a step boundary, and chooses a label format from the visible span. Ranges
/// that cross a year boundary label days with the year so Dec→Jan reads
/// unambiguously (C11).
pub fn time_ticks(from_ms: i64, to_ms: i64, max_ticks: usize) -> Vec<TimeTick> {
    if to_ms <= from_ms || max_ticks == 0 {
        return vec![];
    }
    // Do the span arithmetic in i128: a valid i64 domain can be wider than
    // i64::MAX, and `to - from`/`t += step` must not wrap into an endless loop.
    let span = to_ms as i128 - from_ms as i128;
    let target = span / max_ticks.max(1) as i128;
    let step = TIME_STEPS_MS
        .iter()
        .copied()
        .find(|s| (*s as i128) >= target)
        .unwrap_or(*TIME_STEPS_MS.last().unwrap()) as i128;

    let crosses_year = {
        let (y0, ..) = civil_from_unix(from_ms.div_euclid(1000));
        let (y1, ..) = civil_from_unix(to_ms.div_euclid(1000));
        y0 != y1
    };
    let fmt = match (
        label_format(span.min(i64::MAX as i128) as i64),
        crosses_year,
    ) {
        (TimeFormat::Day, true) => TimeFormat::DayYear,
        (f, _) => f,
    };
    let first = (from_ms as i128).div_euclid(step) * step;
    let mut ticks = Vec::new();
    let mut t = first;
    let limit = (to_ms as i128).min((from_ms as i128) + step * max_ticks as i128);
    // A malicious/very large range must still be bounded even when the time
    // ladder cannot provide a step large enough to cover it.
    let output_limit = max_ticks.saturating_add(2);
    while t <= limit && ticks.len() < output_limit {
        if t >= from_ms as i128
            && let Ok(ms) = i64::try_from(t)
        {
            ticks.push(TimeTick {
                ms,
                label: format_ts(ms, fmt),
            });
        }
        t += step;
    }
    ticks
}

/// Ticks for a categorical axis with domain `[0, labels.len()]`: one tick per
/// label, positioned at each band's center (`i + 0.5`).
pub fn category_ticks(labels: &[String]) -> Vec<(f64, String)> {
    labels
        .iter()
        .enumerate()
        .map(|(i, l)| (i as f64 + 0.5, l.clone()))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeFormat {
    Hms,
    Hm,
    DayHm,
    Day,
    /// `YYYY-MM-DD` — used when the visible range crosses a year boundary.
    DayYear,
}

fn label_format(span_ms: i64) -> TimeFormat {
    if span_ms < 5 * 60_000 {
        TimeFormat::Hms
    } else if span_ms < 86_400_000 {
        TimeFormat::Hm
    } else if span_ms < 7 * 86_400_000 {
        TimeFormat::DayHm
    } else {
        TimeFormat::Day
    }
}

/// Minimal UTC timestamp formatter (no chrono dep in the core).
pub fn format_ts(ms: i64, fmt: TimeFormat) -> String {
    let secs = ms.div_euclid(1000);
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    match fmt {
        TimeFormat::Hms => format!("{h:02}:{mi:02}:{s:02}"),
        TimeFormat::Hm => format!("{h:02}:{mi:02}"),
        TimeFormat::DayHm => format!("{mo:02}-{d:02} {h:02}:{mi:02}"),
        TimeFormat::Day => format!("{mo:02}-{d:02}"),
        TimeFormat::DayYear => format!("{y}-{mo:02}-{d:02}"),
    }
}

/// Full timestamp for tooltips: `MM-DD HH:MM:SS.mmm`.
pub fn format_ts_full(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let (_, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}.{millis:03}")
}

/// Days-from-epoch → civil date (Howard Hinnant's algorithm), plus time.
fn civil_from_unix(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let h = (rem / 3600) as u32;
    let mi = ((rem % 3600) / 60) as u32;
    let s = (rem % 60) as u32;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, h, mi, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_ticks_walk_1_2_5_within_a_few_decades() {
        let ticks = log_ticks(1.0, 100.0, 6);
        assert_eq!(ticks, vec![1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0]);
    }

    #[test]
    fn log_ticks_thin_to_decades_over_a_wide_span() {
        let ticks = log_ticks(1e-3, 1e6, 5);
        assert!(ticks.len() <= 6, "{ticks:?}");
        assert!(
            ticks
                .iter()
                .all(|t| (t.log10().round() - t.log10()).abs() < 1e-9)
        );
        assert!(ticks.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn log_ticks_fall_back_to_linear_inside_one_decade() {
        // 1.4..3.1 contains only "2", which is not an axis a reader can use.
        let ticks = log_ticks(1.4, 3.1, 5);
        assert!(ticks.len() >= 3, "{ticks:?}");
    }

    #[test]
    fn log_ticks_reject_non_positive_domains() {
        assert!(log_ticks(0.0, 10.0, 5).is_empty());
        assert!(log_ticks(-5.0, -1.0, 5).is_empty());
        assert!(log_ticks(f64::NAN, 10.0, 5).is_empty());
    }

    #[test]
    fn nice_log_domain_snaps_to_enclosing_boundaries() {
        let (lo, hi) = nice_log_domain(f64::from_bits(1), f64::from_bits(1));
        assert!(lo.is_finite() && hi.is_finite() && lo > 0.0 && lo < hi);
        assert_eq!(nice_log_domain(3.0, 40.0), (2.0, 50.0));
        assert_eq!(nice_log_domain(120.0, 900.0), (100.0, 1000.0));
        // Already-nice bounds stay put.
        assert_eq!(nice_log_domain(10.0, 100.0), (10.0, 100.0));
    }

    #[test]
    fn linear_125_steps() {
        let ticks = linear_ticks(0.0, 100.0, 5);
        assert_eq!(ticks, vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
        let ticks = linear_ticks(0.0, 7.0, 5);
        assert_eq!(ticks, vec![0.0, 2.0, 4.0, 6.0]);
    }

    #[test]
    fn linear_handles_negative_and_fractional() {
        let ticks = linear_ticks(-1.3, 1.3, 4);
        assert!(ticks.contains(&0.0));
        assert!(ticks.len() >= 3);
    }

    #[test]
    fn linear_ticks_are_bounded_at_large_offsets_and_extreme_spans() {
        let narrow = linear_ticks(1e16, 1e16 + 2.0, 5);
        assert!(narrow.len() <= 7);
        assert!(narrow.windows(2).all(|w| w[1] > w[0]));
        assert!(narrow.iter().all(|value| value.is_finite()));

        let extreme = linear_ticks(-f64::MAX, f64::MAX, 5);
        assert!(extreme.len() <= 7);
        assert!(extreme.windows(2).all(|w| w[1] > w[0]));
        assert!(extreme.iter().all(|value| value.is_finite()));

        let subnormal = linear_ticks(0.0, f64::from_bits(1), 5);
        assert!(subnormal.len() <= 7);
        assert!(subnormal.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn time_ticks_bound_extreme_ranges_without_overflow() {
        let ticks = time_ticks(i64::MIN, i64::MAX, 8);
        assert!(ticks.len() <= 10, "unbounded time ticks: {}", ticks.len());
        assert!(ticks.windows(2).all(|pair| pair[0].ms < pair[1].ms));
    }

    #[test]
    fn time_ticks_align_to_step() {
        // 1h span → 5m or 15m steps; all ticks must be step-aligned.
        let from = 1_700_000_000_000;
        let to = from + 3_600_000;
        let ticks = time_ticks(from, to, 8);
        assert!(!ticks.is_empty());
        for pair in ticks.windows(2) {
            assert_eq!(pair[1].ms - pair[0].ms, ticks[1].ms - ticks[0].ms);
        }
        let step = ticks[1].ms - ticks[0].ms;
        assert!(ticks[0].ms % step == 0);
    }

    #[test]
    fn category_ticks_center_bands() {
        let labels = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let ticks = category_ticks(&labels);
        assert_eq!(ticks.len(), 3);
        assert_eq!(ticks[0], (0.5, "a".to_string()));
        assert_eq!(ticks[2], (2.5, "c".to_string()));
    }

    #[test]
    fn civil_date_known_value() {
        // 2024-01-15T10:30:45Z = 1705314645
        let (y, mo, d, h, mi, s) = civil_from_unix(1_705_314_645);
        assert_eq!((y, mo, d, h, mi, s), (2024, 1, 15, 10, 30, 45));
    }

    #[test]
    fn format_full() {
        assert_eq!(format_ts_full(1_705_314_645_123), "01-15 10:30:45.123");
    }

    #[test]
    fn year_shown_when_range_crosses_january() {
        // 2023-12-01 .. 2024-02-01 — a Dec→Jan range must label with years,
        // otherwise "12-15" vs "01-15" is ambiguous (C11).
        let from = 1_701_388_800_000; // 2023-12-01T00:00:00Z
        let to = 1_706_745_600_000; // 2024-02-01T00:00:00Z
        let ticks = time_ticks(from, to, 8);
        assert!(!ticks.is_empty());
        assert!(
            ticks
                .iter()
                .all(|t| t.label.starts_with("2023-") || t.label.starts_with("2024-")),
            "labels missing year: {:?}",
            ticks.iter().map(|t| &t.label).collect::<Vec<_>>()
        );
        // Same-year day ranges keep the short form.
        let ticks = time_ticks(from, from + 14 * 86_400_000, 8);
        assert!(ticks.iter().all(|t| t.label.starts_with("12-")));
    }
}
