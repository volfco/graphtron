/// Owned RGBA color for themes and render options.
///
/// Replaces the `&'static str` CSS color fields that made runtime theming
/// (user-chosen accents, CSS-variable-driven palettes) impossible: an `Rgba`
/// can be constructed at runtime and serialized to a CSS color on demand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// Alpha in [0, 1].
    pub a: f64,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: f64) -> Self {
        Self {
            r,
            g,
            b,
            a: sanitize_alpha(a),
        }
    }

    /// Opaque color from a 0xRRGGBB literal.
    pub const fn hex(rgb: u32) -> Self {
        Self::hex_a(rgb, 1.0)
    }

    /// Color from a 0xRRGGBB literal with explicit alpha.
    pub const fn hex_a(rgb: u32, a: f64) -> Self {
        Self::new(
            ((rgb >> 16) & 0xff) as u8,
            ((rgb >> 8) & 0xff) as u8,
            (rgb & 0xff) as u8,
            a,
        )
    }

    pub const fn with_alpha(self, a: f64) -> Self {
        Self::new(self.r, self.g, self.b, a)
    }

    /// CSS color string. Fully-opaque colors use the compact `#rrggbb` form.
    pub fn to_css(&self) -> String {
        let alpha = sanitize_alpha(self.a);
        if alpha >= 1.0 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("rgba({},{},{},{})", self.r, self.g, self.b, alpha)
        }
    }
}

const fn sanitize_alpha(alpha: f64) -> f64 {
    if alpha.is_nan() || alpha < 0.0 {
        0.0
    } else if alpha > 1.0 {
        1.0
    } else {
        alpha
    }
}

/// Color mapping for heatmap cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeatmapColorScale {
    /// Perceptually uniform, colorblind-safe (the default).
    #[default]
    Viridis,
    /// Perceptually uniform, dark-to-bright magma.
    Magma,
    /// The original neon ramp (dark → cyan → magenta → white) for the
    /// cyberpunk aesthetic. Not perceptually uniform.
    Neon,
}

/// Viridis anchor colors, uniformly sampled at t = 0, 1/15, …, 1.
const VIRIDIS: [(u8, u8, u8); 16] = [
    (68, 1, 84),
    (72, 26, 108),
    (71, 47, 125),
    (65, 68, 135),
    (57, 86, 140),
    (49, 104, 142),
    (42, 120, 142),
    (35, 136, 142),
    (31, 152, 139),
    (34, 168, 132),
    (53, 183, 121),
    (84, 197, 104),
    (122, 209, 81),
    (165, 219, 54),
    (210, 226, 27),
    (253, 231, 37),
];

/// Magma anchor colors, uniformly sampled at t = 0, 1/15, …, 1.
const MAGMA: [(u8, u8, u8); 16] = [
    (0, 0, 4),
    (10, 7, 34),
    (28, 16, 68),
    (53, 15, 106),
    (80, 18, 123),
    (105, 28, 128),
    (130, 37, 129),
    (156, 46, 127),
    (182, 54, 121),
    (207, 68, 111),
    (227, 89, 99),
    (241, 115, 90),
    (250, 144, 92),
    (254, 174, 107),
    (254, 203, 132),
    (252, 253, 191),
];

impl HeatmapColorScale {
    /// Map a normalized value `t ∈ [0, 1]` to a color. Perceptual scales
    /// interpolate between anchor samples; `Neon` reproduces the legacy ramp.
    pub fn color(&self, t: f64) -> Rgba {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Viridis => lerp_anchors(&VIRIDIS, t),
            Self::Magma => lerp_anchors(&MAGMA, t),
            Self::Neon => neon_ramp(t),
        }
    }
}

/// Resolve a heatmap cell color from the chart's already-computed y domain.
/// Keeping this independent of the raw series makes canvas rendering and
/// pointer hover agree without rescanning every heatmap cell on mouse move.
pub(crate) fn heatmap_color_for_value(
    scale: HeatmapColorScale,
    value: f64,
    domain_start: f64,
    domain_end: f64,
) -> Rgba {
    let lo = domain_start.min(domain_end);
    let hi = domain_start.max(domain_end);
    let range = hi - lo;
    let t = if value.is_finite() && lo.is_finite() && range.is_finite() && range > 0.0 {
        ((value - lo) / range).clamp(0.0, 1.0)
    } else {
        // Flat domains intentionally keep the old midpoint behavior so a
        // single-valued / single-column matrix remains visible.
        0.5
    };
    scale.color(t)
}

