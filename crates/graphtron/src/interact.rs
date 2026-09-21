use crate::layout::ChartLayout;

pub const DRAG_THRESHOLD_PX: f64 = 5.0;
pub const MIN_ZOOM_SPAN_MS: f64 = 1_000.0;
const PAN_FRACTION: f64 = 0.1;
const ZOOM_FRACTION: f64 = 0.2;

/// Stable viewport limits derived from the full data extent, not the current
/// viewport. The default permits at most 100× magnification. Units match x.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomBounds {
    from: f64,
    to: f64,
    min_span: f64,
}

impl ZoomBounds {
    pub fn new(from: f64, to: f64, min_span: Option<f64>) -> Option<Self> {
        let span = to - from;
        if !from.is_finite() || !to.is_finite() || !span.is_finite() || span <= 0.0 {
            return None;
        }
        let floor = (from.abs().max(to.abs()) * f64::EPSILON * 4.0).min(span);
        let min_span = min_span
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(span / 100.0)
            .max(floor)
            .min(span);
        Some(Self { from, to, min_span })
    }

    /// Clamp zoom and pan together, preserving span near an edge. Invalid
    /// requests return the full data extent instead of poisoning the scale.
    pub fn clamp(self, from: f64, to: f64) -> (f64, f64) {
        if !from.is_finite() || !to.is_finite() || !(to - from).is_finite() {
            return (self.from, self.to);
        }
        let span = (to - from).abs().clamp(self.min_span, self.to - self.from);
        if span >= self.to - self.from {
            return (self.from, self.to);
        }
        let center = from / 2.0 + to / 2.0;
        let left = (center - span / 2.0).clamp(self.from, self.to - span);
        (left, (left + span).min(self.to))
    }
}

