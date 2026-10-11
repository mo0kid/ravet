#![cfg(feature = "text")]
#[cfg(feature = "postscript-outlines")]
use ravet_surface::drawing::PathCommand;
use ravet_surface::text::*;
const CFF: &[u8] = include_bytes!("fixtures/postscript/analytic-cff.otf");
const CFF2: &[u8] = include_bytes!("fixtures/postscript/analytic-cff2.otf");
#[cfg(feature = "postscript-outlines")]
const SUBR: &[u8] = include_bytes!("fixtures/postscript/subr-cff.otf");
#[cfg(feature = "postscript-outlines")]
const MATRIX: &[u8] = include_bytes!("fixtures/postscript/matrix-cff.otf");
fn options() -> TextOptions<'static> {
    TextOptions {
        size: 1000.,
        direction: TextDirection::LeftToRight,
        script: Some(*b"Latn"),
        language: None,
        features: &[],
    }
}
#[test]
fn source_metadata_is_explicit_separate_from_opt_in_rendering() {
    let a = TextFont::from_bytes(CFF, 0).unwrap();
    let b = TextFont::from_bytes(CFF2, 0).unwrap();
    assert_eq!(a.outline_format(), Ok(TextOutlineFormat::Cff));
    assert_eq!(b.outline_format(), Ok(TextOutlineFormat::Cff2));
    assert_eq!(a.shape("A", options()).unwrap().glyphs()[0].id, 2);
    assert_eq!(a.shape("A", options()).unwrap().advance().0, 600.);
    #[cfg(not(feature = "postscript-outlines"))]
    {
        assert_eq!(a.outline_glyph(2), Err(TextError::UnsupportedFont));
        assert_eq!(b.outline_glyph(2), Err(TextError::UnsupportedFont));
    }
    assert_eq!(a.outline_glyph(4), Err(TextError::InvalidInput));
}
#[cfg(feature = "postscript-outlines")]
fn analytic(width: f64) -> Vec<PathCommand> {
    vec![
        PathCommand::MoveTo(50., 0.),
        PathCommand::CubicTo {
            first: (50., 100.),
            second: (50. + width, 200.),
            end: (50. + 2. * width, 200.),
        },
        PathCommand::CubicTo {
            first: (50. + 3. * width, 200.),
            second: (50. + 4. * width, 100.),
            end: (50. + 4. * width, 0.),
        },
        PathCommand::Close,
    ]
}
#[cfg(feature = "postscript-outlines")]
#[test]
fn independently_authored_cubic_paths_subroutines_empty_and_metrics() {
    for data in [CFF, SUBR] {
        let font = TextFont::from_bytes(data, 0).unwrap();
        assert_eq!(font.outline_glyph(2).unwrap(), analytic(100.));
        assert_eq!(font.outline_glyph(0).unwrap(), vec![]);
        assert_eq!(font.outline_glyph(1).unwrap(), vec![]);
        assert_eq!(font.shape("AB", options()).unwrap().advance().0, 1200.);
    }
}
#[cfg(feature = "postscript-outlines")]
#[test]
fn actual_selected_cff2_normalized_avar_coordinates_drive_geometry_and_advances() {
    let base = TextFont::from_bytes(CFF2, 0).unwrap();
    for user in [100., 400., 650., 900.] {
        let selected = base
            .with_variations(&[TextVariation {
                tag: *b"wght",
                value: user,
            }])
            .unwrap();
        let coordinate = selected.variation_coordinates().unwrap()[0];
        let n = f64::from(coordinate.normalized);
        if user == 650. {
            assert!((n - 0.8).abs() < 1. / 16384.);
        }
        let width = 100. + if n < 0. { 40. * n } else { 80. * n };
        let actual = selected.outline_glyph(2).unwrap();
        // Independently authored source points use exact selected normalized
        // coordinates. The explicit backend unscaled geometry policy floors
        // every coordinate to integer font units before matrix transformation.
        let expected = vec![
            PathCommand::MoveTo(50., 0.),
            PathCommand::CubicTo {
                first: (50., 100.),
                second: ((50. + width).floor(), 200.),
                end: ((50. + 2. * width).floor(), 200.),
            },
            PathCommand::CubicTo {
                first: ((50. + 3. * width).floor(), 200.),
                second: ((50. + 4. * width).floor(), 100.),
                end: ((50. + 4. * width).floor(), 0.),
            },
            PathCommand::Close,
        ];
        assert_eq!(
            actual, expected,
            "user {user}, actual normalized coordinate {n}"
        );
        assert!(
            (selected.shape("A", options()).unwrap().advance().0 - (600. + 4. * (width - 100.)))
                .abs()
                <= 1.
        );
    }
    assert_eq!(base.outline_glyph(2).unwrap(), analytic(100.));
    assert!(
        base.with_variations(&[TextVariation {
            tag: *b"wght",
            value: 901.
        }])
        .is_err()
    );
}
#[cfg(feature = "postscript-outlines")]
#[test]
fn explicit_nondefault_font_matrix_is_not_silently_ignored() {
    let font = TextFont::from_bytes(MATRIX, 0).unwrap();
    let expected = vec![
        PathCommand::MoveTo(50., 0.),
        PathCommand::CubicTo {
            first: (75., 100.),
            second: (200., 200.),
            end: (300., 200.),
        },
        PathCommand::CubicTo {
            first: (400., 200.),
            second: (475., 100.),
            end: (450., 0.),
        },
        PathCommand::Close,
    ];
    assert_eq!(font.outline_glyph(2).unwrap(), expected);
}

