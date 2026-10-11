use ravet_surface::layout::{GridArea, LayoutError, LayoutItem, Rect, layout_grid};
fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(x, y, w, h).unwrap()
}
fn area(row: usize, column: usize, row_span: usize, column_span: usize) -> GridArea {
    GridArea {
        row,
        column,
        row_span,
        column_span,
    }
}
fn flexible(minimum: f64, preferred: f64, maximum: f64, grow: f64) -> LayoutItem {
    LayoutItem {
        minimum,
        preferred,
        maximum,
        grow,
    }
}
#[test]
fn independent_constraints_spans_gaps_overlap_and_order() {
    let bounds = rect(10., -20., 300., 180.);
    let mut out = [bounds; 5];
    let columns = [
        LayoutItem::fixed(50.),
        flexible(20., 40., 60., 1.),
        flexible(20., 40., 400., 1.),
    ];
    let rows = [LayoutItem::fixed(20.), flexible(30., 60., 300., 1.)];
    let areas = [
        area(0, 0, 1, 3),
        area(1, 2, 1, 1),
        area(0, 1, 2, 2),
        area(1, 0, 1, 2),
        area(1, 2, 1, 1),
    ];
    layout_grid(bounds, &columns, &rows, 10., 5., &areas, &mut out).unwrap();
    assert_eq!(
        out,
        [
            rect(10., -20., 300., 20.),
            rect(140., 5., 170., 155.),
            rect(70., -20., 240., 180.),
            rect(10., 5., 120., 155.),
            rect(140., 5., 170., 155.)
        ]
    );
    layout_grid(
        rect(0., 0., 120., 60.),
        &columns,
        &rows,
        10.,
        5.,
        &areas,
        &mut out,
    )
    .unwrap();
    assert_eq!(out[0], rect(0., 0., 120., 20.));
    assert_eq!(out[1], rect(95., 25., 25., 35.));
    assert_eq!(out[2], rect(60., 0., 60., 60.));
}
#[test]
fn subpixel_trailing_space_zero_tracks_and_large_normalized_weights() {
    let bounds = rect(-4.25, 3.5, 20.5, 30.25);
    let mut out = [bounds; 2];
    layout_grid(
        bounds,
        &[LayoutItem::fixed(2.25), LayoutItem::fixed(0.)],
        &[LayoutItem::fixed(3.75)],
        0.5,
        0.,
        &[area(0, 0, 1, 2), area(0, 1, 1, 1)],
        &mut out,
    )
    .unwrap();
    assert_eq!(
        out,
        [rect(-4.25, 3.5, 2.75, 3.75), rect(-1.5, 3.5, 0., 3.75)]
    );
    let weight = flexible(0., 0., 100., f64::MAX);
    layout_grid(
        rect(0., 0., 10., 12.),
        &[weight; 2],
        &[weight; 2],
        0.,
        0.,
        &[area(0, 0, 1, 1), area(1, 1, 1, 1)],
        &mut out,
    )
    .unwrap();
    assert_eq!(out, [rect(0., 0., 5., 6.), rect(5., 6., 5., 6.)]);
}
#[test]
fn exact_track_area_limits_empty_area_validation_and_fixed_order() {
    let bounds = rect(0., 0., 63., 63.);
    let mut out = [bounds; 256];
    let mut areas = [area(0, 0, 1, 1); 256];
    for (index, a) in areas.iter_mut().enumerate() {
        *a = area(index / 32, index % 32, 1, 1);
    }
    areas[254] = area(0, 0, 32, 32);
    areas[255] = area(31, 31, 1, 1);
    layout_grid(
        bounds,
        &[LayoutItem::fixed(1.); 32],
        &[LayoutItem::fixed(1.); 32],
        1.,
        1.,
        &areas,
        &mut out,
    )
    .unwrap();
    assert_eq!(out[255], rect(62., 62., 1., 1.));
    assert_eq!(out[254], bounds);
    assert!(
        layout_grid(
            bounds,
            &[LayoutItem::fixed(1.)],
            &[LayoutItem::fixed(1.)],
            0.,
            0.,
            &[],
            &mut []
        )
        .is_ok()
    );
    assert_eq!(
        layout_grid(
            bounds,
            &[LayoutItem::fixed(1.)],
            &[LayoutItem::fixed(64.)],
            0.,
            0.,
            &[],
            &mut []
        ),
        Err(LayoutError::InsufficientSpace)
    );
}
#[test]
fn rejected_inputs_and_late_axis_failures_preserve_all_output() {
    let bounds = rect(0., 0., 100., 100.);
    let sentinel = rect(-7., -8., 1., 2.);
    let mut out = [sentinel; 2];
    let tracks = [LayoutItem::fixed(10.); 2];
    let areas = [area(0, 0, 1, 1), area(1, 1, 1, 1)];
    let mut check = |columns: &[LayoutItem],
                     rows: &[LayoutItem],
                     cg: f64,
                     rg: f64,
                     a: &[GridArea],
                     expected| {
        assert_eq!(
            layout_grid(bounds, columns, rows, cg, rg, a, &mut out),
            Err(expected)
        );
        assert_eq!(out, [sentinel; 2]);
    };
    check(&[], &tracks, 0., 0., &areas, LayoutError::InvalidInput);
    check(&tracks, &[], 0., 0., &areas, LayoutError::InvalidInput);
    check(
        &[LayoutItem::fixed(1.); 33],
        &tracks,
        0.,
        0.,
        &areas,
        LayoutError::Capacity,
    );
    check(
        &tracks,
        &[LayoutItem::fixed(1.); 33],
        0.,
        0.,
        &areas,
        LayoutError::Capacity,
    );
    check(
        &tracks,
        &tracks,
        0.,
        0.,
        &[area(0, 0, 1, 1); 257],
        LayoutError::Capacity,
    );
    check(&tracks, &tracks, 0., 0., &areas[..1], LayoutError::Capacity);
    for a in [
        area(0, 0, 0, 1),
        area(0, 0, 1, 0),
        area(1, 0, 2, 1),
        area(0, 1, 1, 2),
        area(usize::MAX, 0, 1, 1),
        area(0, usize::MAX, 1, 1),
        area(0, 0, usize::MAX, 1),
    ] {
        check(
            &tracks,
            &tracks,
            0.,
            0.,
            &[areas[0], a],
            LayoutError::InvalidInput,
        );
    }
    for bad in [f64::NAN, f64::INFINITY, -1.] {
        check(&tracks, &tracks, bad, 0., &areas, LayoutError::InvalidInput);
        check(&tracks, &tracks, 0., bad, &areas, LayoutError::InvalidInput);
        check(
            &tracks,
            &[flexible(0., 1., 10., bad); 2],
            0.,
            0.,
            &areas,
            LayoutError::InvalidInput,
        );
    }
    check(
        &tracks,
        &[flexible(2., 1., 10., 0.); 2],
        0.,
        0.,
        &areas,
        LayoutError::InvalidInput,
    );
    check(
        &tracks,
        &[flexible(0., 11., 10., 0.); 2],
        0.,
        0.,
        &areas,
        LayoutError::InvalidInput,
    );
    check(
        &tracks,
        &[LayoutItem::fixed(51.); 2],
        0.,
        0.,
        &areas,
        LayoutError::InsufficientSpace,
    );
    check(
        &tracks,
        &tracks,
        0.,
        101.,
        &areas,
        LayoutError::InsufficientSpace,
    );
    check(
        &tracks,
        &[LayoutItem::fixed(f64::MAX); 2],
        0.,
        0.,
        &areas,
        LayoutError::Arithmetic,
    );
    check(
        &tracks,
        &[LayoutItem::fixed(1.); 3],
        0.,
        f64::MAX,
        &areas,
        LayoutError::Arithmetic,
    );
}
#[test]
fn resize_sweep_span_conservation_bounds_and_no_allocation() {
    let columns = [
        flexible(10., 80., 90., 2.),
        flexible(10., 30., 250., 1.),
        flexible(10., 30., 250., 3.),
    ];
    let rows = [flexible(10., 40., 100., 1.), flexible(10., 40., 200., 2.)];
    let areas = [area(0, 0, 1, 1), area(1, 1, 1, 2), area(0, 0, 2, 3)];
    let mut out = [rect(0., 0., 1., 1.); 3];
    let counts = allocation_counter::measure(|| {
        for step in 0..10000 {
            let width = 40. + f64::from(step % 400);
            let height = 25. + f64::from(step % 270);
            layout_grid(
                rect(-5., 7., width, height),
                &columns,
                &rows,
                5.,
                5.,
                &areas,
                &mut out,
            )
            .unwrap();
            for r in out {
                assert!(r.x() >= -5. && r.y() >= 7.);
                assert!(r.x() + r.width() <= width - 5. + 1e-10);
                assert!(r.y() + r.height() <= height + 7. + 1e-10);
            }
            assert!(out[0].width() >= 10. - 1e-10 && out[0].width() <= 90. + 1e-10);
            assert!(out[1].x() >= out[0].x() + out[0].width() + 5. - 1e-10);
            assert!((out[2].width() - (out[0].width() + 5. + out[1].width())).abs() < 1e-10);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
