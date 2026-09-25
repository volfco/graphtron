use graphtron::partition::{Shape, geometry};
use graphtron::*;

fn items(values: &[f64]) -> Vec<PartitionItem> {
    values
        .iter()
        .enumerate()
        .map(|(i, &value)| PartitionItem {
            name: format!("item {i}"),
            value,
            color: None,
        })
        .collect()
}
fn plot() -> Rect {
    Rect {
        x: 10.0,
        y: 20.0,
        w: 400.0,
        h: 200.0,
    }
}

#[test]
fn pie_angles_are_proportional_and_large_weights_do_not_overflow() {
    for weights in [[1.0, 3.0], [f64::MAX / 3.0, f64::MAX]] {
        let shapes = geometry(ChartKind::Pie, &items(&weights), plot());
        let Shape::Slice { start, end, .. } = shapes[0].1 else {
            panic!()
        };
        assert_eq!(start, 0.0);
        assert!((end - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        let Shape::Slice { end, .. } = shapes[1].1 else {
            panic!()
        };
        assert_eq!(end, std::f64::consts::TAU);
    }
}

#[test]
fn partitions_hit_their_own_shapes_and_exclude_outside_points() {
    let layout = ChartLayout::compute(500.0, 300.0, 0.0, 1.0, 0.0, 1.0);
    for data in [
        ChartData::Pie(items(&[1.0, 3.0, 2.0])),
        ChartData::Treemap(items(&[1.0, 3.0, 2.0])),
        ChartData::HostMap(items(&[1.0, 3.0, 2.0])),
    ] {
        let shapes = geometry(
            data.kind(),
            graphtron::partition::items(&data).unwrap(),
            layout.plot,
        );
        for (index, shape) in &shapes {
            let (x, y) = shape.center();
            let hit = graphtron::partition::hit_test(&layout, &data, &shapes, x, y).unwrap();
            assert_eq!(hit.values[0].series, *index);
            assert_eq!(hit.values[0].shape, Some(*shape));
        }
        assert!(graphtron::partition::hit_test(&layout, &data, &shapes, -1.0, -1.0).is_none());
    }
}

#[test]
fn treemap_preserves_weight_ratios_and_host_tiles_are_equal_squares() {
    let shapes = geometry(ChartKind::Treemap, &items(&[1.0, 3.0]), plot());
    let (Shape::Tile(a), Shape::Tile(b)) = (shapes[0].1, shapes[1].1) else {
        panic!()
    };
    assert!(((b.w + 2.0) / (a.w + 2.0) - 3.0).abs() < 1e-12);
    assert_eq!(a.h, b.h);
    let shapes = geometry(
        ChartKind::HostMap,
        &items(&[0.0, 10.0, 100.0, f64::NAN, -1.0]),
        plot(),
    );
    assert_eq!(shapes.len(), 5);
    let mut side = None;
    for (_, shape) in shapes {
        let Shape::Tile(r) = shape else { panic!() };
        assert!((r.w - r.h).abs() < 1e-12);
        assert_eq!(*side.get_or_insert(r.w), r.w);
        assert!(plot().contains(r.x, r.y) && plot().contains(r.x + r.w, r.y + r.h));
    }
}

#[test]
fn invalid_weights_are_reported_and_cannot_create_invalid_geometry() {
    let data = ChartData::Pie(items(&[0.0, -1.0, f64::NAN, f64::INFINITY]));
    assert!(data.is_empty());
    assert_eq!(data.validate().unwrap_err().len(), 3);
    assert!(
        geometry(
            data.kind(),
            graphtron::partition::items(&data).unwrap(),
            plot()
        )
        .is_empty()
    );
    let tiny = Rect {
        w: 0.01,
        h: 0.01,
        ..plot()
    };
    for (_, shape) in geometry(
        ChartKind::Treemap,
        &items(&[f64::MIN_POSITIVE, f64::MAX, 1.0]),
        tiny,
    ) {
        let Shape::Tile(r) = shape else { panic!() };
        assert!(r.w.is_finite() && r.h.is_finite() && r.w >= 0.0 && r.h >= 0.0);
    }
}

#[test]
fn timeline_hit_excludes_gutters_and_uses_equal_insets() {
    let layout = ChartLayout::compute(500.0, 300.0, 0.0, 100.0, 0.0, 2.0);
    let row = StateTimelineSeries {
        name: "host".into(),
        segments: vec![StateSegment {
            start_ms: 0.0,
            end_ms: 50.0,
            label: "up".into(),
            color: 0x00ff00,
        }],
    };
    let data = ChartData::StateTimeline(vec![row.clone(), row]);
    let spec = ChartSpec::state_timeline();
    let x = layout.x_scale.to_px(25.0);
    assert!(hit_test_chart(&layout, &spec, &data, x, layout.plot.y + 0.5).is_none());
    let hit = hit_test_chart(&layout, &spec, &data, x, layout.plot.y + 10.0).unwrap();
    let Some(Shape::Tile(r)) = hit.values[0].shape else {
        panic!()
    };
    assert_eq!(r.x - layout.plot.x, r.y - layout.plot.y);
    assert_eq!(r.w, layout.plot.w / 2.0 - 2.0);
    assert_eq!(r.h, layout.plot.h / 2.0 - 2.0);
}

#[test]
fn point_plot_uses_numeric_values_and_the_requested_axis_mapping() {
    let data = ChartData::Points(vec![SeriesData {
        name: "sample".into(),
        xs: vec![50.0],
        ys: vec![40.0],
        color: None,
    }]);
    let layout = ChartLayout::compute(500.0, 300.0, 0.0, 100.0, 0.0, 100.0);
    let hit = hit_test_chart(
        &layout,
        &ChartSpec::point(Unit::None),
        &data,
        layout.x_scale.to_px(50.0),
        layout.y_px(40.0),
    )
    .unwrap();
    assert_eq!(hit.values[0].y, 40.0);
    assert_eq!(
        layout.y_at(layout.y_px(50.0)) - layout.y_at(hit.values[0].py),
        10.0
    );
}
