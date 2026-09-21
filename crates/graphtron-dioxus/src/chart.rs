use dioxus::prelude::*;
use graphtron::hit::{hit_test_chart_with_cache, prepare_histogram_hover};
use graphtron::{
    Action, Animation, Annotation, CanvasSurface, ChartData, ChartKind, ChartSpec, CursorOverlay,
    DragMode, InteractState, LegendFormat, LegendPosition, PALETTE, RenderOptions, SeriesData,
    XAxisKind, animate_range, annotate_delta, draw, draw_overlay, series_color,
};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::cursor::Cursor;
use crate::legend::Legend;

/// Per-chart mutable state that lives outside the reactive graph: surfaces,
/// last layout (for pixel↔ts inversion in event handlers), interaction state,
/// and animation baselines.
struct ChartLocals {
    data_surface: Option<CanvasSurface>,
    overlay_surface: Option<CanvasSurface>,
    layout: Option<graphtron::ChartLayout>,
    partition_geometry: Rc<Vec<(usize, graphtron::partition::Shape)>>,
    interact: InteractState,
    css_w: f64,
    css_h: f64,
    /// Last drawn range (animation interpolation baseline).
    last_range: (f64, f64),
    /// Smoothed hover timestamp for the animated cursor crosshair.
    cursor_display_ts: Option<f64>,
    /// Most recent local pointer. A shared cursor carries an x coordinate but
    /// not a y coordinate, so retaining its origin lets row-oriented charts
    /// (heatmaps, horizontal bars, and timelines) hit-test accurately without
    /// inventing a row in a synchronized sibling chart.
    hover_pointer: Option<HoverPointer>,
}

#[derive(Debug, Clone, Copy)]
struct HoverPointer {
    x: f64,
    y: f64,
    /// The integer cursor value written by this chart's interaction state.
    hover_ms: i64,
    /// Exact x-domain value at the local pointer. `Cursor` remains i64 for
    /// backwards-compatible synchronization, but local linear charts retain
    /// this precision for hover guides and semantic hit testing.
    ts: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DomainAnimation {
    start: (f64, f64),
    target: (f64, f64),
    clock: Animation,
}

impl DomainAnimation {
    fn new(start: (f64, f64), target: (f64, f64), now_ms: f64, duration_ms: f64) -> Self {
        Self {
            start,
            target,
            clock: Animation::new(start.0, target.0, now_ms, duration_ms),
        }
    }

    fn is_done(&self, now_ms: f64) -> bool {
        self.clock.is_done(now_ms)
    }

    fn value_at(&self, now_ms: f64) -> (f64, f64) {
        animate_range(self.start, self.target, now_ms, &Some(self.clock))
    }
}

fn local_hover_timestamp(cursor: Cursor, pointer: Option<HoverPointer>) -> Option<f64> {
    pointer
        .filter(|pointer| cursor.hover == Some(pointer.hover_ms))
        .map(|pointer| pointer.ts)
}

/// Specs own portable chart configuration, while the component prop remains a
/// convenient caller-local extension. Avoid duplicate markers when callers
/// provide the same annotation through both paths.
fn merge_annotations(spec: &[Annotation], additional: &[Annotation]) -> Vec<Annotation> {
    let mut merged = spec.to_vec();
    for annotation in additional {
        if !merged.contains(annotation) {
            merged.push(annotation.clone());
        }
    }
    merged
}

/// A complete, interactive chart: two stacked canvases, container-resize
/// reflow, hover/freeze crosshairs, drag-zoom/pan, wheel zoom, keyboard
/// navigation, optional zoom animation and cursor smoothing, and an optional
/// click-to-toggle legend.
///
/// Minimal embed:
/// ```ignore
/// rsx! {
///     GraphtronChart {
///         spec: ChartSpec::line(Unit::Percent),
///         data: data_signal,   // ReadSignal<ChartData>
///     }
/// }
/// ```
///
/// Cross-panel sync: create one `Signal<Cursor>` (and optionally one range
/// signal) in the parent and pass them to every chart.
#[component]
pub fn GraphtronChart(
    spec: ChartSpec,
    data: ReadSignal<ChartData>,
    /// Visible time range (epoch ms). `None` = autofit to the data and let
    /// interactions drive a chart-local range.
    #[props(default)]
    range: Option<Signal<(i64, i64)>>,
    /// Precise visible x domain for linear/time charts. Takes precedence over
    /// `range` and preserves fractional linear-axis bounds.
    #[props(default)]
    domain: Option<Signal<(f64, f64)>>,
    /// Shared cursor for cross-panel sync. `None` = chart-local cursor.
    #[props(default)]
    cursor: Option<Signal<Cursor>>,
    #[props(default)] options: RenderOptions,
    #[props(default = 220.0)] height: f64,
    /// Fired after any zoom/pan interaction with the new absolute range.
    #[props(default)]
    on_zoom: Option<EventHandler<(i64, i64)>>,
    /// Fired after a zoom/pan with the precise x domain. Use this for linear
    /// axes; `on_zoom` remains available for legacy time-range consumers.
    #[props(default)]
    on_domain_change: Option<EventHandler<(f64, f64)>>,
    /// Fired on double-click / Home. When absent, resets to `default_range`
    /// (or back to autofit when no range signal was provided).
    #[props(default)]
    on_reset: Option<EventHandler<()>>,
    #[props(default)] default_range: Option<(i64, i64)>,
    /// Precise reset target for `domain`; takes precedence over
    /// `default_range` when provided.
    #[props(default)]
    default_domain: Option<(f64, f64)>,
    /// Minimum interactive x span (milliseconds for time axes). Default:
    /// 1% of the full data span, i.e. at most 100× magnification.
    /// Zoom and pan are always contained within the loaded data extent.
    #[props(default)]
    min_zoom_span: Option<f64>,
    #[props(default = true)] interactive: bool,
    /// Animate zoom/pan transitions (rAF-driven).
    #[props(default = false)]
    animate: bool,
    /// Exponentially smooth the hover crosshair toward the pointer.
    #[props(default = false)]
    smooth_cursor: bool,
    /// Render a DOM legend under the chart (click to toggle series).
    #[props(default = false)]
    legend: bool,
    /// Fixed drag semantics. `None` (default) = drag zooms, shift-drag pans.
    #[props(default)]
    drag_mode: Option<DragMode>,
    /// While the cursor is frozen, annotate the live hover with each series'
    /// delta against the frozen point — click a deploy, then read how much
    /// every series moved since.
    #[props(default = true)]
    freeze_delta: bool,
    #[props(default = vec![])] annotations: Vec<Annotation>,
    #[props(default)] class: Option<String>,
) -> Element {
    let locals = use_hook(|| {
        Rc::new(RefCell::new(ChartLocals {
            data_surface: None,
            overlay_surface: None,
            layout: None,
            partition_geometry: Rc::default(),
            interact: InteractState::default(),
            css_w: 0.0,
            css_h: 0.0,
            last_range: (0.0, 1.0),
            cursor_display_ts: None,
            hover_pointer: None,
        }))
    });

    let mut redraw_tick = use_signal(|| 0u32);
    let mut overlay_tick = use_signal(|| 0u32);
    let anim: Signal<Option<DomainAnimation>> = use_signal(|| None);
    let last_drawn_domain: Signal<(f64, f64)> = use_signal(|| (0.0, 1.0));

    // External-or-local cursor / range.
    let local_cursor = use_signal(Cursor::default);
    let cursor = cursor.unwrap_or(local_cursor);
    let local_domain: Signal<Option<(f64, f64)>> = use_signal(|| None);
    let x_axis = spec.x_axis.clone();

    // Hidden-series set (legend toggles).
    let hidden: Signal<HashSet<usize>> = use_signal(HashSet::new);

    // In a Dioxus embedding the DOM legend is interactive, so it replaces the
    // canvas-only legend when either legend control asks to show one. This
    // avoids duplicate entries and lets `spec.legend` control the same legend
    // users operate with a mouse, touch, or keyboard.
    let show_dom_legend = legend || spec.legend.show;
    // Preserve `legend: true` as the original name-only, below-chart DOM
    // legend. A visible `spec.legend` opts into its full configuration.
    let dom_legend_format = if spec.legend.show {
        spec.legend.format
    } else {
        LegendFormat::NameOnly
    };
    let mut draw_spec = spec.clone();
    if show_dom_legend {
        draw_spec.legend.show = false;
    }

    // Data as drawn: hidden series removed, palette colors materialized so
    // hiding one series doesn't recolor the rest.
    let display_data = use_memo(move || filter_hidden(&data(), &hidden()));
    // Cache the full extent once per data update, never scan dense data on
    // wheel/pointer events or shrink bounds when a legend entry is hidden.
    let zoom_extent = use_memo(move || data_x_domain(&data()));
    let constrain_domain = move |from, to| {
        zoom_extent
            .peek()
            .and_then(|(lo, hi)| graphtron::ZoomBounds::new(lo, hi, min_zoom_span))
            .map(|limits| limits.clamp(from, to))
    };
    // Prefix sums are prepared once when histogram data changes. The overlay
    // effect can then answer cumulative hovers without rebuilding a prefix on
    // every pointer move.
    let histogram_hover_cache = use_memo(move || match &display_data() {
        ChartData::Histogram(series) => prepare_histogram_hover(series),
        _ => Default::default(),
    });
    let mut accessible_hover = use_signal(|| None::<graphtron::HoverInfo>);
    let mut inspected_frozen = use_signal(|| None::<i64>);
    let mut tooltip_page = use_signal(|| 0usize);
    let mut inspector_expanded = use_signal(|| false);
    const TOOLTIP_PAGE_SIZE: usize = 16;

    let legend_entries =
        use_memo(move || legend_entries_for_data(&data(), dom_legend_format, spec.unit));

    // The effective domain: external precise signal, legacy time range,
    // interaction-local domain, then autofit.
    let resolve_domain = move || -> (f64, f64) {
        if let Some(d) = domain {
            return d();
        }
        if let Some(r) = range {
            let (from, to) = r();
            return (from as f64, to as f64);
        }
        if let Some(d) = local_domain() {
            return d;
        }
        data_x_domain(&data.peek()).unwrap_or((0.0, 60_000.0))
    };

    // Domain writes preserve precision internally and through the optional
    // f64 callback, while maintaining the i64 range/on_zoom API.
    let set_domain = move |from: f64, to: f64| {
        let Some((from, to)) = constrain_domain(from, to) else {
            return;
        };
        if let Some(d) = domain {
            let mut d = d;
            d.set((from, to));
        } else if let Some(r) = range {
            let mut r = r;
            r.set((from as i64, to as i64));
        } else {
            let mut local_domain = local_domain;
            local_domain.set(Some((from, to)));
        }
        if let Some(cb) = &on_domain_change {
            cb.call((from, to));
        }
        if let Some(cb) = &on_zoom {
            cb.call((from as i64, to as i64));
        }
    };

    // Keep the DPR-scaled backing store fresh when the window (and possibly
    // the devicePixelRatio) changes — `sync_size` re-reads DPR on every draw,
    // this just guarantees a draw is triggered. Closure stays alive for the
    // component's lifetime via use_hook.
    use_hook(|| {
        let mut tick = redraw_tick;
        let mut otick = overlay_tick;
        let closure = Closure::<dyn FnMut()>::new(move || {
            tick += 1;
            otick += 1;
        });
        if let Some(win) = web_sys::window() {
            let _ =
                win.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref());
        }
        Rc::new(closure)
    });