#[cfg(feature = "postscript-outlines")]
#[test]
fn corrected_tiny_contours_preserve_commands_and_explicit_signed_quantization() {
    for (data, coordinate) in [
        (
            include_bytes!("fixtures/postscript/tiny-positive-cff.otf").as_slice(),
            0.,
        ),
        (
            include_bytes!("fixtures/postscript/tiny-negative-cff.otf").as_slice(),
            -1.,
        ),
    ] {
        let font = TextFont::from_bytes(data, 0).unwrap();
        // The authored cubic controls/end are (+/-.1,+/-.2),
        // (+/-.3,+/-.5), (+/-.6,+/-.6). Signed arithmetic-shift
        // quantization floors negative fractions; it never truncates to zero.
        assert_eq!(
            font.outline_glyph(2).unwrap(),
            vec![
                PathCommand::MoveTo(0., 0.),
                PathCommand::CubicTo {
                    first: (coordinate, coordinate),
                    second: (coordinate, coordinate),
                    end: (coordinate, coordinate)
                },
                PathCommand::Close
            ]
        );
        assert_eq!(font.shape("A", options()).unwrap().advance(), (600., 0.));
    }
}

#[cfg(feature = "postscript-outlines")]
#[test]
fn cid_fdselect_actual_sources_and_exact_font_dict_limit_are_checked() {
    for bytes in [
        include_bytes!("fixtures/postscript/cid-2-cff.otf").as_slice(),
        include_bytes!("fixtures/postscript/cid-256-cff.otf").as_slice(),
    ] {
        let f = TextFont::from_bytes(bytes, 0).unwrap();
        assert_eq!(f.outline_format(), Ok(TextOutlineFormat::Cff));
        assert_eq!(f.outline_glyph(2).unwrap(), analytic(100.));
        assert_eq!(f.shape("A", options()).unwrap().advance(), (600., 0.));
    }
    let f = TextFont::from_bytes(include_bytes!("fixtures/postscript/cid-257-cff.otf"), 0).unwrap();
    assert_eq!(f.outline_format(), Err(TextError::Capacity));
    assert_eq!(f.outline_glyph(2), Err(TextError::Capacity));
}
#[cfg(feature = "postscript-outlines")]
#[test]
fn actual_output_limit_preserves_existing_svg_on_late_capacity() {
    use ravet_surface::drawing::{Color, SvgDrawing};
    let accepted = TextFont::from_bytes(
        include_bytes!("fixtures/postscript/commands-4096-cff.otf"),
        0,
    )
    .unwrap();
    let path = accepted.outline_glyph(2).unwrap();
    assert_eq!(path.len(), 4096);
    assert_eq!(path[4094], PathCommand::LineTo(4094., 4094.));
    assert_eq!(path[4095], PathCommand::Close);
    let rejected = TextFont::from_bytes(
        include_bytes!("fixtures/postscript/commands-4097-cff.otf"),
        0,
    )
    .unwrap();
    assert_eq!(rejected.outline_glyph(2), Err(TextError::Capacity));
    let mut actual = SvgDrawing::new(100., 100., "Before").unwrap();
    let before = SvgDrawing::new(100., 100., "Before").unwrap();
    assert_eq!(
        actual.shaped_text(
            &rejected,
            "A",
            TextOptions {
                size: 10.,
                ..options()
            },
            (1., 20.),
            Color::rgb(0, 0, 0)
        ),
        Err(TextError::Capacity)
    );
    assert_eq!(actual.command_count(), 0);
    assert_eq!(actual.finish(), before.finish());
}

