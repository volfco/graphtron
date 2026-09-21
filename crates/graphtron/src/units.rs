/// Display unit for panel values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Unit {
    /// Raw number with SI abbreviation (1.2k, 3.4M).
    #[default]
    Short,
    /// 0..1 ratio rendered as percent.
    Percent01,
    /// Already 0..100.
    Percent,
    /// Bytes, IEC (KiB/MiB/GiB).
    Bytes,
    BytesPerSec,
    Seconds,
    Millis,
    /// Plain count, no abbreviation.
    None,
}

impl Unit {
    pub fn format(&self, v: f64) -> String {
        if !v.is_finite() {
            return "—".to_string();
        }
        match self {
            Unit::Short => si(v, ""),
            Unit::Percent01 => percent(v * 100.0),
            Unit::Percent => percent(v),
            Unit::Bytes => iec(v, "B"),
            Unit::BytesPerSec => iec(v, "B/s"),
            Unit::Seconds => duration(v),
            Unit::Millis => duration(v / 1000.0),
            Unit::None => trim_zeros(v, 2),
        }
    }
}

fn trim_zeros(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn si(v: f64, suffix: &str) -> String {
    let abs = v.abs();
    let (scaled, prefix) = if abs >= 1e18 {
        (v / 1e18, "E")
    } else if abs >= 1e15 {
        (v / 1e15, "P")
    } else if abs >= 1e12 {
        (v / 1e12, "T")
    } else if abs >= 1e9 {
        (v / 1e9, "G")
    } else if abs >= 1e6 {
        (v / 1e6, "M")
    } else if abs >= 1e3 {
        (v / 1e3, "k")
    } else if abs > 0.0 && abs < 1e-9 {
        (v * 1e12, "p")
    } else if abs > 0.0 && abs < 1e-6 {
        (v * 1e9, "n")
    } else if abs > 0.0 && abs < 1e-3 {
        (v * 1e6, "µ")
    } else if abs > 0.0 && abs < 1.0 {
        (v * 1e3, "m")
    } else {
        (v, "")
    };
    format!("{}{}{}", trim_zeros(scaled, 2), prefix, suffix)
}

fn iec(v: f64, suffix: &str) -> String {
    let abs = v.abs();
    const KI: f64 = 1024.0;
    let (scaled, prefix) = if abs >= KI * KI * KI * KI * KI * KI {
        (v / (KI * KI * KI * KI * KI * KI), "Ei")
    } else if abs >= KI * KI * KI * KI * KI {
        (v / (KI * KI * KI * KI * KI), "Pi")
    } else if abs >= KI * KI * KI * KI {
        (v / (KI * KI * KI * KI), "Ti")
    } else if abs >= KI * KI * KI {
        (v / (KI * KI * KI), "Gi")
    } else if abs >= KI * KI {
        (v / (KI * KI), "Mi")
    } else if abs >= KI {
        (v / KI, "Ki")
    } else {
        (v, "")
    };
    format!("{} {}{}", trim_zeros(scaled, 2), prefix, suffix)
}

fn duration(secs: f64) -> String {
    let abs = secs.abs();
    if abs >= 3600.0 {
        format!("{}h", trim_zeros(secs / 3600.0, 1))
    } else if abs >= 60.0 {
        format!("{}m", trim_zeros(secs / 60.0, 1))
    } else if abs >= 1.0 {
        format!("{}s", trim_zeros(secs, 2))
    } else if abs >= 1e-3 {
        format!("{}ms", trim_zeros(secs * 1e3, 2))
    } else if abs >= 1e-6 {
        format!("{}µs", trim_zeros(secs * 1e6, 2))
    } else if abs >= 1e-9 {
        format!("{}ns", trim_zeros(secs * 1e9, 2))
    } else if abs == 0.0 {
        "0s".to_string()
    } else {
        format!("{}ps", trim_zeros(secs * 1e12, 2))
    }
}

fn percent(value: f64) -> String {
    let abs = value.abs();
    let decimals = if abs >= 1.0 {
        1
    } else if abs >= 0.01 {
        2
    } else {
        4
    };
    format!("{}%", trim_zeros(value, decimals))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(Unit::Short.format(1_234.0), "1.23k");
        assert_eq!(Unit::Percent01.format(0.123), "12.3%");
        assert_eq!(Unit::Bytes.format(1536.0), "1.5 KiB");
        assert_eq!(Unit::Seconds.format(0.005), "5ms");
        assert_eq!(Unit::Millis.format(250.0), "250ms");
        assert_eq!(Unit::Short.format(f64::NAN), "—");
        assert_eq!(Unit::Short.format(0.000_002), "2µ");
        assert_eq!(Unit::Short.format(2_000_000_000_000.0), "2T");
        assert_eq!(Unit::Percent.format(0.004), "0.004%");
        assert_eq!(Unit::Seconds.format(0.000_000_005), "5ns");
        assert_eq!(Unit::Bytes.format(1024_f64.powi(5)), "1 PiB");
    }
}