    // Data layer: redraw on data / range / options / size change. When a zoom
    // animation is active, draw the interpolated range and schedule a rAF to
    // advance it.
    {
        let locals = locals.clone();
        let spec = draw_spec;
        let options = options.clone();
        let mut anim = anim;
        let mut last_drawn_domain = last_drawn_domain;
        use_effect(move || {
            let data_now = display_data();
            let options = filtered_options(&options, &hidden.peek());
            let (target_from, target_to, explicit_domain) = if let Some(d) = domain {
                let (from, to) = d();
                (from, to, true)
            } else if let Some(r) = range {
                let (from, to) = r();
                (from as f64, to as f64, true)
            } else if let Some((from, to)) = local_domain() {
                (from, to, true)
            } else {
                let (from, to) = data_x_domain(&data_now).unwrap_or((0.0, 60_000.0));
                (from, to, false)
            };
            let _ = redraw_tick();

            let mut l = locals.borrow_mut();
            let (css_w, css_h) = (l.css_w, l.css_h);
            if css_w <= 0.0 || css_h <= 0.0 || l.data_surface.is_none() {
                return;
            }

            let now = js_sys::Date::now();
            let anim_opt = anim();
            let (from_ms, to_ms) = if let Some(ref a) = anim_opt {
                if a.is_done(now) {
                    anim.set(None);
                    (target_from, target_to)
                } else {
                    let mut tick = redraw_tick;
                    if let Some(win) = web_sys::window() {
                        let cb = Closure::once_into_js(move || {
                            tick += 1;
                        });
                        let _ = win.request_animation_frame(cb.unchecked_ref());
                    }
                    a.value_at(now)
                }
            } else {
                (target_from, target_to)
            };
            l.last_range = (from_ms, to_ms);
            last_drawn_domain.set((from_ms, to_ms));

            let surface = l.data_surface.as_mut().unwrap();
            surface.sync_size(css_w, css_h);
            // `graphtron::resolve_x_domain` deliberately autofits unpinned linear
            // axes. An adapter-owned viewport therefore pins only an explicit
            // interaction/external domain; the first draw remains autofit.
            let mut effective_spec = spec.clone();
            if explicit_domain && matches!(&effective_spec.x_axis, XAxisKind::Linear { .. }) {
                effective_spec.x_axis = XAxisKind::Linear {
                    min: Some(from_ms),
                    max: Some(to_ms),
                };
            }
            let layout = draw(
                surface,
                &effective_spec,
                &data_now,
                from_ms,
                to_ms,
                &options,
            );
            l.layout = Some(layout);
            l.partition_geometry = Rc::new(
                graphtron::partition::items(&data_now)
                    .map(|items| graphtron::partition::geometry(data_now.kind(), items, layout.plot))
                    .unwrap_or_default(),
            );
        });
    }