/// Pinch-to-zoom state tracking two touch points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PinchState {
    pub start_dist: f64,
    pub cur_dist: f64,
    pub cx: f64,
    pub cy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DragState {
    pub start_px: f64,
    pub cur_px: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum InteractState {
    #[default]
    Idle,
    Pressed(DragState),
    Dragging(DragState),
}

/// Interaction mode: zoom (drag to select a range) or pan (drag to shift view).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DragMode {
    #[default]
    Zoom,
    Pan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Hover(f64),
    ClearHover,
    ToggleFreeze(f64),
    ZoomTo {
        from_ms: f64,
        to_ms: f64,
    },
    ResetZoom,
    SelectionChanged,
    ClearFreeze,
    /// Pan the view by the given delta in ms (positive = right).
    PanBy(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    Plus,
    Minus,
    Escape,
}

pub fn wheel_zoom(from_ms: f64, to_ms: f64, anchor_ms: f64, delta_y: f64) -> (f64, f64) {
    wheel_zoom_with_min_span(from_ms, to_ms, anchor_ms, delta_y, MIN_ZOOM_SPAN_MS)
}

/// Axis-agnostic wheel zoom with an application-selected minimum domain span.
/// Use this for linear/category axes where the millisecond default is not
/// meaningful.
pub fn wheel_zoom_with_min_span(
    from: f64,
    to: f64,
    anchor: f64,
    delta_y: f64,
    min_span: f64,
) -> (f64, f64) {
    if !from.is_finite()
        || !to.is_finite()
        || !anchor.is_finite()
        || !delta_y.is_finite()
        || delta_y == 0.0
    {
        return (from, to);
    }
    let (from, to) = if from <= to { (from, to) } else { (to, from) };
    let min_span = min_span.max(f64::EPSILON);
    let factor = if delta_y < 0.0 { 0.9 } else { 1.0 / 0.9 };
    let anchor = anchor.clamp(from, to);
    let new_from = anchor - (anchor - from) * factor;
    let new_to = anchor + (to - anchor) * factor;
    if new_to - new_from < min_span {
        (from, to)
    } else {
        (new_from, new_to)
    }
}

impl InteractState {
    pub fn on_mouse_down(&mut self, x_px: f64) {
        *self = InteractState::Pressed(DragState {
            start_px: x_px,
            cur_px: x_px,
        });
    }

    pub fn on_mouse_move(&mut self, x_px: f64, layout: &ChartLayout) -> Vec<Action> {
        match self {
            InteractState::Idle => vec![Action::Hover(layout.ts_at(x_px))],
            InteractState::Pressed(drag) => {
                drag.cur_px = x_px;
                if (drag.cur_px - drag.start_px).abs() >= DRAG_THRESHOLD_PX {
                    let drag = *drag;
                    *self = InteractState::Dragging(drag);
                    vec![Action::SelectionChanged]
                } else {
                    vec![Action::Hover(layout.ts_at(x_px))]
                }
            }
            InteractState::Dragging(drag) => {
                drag.cur_px = x_px;
                vec![Action::SelectionChanged]
            }
        }
    }

    pub fn on_mouse_up(&mut self, x_px: f64, layout: &ChartLayout, mode: DragMode) -> Vec<Action> {
        self.on_mouse_up_with_min_span(x_px, layout, mode, MIN_ZOOM_SPAN_MS)
    }

    /// Complete a click/drag with an axis-specific minimum selectable span.
    pub fn on_mouse_up_with_min_span(
        &mut self,
        x_px: f64,
        layout: &ChartLayout,
        mode: DragMode,
        min_span: f64,
    ) -> Vec<Action> {
        let state = std::mem::take(self);
        match state {
            InteractState::Idle => vec![],
            InteractState::Pressed(_) => vec![Action::ToggleFreeze(layout.ts_at(x_px))],
            InteractState::Dragging(mut drag) => {
                // Pointer-up may arrive at a newer coordinate than the last
                // move event; always use the final event for the committed
                // range/pan.
                drag.cur_px = x_px;
                match mode {
                    DragMode::Pan => {
                        let delta_px = drag.cur_px - drag.start_px;
                        let span = layout.x_scale.d1 - layout.x_scale.d0;
                        let plot_w = layout.plot.w;
                        let delta_ms = -(delta_px / plot_w) * span;
                        vec![Action::PanBy(delta_ms)]
                    }
                    DragMode::Zoom => {
                        let (a, b) = if drag.start_px <= drag.cur_px {
                            (drag.start_px, drag.cur_px)
                        } else {
                            (drag.cur_px, drag.start_px)
                        };
                        let from_ms = layout.ts_at(a);
                        let to_ms = layout.ts_at(b);
                        if to_ms - from_ms >= min_span.max(f64::EPSILON) {
                            vec![Action::SelectionChanged, Action::ZoomTo { from_ms, to_ms }]
                        } else {
                            vec![Action::SelectionChanged]
                        }
                    }
                }
            }
        }
    }

    pub fn on_mouse_leave(&mut self) -> Vec<Action> {
        *self = InteractState::Idle;
        vec![Action::ClearHover, Action::SelectionChanged]
    }

    pub fn on_touch_start(&mut self, x_px: f64) {
        self.on_mouse_down(x_px);
    }
    pub fn on_touch_move(&mut self, x_px: f64, layout: &ChartLayout) -> Vec<Action> {
        self.on_mouse_move(x_px, layout)
    }

    pub fn on_touch_end(&mut self, x_px: f64, layout: &ChartLayout) -> Vec<Action> {
        self.on_mouse_up(x_px, layout, DragMode::Zoom)
    }

    /// Handle pinch-to-zoom from two touch points.
    /// Returns a `ZoomTo` action with the zoom focused around the pinch center.
    pub fn on_pinch(
        &self,
        from: &PinchState,
        to: &PinchState,
        from_ms: f64,
        to_ms: f64,
    ) -> Vec<Action> {
        let layout = ChartLayout::compute_with_margins(
            100.0, 100.0, from_ms, to_ms, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
        );
        self.on_pinch_with_layout(from, to, &layout, MIN_ZOOM_SPAN_MS)
    }

    /// Pinch zoom using the actual plot geometry. This avoids the legacy
    /// 100px synthetic viewport and keeps the zoom anchor correct in resized
    /// canvases with axis margins.
    pub fn on_pinch_with_layout(
        &self,
        from: &PinchState,
        to: &PinchState,
        layout: &ChartLayout,
        min_span: f64,
    ) -> Vec<Action> {
        if !from.start_dist.is_finite()
            || !to.cur_dist.is_finite()
            || from.start_dist <= 0.0
            || to.cur_dist <= 0.0
        {
            return vec![];
        }
        let factor = from.start_dist / to.cur_dist.max(1.0);
        let center = layout.ts_at(to.cx);
        let from = layout.x_scale.d0;
        let to_domain = layout.x_scale.d1;
        let new_span = ((to_domain - from) * factor).max(min_span.max(f64::EPSILON));
        let anchor_fraction = if to_domain == from {
            0.5
        } else {
            ((center - from) / (to_domain - from)).clamp(0.0, 1.0)
        };
        vec![Action::ZoomTo {
            from_ms: center - new_span * anchor_fraction,
            to_ms: center + new_span * (1.0 - anchor_fraction),
        }]
    }

    pub fn on_key(&self, key: Key, from_ms: f64, to_ms: f64) -> Vec<Action> {
        self.on_key_with_min_span(key, from_ms, to_ms, MIN_ZOOM_SPAN_MS)
    }

    /// Keyboard navigation with an axis-specific minimum span, matching
    /// [`wheel_zoom_with_min_span`] and [`Self::on_mouse_up_with_min_span`].
    /// Without it, holding `+` collapses the viewport to nothing — every other
    /// zoom path already refuses to.
    pub fn on_key_with_min_span(
        &self,
        key: Key,
        from_ms: f64,
        to_ms: f64,
        min_span: f64,
    ) -> Vec<Action> {
        if !from_ms.is_finite() || !to_ms.is_finite() || !min_span.is_finite() {
            return vec![];
        }
        let (from_ms, to_ms) = if from_ms <= to_ms {
            (from_ms, to_ms)
        } else {
            (to_ms, from_ms)
        };
        let span = (to_ms - from_ms).max(f64::EPSILON);
        let min_span = min_span.max(f64::EPSILON);
        match key {
            Key::ArrowLeft => {
                let shift = span * PAN_FRACTION;
                vec![Action::ZoomTo {
                    from_ms: from_ms - shift,
                    to_ms: to_ms - shift,
                }]
            }
            Key::ArrowRight => {
                let shift = span * PAN_FRACTION;
                vec![Action::ZoomTo {
                    from_ms: from_ms + shift,
                    to_ms: to_ms + shift,
                }]
            }
            Key::ArrowUp | Key::Plus => {
                let center = (from_ms + to_ms) / 2.0;
                let zoomed = span * (1.0 - ZOOM_FRACTION);
                if zoomed < min_span {
                    return vec![];
                }
                let half = zoomed / 2.0;
                vec![Action::ZoomTo {
                    from_ms: center - half,
                    to_ms: center + half,
                }]
            }
            Key::ArrowDown | Key::Minus => {
                let center = (from_ms + to_ms) / 2.0;
                let half = span * (1.0 + ZOOM_FRACTION) / 2.0;
                vec![Action::ZoomTo {
                    from_ms: center - half,
                    to_ms: center + half,
                }]
            }
            Key::Home | Key::End => vec![Action::ResetZoom],
            Key::Escape => vec![Action::ClearFreeze],
        }
    }

    pub fn selection(&self) -> Option<DragState> {
        match self {
            InteractState::Dragging(d) => Some(*d),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_bounds_limit_both_directions_and_pan_without_shrinking() {
        let bounds = ZoomBounds::new(0.0, 1000.0, None).unwrap();
        assert_eq!(bounds.clamp(-1000.0, 2000.0), (0.0, 1000.0));
        assert_eq!(bounds.clamp(499.0, 501.0), (495.0, 505.0));
        assert_eq!(bounds.clamp(-100.0, 100.0), (0.0, 200.0));
        assert_eq!(bounds.clamp(900.0, 1100.0), (800.0, 1000.0));
    }

    #[test]
    fn zoom_bounds_are_stable_under_repeated_zoom_and_fractional_domains() {
        let bounds = ZoomBounds::new(0.001, 0.011, None).unwrap();
        let mut view = (0.001, 0.011);
        for _ in 0..1000 {
            let center = (view.0 + view.1) / 2.0;
            view = bounds.clamp(
                center - (view.1 - view.0) * 0.4,
                center + (view.1 - view.0) * 0.4,
            );
        }
        assert!((view.1 - view.0 - 0.0001).abs() < 1e-15);
        for _ in 0..1000 {
            view = bounds.clamp(view.0 - 0.001, view.1 + 0.001);
        }
        assert_eq!(view, (0.001, 0.011));
    }

    #[test]
    fn zoom_bounds_validate_custom_minimum_and_bad_data() {
        assert!(ZoomBounds::new(1.0, 1.0, None).is_none());
        assert!(ZoomBounds::new(f64::NAN, 2.0, None).is_none());
        assert!(ZoomBounds::new(-f64::MAX, f64::MAX, None).is_none());
        let bounds = ZoomBounds::new(0.0, 100.0, Some(20.0)).unwrap();
        assert_eq!(bounds.clamp(50.0, 50.0), (40.0, 60.0));
        assert_eq!(bounds.clamp(f64::NAN, 10.0), (0.0, 100.0));
        assert_eq!(
            ZoomBounds::new(0.0, 100.0, Some(200.0))
                .unwrap()
                .clamp(30.0, 40.0),
            (0.0, 100.0)
        );
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                ZoomBounds::new(0.0, 100.0, Some(invalid)),
                ZoomBounds::new(0.0, 100.0, None)
            );
        }
    }

    fn layout() -> ChartLayout {
        ChartLayout::compute(652.0, 200.0, 0.0, 60_000.0, 0.0, 100.0)
    }

    #[test]
    fn click_toggles_freeze() {
        let l = layout();
        let mut s = InteractState::default();
        s.on_mouse_down(100.0);
        let actions = s.on_mouse_up(102.0, &l, DragMode::Zoom);
        assert!(matches!(actions[..], [Action::ToggleFreeze(_)]));
        assert_eq!(s, InteractState::Idle);
    }

    #[test]
    fn drag_zooms() {
        let l = layout();
        let mut s = InteractState::default();
        s.on_mouse_down(100.0);
        s.on_mouse_move(300.0, &l);
        let actions = s.on_mouse_up(300.0, &l, DragMode::Zoom);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::ZoomTo { from_ms, to_ms } if to_ms > from_ms))
        );
    }

    #[test]
    fn drag_pans() {
        let l = layout();
        let mut s = InteractState::default();
        s.on_mouse_down(300.0);
        s.on_mouse_move(100.0, &l);
        let actions = s.on_mouse_up(100.0, &l, DragMode::Pan);
        assert!(actions.iter().any(|a| matches!(a, Action::PanBy(_))));
    }

    #[test]
    fn reverse_drag_normalizes() {
        let l = layout();
        let mut s = InteractState::default();
        s.on_mouse_down(300.0);
        s.on_mouse_move(100.0, &l);
        let actions = s.on_mouse_up(100.0, &l, DragMode::Zoom);
        let zoom = actions
            .iter()
            .find_map(|a| match a {
                Action::ZoomTo { from_ms, to_ms } => Some((*from_ms, *to_ms)),
                _ => None,
            })
            .unwrap();
        assert!(zoom.0 < zoom.1);
    }

    #[test]
    fn hover_when_idle() {
        let l = layout();
        let mut s = InteractState::default();
        let actions = s.on_mouse_move(l.plot.x + 10.0, &l);
        assert!(matches!(actions[..], [Action::Hover(_)]));
    }

    #[test]
    fn wheel_zoom_in_shrinks_window_around_anchor() {
        let (from, to) = wheel_zoom(0.0, 3_600_000.0, 1_800_000.0, -1.0);
        assert!(to - from < 3_600_000.0);
        assert!((from - 180_000.0).abs() < 1.0 && (to - 3_420_000.0).abs() < 1.0);
    }

    #[test]
    fn wheel_zoom_out_grows_window() {
        let (from, to) = wheel_zoom(600_000.0, 3_000_000.0, 1_800_000.0, 1.0);
        assert!(to - from > 2_400_000.0);
    }

    #[test]
    fn wheel_zoom_respects_min_span() {
        let (from, to) = wheel_zoom(0.0, MIN_ZOOM_SPAN_MS, 0.0, -1.0);
        assert_eq!((from, to), (0.0, MIN_ZOOM_SPAN_MS));
    }

    #[test]
    fn axis_agnostic_wheel_zoom_accepts_small_linear_domains() {
        let (from, to) = wheel_zoom_with_min_span(0.0, 10.0, 5.0, -1.0, 0.01);
        assert!(to - from < 10.0);
        assert!((from - 0.5).abs() < 1e-9);
        assert!((to - 9.5).abs() < 1e-9);
    }

    #[test]
    fn zero_or_invalid_wheel_delta_is_a_noop() {
        assert_eq!(
            wheel_zoom_with_min_span(0.0, 10.0, 5.0, 0.0, 0.01),
            (0.0, 10.0)
        );
        assert_eq!(
            wheel_zoom_with_min_span(0.0, 10.0, 5.0, f64::NAN, 0.01),
            (0.0, 10.0)
        );
    }

    #[test]
    fn keyboard_pan_left() {
        let s = InteractState::default();
        let actions = s.on_key(Key::ArrowLeft, 0.0, 60_000.0);
        match &actions[..] {
            [Action::ZoomTo { from_ms, to_ms }] => {
                assert!(*from_ms < 0.0);
                assert!(*to_ms < 60_000.0);
                assert!((*to_ms - *from_ms - 60_000.0).abs() < 1.0);
            }
            _ => panic!("expected ZoomTo"),
        }
    }

    #[test]
    fn keyboard_zoom_in() {
        let s = InteractState::default();
        let actions = s.on_key(Key::Plus, 0.0, 100_000.0);
        match &actions[..] {
            [Action::ZoomTo { from_ms, to_ms }] => {
                let new_span = to_ms - from_ms;
                assert!(new_span < 100_000.0);
                let center = (from_ms + to_ms) / 2.0;
                assert!((center - 50_000.0).abs() < 1.0);
            }
            _ => panic!("expected ZoomTo"),
        }
    }

    #[test]
    fn keyboard_escape_clears_freeze() {
        let s = InteractState::default();
        let actions = s.on_key(Key::Escape, 0.0, 100_000.0);
        assert!(matches!(actions[..], [Action::ClearFreeze]));
    }

    #[test]
    fn touch_start_move_end() {
        let l = layout();
        let mut s = InteractState::default();
        s.on_touch_start(100.0);
        let actions = s.on_touch_move(102.0, &l);
        assert!(actions.iter().any(|a| matches!(a, Action::Hover(_))));
        let actions = s.on_touch_end(102.0, &l);
        assert!(actions.iter().any(|a| matches!(a, Action::ToggleFreeze(_))));
    }

    #[test]
    fn mouse_up_commits_its_final_coordinate() {
        let l = layout();
        let mut state = InteractState::default();
        state.on_mouse_down(100.0);
        state.on_mouse_move(200.0, &l);
        let actions = state.on_mouse_up(350.0, &l, DragMode::Zoom);
        let end = actions.iter().find_map(|action| match action {
            Action::ZoomTo { to_ms, .. } => Some(*to_ms),
            _ => None,
        });
        assert_eq!(end, Some(l.ts_at(350.0)));
    }

    #[test]
    fn pinch_zooms_around_center() {
        let s = InteractState::default();
        let from = PinchState {
            start_dist: 200.0,
            cur_dist: 200.0,
            cx: 50.0,
            cy: 100.0,
        };
        let to = PinchState {
            start_dist: 200.0,
            cur_dist: 100.0,
            cx: 50.0,
            cy: 100.0,
        };
        let actions = s.on_pinch(&from, &to, 0.0, 60_000.0);
        assert!(actions.iter().any(|a| matches!(a, Action::ZoomTo { .. })));
    }

    #[test]
    fn pinch_with_layout_uses_real_plot_anchor() {
        let state = InteractState::default();
        let layout = ChartLayout::compute(800.0, 200.0, 0.0, 100.0, 0.0, 1.0);
        let anchor_px = layout.x_scale.to_px(25.0);
        let from = PinchState {
            start_dist: 100.0,
            cur_dist: 100.0,
            cx: anchor_px,
            cy: 50.0,
        };
        let to = PinchState {
            start_dist: 100.0,
            cur_dist: 200.0,
            cx: anchor_px,
            cy: 50.0,
        };
        let [Action::ZoomTo { from_ms, to_ms }] =
            state.on_pinch_with_layout(&from, &to, &layout, 0.01)[..]
        else {
            panic!("expected ZoomTo");
        };
        assert!((from_ms - 12.5).abs() < 1e-9);
        assert!((to_ms - 62.5).abs() < 1e-9);
    }

    #[test]
    fn keyboard_zoom_in_stops_at_the_minimum_span() {
        let state = InteractState::default();
        // Well above the floor: the zoom happens.
        let actions = state.on_key(Key::Plus, 0.0, 60_000.0);
        assert!(matches!(actions.as_slice(), [Action::ZoomTo { .. }]));

        // At the floor, every other zoom path refuses; so does the keyboard.
        assert!(state.on_key(Key::Plus, 0.0, MIN_ZOOM_SPAN_MS).is_empty());
        // Zooming back out is always allowed.
        assert!(!state.on_key(Key::Minus, 0.0, MIN_ZOOM_SPAN_MS).is_empty());
    }

    #[test]
    fn keyboard_zoom_honors_an_axis_specific_minimum() {
        let state = InteractState::default();
        // A linear axis in units, not milliseconds.
        assert!(
            state
                .on_key_with_min_span(Key::Plus, 0.0, 1.0, 0.9)
                .is_empty()
        );
        assert!(
            !state
                .on_key_with_min_span(Key::Plus, 0.0, 1.0, 0.1)
                .is_empty()
        );
    }
}