#[test]
fn malformed_whole_fdselect_coverage_never_defaults_to_first_font_dict() {
    for data in [
        include_bytes!("fixtures/postscript/invalid-select-first-gap.otf").as_slice(),
        include_bytes!("fixtures/postscript/invalid-select-bad-fd.otf").as_slice(),
        include_bytes!("fixtures/postscript/invalid-select-bad-sentinel.otf").as_slice(),
        include_bytes!("fixtures/postscript/invalid-select-duplicate-first.otf").as_slice(),
    ] {
        let font = TextFont::from_bytes(data, 0).unwrap();
        assert_eq!(font.outline_format(), Err(TextError::UnsupportedFont));
        #[cfg(feature = "postscript-outlines")]
        assert_eq!(font.outline_glyph(2), Err(TextError::UnsupportedFont));
    }
}
#[test]
fn cff2_uniform_source_matrix_uses_exact_decimal_policy_not_backend_projection() {
    for data in [
        include_bytes!("fixtures/postscript/cff2-matrix-valid.otf").as_slice(),
        include_bytes!("fixtures/postscript/cff2-matrix-decimal-near.otf").as_slice(),
    ] {
        assert_eq!(
            TextFont::from_bytes(data, 0).unwrap().outline_format(),
            Ok(TextOutlineFormat::Cff2)
        );
    }
    for data in [
        include_bytes!("fixtures/postscript/cff2-matrix-decimal-far.otf").as_slice(),
        include_bytes!("fixtures/postscript/cff2-matrix-shear.otf").as_slice(),
        include_bytes!("fixtures/postscript/cff2-matrix-translation.otf").as_slice(),
        include_bytes!("fixtures/postscript/cff2-matrix-unequal.otf").as_slice(),
    ] {
        assert_eq!(
            TextFont::from_bytes(data, 0).unwrap().outline_format(),
            Err(TextError::UnsupportedFont)
        );
    }
}

#[cfg(feature = "postscript-outlines")]
#[test]
fn private_metadata_validates_actual_blends_and_refuses_bad_hint_shapes() {
    let base = TextFont::from_bytes(
        include_bytes!("fixtures/postscript/private-blend-cff2.otf"),
        0,
    )
    .unwrap();
    for user in [100., 400., 650., 900.] {
        let selected = base
            .with_variations(&[TextVariation {
                tag: *b"wght",
                value: user,
            }])
            .unwrap();
        assert_eq!(selected.outline_format(), Ok(TextOutlineFormat::Cff2));
        let n = f64::from(selected.variation_coordinates().unwrap()[0].normalized);
        let width = 100. + if n < 0. { 40. * n } else { 80. * n };
        assert_eq!(
            selected.outline_glyph(2).unwrap()[1],
            PathCommand::CubicTo {
                first: (50., 100.),
                second: ((50. + width).floor(), 200.),
                end: ((50. + 2. * width).floor(), 200.)
            }
        );
    }
    let invalid = TextFont::from_bytes(
        include_bytes!("fixtures/postscript/invalid-private-bluevalues-cff.otf"),
        0,
    )
    .unwrap();
    assert_eq!(invalid.outline_format(), Err(TextError::UnsupportedFont));
    assert_eq!(invalid.outline_glyph(2), Err(TextError::UnsupportedFont));
}