    // Overlay layer: redraw on cursor / selection / range change. Optional
    // exponential cursor smoothing (rAF-driven until converged).
    {
        let locals = locals.clone();
        let spec = spec.clone();
        let options = options.clone();
        let annotations = annotations.clone();
        use_effect(move || {
            let cur = cursor();
            let _ = overlay_tick();
            let _ = redraw_tick();

            let mut l = locals.borrow_mut();
            let local_hover = local_hover_timestamp(cur, l.hover_pointer);
            let target = local_hover.or_else(|| cur.hover.map(|t| t as f64));

            let display_ts = if smooth_cursor {
                let smoothed = match (l.cursor_display_ts, target) {
                    (Some(prev), Some(t)) if prev != t => Some(prev + (t - prev) * 0.35),
                    (_, t) => t,
                };
                // Schedule another frame while still converging.
                if let (Some(d), Some(t)) = (smoothed, target)
                    && (d - t).abs() > 0.5
                    && cur.frozen.is_none()
                {
                    let mut tick = overlay_tick;
                    if let Some(win) = web_sys::window() {
                        let cb = Closure::once_into_js(move || {
                            tick += 1;
                        });
                        let _ = win.request_animation_frame(cb.unchecked_ref());
                    }
                }
                smoothed
            } else {
                target
            };
            l.cursor_display_ts = display_ts;

            let (css_w, css_h) = (l.css_w.max(1.0), l.css_h.max(1.0));
            let Some(layout) = l.layout else { return };
            let Some(surface) = l.overlay_surface.as_mut() else {
                return;
            };
            surface.sync_size(css_w, css_h);
            let surface = surface.clone();
            let selection = l.interact.selection().map(|d| (d.start_px, d.cur_px));
            let local_pointer = l.hover_pointer;
            let partition_geometry = l.partition_geometry.clone();
            drop(l);

            // Overlay rendering is pointer-rate. Borrow the memoized display
            // data for this effect instead of cloning every series on every
            // hover/freeze redraw; the borrow is dropped before the effect
            // returns and is never held across an await.
            let data_now = display_data.peek();
            let options = filtered_options(&options, &hidden.peek());
            let histogram_cache = histogram_hover_cache.peek();
            let merged_annotations = merge_annotations(&spec.annotations, &annotations);
            // The free cursor readout only makes sense on the chart the
            // pointer is actually over; a synced sibling has no pointer row.
            let local_row = local_pointer
                .filter(|pointer| {
                    cur.hover == Some(pointer.hover_ms) && local_hover == Some(pointer.ts)
                })
                .map(|pointer| pointer.y);
            let overlay = CursorOverlay {
                hover: display_ts,
                frozen: cur.frozen.map(|t| t as f64),
                unit: spec.unit,
                hover_y_px: local_row,
            };
            // `hit_test_chart` is deliberately given the real pointer y when
            // this chart originated the cursor. A synced cursor has no
            // meaningful y coordinate in a sibling chart, so row-sensitive
            // chart kinds do not guess a row; x-only kinds remain synced.
            let hover_info = target.and_then(|_| {
                let local_hit = local_pointer.filter(|pointer| {
                    cur.hover == Some(pointer.hover_ms) && local_hover == Some(pointer.ts)
                });
                let fallback_hit = display_ts.map(|ts| {
                    (
                        layout.x_scale.to_px(ts),
                        layout.plot.y + layout.plot.h / 2.0,
                    )
                });
                let (mouse_x, mouse_y) = if let Some(pointer) = local_hit {
                    (pointer.x, pointer.y)
                } else if !chart_kind_requires_local_pointer(data_now.kind()) {
                    fallback_hit?
                } else {
                    return None;
                };
                if let Some(style) = &options.rrdtool
                    && graphtron::rrd::supports(&data_now)
                {
                    return graphtron::rrd::hit_test(&layout, &spec, &data_now, style, mouse_x);
                }
                if graphtron::partition::items(&data_now).is_some() {
                    return graphtron::partition::hit_test(
                        &layout,
                        &data_now,
                        &partition_geometry,
                        mouse_x,
                        mouse_y,
                    );
                }
                hit_test_chart_with_cache(
                    &layout,
                    &spec,
                    &data_now,
                    mouse_x,
                    mouse_y,
                    Some(&histogram_cache),
                )
            });
            // Freezing a point turns the live cursor into a measuring tape.
            // The frozen hit-test reuses the live pointer's row so row-oriented
            // kinds compare like against like.
            let mut hover_info = hover_info;
            if freeze_delta
                && graphtron::partition::items(&data_now).is_none()
                && let (Some(info), Some(frozen_ts)) = (hover_info.as_mut(), overlay.frozen)
                && frozen_ts != overlay.hover.unwrap_or(f64::NAN)
                && let Some(frozen_info) = if let Some(style) = &options.rrdtool
                    && graphtron::rrd::supports(&data_now)
                {
                    graphtron::rrd::hit_test(
                        &layout,
                        &spec,
                        &data_now,
                        style,
                        layout.x_scale.to_px(frozen_ts),
                    )
                } else {
                    hit_test_chart_with_cache(
                        &layout,
                        &spec,
                        &data_now,
                        layout.x_scale.to_px(frozen_ts),
                        info.values.first().map_or(layout.plot.y, |value| value.py),
                        Some(&histogram_cache),
                    )
                }
            {
                annotate_delta(info, &frozen_info, spec.unit);
            }
            // Capture once per frozen reference. Moving toward the inspector's
            // controls must not replace its values or collapse the open panel.
            // This is a snapshot, retained until the reference is cleared.
            if *inspected_frozen.peek() != cur.frozen {
                inspected_frozen.set(cur.frozen);
                accessible_hover.set(cur.frozen.and_then(|_| hover_info.clone()));
                inspector_expanded.set(false);
                if *tooltip_page.peek() != 0 {
                    tooltip_page.set(0);
                }
            }
            draw_overlay(
                &surface,
                &layout,
                &data_now,
                &overlay,
                hover_info.as_ref(),
                selection,
                &merged_annotations,
                &options,
            );
        });
    }

    let on_data_mount = {
        let locals = locals.clone();
        move |evt: MountedEvent| {
            if let Some(el) = element_canvas(&evt) {
                let w = el.client_width() as f64;
                let h = el.client_height() as f64;
                if let Some(surface) = CanvasSurface::new(el) {
                    let mut l = locals.borrow_mut();
                    l.css_w = if w > 0.0 { w } else { 600.0 };
                    l.css_h = if h > 0.0 { h } else { height };
                    l.data_surface = Some(surface);
                }
                redraw_tick += 1;
                overlay_tick += 1;
            }
        }
    };

