use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// DPR-aware canvas wrapper: the backing store is sized at
/// css × devicePixelRatio and the context transform set so all drawing uses
/// CSS-pixel coordinates.
#[derive(Clone)]
pub struct CanvasSurface {
    pub canvas: HtmlCanvasElement,
    pub ctx: CanvasRenderingContext2d,
    pub css_w: f64,
    pub css_h: f64,
    pub dpr: f64,
    /// Effective physical-pixel/CSS ratios after backing-store rounding.
    pub scale_x: f64,
    pub scale_y: f64,
}

impl CanvasSurface {
    pub fn new(canvas: HtmlCanvasElement) -> Option<Self> {
        let ctx = canvas
            .get_context("2d")
            .ok()??
            .dyn_into::<CanvasRenderingContext2d>()
            .ok()?;
        Some(Self {
            canvas,
            ctx,
            css_w: 0.0,
            css_h: 0.0,
            dpr: 1.0,
            scale_x: 1.0,
            scale_y: 1.0,
        })
    }

    /// Resize the backing store if the CSS size or DPR changed.
    /// Compare-before-set: resizing clears the canvas and can trigger layout,
    /// so it must be a no-op when nothing changed.
    pub fn sync_size(&mut self, css_w: f64, css_h: f64) -> bool {
        if !css_w.is_finite() || !css_h.is_finite() || css_w <= 0.0 || css_h <= 0.0 {
            return false;
        }
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0)
            .max(1.0);
        if !dpr.is_finite() {
            return false;
        }
        if (css_w, css_h, dpr) == (self.css_w, self.css_h, self.dpr) && self.canvas.width() > 0 {
            return false;
        }
        self.css_w = css_w;
        self.css_h = css_h;
        self.dpr = dpr;
        let pixel_w = (css_w * dpr).round().max(1.0) as u32;
        let pixel_h = (css_h * dpr).round().max(1.0) as u32;
        self.scale_x = pixel_w as f64 / css_w;
        self.scale_y = pixel_h as f64 / css_h;
        self.canvas.set_width(pixel_w);
        self.canvas.set_height(pixel_h);
        // The backing store is rounded independently in each dimension. Use
        // the resulting physical/CSS ratios so fractional CSS sizes and DPRs
        // do not leave a partially transformed strip.
        let _ = self
            .ctx
            .set_transform(self.scale_x, 0.0, 0.0, self.scale_y, 0.0, 0.0);
        true
    }

    pub fn crisp_rect(&self, x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
        let sx = (x * self.scale_x).round() / self.scale_x;
        let sy = (y * self.scale_y).round() / self.scale_y;
        let ex = ((x + w) * self.scale_x).round() / self.scale_x;
        let ey = ((y + h) * self.scale_y).round() / self.scale_y;
        (sx.min(ex), sy.min(ey), (ex - sx).abs(), (ey - sy).abs())
    }

    pub fn clear(&self) {
        self.ctx.save();
        let _ = self.ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        self.ctx.clear_rect(
            0.0,
            0.0,
            self.canvas.width() as f64,
            self.canvas.height() as f64,
        );
        self.ctx.restore();
    }
}

/// Half-pixel alignment for crisp 1px strokes.
pub fn crisp(v: f64) -> f64 {
    crisp_stroke(v, 1.0, 1.0)
}

/// Align a stroke center to the device pixel grid while keeping the public
/// drawing API in CSS pixels. Odd device-pixel widths use half-pixel centers;
/// even widths use whole-pixel centers. This remains correct at fractional
/// DPRs such as 1.25 and 1.5.
pub fn crisp_stroke(v: f64, dpr: f64, stroke_width: f64) -> f64 {
    crisp_stroke_geometry(v, dpr, stroke_width).0
}

/// Return the snapped center and physical CSS width for a stroke. Callers
/// that need exact device alignment should apply both values to the canvas;
/// snapping only the center leaves a fractional-width stroke at fractional
/// DPRs.
pub fn crisp_stroke_geometry(v: f64, dpr: f64, stroke_width: f64) -> (f64, f64) {
    if !v.is_finite() || !dpr.is_finite() || dpr <= 0.0 {
        return (v, stroke_width);
    }
    let width = (stroke_width.abs() * dpr).round().max(1.0) as u64;
    let device = if width % 2 == 1 {
        (v * dpr).floor() + 0.5
    } else {
        (v * dpr).round()
    };
    (device / dpr, width as f64 / dpr)
}

/// Snap a filled rectangle to the DPR=1 pixel grid. Both endpoints are
/// quantized together so adjacent rectangles that share an edge retain that
/// edge without order-dependent overlap.
pub fn crisp_rect(x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
    crisp_rect_at_dpr(x, y, w, h, 1.0)
}

/// Quantize both rectangle endpoints once in device pixels, then convert back
/// to CSS coordinates. Snapping endpoints rather than independently flooring
/// origins and ceiling widths lets adjacent cells share exactly one edge and
/// avoids order-dependent overlap. Call this with `surface.dpr` for canvas
/// drawing; [`crisp_rect`] remains the DPR=1 convenience form.
pub fn crisp_rect_at_dpr(x: f64, y: f64, w: f64, h: f64, dpr: f64) -> (f64, f64, f64, f64) {
    if !dpr.is_finite() || dpr <= 0.0 || !x.is_finite() || !y.is_finite() {
        return (x, y, w, h);
    }
    let x1 = x + w;
    let y1 = y + h;
    let sx = (x * dpr).round() / dpr;
    let sy = (y * dpr).round() / dpr;
    let ex = (x1 * dpr).round() / dpr;
    let ey = (y1 * dpr).round() / dpr;
    if ex >= sx && ey >= sy {
        (sx, sy, ex - sx, ey - sy)
    } else {
        // Keep the helper useful for callers that pass reversed dimensions.
        (ex.min(sx), ey.min(sy), (ex - sx).abs(), (ey - sy).abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_neighbors_share_quantized_edges() {
        let left = crisp_rect_at_dpr(0.2, 0.0, 1.1, 1.0, 1.25);
        let right = crisp_rect_at_dpr(1.3, 0.0, 1.1, 1.0, 1.25);
        assert!((left.0 + left.2 - right.0).abs() < 1e-12);
    }

    #[test]
    fn fractional_dpr_transform_matches_rounded_backing_store() {
        // This is the transform selected by sync_size for a 1.2 CSS pixel
        // canvas at DPR 1.25: 1.5 physical pixels are rounded to two.
        let pixel_w = (1.2_f64 * 1.25_f64).round() as u32;
        assert_eq!(pixel_w as f64 / 1.2, 5.0 / 3.0);
    }

    #[test]
    fn stroke_snap_uses_device_pixels() {
        let at_one = crisp_stroke(0.2, 1.0, 1.0);
        let at_two = crisp_stroke(0.2, 2.0, 1.0);
        assert!((at_one - 0.5).abs() < 1e-12);
        assert!((at_two - 0.0).abs() < 1e-12);
        assert_eq!(crisp_stroke_geometry(0.2, 2.0, 1.0).1, 1.0);
    }
}