fn lerp_anchors(anchors: &[(u8, u8, u8); 16], t: f64) -> Rgba {
    let pos = t * 15.0;
    let i = (pos.floor() as usize).min(14);
    let f = pos - i as f64;
    let (r0, g0, b0) = anchors[i];
    let (r1, g1, b1) = anchors[i + 1];
    Rgba::new(
        (r0 as f64 + (r1 as f64 - r0 as f64) * f).round() as u8,
        (g0 as f64 + (g1 as f64 - g0 as f64) * f).round() as u8,
        (b0 as f64 + (b1 as f64 - b0 as f64) * f).round() as u8,
        1.0,
    )
}

/// Legacy cyberpunk ramp: dark → cyan → magenta-white.
fn neon_ramp(t: f64) -> Rgba {
    if t < 0.33 {
        let s = t / 0.33;
        Rgba::new(0, (s * 255.0) as u8, (s * 255.0) as u8, 0.85)
    } else if t < 0.66 {
        let s = (t - 0.33) / 0.33;
        Rgba::new(
            (255.0 * s) as u8,
            (255.0 * (1.0 - s * 0.5)) as u8,
            255,
            0.85,
        )
    } else {
        let s = (t - 0.66) / 0.34;
        let v = (200.0 + 55.0 * s) as u8;
        Rgba::new(v, v, v, 0.90)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viridis_endpoints_and_monotone_brightness() {
        let lo = HeatmapColorScale::Viridis.color(0.0);
        let hi = HeatmapColorScale::Viridis.color(1.0);
        assert_eq!((lo.r, lo.g, lo.b), (68, 1, 84));
        assert_eq!((hi.r, hi.g, hi.b), (253, 231, 37));
        // Perceptual scales brighten monotonically — sample luma along t.
        let luma = |c: Rgba| 0.2126 * c.r as f64 + 0.7152 * c.g as f64 + 0.0722 * c.b as f64;
        let mut last = -1.0;
        for i in 0..=20 {
            let l = luma(HeatmapColorScale::Viridis.color(i as f64 / 20.0));
            assert!(l > last, "luma not monotone at t={}", i as f64 / 20.0);
            last = l;
        }
    }

    #[test]
    fn scale_clamps_out_of_range() {
        assert_eq!(
            HeatmapColorScale::Magma.color(-1.0),
            HeatmapColorScale::Magma.color(0.0)
        );
        assert_eq!(
            HeatmapColorScale::Magma.color(2.0),
            HeatmapColorScale::Magma.color(1.0)
        );
    }

    #[test]
    fn heatmap_value_color_keeps_flat_domains_visible_at_midpoint() {
        assert_eq!(
            heatmap_color_for_value(HeatmapColorScale::Viridis, 42.0, 42.0, 42.0),
            HeatmapColorScale::Viridis.color(0.5)
        );
    }

    #[test]
    fn hex_roundtrip() {
        let c = Rgba::hex(0x5794F2);
        assert_eq!((c.r, c.g, c.b, c.a), (0x57, 0x94, 0xF2, 1.0));
        assert_eq!(c.to_css(), "#5794f2");
    }

    #[test]
    fn alpha_is_sanitized_at_construction_boundaries() {
        assert_eq!(Rgba::new(1, 2, 3, -4.0).a, 0.0);
        assert_eq!(Rgba::new(1, 2, 3, 4.0).a, 1.0);
        assert_eq!(Rgba::new(1, 2, 3, f64::NAN).a, 0.0);
    }

    #[test]
    fn alpha_uses_rgba_form() {
        let c = Rgba::hex_a(0x00FFFF, 0.15);
        assert_eq!(c.to_css(), "rgba(0,255,255,0.15)");
        assert_eq!(c.with_alpha(1.0).to_css(), "#00ffff");
    }
}
