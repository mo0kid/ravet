use ravet_surface::{
    animation::{Easing, Tween},
    drawing::{Color, DrawingError, Font, SvgDrawing, TextAnchor, Theme},
    layout::{Axis, LayoutError, LayoutItem, Rect, layout_axis},
};
fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(x, y, width, height).unwrap()
}
#[test]
fn layout_growth_caps_shrink_nested_axes_and_failure_preservation() {
    let bounds = rect(10., 20., 300., 80.);
    let mut output = [bounds; 3];
    let items = [
        LayoutItem::fixed(50.),
        LayoutItem {
            minimum: 20.,
            preferred: 40.,
            maximum: 60.,
            grow: 1.,
        },
        LayoutItem {
            minimum: 20.,
            preferred: 40.,
            maximum: 400.,
            grow: 1.,
        },
    ];
    layout_axis(bounds, Axis::Horizontal, 10., &items, &mut output).unwrap();
    assert_eq!(
        output,
        [
            rect(10., 20., 50., 80.),
            rect(70., 20., 60., 80.),
            rect(140., 20., 170., 80.)
        ]
    );
    layout_axis(
        rect(0., 0., 120., 80.),
        Axis::Horizontal,
        10.,
        &items,
        &mut output,
    )
    .unwrap();
    assert_eq!(
        output,
        [
            rect(0., 0., 50., 80.),
            rect(60., 0., 25., 80.),
            rect(95., 0., 25., 80.)
        ]
    );
    let prior = output;
    assert_eq!(
        layout_axis(
            rect(0., 0., 80., 80.),
            Axis::Horizontal,
            10.,
            &items,
            &mut output
        ),
        Err(LayoutError::InsufficientSpace)
    );
    assert_eq!(output, prior);
    let mut invalid = items;
    invalid[1].grow = f64::NAN;
    assert_eq!(
        layout_axis(bounds, Axis::Horizontal, 10., &invalid, &mut output),
        Err(LayoutError::InvalidInput)
    );
    assert_eq!(output, prior);
    let mut rows = [bounds; 2];
    let inner = bounds.inset(10.).unwrap();
    layout_axis(
        inner,
        Axis::Vertical,
        5.,
        &[LayoutItem::fixed(20.), LayoutItem::fixed(15.)],
        &mut rows,
    )
    .unwrap();
    assert_eq!(rows, [rect(20., 30., 280., 20.), rect(20., 55., 280., 15.)]);
    assert!(rows[0].contains(20., 30.));
    assert!(!rows[0].contains(300., 30.));
    assert_eq!(rows[0].intersection(rows[1]).height(), 0.);
    assert!(bounds.inset(41.).is_none());
    assert!(Rect::new(f64::MAX, 0., f64::MAX, 1.).is_none());
    let huge = [LayoutItem {
        minimum: 0.,
        preferred: 0.,
        maximum: 10.,
        grow: f64::MAX,
    }; 2];
    layout_axis(
        rect(0., 0., 10., 1.),
        Axis::Horizontal,
        0.,
        &huge,
        &mut rows,
    )
    .unwrap();
    assert_eq!(rows[0].width(), 5.);
    assert_eq!(rows[1].width(), 5.);
}
#[test]
fn animation_exact_edges_retarget_reduced_motion_large_values_and_time_overflow() {
    for easing in [Easing::Linear, Easing::SmoothStep, Easing::CubicInOut] {
        let mut tween = Tween::new(-2., 2., 100, 100, easing).unwrap();
        assert_eq!(tween.sample(99), -2.);
        assert_eq!(tween.sample(100), -2.);
        assert_eq!(tween.sample(150), 0.);
        assert_eq!(tween.sample(200), 2.);
        assert!(tween.finished(200));
        assert!(!tween.retarget(f64::NAN, 150, 20, easing));
        assert_eq!(tween.sample(150), 0.);
        assert!(tween.retarget(4., 150, 20, easing));
        assert_eq!(tween.sample(150), 0.);
        assert_eq!(tween.sample(160), 2.);
        assert_eq!(tween.sample(170), 4.);
        let extreme = Tween::new(-f64::MAX, f64::MAX, 0, 100, easing).unwrap();
        for time in 0..=100 {
            assert!(extreme.sample(time).is_finite());
        }
    }
    let tween = Tween::new(0., 1., u64::MAX - 10, 20, Easing::Linear).unwrap();
    assert_eq!(tween.sample(u64::MAX), 0.5);
    assert!(!tween.finished(u64::MAX));
    let snap = Tween::new(0., 1., 20, 0, Easing::Linear).unwrap();
    assert_eq!(snap.sample(19), 0.);
    assert_eq!(snap.sample(20), 1.);
    assert!(snap.finished(20));
}
#[test]
fn drawing_escapes_text_limits_document_and_preserves_rejected_commands() {
    let mut scene = SvgDrawing::new(100., 50., "<Title & \"label\">").unwrap();
    let font = Font {
        family: "test\" onload=\"bad",
        size: 12.,
        weight: 400,
    };
    scene
        .text(
            (2., 14.),
            "<script>&'",
            font,
            TextAnchor::Start,
            Theme::DARK.foreground,
        )
        .unwrap();
    scene
        .rectangle(rect(0., 0., 10., 5.), 10., Color::rgb(1, 2, 3))
        .unwrap();
    scene
        .polyline(&[(0., 0.), (1., 2.)], 1., Theme::DARK.accent)
        .unwrap();
    assert_eq!(
        scene.polyline(&[(0., 0.), (f64::NAN, 2.)], 1., Theme::DARK.accent),
        Err(DrawingError::InvalidInput)
    );
    assert_eq!(scene.command_count(), 3);
    let document = scene.finish();
    assert!(document.contains("rx=\"2.5\""));
    assert!(document.contains("&lt;script&gt;&amp;&apos;"));
    assert!(!document.contains("<script>"));
    assert!(document.contains("test&quot; onload=&quot;bad"));
    assert!(document.ends_with("</svg>"));
    let mut bounded = SvgDrawing::new(1., 1., "").unwrap();
    for _ in 0..4096 {
        bounded
            .rectangle(rect(0., 0., 1., 1.), 0., Theme::LIGHT.panel)
            .unwrap();
    }
    assert_eq!(
        bounded.rectangle(rect(0., 0., 1., 1.), 0., Theme::LIGHT.panel),
        Err(DrawingError::Capacity)
    );
    assert_eq!(bounded.command_count(), 4096);
    let mut bounded = SvgDrawing::new(1., 1., "").unwrap();
    let text = "&".repeat(4096);
    let mut accepted = 0;
    while bounded
        .text(
            (0., 0.),
            &text,
            font,
            TextAnchor::Start,
            Color::rgb(0, 0, 0),
        )
        .is_ok()
    {
        accepted += 1;
    }
    assert!(accepted < 4096);
    assert_eq!(bounded.command_count(), accepted);
    assert!(bounded.finish().len() <= 1_048_576);
}