    let on_overlay_mount = {
        let locals = locals.clone();
        move |evt: MountedEvent| {
            if let Some(el) = element_canvas(&evt) {
                if let Some(surface) = CanvasSurface::new(el) {
                    locals.borrow_mut().overlay_surface = Some(surface);
                }
                overlay_tick += 1;
            }
        }
    };

    // Container reflow — the ResizeObserver-backed event both hand-rolled
    // consumers were missing.
    let on_resize = {
        let locals = locals.clone();
        move |evt: Event<ResizeData>| {
            if let Ok(sz) = evt.get_content_box_size() {
                {
                    let mut l = locals.borrow_mut();
                    l.css_w = sz.width;
                    l.css_h = sz.height;
                }
                redraw_tick += 1;
                overlay_tick += 1;
            }
        }
    };

    let apply = move |actions: Vec<Action>| {
        let mut cursor = cursor;
        let mut overlay_tick = overlay_tick;
        let mut anim = anim;
        for action in actions {
            match action {
                Action::Hover(ts) => {
                    let mut c = cursor();
                    if c.frozen.is_none() && c.hover != Some(ts as i64) {
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
                Action::ClearFreeze => {
                    let mut c = cursor();
                    if c.frozen.is_some() {
                        c.frozen = None;
                        cursor.set(c);
                    }
                }
                Action::ToggleFreeze(ts) => {
                    let mut c = cursor();
                    // A tap can complete without a preceding move. Keep the
                    // live cursor at that position so the frozen semantic
                    // tooltip is available immediately.
                    c.hover = Some(ts as i64);
                    c.toggle_freeze(ts as i64);
                    cursor.set(c);
                }
                Action::ZoomTo { from_ms, to_ms } => {
                    let Some((from_ms, to_ms)) = constrain_domain(from_ms, to_ms) else {
                        continue;
                    };
                    if animate {
                        let start = last_drawn_domain();
                        anim.set(Some(DomainAnimation::new(
                            start,
                            (from_ms, to_ms),
                            js_sys::Date::now(),
                            200.0,
                        )));
                    }
                    set_domain(from_ms, to_ms);
                }
                Action::PanBy(delta_ms) => {
                    anim.set(None);
                    let (from, to) = resolve_domain();
                    set_domain(from + delta_ms, to + delta_ms);
                }
                Action::ResetZoom => {
                    let reset_target = default_domain
                        .or_else(|| default_range.map(|(from, to)| (from as f64, to as f64)))
                        .or_else(|| {
                            (domain.is_none() && range.is_none())
                                .then(|| data_x_domain(&data.peek()))
                                .flatten()
                        })
                        .and_then(|(f, t)| constrain_domain(f, t));
                    if animate
                        && on_reset.is_none()
                        && let Some(target) = reset_target
                    {
                        let start = last_drawn_domain();
                        anim.set(Some(DomainAnimation::new(
                            start,
                            target,
                            js_sys::Date::now(),
                            250.0,
                        )));
                    }
                    if let Some(cb) = &on_reset {
                        cb.call(());
                    } else if let Some((f, t)) = default_domain {
                        set_domain(f, t);
                    } else if let Some((f, t)) = default_range {
                        set_domain(f as f64, t as f64);
                    } else if domain.is_none() && range.is_none() {
                        // Back to autofit.
                        let mut local_domain = local_domain;
                        local_domain.set(None);
                        if let Some((f, t)) = data_x_domain(&data.peek()) {
                            if let Some(cb) = &on_domain_change {
                                cb.call((f, t));
                            }
                            if let Some(cb) = &on_zoom {
                                cb.call((f as i64, t as i64));
                            }
                        }
                    }
                }
                Action::SelectionChanged => overlay_tick += 1,
            }
        }
    };

    let on_pointer_down = {
        let locals = locals.clone();
        move |evt: PointerEvent| {
            if !interactive || !evt.data().is_primary() {
                return;
            }
            capture_pointer(&evt);
            let point = evt.element_coordinates();
            let mut l = locals.borrow_mut();
            if let Some(layout) = l.layout {
                let ts = layout.ts_at(point.x);
                l.hover_pointer = Some(HoverPointer {
                    x: point.x,
                    y: point.y,
                    hover_ms: ts as i64,
                    ts,
                });
            }
            l.interact.on_mouse_down(point.x);
        }
    };

    let on_pointer_move = {
        let locals = locals.clone();
        let mut overlay_tick = overlay_tick;
        move |evt: PointerEvent| {
            if !interactive || !evt.data().is_primary() {
                return;
            }
            let point = evt.element_coordinates();
            let (x, y) = (point.x, point.y);
            let actions = {
                let mut l = locals.borrow_mut();
                match l.layout {
                    Some(layout) => {
                        let ts = layout.ts_at(x);
                        l.hover_pointer = Some(HoverPointer {
                            x,
                            y,
                            hover_ms: ts as i64,
                            ts,
                        });
                        l.interact.on_mouse_move(x, &layout)
                    }
                    None => vec![],
                }
            };
            apply(actions);
            // The shared legacy cursor stores whole milliseconds. A fractional
            // linear pointer may therefore retain the same cursor value while
            // still needing an exact local crosshair and hit-test redraw.
            overlay_tick += 1;
        }
    };

    let on_pointer_up = {
        let locals = locals.clone();
        let x_axis = x_axis.clone();
        move |evt: PointerEvent| {
            if !interactive || !evt.data().is_primary() {
                return;
            }
            release_pointer(&evt);
            let point = evt.element_coordinates();
            let x = point.x;
            let mode = drag_mode.unwrap_or(if evt.data().modifiers().shift() {
                DragMode::Pan
            } else {
                DragMode::Zoom
            });
            let actions = {
                let mut l = locals.borrow_mut();
                match l.layout {
                    Some(layout) => {
                        let ts = layout.ts_at(x);
                        l.hover_pointer = Some(HoverPointer {
                            x,
                            y: point.y,
                            hover_ms: ts as i64,
                            ts,
                        });
                        let min_span =
                            interaction_min_span(&x_axis, layout.x_scale.d0, layout.x_scale.d1)
                                .unwrap_or(f64::INFINITY);
                        l.interact
                            .on_mouse_up_with_min_span(x, &layout, mode, min_span)
                    }
                    None => vec![],
                }
            };
            apply(actions);
        }
    };

    let on_pointer_leave = {
        let locals = locals.clone();
        move |_evt: PointerEvent| {
            if !interactive {
                return;
            }
            // A frozen hover owns the inspector's page controls. Moving from
            // the canvas into that DOM overlay must not clear the frozen
            // target before the user can activate Previous/Next.
            if cursor().frozen.is_some() {
                return;
            }
            let actions = {
                let mut l = locals.borrow_mut();
                l.hover_pointer = None;
                l.interact.on_mouse_leave()
            };
            apply(actions);
        }
    };

    let on_pointer_cancel = {
        let locals = locals.clone();
        move |evt: PointerEvent| {
            if !interactive {
                return;
            }
            release_pointer(&evt);
            let actions = {
                let mut l = locals.borrow_mut();
                l.hover_pointer = None;
                l.interact.on_mouse_leave()
            };
            apply(actions);
        }
    };

    let on_double_click = move |_evt: MouseEvent| {
        if interactive {
            apply(vec![Action::ResetZoom]);
        }
    };

    let on_wheel = {
        let locals = locals.clone();
        let x_axis = x_axis.clone();
        move |evt: WheelEvent| {
            if !interactive {
                return;
            }
            let (from, to) = resolve_domain();
            let Some(min_span) = interaction_min_span(&x_axis, from, to) else {
                return;
            };
            let Some(layout) = locals.borrow().layout else {
                return;
            };
            let delta = evt.delta().strip_units().y;
            if !delta.is_finite() || delta == 0.0 {
                return;
            }
            evt.prevent_default();
            let anchor = layout.ts_at(evt.element_coordinates().x);
            let (from_ms, to_ms) =
                graphtron::wheel_zoom_with_min_span(from, to, anchor, delta, min_span);
            apply(vec![Action::ZoomTo { from_ms, to_ms }]);
        }
    };

    let on_key_down = {
        let x_axis = x_axis.clone();
        move |evt: KeyboardEvent| {
            if !interactive {
                return;
            }
            let Some(key) = map_key(&evt.key()) else {
                return;
            };
            if matches!(&x_axis, XAxisKind::Category { .. }) && key != graphtron::Key::Escape {
                return;
            }
            evt.prevent_default();
            let (from, to) = resolve_domain();
            let min_span = interaction_min_span(&x_axis, from, to).unwrap_or(f64::EPSILON);
            let actions = InteractState::default().on_key_with_min_span(key, from, to, min_span);
            apply(actions);
        }
    };

    let css_cursor = if interactive { "crosshair" } else { "default" };
    let container_class = class.unwrap_or_else(|| "graphtron-chart".to_string());
    let show_legend = show_dom_legend;
    // The legacy `legend: true` prop historically placed a name-only legend
    // below the chart. Respect that contract unless the spec explicitly opts
    // into its own legend settings.
    let dom_legend_position = if spec.legend.show {
        spec.legend.position
    } else {
        LegendPosition::Bottom
    };
    let legend_before = show_legend
        && matches!(
            dom_legend_position,
            LegendPosition::Top | LegendPosition::Left
        );
    let legend_after = show_legend && !legend_before;
    let shell_direction = match dom_legend_position {
        LegendPosition::Left | LegendPosition::Right => "row",
        LegendPosition::Top | LegendPosition::Bottom => "column",
    };
    let chart_summary = use_memo(move || chart_summary(&data()));
    let interaction_instructions = if interactive && !matches!(&x_axis, XAxisKind::Category { .. })
    {
        "Pointer, touch, or pen: drag to zoom, Shift-drag to pan, click to freeze the cursor, and double-click to reset. Keyboard: arrow keys pan, plus or minus zoom, Home or End resets, and Escape clears the frozen cursor."
    } else if interactive {
        "Pointer, touch, or pen: click to freeze the cursor. Keyboard: Escape clears the frozen cursor. Categorical x axes do not support viewport zooming."
    } else {
        "This chart is read-only."
    };

    let hover_inspector = accessible_hover().map(|info| {
        let expanded = inspector_expanded();
        let page = tooltip_page();
        let page_start = page * TOOLTIP_PAGE_SIZE;
        let page_values = info
            .values
            .iter()
            .skip(page_start)
            .take(TOOLTIP_PAGE_SIZE)
            .cloned()
            .collect::<Vec<_>>();
        let page_rows = page_values.iter().map(|value| {
            let details = value.extra.iter()
                .map(|(label, number)| (label.clone(), spec.unit.format(*number)))
                .chain(value.extra_text.iter().cloned()).collect::<Vec<_>>();
            (value.name.clone(), value.formatted_value.clone()
                .unwrap_or_else(|| spec.unit.format(value.y)), value.series, details)
        }).collect::<Vec<_>>();
        let page_count = info.values.len().div_ceil(TOOLTIP_PAGE_SIZE).max(1);
        let page_label = format!("{} / {}", page + 1, page_count);
        let header = info
            .header
            .clone()
            .unwrap_or_else(|| format!("x = {}", spec.x_unit.format(info.ts)));
        let (inspector_bg, inspector_fg, inspector_accent) = if options.theme.dark {
            ("rgba(10,14,23,0.94)", "#d7dbe0", "#00ffff")
        } else {
            ("rgba(255,255,255,0.96)", "#1a1a1a", "#5794f2")
        };
        rsx! {
            div {
                class: "graphtron-tooltip-inspector",
                role: "group",
                aria_label: "Frozen chart details",
                onkeydown: move |evt| evt.stop_propagation(),
                style: "position: absolute; z-index: 2; right: 4px; top: 4px; max-width: calc(100% - 8px); max-height: calc(100% - 8px); overflow: auto; padding: 8px 10px; background: {inspector_bg}; color: {inspector_fg}; font: 11px monospace; pointer-events: auto;",
                button {
                    r#type: "button",
                    aria_expanded: "{expanded}",
                    onclick: move |_| inspector_expanded.set(!expanded),
                    style: "color: {inspector_accent}; background: transparent; border: 0; padding: 0; font: inherit; cursor: pointer;",
                    if expanded { "Hide values" } else { "Inspect values" }
                }
                if expanded {
                    div { style: "margin-top: 4px;",
                        div { style: "color: {inspector_accent}; margin-bottom: 4px;", "{header}" }
                        for (name, value_text, series_index, details) in page_rows {
                            div {
                                key: "{series_index}:{name}",
                                div {
                                    style: "display: flex; justify-content: space-between; gap: 12px; white-space: nowrap;",
                                    span { title: "{name}", style: "min-width: 0; overflow: hidden; text-overflow: ellipsis;", "{name}" }
                                    span { style: "flex: 0 0 auto;", "{value_text}" }
                                }
                                for (label, detail) in details {
                                    div {
                                        style: "display: flex; justify-content: space-between; gap: 12px; white-space: nowrap; opacity: 0.8;",
                                        span { "{label}" }
                                        span { "{detail}" }
                                    }
                                }
                            }
                        }
                        if page_count > 1 {
                            div { style: "display: flex; justify-content: space-between; margin-top: 6px;",
                                button {
                                    r#type: "button",
                                    disabled: page == 0,
                                    onclick: move |_| tooltip_page.set(page.saturating_sub(1)),
                                    "Previous"
                                }
                                span { "{page_label}" }
                                button {
                                    r#type: "button",
                                    disabled: page + 1 >= page_count,
                                    onclick: move |_| tooltip_page.set((page + 1).min(page_count - 1)),
                                    "Next"
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    rsx! {
        div {
            class: "graphtron-chart-shell",
            style: "display: flex; flex-direction: {shell_direction}; align-items: stretch; gap: 4px; width: 100%;",
            if legend_before {
                Legend { entries: legend_entries(), hidden, format: dom_legend_format }
            }
            div {
                class: "{container_class}",
                role: "region",
                aria_label: "{chart_summary()}. {interaction_instructions}",
                style: "position: relative; flex: 1 1 auto; min-width: 0; width: 100%; height: {height}px; outline: none;",
                tabindex: if interactive { "0" } else { "-1" },
                onresize: on_resize,
                onkeydown: on_key_down,
                {hover_inspector}
                canvas {
                    aria_hidden: "true",
                    style: "position: absolute; inset: 0; width: 100%; height: 100%;",
                    onmounted: on_data_mount,
                }
                canvas {
                    aria_hidden: "true",
                    style: "position: absolute; inset: 0; width: 100%; height: 100%; cursor: {css_cursor}; touch-action: none;",
                    onmounted: on_overlay_mount,
                    onpointerdown: on_pointer_down,
                    onpointermove: on_pointer_move,
                    onpointerup: on_pointer_up,
                    onpointerleave: on_pointer_leave,
                    onpointercancel: on_pointer_cancel,
                    ondoubleclick: on_double_click,
                    onwheel: on_wheel,
                }
            }
            if legend_after {
                Legend { entries: legend_entries(), hidden, format: dom_legend_format }
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

fn capture_pointer(evt: &PointerEvent) {
    let event_data = evt.data();
    let Some(pointer) = event_data.downcast::<web_sys::PointerEvent>() else {
        return;
    };
    // Dioxus delegates native events; current_target is the delegation root.
    // Capture on the canvas that actually received the pointer instead.
    let Some(target) = pointer.target() else {
        return;
    };
    if let Ok(element) = target.dyn_into::<web_sys::Element>() {
        let _ = element.set_pointer_capture(pointer.pointer_id());
    }
}

fn release_pointer(evt: &PointerEvent) {
    let event_data = evt.data();
    let Some(pointer) = event_data.downcast::<web_sys::PointerEvent>() else {
        return;
    };
    // Dioxus delegates native events; current_target is the delegation root.
    // Capture on the canvas that actually received the pointer instead.
    let Some(target) = pointer.target() else {
        return;
    };
    if let Ok(element) = target.dyn_into::<web_sys::Element>() {
        let _ = element.release_pointer_capture(pointer.pointer_id());
    }
}

/// Remove hidden series while materializing palette colors before reindexing,
/// so a visible series keeps its original hue in every chart variant.
fn filter_hidden(data: &ChartData, hidden: &HashSet<usize>) -> ChartData {
    if hidden.is_empty() {
        return data.clone();
    }
    let filter = |series: &[SeriesData]| -> Vec<SeriesData> {
        series
            .iter()
            .enumerate()
            .filter(|(i, _)| !hidden.contains(i))
            .map(|(i, s)| SeriesData {
                color: Some(series_color(s, i)),
                ..s.clone()
            })
            .collect()
    };
    let filter_ohlc = |series: &[graphtron::OhlcSeriesData]| {
        series
            .iter()
            .enumerate()
            .filter(|(i, _)| !hidden.contains(i))
            .map(|(i, s)| graphtron::OhlcSeriesData {
                color: Some(graphtron::ohlc_series_color(s, i)),
                ..s.clone()
            })
            .collect()
    };
    let filter_histogram = |series: &[graphtron::HistogramSeries]| {
        series
            .iter()
            .enumerate()
            .filter(|(i, _)| !hidden.contains(i))
            .map(|(i, s)| graphtron::HistogramSeries {
                color: Some(s.color.unwrap_or(PALETTE[i % PALETTE.len()])),
                ..s.clone()
            })
            .collect()
    };
    let filter_hbar = |series: &[graphtron::HBarSeries]| {
        series
            .iter()
            .enumerate()
            .filter(|(i, _)| !hidden.contains(i))
            .map(|(i, s)| graphtron::HBarSeries {
                color: Some(s.color.unwrap_or(PALETTE[i % PALETTE.len()])),
                ..s.clone()
            })
            .collect()
    };
    let filter_band = |series: &[graphtron::BandSeries]| {
        series
            .iter()
            .enumerate()
            .filter(|(i, _)| !hidden.contains(i))
            .map(|(i, s)| graphtron::BandSeries {
                color: Some(s.color.unwrap_or(PALETTE[i % PALETTE.len()])),
                ..s.clone()
            })
            .collect()
    };
    match data {
        ChartData::Lines(s) => ChartData::Lines(filter(s)),
        ChartData::Pie(items) | ChartData::Treemap(items) | ChartData::HostMap(items) => {
            let visible = items
                .iter()
                .enumerate()
                .filter(|(i, _)| !hidden.contains(i))
                .map(|(i, item)| graphtron::PartitionItem {
                    color: Some(graphtron::partition::item_color(data.kind(), items, i)),
                    ..item.clone()
                })
                .collect();
            match data {
                ChartData::Pie(_) => ChartData::Pie(visible),
                ChartData::Treemap(_) => ChartData::Treemap(visible),
                _ => ChartData::HostMap(visible),
            }
        }
        ChartData::Areas(s) => ChartData::Areas(filter(s)),
        ChartData::Bars(s) => ChartData::Bars(filter(s)),
        ChartData::Points(s) => ChartData::Points(filter(s)),
        ChartData::Scatter(s) => ChartData::Scatter(filter(s)),
        ChartData::Heatmap(s) => ChartData::Heatmap(filter(s)),
        ChartData::Step(s) => ChartData::Step(filter(s)),
        ChartData::Ohlc(s) => ChartData::Ohlc(filter_ohlc(s)),
        ChartData::Histogram(s) => ChartData::Histogram(filter_histogram(s)),
        ChartData::HBar(s) => ChartData::HBar(filter_hbar(s)),
        ChartData::StateTimeline(s) => ChartData::StateTimeline(
            s.iter()
                .enumerate()
                .filter(|(i, _)| !hidden.contains(i))
                .map(|(_, series)| series.clone())
                .collect(),
        ),
        ChartData::Band(s) => ChartData::Band(filter_band(s)),
    }
}

/// Entries use original indices and colors rather than the filtered draw data,
/// making visibility toggles stable across every chart kind.
fn legend_entries_for_data(
    data: &ChartData,
    format: LegendFormat,
    unit: graphtron::Unit,
) -> Vec<(String, u32)> {
    match data {
        ChartData::Pie(items) | ChartData::Treemap(items) | ChartData::HostMap(items) => items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                (
                    format_legend_label(&item.name, Some(&unit.format(item.value)), format),
                    graphtron::partition::item_color(data.kind(), items, i),
                )
            })
            .collect(),
        ChartData::Lines(series)
        | ChartData::Areas(series)
        | ChartData::Bars(series)
        | ChartData::Points(series)
        | ChartData::Scatter(series)
        | ChartData::Heatmap(series)
        | ChartData::Step(series) => series
            .iter()
            .enumerate()
            .map(|(i, series)| {
                let value = series
                    .ys
                    .iter()
                    .rev()
                    .copied()
                    .find(|value| value.is_finite())
                    .map(|value| unit.format(value));
                (
                    format_legend_label(&series.name, value.as_deref(), format),
                    series_color(series, i),
                )
            })
            .collect(),
        ChartData::Ohlc(series) => series
            .iter()
            .enumerate()
            .map(|(i, series)| {
                let value = series
                    .ticks
                    .iter()
                    .rev()
                    .find(|tick| tick.close.is_finite())
                    .map(|tick| unit.format(tick.close));
                (
                    format_legend_label(&series.name, value.as_deref(), format),
                    graphtron::ohlc_series_color(series, i),
                )
            })
            .collect(),
        ChartData::Histogram(series) => series
            .iter()
            .enumerate()
            .map(|(i, series)| {
                let value = if series.cumulative {
                    series
                        .counts
                        .len()
                        .checked_sub(1)
                        .and_then(|index| series.plot_count_at(index))
                        .filter(|value| value.is_finite())
                } else {
                    series
                        .counts
                        .iter()
                        .copied()
                        .filter(|value| value.is_finite())
                        .reduce(|sum, value| sum + value)
                }
                .map(|value| unit.format(value));
                (
                    format_legend_label(&series.name, value.as_deref(), format),
                    series.color.unwrap_or(PALETTE[i % PALETTE.len()]),
                )
            })
            .collect(),
        ChartData::HBar(series) => series
            .iter()
            .enumerate()
            .map(|(i, series)| {
                let aggregate = series
                    .values
                    .iter()
                    .copied()
                    .filter(|value| value.is_finite())
                    .reduce(|sum, value| sum + value);
                (
                    format_legend_label(
                        &series.name,
                        aggregate.map(|value| unit.format(value)).as_deref(),
                        format,
                    ),
                    series.color.unwrap_or(PALETTE[i % PALETTE.len()]),
                )
            })
            .collect(),
        ChartData::StateTimeline(series) => series
            .iter()
            .enumerate()
            .map(|(i, series)| {
                let last = series.segments.last();
                (
                    format_legend_label(
                        &series.name,
                        last.map(|segment| segment.label.as_str()),
                        format,
                    ),
                    last.map(|segment| segment.color)
                        .unwrap_or(PALETTE[i % PALETTE.len()]),
                )
            })
            .collect(),
        ChartData::Band(series) => series
            .iter()
            .enumerate()
            .map(|(i, series)| {
                let value = series
                    .center
                    .iter()
                    .rev()
                    .copied()
                    .find(|value| value.is_finite())
                    .map(|value| unit.format(value));
                (
                    format_legend_label(&series.name, value.as_deref(), format),
                    series.color.unwrap_or(PALETTE[i % PALETTE.len()]),
                )
            })
            .collect(),
    }
}

fn format_legend_label(name: &str, value: Option<&str>, format: LegendFormat) -> String {
    match format {
        LegendFormat::NameOnly => name.to_string(),
        LegendFormat::NameAndValue => value
            .map(|value| format!("{name}  {value}"))
            .unwrap_or_else(|| name.to_string()),
        LegendFormat::ValueOnly => value.unwrap_or(name).to_string(),
    }
}

fn filtered_options(options: &RenderOptions, hidden: &HashSet<usize>) -> RenderOptions {
    let mut options = options.clone();
    if let Some(style) = &mut options.rrdtool {
        style.line_series = style
            .line_series
            .iter()
            .filter(|i| !hidden.contains(i))
            .map(|i| i - hidden.iter().filter(|j| *j < i).count())
            .collect();
    }
    options
}

fn chart_kind_requires_local_pointer(kind: ChartKind) -> bool {
    matches!(
        kind,
        ChartKind::Point
            | ChartKind::Pie
            | ChartKind::Treemap
            | ChartKind::HostMap
            | ChartKind::Scatter
            | ChartKind::Heatmap
            | ChartKind::HBar
            | ChartKind::StateTimeline
    )
}

/// A concise text alternative: it describes the chart shape without creating a
/// hidden DOM node for every plotted point (which is prohibitively expensive
/// for dense telemetry panels).
fn chart_summary(data: &ChartData) -> String {
    let (kind, series, marks) = match data {
        ChartData::Pie(items) => ("pie", 1, items.len()),
        ChartData::Treemap(items) => ("treemap", 1, items.len()),
        ChartData::HostMap(items) => ("host map", 1, items.len()),
        ChartData::Lines(series) => ("line", series.len(), point_count(series)),
        ChartData::Areas(series) => ("area", series.len(), point_count(series)),
        ChartData::Bars(series) => ("bar", series.len(), point_count(series)),
        ChartData::Points(series) => ("point", series.len(), point_count(series)),
        ChartData::Scatter(series) => ("scatter", series.len(), point_count(series)),
        ChartData::Heatmap(series) => ("heatmap", series.len(), point_count(series)),
        ChartData::Step(series) => ("step", series.len(), point_count(series)),
        ChartData::Ohlc(series) => (
            "OHLC",
            series.len(),
            series.iter().map(|series| series.ticks.len()).sum(),
        ),
        ChartData::Histogram(series) => (
            "histogram",
            series.len(),
            series.iter().map(|series| series.counts.len()).sum(),
        ),
        ChartData::HBar(series) => (
            "horizontal bar",
            series.len(),
            series.iter().map(|series| series.values.len()).sum(),
        ),
        ChartData::StateTimeline(series) => (
            "state timeline",
            series.len(),
            series.iter().map(|series| series.segments.len()).sum(),
        ),
        ChartData::Band(series) => (
            "band",
            series.len(),
            series.iter().map(|series| series.center.len()).sum(),
        ),
    };
    format!("{kind} chart with {series} series and {marks} data marks")
}

fn point_count(series: &[SeriesData]) -> usize {
    series
        .iter()
        .map(|series| series.xs.len().min(series.ys.len()))
        .sum()
}

/// Finite x-domain of the data for autofit when no viewport is provided.
fn data_x_domain(data: &ChartData) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut observe = |value: f64| {
        if value.is_finite() {
            lo = lo.min(value);
            hi = hi.max(value);
        }
    };
    match data {
        ChartData::Ohlc(series) => {
            for s in series {
                for tick in &s.ticks {
                    observe(tick.ts);
                }
            }
        }
        ChartData::Band(series) => {
            for s in series {
                for x in &s.xs {
                    observe(*x);
                }
            }
        }
        ChartData::StateTimeline(series) => {
            for s in series {
                for seg in &s.segments {
                    lo = lo.min(seg.start_ms);
                    hi = hi.max(seg.end_ms);
                }
            }
        }
        ChartData::Histogram(series) => {
            for s in series {
                for edge in &s.buckets {
                    observe(*edge);
                }
            }
        }
        ChartData::HBar(series) => {
            observe(0.0);
            for s in series {
                for value in &s.values {
                    observe(*value);
                }
            }
        }
        _ => {
            for s in data.point_series() {
                for x in &s.xs {
                    observe(*x);
                }
            }
        }
    }
    (lo.is_finite() && hi.is_finite() && hi > lo).then_some((lo, hi))
}

/// Minimum selectable span appropriate to the axis. Categories currently have
/// no viewport support in graphtron, so they return `None` rather than pretending
/// to zoom/pan an unchanged chart.
fn interaction_min_span(axis: &XAxisKind, from: f64, to: f64) -> Option<f64> {
    match axis {
        XAxisKind::Time => Some(1_000.0),
        XAxisKind::Linear { .. } => Some(((to - from).abs().max(1.0) * 1e-6).max(f64::EPSILON)),
        XAxisKind::Category { .. } => None,
    }
}

fn map_key(key: &dioxus::html::Key) -> Option<graphtron::Key> {
    use dioxus::html::Key as DKey;
    match key {
        DKey::ArrowLeft => Some(graphtron::Key::ArrowLeft),
        DKey::ArrowRight => Some(graphtron::Key::ArrowRight),
        DKey::ArrowUp => Some(graphtron::Key::ArrowUp),
        DKey::ArrowDown => Some(graphtron::Key::ArrowDown),
        DKey::Home => Some(graphtron::Key::Home),
        DKey::End => Some(graphtron::Key::End),
        DKey::Escape => Some(graphtron::Key::Escape),
        DKey::Character(c) if c == "+" || c == "=" => Some(graphtron::Key::Plus),
        DKey::Character(c) if c == "-" => Some(graphtron::Key::Minus),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hiding_hbar_preserves_the_original_palette_color() {
        let data = ChartData::HBar(vec![
            graphtron::HBarSeries {
                name: "first".into(),
                categories: vec!["one".into()],
                values: vec![1.0],
                color: None,
            },
            graphtron::HBarSeries {
                name: "second".into(),
                categories: vec!["one".into()],
                values: vec![2.0],
                color: None,
            },
        ]);
        let hidden = HashSet::from([0]);

        let ChartData::HBar(visible) = filter_hidden(&data, &hidden) else {
            panic!("hbar data must remain hbar data");
        };
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "second");
        assert_eq!(visible[0].color, Some(PALETTE[1]));
    }

    #[test]
    fn legend_uses_requested_value_format() {
        let data = ChartData::Lines(vec![SeriesData {
            name: "requests".into(),
            xs: vec![0.0, 1.0],
            ys: vec![1.0, 2.5],
            color: None,
        }]);

        assert_eq!(
            legend_entries_for_data(&data, LegendFormat::NameAndValue, graphtron::Unit::None)[0].0,
            "requests  2.5"
        );
        assert_eq!(
            legend_entries_for_data(&data, LegendFormat::ValueOnly, graphtron::Unit::None)[0].0,
            "2.5"
        );
    }

    #[test]
    fn histogram_legend_uses_sum_or_cumulative_final_value() {
        let histogram = graphtron::HistogramSeries {
            name: "latency".into(),
            buckets: vec![0.0, 1.0, 2.0],
            counts: vec![2.0, 3.0],
            color: None,
            cumulative: false,
        };
        let raw = ChartData::Histogram(vec![histogram.clone()]);
        assert_eq!(
            legend_entries_for_data(&raw, LegendFormat::ValueOnly, graphtron::Unit::None)[0].0,
            "5"
        );

        let cumulative = ChartData::Histogram(vec![graphtron::HistogramSeries {
            cumulative: true,
            ..histogram
        }]);
        assert_eq!(
            legend_entries_for_data(&cumulative, LegendFormat::ValueOnly, graphtron::Unit::None)[0].0,
            "5"
        );
    }

    #[test]
    fn interaction_minimum_respects_axis_semantics() {
        assert_eq!(
            interaction_min_span(&XAxisKind::Time, 0.0, 10.0),
            Some(1_000.0)
        );
        assert_eq!(
            interaction_min_span(
                &XAxisKind::Linear {
                    min: None,
                    max: None,
                },
                0.0,
                0.5,
            ),
            Some(0.000_001)
        );
        assert_eq!(
            interaction_min_span(
                &XAxisKind::Category {
                    labels: vec!["one".into()],
                },
                0.0,
                1.0,
            ),
            None
        );
    }

    #[test]
    fn linear_minimum_allows_a_sub_second_drag_selection() {
        let layout = graphtron::ChartLayout::compute(600.0, 200.0, 0.0, 0.5, 0.0, 1.0);
        let mut interaction = InteractState::default();
        interaction.on_mouse_down(100.0);
        interaction.on_mouse_move(200.0, &layout);

        let min_span = interaction_min_span(
            &XAxisKind::Linear {
                min: None,
                max: None,
            },
            0.0,
            0.5,
        )
        .expect("linear axes have a minimum span");
        let actions =
            interaction.on_mouse_up_with_min_span(200.0, &layout, DragMode::Zoom, min_span);
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, Action::ZoomTo { from_ms, to_ms } if to_ms - from_ms < 1.0 && to_ms - from_ms >= min_span))
        );
    }

    #[test]
    fn local_hover_keeps_fractional_x_when_legacy_cursor_is_unchanged() {
        let cursor = Cursor {
            hover: Some(7),
            frozen: None,
        };
        let first = HoverPointer {
            x: 10.0,
            y: 20.0,
            hover_ms: 7,
            ts: 7.125,
        };
        let second = HoverPointer {
            x: 11.0,
            y: 20.0,
            hover_ms: 7,
            ts: 7.875,
        };

        assert_eq!(local_hover_timestamp(cursor, Some(first)), Some(7.125));
        assert_eq!(local_hover_timestamp(cursor, Some(second)), Some(7.875));
    }

    #[test]
    fn domain_animation_interpolates_both_edges() {
        let animation = DomainAnimation::new((0.0, 100.0), (50.0, 200.0), 0.0, 1_000.0);
        let midpoint = animation.value_at(500.0);
        assert!(midpoint.0 > 0.0 && midpoint.0 < 50.0);
        assert!(midpoint.1 > 100.0 && midpoint.1 < 200.0);
    }

    #[test]
    fn data_x_domain_ignores_non_finite_edges() {
        let data = ChartData::Lines(vec![SeriesData {
            name: "finite middle".into(),
            xs: vec![f64::NAN, 1.25, 3.75, f64::INFINITY],
            ys: vec![0.0; 4],
            color: None,
        }]);
        assert_eq!(data_x_domain(&data), Some((1.25, 3.75)));
    }

    #[test]
    fn spec_annotations_are_merged_with_component_annotations_without_duplicates() {
        let threshold = Annotation::threshold(5.0, "warning", PALETTE[0]);
        let marker = Annotation::marker(10.0, "deploy", PALETTE[1]);
        let merged = merge_annotations(
            std::slice::from_ref(&threshold),
            &[threshold.clone(), marker.clone()],
        );

        assert_eq!(merged, vec![threshold, marker]);
    }

    #[test]
    fn summary_is_bounded_by_series_and_mark_counts() {
        let data = ChartData::StateTimeline(vec![graphtron::StateTimelineSeries {
            name: "api".into(),
            segments: vec![graphtron::StateSegment {
                start_ms: 0.0,
                end_ms: 1.0,
                label: "up".into(),
                color: PALETTE[0],
            }],
        }]);

        assert_eq!(
            chart_summary(&data),
            "state timeline chart with 1 series and 1 data marks"
        );
    }
}
