#![cfg(feature = "raster")]
use ravet_surface::{
    drawing::{Color, PathCommand as P},
    raster::*,
};
fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<P> {
    vec![
        P::MoveTo(x, y),
        P::LineTo(x + w, y),
        P::LineTo(x + w, y + h),
        P::LineTo(x, y + h),
        P::Close,
    ]
}
fn path(commands: &[P]) -> RasterPath {
    RasterPath::new(commands, RasterTransform::default()).unwrap()
}
fn pixel(c: &RasterCanvas, x: usize, y: usize) -> [u8; 4] {
    c.pixels()[4 * (y * c.width() as usize + x)..][..4]
        .try_into()
        .unwrap()
}
fn state(c: &RasterCanvas) -> (Vec<u8>, usize, usize, u64) {
    (
        c.pixels().to_vec(),
        c.operation_count(),
        c.segment_count(),
        c.covered_pixel_work(),
    )
}
#[test]
fn clipped_integer_rectangles_match_independent_source_over_and_owned_readbacks() {
    let mut c = RasterCanvas::new(13, 9, 8).unwrap();
    c.clear(Color {
        red: 20,
        green: 40,
        blue: 60,
        alpha: 128,
    });
    c.fill_path(
        &path(&rect(-2., 2., 9., 4.)),
        Color {
            red: 210,
            green: 50,
            blue: 110,
            alpha: 128,
        },
        RasterFillRule::NonZero,
        false,
    )
    .unwrap();
    for y in 0..9 {
        for x in 0..13 {
            let mut expected = [
                20. * 128. / 255.,
                40. * 128. / 255.,
                60. * 128. / 255.,
                128.,
            ];
            if x < 7 && (2..6).contains(&y) {
                for channel in 0..3 {
                    expected[channel] = [210., 50., 110.][channel] * 128. / 255.
                        + expected[channel] * (127. / 255.);
                }
                expected[3] = 128. + 128. * 127. / 255.;
            }
            let actual = pixel(&c, x, y);
            for channel in 0..4 {
                assert!(
                    (actual[channel] as f64 - expected[channel]).abs() <= 1.5,
                    "{x}/{y}: {actual:?} vs {expected:?}"
                );
            }
        }
    }
    let owned = c.snapshot();
    let before = owned.pixels.clone();
    c.clear(Color::rgb(1, 2, 3));
    assert_eq!(owned.pixels, before);
    let transferred = c.into_image();
    assert_eq!(transferred.width, 13);
    assert_eq!(transferred.height, 9);
    assert_eq!(transferred.pixels.len(), 13 * 9 * 4);
}
#[test]
fn winding_holes_affine_geometry_and_antialiasing_are_explicit() {
    let mut commands = rect(1., 1., 10., 10.);
    commands.extend(rect(4., 4., 4., 4.));
    let p = path(&commands);
    let mut c = RasterCanvas::new(13, 13, 8).unwrap();
    c.fill_path(
        &p,
        Color::rgb(255, 255, 255),
        RasterFillRule::NonZero,
        false,
    )
    .unwrap();
    assert_eq!(pixel(&c, 5, 5)[3], 255);
    c.clear(Color {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    });
    c.fill_path(
        &p,
        Color::rgb(255, 255, 255),
        RasterFillRule::EvenOdd,
        false,
    )
    .unwrap();
    assert_eq!(pixel(&c, 5, 5)[3], 0);
    assert_eq!(pixel(&c, 2, 2)[3], 255);
    let p = RasterPath::new(
        &rect(0., 0., 2., 3.),
        RasterTransform {
            scale_x: 2.,
            scale_y: 1.,
            translate_x: 3.,
            translate_y: 2.,
            ..Default::default()
        },
    )
    .unwrap();
    c.clear(Color {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    });
    c.fill_path(&p, Color::rgb(255, 0, 0), RasterFillRule::NonZero, false)
        .unwrap();
    for y in 0..13 {
        for x in 0..13 {
            assert_eq!(
                pixel(&c, x, y)[3],
                if (3..7).contains(&x) && (2..5).contains(&y) {
                    255
                } else {
                    0
                }
            );
        }
    }
    let triangle = path(&[
        P::MoveTo(1.25, 1.25),
        P::LineTo(10.75, 3.25),
        P::LineTo(2.25, 11.75),
        P::Close,
    ]);
    c.clear(Color {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    });
    c.fill_path(
        &triangle,
        Color::rgb(80, 160, 240),
        RasterFillRule::NonZero,
        true,
    )
    .unwrap();
    assert!(
        c.pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] > 0 && p[3] < 255)
    );
    for p in c.pixels().as_chunks::<4>().0 {
        assert!(p[..3].iter().all(|v| *v <= p[3]));
    }
}
#[test]
fn curves_and_device_pixel_stroke_caps_render() {
    let mut c = RasterCanvas::new(32, 24, 8).unwrap();
    let line = path(&[P::MoveTo(8., 12.), P::LineTo(24., 12.)]);
    c.stroke_path(
        &line,
        Color::rgb(255, 255, 255),
        RasterStroke {
            width: 6.,
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_eq!(pixel(&c, 6, 12)[3], 0);
    assert_eq!(pixel(&c, 8, 12)[3], 255);
    assert_eq!(pixel(&c, 24, 12)[3], 0);
    c.clear(Color {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    });
    c.stroke_path(
        &line,
        Color::rgb(255, 255, 255),
        RasterStroke {
            width: 6.,
            cap: RasterLineCap::Round,
            ..Default::default()
        },
        true,
    )
    .unwrap();
    assert_eq!(pixel(&c, 6, 12)[3], 255);
    assert_eq!(pixel(&c, 25, 12)[3], 255);
    let curve = path(&[
        P::MoveTo(2., 20.),
        P::QuadraticTo {
            control: (3., 2.),
            end: (16., 3.),
        },
        P::CubicTo {
            first: (28., 3.),
            second: (30., 18.),
            end: (20., 20.),
        },
        P::Close,
    ]);
    c.clear(Color {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    });
    c.fill_path(
        &curve,
        Color::rgb(255, 100, 50),
        RasterFillRule::NonZero,
        true,
    )
    .unwrap();
    assert!(pixel(&c, 16, 12)[3] > 200);
    assert_eq!(pixel(&c, 0, 0)[3], 0);
}
#[test]
fn invalid_geometry_styles_and_raw_operation_budget_preserve_pixels() {
    assert!(matches!(
        RasterCanvas::new(0, 2, 1),
        Err(RasterError::InvalidInput)
    ));
    assert!(matches!(
        RasterCanvas::new(2, 2, 4097),
        Err(RasterError::Capacity)
    ));
    for commands in [
        vec![P::LineTo(1., 1.)],
        vec![P::MoveTo(0., 0.), P::Close, P::LineTo(1., 1.)],
        vec![P::MoveTo(0., 0.), P::LineTo(f64::NAN, 1.)],
        rect(65536., 0., 1., 1.),
    ] {
        assert!(matches!(
            RasterPath::new(&commands, RasterTransform::default()),
            Err(RasterError::InvalidInput)
        ));
    }
    assert!(matches!(
        RasterPath::new(&vec![P::MoveTo(0., 0.); 4097], RasterTransform::default()),
        Err(RasterError::Capacity)
    ));
    let p = path(&rect(1., 1., 4., 4.));
    let mut c = RasterCanvas::new(8, 8, 1).unwrap();
    let before = state(&c);
    for style in [
        RasterStroke {
            width: 0.,
            ..Default::default()
        },
        RasterStroke {
            width: f64::NAN,
            ..Default::default()
        },
        RasterStroke {
            miter_limit: 17.,
            ..Default::default()
        },
    ] {
        assert_eq!(
            c.stroke_path(&p, Color::rgb(1, 2, 3), style, true),
            Err(RasterError::InvalidInput)
        );
        assert_eq!(state(&c), before);
    }
    c.fill_path(&p, Color::rgb(255, 0, 0), RasterFillRule::NonZero, false)
        .unwrap();
    let before = state(&c);
    assert_eq!(
        c.fill_path(&p, Color::rgb(0, 255, 0), RasterFillRule::NonZero, false),
        Err(RasterError::Capacity)
    );
    assert_eq!(state(&c), before);
    c.clear(Color::rgb(0, 0, 0));
    assert_eq!(c.operation_count(), 0);
    c.fill_path(&p, Color::rgb(255, 0, 0), RasterFillRule::NonZero, false)
        .unwrap();
}
#[test]
fn cumulative_segment_and_full_canvas_work_limits_reject_atomically() {
    let single = path(&[P::MoveTo(0., 0.)]);
    let mut tiny = RasterCanvas::new(1, 1, 4096).unwrap();
    for _ in 0..4096 {
        tiny.fill_path(&single, Color::rgb(1, 2, 3), RasterFillRule::NonZero, false)
            .unwrap();
    }
    assert_eq!(tiny.operation_count(), 4096);
    let before = state(&tiny);
    assert_eq!(
        tiny.fill_path(&single, Color::rgb(1, 2, 3), RasterFillRule::NonZero, false),
        Err(RasterError::Capacity)
    );
    assert_eq!(state(&tiny), before);
    let p = path(&vec![P::MoveTo(0., 0.); 4096]);
    let mut c = RasterCanvas::new(1, 1, 17).unwrap();
    for _ in 0..16 {
        c.fill_path(&p, Color::rgb(1, 2, 3), RasterFillRule::NonZero, false)
            .unwrap();
    }
    assert_eq!(c.segment_count(), 65536);
    let before = state(&c);
    assert_eq!(
        c.fill_path(&p, Color::rgb(9, 9, 9), RasterFillRule::NonZero, false),
        Err(RasterError::Capacity)
    );
    assert_eq!(state(&c), before);
    let mut c = RasterCanvas::new(4096, 4096, 4).unwrap();
    let p = path(&rect(0., 0., 4096., 4096.));
    for _ in 0..3 {
        c.fill_path(&p, Color::rgb(1, 2, 3), RasterFillRule::NonZero, false)
            .unwrap();
    }
    assert_eq!(c.covered_pixel_work(), 67108864);
    let before = state(&c);
    assert_eq!(
        c.fill_path(&p, Color::rgb(4, 5, 6), RasterFillRule::NonZero, false),
        Err(RasterError::Capacity)
    );
    assert_eq!(state(&c), before);
}
#[cfg(feature = "text")]
#[test]
fn prepared_font_glyphs_match_synthetic_geometry_and_whole_run_capacity() {
    use ravet_surface::text::{TextDirection, TextFont, TextOptions};
    let bytes = include_bytes!("fixtures/caret/RavetSyntheticLigature.ttf");
    let font = TextFont::from_bytes(bytes, 0).unwrap();
    let options = TextOptions {
        size: 100.,
        direction: TextDirection::LeftToRight,
        script: None,
        language: None,
        features: &[],
    };
    let run = font.shape("ffi", options).unwrap();
    assert_eq!(run.glyphs().len(), 1);
    let prepared = RasterText::from_shaped(&font, &run, 100., (3., 90.)).unwrap();
    assert_eq!(prepared.missing_glyphs(), 0);
    drop(run);
    drop(font);
    let mut c = RasterCanvas::new(128, 100, 1).unwrap();
    c.draw_text(&prepared, Color::rgb(30, 90, 180), false)
        .unwrap();
    for y in 0..100 {
        for x in 0..128 {
            assert_eq!(
                pixel(&c, x, y),
                // Independent fontTools GlyphSet bounds are (0,0,970,700):
                // hmtx left bearing0 shifts raw glyf x10..980 by-10.
                if (3..100).contains(&x) && (20..90).contains(&y) {
                    [30, 90, 180, 255]
                } else {
                    [0; 4]
                },
                "{x}/{y}"
            );
        }
    }
    let before = state(&c);
    assert_eq!(
        c.draw_text(&prepared, Color::rgb(1, 2, 3), true),
        Err(RasterError::Capacity)
    );
    assert_eq!(state(&c), before);
    let font = TextFont::from_bytes(bytes, 0).unwrap();
    let run = font.shape(" ", options).unwrap();
    let empty = RasterText::from_shaped(&font, &run, 100., (0., 90.)).unwrap();
    assert_eq!(empty.segment_count(), 0);
    c.clear(Color::rgb(0, 0, 0));
    let before = c.pixels().to_vec();
    c.draw_text(&empty, Color::rgb(255, 0, 0), true).unwrap();
    assert_eq!(c.pixels(), before);
    assert_eq!(c.operation_count(), 1);
    assert!(matches!(
        RasterText::from_shaped(&font, &run, f64::NAN, (0., 0.)),
        Err(RasterError::InvalidInput)
    ));
    let many = font.shape(&"f".repeat(16), options).unwrap();
    let many = RasterText::from_shaped(&font, &many, 100., (0., 90.)).unwrap();
    let mut c = RasterCanvas::new(8, 8, 2).unwrap();
    let p = path(&vec![P::MoveTo(0., 0.); 4096]);
    // Account for nearly all segments without pixels, then reject a complete text
    // run before any of its glyphs can alter the canvas.
    let mut c2 = RasterCanvas::new(8, 8, 18).unwrap();
    for _ in 0..16 {
        c2.fill_path(&p, Color::rgb(0, 0, 0), RasterFillRule::NonZero, false)
            .unwrap();
    }
    let before = state(&c2);
    assert_eq!(
        c2.draw_text(&many, Color::rgb(255, 0, 0), false),
        Err(RasterError::Capacity)
    );
    assert_eq!(state(&c2), before);
    c.clear(Color::rgb(0, 0, 0));
    assert_eq!(c.operation_count(), 0);
}
#[cfg(feature = "text")]
#[test]
fn local_arabic_outlines_have_antialiased_premultiplied_ink() {
    use ravet_surface::text::{TextDirection, TextFont, TextOptions};
    let font = TextFont::from_bytes(include_bytes!("fixtures/text/NotoSansArabic.ttf"), 0).unwrap();
    let run = font
        .shape(
            "مرحبا",
            TextOptions {
                size: 32.,
                direction: TextDirection::RightToLeft,
                script: None,
                language: None,
                features: &[],
            },
        )
        .unwrap();
    assert_eq!(run.missing_glyphs(), 0);
    let text = RasterText::from_shaped(&font, &run, 32., (4., 48.)).unwrap();
    let mut c = RasterCanvas::new(128, 64, 1).unwrap();
    c.draw_text(
        &text,
        Color {
            red: 200,
            green: 90,
            blue: 40,
            alpha: 180,
        },
        true,
    )
    .unwrap();
    assert!(c.pixels().as_chunks::<4>().0.iter().any(|p| p[3] > 0));
    assert!(
        c.pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] > 0 && p[3] < 180)
    );
    for p in c.pixels().as_chunks::<4>().0 {
        assert!(p[..3].iter().all(|v| *v <= p[3]));
    }
}