#[test]
fn layout_resize_sweep_and_animation_do_not_allocate() {
    let items = [
        LayoutItem {
            minimum: 10.,
            preferred: 80.,
            maximum: 90.,
            grow: 2.,
        },
        LayoutItem {
            minimum: 10.,
            preferred: 30.,
            maximum: 250.,
            grow: 1.,
        },
        LayoutItem {
            minimum: 10.,
            preferred: 30.,
            maximum: 250.,
            grow: 3.,
        },
    ];
    let mut output = [rect(0., 0., 1., 1.); 3];
    let tween = Tween::new(0., 1., 0, 10000, Easing::SmoothStep).unwrap();
    let counts = allocation_counter::measure(|| {
        for step in 0..10000 {
            let width = 40. + f64::from(step % 400);
            layout_axis(
                rect(0., 0., width, 50.),
                Axis::Horizontal,
                5.,
                &items,
                &mut output,
            )
            .unwrap();
            assert!(output[2].x() + output[2].width() <= width + 1e-10);
            for (bounds, item) in output.iter().zip(items) {
                assert!(
                    bounds.width() >= item.minimum - 1e-10
                        && bounds.width() <= item.maximum + 1e-10
                );
            }
            assert!(output[0].x() + output[0].width() <= output[1].x());
            assert!((0. ..=1.).contains(&tween.sample(step as u64)));
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
