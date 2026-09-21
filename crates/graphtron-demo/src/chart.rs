use dioxus::prelude::*;
use graphtron::{
    Action, Animation, Annotation, CanvasSurface, ChartData, ChartKind, ChartSpec,
    CursorOverlay, DragMode, InteractState, RenderOptions, SeriesData, animate_range,
    draw, draw_overlay, hit_test,
};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DemoCursor {
    pub hover: Option<i64>,
    pub frozen: Option<i64>,
}

impl DemoCursor {
    fn toggle_freeze(&mut self, ts: i64) {
        self.frozen = if self.frozen.is_some() { None } else { Some(ts) };
    }
}

struct ChartLocals {
    data_surface: Option<CanvasSurface>,
    overlay_surface: Option<CanvasSurface>,
    layout: Option<graphtron::ChartLayout>,
    interact: InteractState,
    css_w: f64,
    css_h: f64,
    /// Last drawn range (for animation interpolation baseline).
    last_range: (f64, f64),
    /// Smoothed hover timestamp for animated cursor crosshair.
    cursor_display_ts: Option<f64>,
    /// Raw pointer row, for the free crosshair y readout.
    hover_y_px: Option<f64>,
}

#[component]
pub fn DemoChart(
    spec: ChartSpec,
    series: ReadSignal<Vec<SeriesData>>,
    range: Signal<(i64, i64)>,
    cursor: Signal<DemoCursor>,
    default_range: (i64, i64),
    #[props(default = 240.0)] height: f64,
    #[props(default = true)] dark: bool,
    #[props(default = vec![])] annotations: Vec<Annotation>,
    /// Point-series chart kind; the kind travels with the data, not the spec.
    #[props(default)] kind: ChartKind,
) -> Element {
    let opts = RenderOptions::from(dark);
    let locals = use_hook(|| {
        Rc::new(RefCell::new(ChartLocals {
            data_surface: None,
            overlay_surface: None,
            layout: None,
            interact: InteractState::default(),
            css_w: 0.0,
            css_h: 0.0,
            last_range: (0.0, 1.0),
            cursor_display_ts: None,
            hover_y_px: None,
        }))
    });

    let mut data_canvas = use_signal(|| None::<web_sys::HtmlCanvasElement>);
    let mut redraw_tick = use_signal(|| 0u32);
    let mut overlay_tick = use_signal(|| 0u32);
    let interactive = spec.chrome;

    // Animation state for smooth range transitions.
    let anim: Signal<Option<Animation>> = use_signal(|| None);

    // Data layer: redraw on series / range / anim / size change.
    // If an animation is active, the interpolated range is used for drawing
    // and a rAF is scheduled to drive progress until completion.
    {
        let locals = locals.clone();
        let spec = spec.clone();
        let opts = opts.clone();
        let mut anim = anim;
        use_effect(move || {
            let series = series();
            let (target_from, target_to) = range();
            let _ = redraw_tick();
            let _ = data_canvas();
            let now = js_sys::Date::now();

            let mut l = locals.borrow_mut();
            let (css_w, css_h) = (l.css_w, l.css_h);
            if css_w <= 0.0 || css_h <= 0.0 || l.data_surface.is_none() { return; }

            // Compute display range (interpolated if animating).
            let anim_opt = anim();
            let (from_ms, to_ms) = if let Some(ref a) = anim_opt {
                if a.is_done(now) {
                    anim.set(None);
                    (target_from as f64, target_to as f64)
                } else {
                    // Schedule next frame to advance the animation.
                    let mut tick = redraw_tick;
                    if let Some(win) = web_sys::window() {
                        let cb = Closure::once_into_js(move || { tick += 1; });
                        let _ = win.request_animation_frame(cb.unchecked_ref());
                    }
                    animate_range(
                        l.last_range,
                        (target_from as f64, target_to as f64),
                        now,
                        &anim_opt,
                    )
                }
            } else {
                (target_from as f64, target_to as f64)
            };
            l.last_range = (from_ms, to_ms);

            let surface = l.data_surface.as_mut().unwrap();
            surface.sync_size(css_w, css_h);
            let data = ChartData::from_point_kind(kind, series)
                .unwrap_or_else(|| ChartData::Lines(vec![]));
            let layout = draw(surface, &spec, &data, from_ms, to_ms, &opts);
            l.layout = Some(layout);
        });
    }

    // Overlay layer: redraw on cursor / tick / anim change.
    // Includes cursor smoothing (exponential interpolation toward target).
    {
        let locals = locals.clone();
        let annotations = annotations.clone();
        let opts = opts.clone();
        let overlay_tick = overlay_tick;
        use_effect(move || {
            let cur = cursor();
            let _ = overlay_tick();
            let _ = range();

            let mut l = locals.borrow_mut();
            let target = cur.hover.map(|t| t as f64);

            // Smooth cursor: exponential decay toward target (tau ~ 40ms).
            let display_ts = match (l.cursor_display_ts, target) {
                (Some(prev), Some(t)) if prev != t => {
                    Some(prev + (t - prev) * 0.35)
                }
                (_, t) => t,
            };
            l.cursor_display_ts = display_ts;

            // Schedule next frame if still smoothing.
            if let (Some(d), Some(t)) = (display_ts, target) {
                if (d - t).abs() > 0.5 && !cur.frozen.is_some() {
                    let mut tick = overlay_tick;
                    if let Some(win) = web_sys::window() {
                        let cb = Closure::once_into_js(move || { tick += 1; });
                        let _ = win.request_animation_frame(cb.unchecked_ref());
                    }
                }
            }

            // Drop the mutable borrow before taking immutable refs to surface/layout.
            let surface = l.overlay_surface.clone();
            let layout = l.layout;
            drop(l);
            let (Some(surface), Some(layout)) = (surface, layout) else { return; };
            let snapshot = series.peek().clone();

            let overlay = CursorOverlay {
                hover: display_ts,
                frozen: cur.frozen.map(|t| t as f64),
                unit: spec.unit,
                hover_y_px: locals.borrow().hover_y_px,
            };
            let hover_info = overlay
                .hover
                .and_then(|ts| hit_test(&layout, &snapshot, layout.x_scale.to_px(ts)));
            let selection = locals.borrow().interact.selection().map(|d| (d.start_px, d.cur_px));
            let data = ChartData::from_point_kind(kind, snapshot)
                .unwrap_or_else(|| ChartData::Lines(vec![]));
            draw_overlay(
                &surface, &layout, &data, &overlay,
                hover_info.as_ref(), selection, &annotations, &opts,
            );
        });
    }

    let on_data_mount = {
        let locals = locals.clone();
        move |evt: MountedEvent| {
            if let Some(el) = element_canvas(&evt) {
                let w = el.client_width() as f64;
                let h = el.client_height() as f64;
                if let Some(surface) = CanvasSurface::new(el.clone()) {
                    let mut l = locals.borrow_mut();
                    l.css_w = if w > 0.0 { w } else { 600.0 };
                    l.css_h = if h > 0.0 { h } else { height };
                    l.data_surface = Some(surface);
                }
                data_canvas.set(Some(el));
                redraw_tick += 1;
                overlay_tick += 1;
            }
        }
    };

    let on_overlay_mount = {
        let locals = locals.clone();
        move |evt: MountedEvent| {
            if let Some(el) = element_canvas(&evt) {
                if let Some(mut surface) = CanvasSurface::new(el.clone()) {
                    let mut l = locals.borrow_mut();
                    let (w, h) = (l.css_w.max(1.0), l.css_h.max(1.0));
                    surface.sync_size(w, h);
                    l.overlay_surface = Some(surface);
                }
                overlay_tick += 1;
            }
        }
    };

    let apply = move |actions: Vec<Action>| {
        let mut cursor = cursor;
        let mut range = range;
        let mut overlay_tick = overlay_tick;
        let mut anim = anim;
        for action in actions {
            match action {
                Action::Hover(ts) => {
                    let mut c = cursor();
                    if c.frozen.is_none() {
                        c.hover = Some(ts as i64);
                        cursor.set(c);
                    }
                }
                Action::ClearHover => {
                    let mut c = cursor();
                    if c.hover.is_some() {
                        c.hover = None;
                        cursor.set(c);
                    }
                }
                Action::ToggleFreeze(ts) => {
                    let mut c = cursor();
                    c.toggle_freeze(ts as i64);
                    cursor.set(c);
                }
                Action::ZoomTo { from_ms, to_ms } => {
                    let now = js_sys::Date::now();
                    let (cur_from, _cur_to) = range();
                    anim.set(Some(graphtron::Animation::new(
                        cur_from as f64, from_ms, now, 200.0,
                    )));
                    range.set((from_ms as i64, to_ms as i64));
                }
                Action::PanBy(delta_ms) => {
                    let (from, to) = range();
                    range.set(((from as f64 + delta_ms) as i64, (to as f64 + delta_ms) as i64));
                }
                Action::ResetZoom => {
                    let now = js_sys::Date::now();
                    let (cur_from, _cur_to) = range();
                    anim.set(Some(graphtron::Animation::new(
                        cur_from as f64, default_range.0 as f64, now, 250.0,
                    )));
                    range.set(default_range);
                }
                Action::SelectionChanged => overlay_tick += 1,
                Action::ClearFreeze => {}
            }
        }
    };

    let on_mouse_down = {
        let locals = locals.clone();
        move |evt: MouseEvent| {
            if interactive {
                let x = evt.element_coordinates().x;
                locals.borrow_mut().interact.on_mouse_down(x);
            }
        }
    };

    let on_mouse_move = {
        let locals = locals.clone();
        let apply = apply;
        move |evt: MouseEvent| {
            if !interactive { return; }
            let point = evt.element_coordinates();
            let actions = {
                let mut l = locals.borrow_mut();
                l.hover_y_px = Some(point.y);
                match l.layout {
                    Some(layout) => l.interact.on_mouse_move(point.x, &layout),
                    None => vec![],
                }
            };
            apply(actions);
        }
    };

    let on_mouse_up = {
        let locals = locals.clone();
        let apply = apply;
        move |evt: MouseEvent| {
            if !interactive { return; }
            let x = evt.element_coordinates().x;
            let shift = evt.data().modifiers().shift();
            let mode = if shift { DragMode::Pan } else { DragMode::Zoom };
            let actions = {
                let mut l = locals.borrow_mut();
                match l.layout {
                    Some(layout) => l.interact.on_mouse_up(x, &layout, mode),
                    None => vec![],
                }
            };
            apply(actions);
        }
    };

    let on_mouse_leave = {
        let locals = locals.clone();
        let apply = apply;
        move |_evt: MouseEvent| {
            if !interactive { return; }
            let actions = {
                let mut l = locals.borrow_mut();
                l.hover_y_px = None;
                l.interact.on_mouse_leave()
            };
            apply(actions);
        }
    };

    let on_double_click = move |_evt: MouseEvent| {
        if interactive { apply(vec![Action::ResetZoom]); }
    };

    let css_cursor = if interactive { "crosshair" } else { "default" };

    rsx! {
        div {
            style: "position: relative; width: 100%; height: {height}px;",
            canvas {
                style: "position: absolute; inset: 0; width: 100%; height: 100%;",
                onmounted: on_data_mount,
            }
            canvas {
                style: "position: absolute; inset: 0; width: 100%; height: 100%; cursor: {css_cursor};",
                onmounted: on_overlay_mount,
                onmousedown: on_mouse_down,
                onmousemove: on_mouse_move,
                onmouseup: on_mouse_up,
                onmouseleave: on_mouse_leave,
                ondoubleclick: on_double_click,
            }
        }
    }
}

fn element_canvas(evt: &MountedEvent) -> Option<web_sys::HtmlCanvasElement> {
    evt.data()
        .downcast::<web_sys::Element>()
        .cloned()
        .and_then(|el| el.dyn_into::<web_sys::HtmlCanvasElement>().ok())
}