#[cfg(feature = "postscript-outlines")]
#[test]
fn signed_floor_precedes_normalized_matrix_fixed_point_multiplication() {
    let font = TextFont::from_bytes(
        include_bytes!("fixtures/postscript/tiny-negative-matrix-cff.otf"),
        0,
    )
    .unwrap();
    // Each authored negative fractional coordinate first floors to −1.
    // The normalized 0.75 shear then multiplies these integer font units using
    // backend fixed-point rounded products: −1 + round(−0.75) = −2.
    // Applying the source affine transform before the first floor would differ.
    assert_eq!(
        font.outline_glyph(2).unwrap(),
        vec![
            PathCommand::MoveTo(0., 0.),
            PathCommand::CubicTo {
                first: (-2., -1.),
                second: (-2., -1.),
                end: (-2., -1.)
            },
            PathCommand::Close
        ]
    );
}

#[cfg(feature = "postscript-outlines")]
#[test]
fn late_second_face_overflow_restores_svg_and_refuses_whole_cpu_preparation() {
    use ravet_surface::{
        drawing::{Color, SvgDrawing},
        fallback::TextFontStack,
        layout::Rect,
    };
    let first = TextFont::from_bytes(
        include_bytes!("fixtures/postscript/fallback-only-b-cff.otf"),
        0,
    )
    .unwrap();
    let second = TextFont::from_bytes(
        include_bytes!("fixtures/postscript/commands-4097-cff.otf"),
        0,
    )
    .unwrap();
    let text = TextFontStack::new(&[&first, &second])
        .unwrap()
        .shape(
            "BA",
            TextOptions {
                size: 10.,
                ..options()
            },
        )
        .unwrap();
    assert_eq!(
        text.runs()
            .iter()
            .map(|r| r.font_index())
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    let mut actual = SvgDrawing::new(100., 100., "Existing drawing").unwrap();
    let mut expected = SvgDrawing::new(100., 100., "Existing drawing").unwrap();
    for s in [&mut actual, &mut expected] {
        s.rectangle(
            Rect::new(1., 2., 3., 4.).unwrap(),
            0.,
            Color::rgb(12, 34, 56),
        )
        .unwrap();
    }
    assert_eq!(
        actual.fallback_text(&text, (1., 20.), Color::rgb(0, 0, 0)),
        Err(TextError::Capacity)
    );
    assert_eq!(actual.command_count(), 1);
    assert_eq!(actual.finish(), expected.finish());
    #[cfg(feature = "raster")]
    {
        use ravet_surface::raster::{RasterCanvas, RasterError, RasterText};
        let mut canvas = RasterCanvas::new(100, 100, 10).unwrap();
        canvas.clear(Color::rgb(12, 34, 56));
        let pixels = canvas.pixels().to_vec();
        let counts = (canvas.operation_count(), canvas.segment_count());
        let result = RasterText::from_fallback(&text, (1., 20.));
        assert!(matches!(
            result,
            Err(RasterError::Text(TextError::Capacity))
        ));
        assert_eq!(canvas.pixels(), pixels);
        assert_eq!((canvas.operation_count(), canvas.segment_count()), counts);
    }
}

#[test]
fn ambiguous_missing_and_present_glyf_profiles_have_explicit_precedence() {
    for bytes in [
        include_bytes!("fixtures/postscript/ambiguous-cff-and-cff2.otf").as_slice(),
        include_bytes!("fixtures/postscript/no-outlines.otf").as_slice(),
    ] {
        let font = TextFont::from_bytes(bytes, 0).unwrap();
        assert_eq!(font.outline_format(), Err(TextError::UnsupportedFont));
        assert_eq!(font.outline_glyph(2), Err(TextError::UnsupportedFont));
    }
    let base =
        TextFont::from_bytes(include_bytes!("fixtures/variation/RavetVariation.ttf"), 0).unwrap();
    let hybrid = TextFont::from_bytes(
        include_bytes!("fixtures/postscript/glyf-with-invalid-cff.ttf"),
        0,
    )
    .unwrap();
    assert_eq!(hybrid.outline_format(), Ok(TextOutlineFormat::TrueType));
    let run = base.shape("a", options()).unwrap();
    assert_eq!(run, hybrid.shape("a", options()).unwrap());
    for glyph in run.glyphs() {
        assert_eq!(
            base.outline_glyph(glyph.id).unwrap(),
            hybrid.outline_glyph(glyph.id).unwrap()
        );
    }
}
