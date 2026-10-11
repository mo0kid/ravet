#![cfg(feature = "text")]
use ravet_surface::{
    drawing::{Color, SvgDrawing},
    fallback::*,
    text::{TextDirection, TextError, TextFeature, TextFont, TextOptions},
};
fn fonts() -> (TextFont, TextFont) {
    (
        TextFont::from_bytes(
            include_bytes!("fixtures/fallback/RavetFallbackPrimary.ttf"),
            0,
        )
        .unwrap(),
        TextFont::from_bytes(
            include_bytes!("fixtures/fallback/RavetFallbackComplement.ttf"),
            0,
        )
        .unwrap(),
    )
}
fn options() -> TextOptions<'static> {
    TextOptions {
        size: 10.,
        direction: TextDirection::LeftToRight,
        script: Some(*b"Latn"),
        language: Some("en"),
        features: &[],
    }
}
#[test]
fn ordered_whole_selection_and_merged_ligature_keep_matching_faces() {
    let (a, b) = fonts();
    let stack = TextFontStack::new(&[&a, &b]).unwrap();
    let run = stack.shape("abcffi", options()).unwrap();
    assert_eq!(run.text(), "abcffi");
    assert_eq!(run.advance(), (30., 0.));
    assert_eq!(
        run.runs()
            .iter()
            .map(|r| (r.font_index(), r.range()))
            .collect::<Vec<_>>(),
        [(0, 0..1), (1, 1..3), (0, 3..6)]
    );
    assert_eq!(run.glyph_count(), 4);
    assert_eq!(run.runs()[2].shape().glyphs().len(), 1);
    assert_eq!(run.runs()[2].source_cluster(0), Some(3));
    assert_eq!(run.runs()[2].source_cluster(1), None);
    assert!(std::ptr::eq(run.runs()[1].font(), &b));
    assert_eq!(run.runs()[1].origin(), (5., 0.));
    assert_eq!(run.runs()[2].origin(), (20., 0.));
    assert_eq!(run.probes(), 8);
    assert!(run.missing_ranges().is_empty());
    let reversed = TextFontStack::new(&[&b, &a])
        .unwrap()
        .shape("abcffi", options())
        .unwrap();
    assert_eq!(reversed.advance(), (31., 0.));
    assert_eq!(reversed.runs()[0].range(), 0..3);
    assert_eq!(reversed.runs()[0].font_index(), 0);
}
#[test]
fn grapheme_marks_and_normalized_decomposition_fallback_whole() {
    let (a, b) = fonts();
    let stack = TextFontStack::new(&[&a, &b]).unwrap();
    for text in ["á", "a\u{301}"] {
        let run = stack.shape(text, options()).unwrap();
        assert_eq!(run.runs().len(), 1);
        assert_eq!(run.runs()[0].font_index(), 1);
        assert_eq!(run.runs()[0].range(), 0..text.len());
        assert_eq!(run.missing_glyphs(), 0);
        assert_eq!(run.advance(), (6., 0.));
        assert!(
            run.runs()[0]
                .shape()
                .glyphs()
                .iter()
                .all(|g| g.cluster == 0)
        );
    }
}
#[test]
fn ignorable_source_and_missing_emoji_grapheme_are_never_split() {
    let (a, b) = fonts();
    let stack = TextFontStack::new(&[&a, &b]).unwrap();
    let hidden = stack.shape("\u{200d}\u{fe0f}", options()).unwrap();
    assert_eq!(hidden.text(), "\u{200d}\u{fe0f}");
    assert_eq!(hidden.glyph_count(), 0);
    assert!(hidden.missing_ranges().is_empty());
    assert!(hidden.runs().iter().all(|r| r.font_index() == 0));
    let text = "👩\u{200d}💻";
    let missing = stack.shape(text, options()).unwrap();
    assert_eq!(missing.missing_ranges().len(), 1);
    assert_eq!(missing.missing_ranges()[0], 0..text.len());
    assert_eq!(missing.runs().len(), 1);
    assert_eq!(missing.runs()[0].font_index(), 0);
    assert!(missing.missing_glyphs() > 0);
}
#[test]
fn rtl_visual_units_and_byte_clusters_retain_logical_source() {
    let (a, b) = fonts();
    let stack = TextFontStack::new(&[&a, &b]).unwrap();
    let run = stack
        .shape(
            "abca",
            TextOptions {
                direction: TextDirection::RightToLeft,
                ..options()
            },
        )
        .unwrap();
    assert_eq!(run.text(), "abca");
    assert_eq!(
        run.runs().iter().map(|r| r.range()).collect::<Vec<_>>(),
        [3..4, 1..3, 0..1]
    );
    assert_eq!(
        run.runs()[1]
            .shape()
            .glyphs()
            .iter()
            .map(|g| g.cluster)
            .collect::<Vec<_>>(),
        [1, 0]
    );
    assert_eq!(run.runs()[1].source_cluster(0), Some(2));
    assert_eq!(run.runs()[1].source_cluster(1), Some(1));
    assert_eq!(run.runs()[0].origin(), (0., 0.));
    assert_eq!(run.runs()[1].origin(), (5., 0.));
    assert_eq!(run.advance(), (25., 0.));
}
#[test]
fn copied_style_and_font_borrows_outlive_temporary_stack_source() {
    let (a, b) = fonts();
    let mut features = vec![TextFeature {
        tag: *b"liga",
        enabled: false,
    }];
    let mut language = String::from("en");
    let run = {
        let stack = TextFontStack::new(&[&a, &b]).unwrap();
        stack
            .shape(
                "ffi",
                TextOptions {
                    features: &features,
                    language: Some(&language),
                    ..options()
                },
            )
            .unwrap()
    };
    features.clear();
    language.clear();
    assert_eq!(
        run.options().features,
        [TextFeature {
            tag: *b"liga",
            enabled: false
        }]
    );
    assert_eq!(run.options().language, Some("en"));
    assert_eq!(run.glyph_count(), 3);
    assert_eq!(run.advance(), (7., 0.));
}
#[test]
fn whole_shape_limits_and_empty_source_options_are_checked() {
    let (a, b) = fonts();
    let stack = TextFontStack::new(&[&a, &b]).unwrap();
    assert!(matches!(
        TextFontStack::new(&[]),
        Err(TextError::InvalidInput)
    ));
    assert!(TextFontStack::new(&[&a; 16]).is_ok());
    assert!(matches!(
        TextFontStack::new(&[&a; 17]),
        Err(TextError::Capacity)
    ));
    let empty = stack.shape("", options()).unwrap();
    assert_eq!(empty.glyph_count(), 0);
    assert_eq!(empty.advance(), (0., 0.));
    for invalid in [
        TextOptions {
            script: None,
            ..options()
        },
        TextOptions {
            size: f64::NAN,
            ..options()
        },
        TextOptions {
            size: 0.,
            ..options()
        },
        TextOptions {
            size: 1024.1,
            ..options()
        },
        TextOptions {
            language: Some("bad tag"),
            ..options()
        },
        TextOptions {
            script: Some(*b"??? "),
            ..options()
        },
    ] {
        assert!(matches!(
            stack.shape("", invalid),
            Err(TextError::InvalidInput)
        ));
    }
    for text in ["\n", "\0", "\u{fdd0}"] {
        assert!(matches!(
            stack.shape(text, options()),
            Err(TextError::InvalidInput)
        ));
    }
    assert!(stack.shape(&"a".repeat(4096), options()).is_ok());
    assert!(matches!(
        stack.shape(&"a".repeat(4097), options()),
        Err(TextError::Capacity)
    ));
    let features = [TextFeature {
        tag: *b"liga",
        enabled: true,
    }; 33];
    assert!(matches!(
        stack.shape(
            "",
            TextOptions {
                features: &features,
                ..options()
            }
        ),
        Err(TextError::Capacity)
    ));
}
#[test]
fn whole_font_run_and_probe_capacity_has_no_partial_publication() {
    let (a, b) = fonts();
    let stack = TextFontStack::new(&[&a, &b]).unwrap();
    assert_eq!(
        stack
            .shape(&"ab".repeat(128), options())
            .unwrap()
            .runs()
            .len(),
        256
    );
    assert!(matches!(
        stack.shape(&"ab".repeat(129), options()),
        Err(TextError::Capacity)
    ));
    let expensive = TextFontStack::new(&[&a; 16]).unwrap();
    assert_eq!(
        expensive
            .shape(&"☃".repeat(512), options())
            .unwrap()
            .probes(),
        8192
    );
    assert!(matches!(
        expensive.shape(&"☃".repeat(513), options()),
        Err(TextError::Capacity)
    ));
}
#[test]
fn matching_multi_font_svg_and_raster_outputs_have_independent_advances() {
    let (a, b) = fonts();
    let text = TextFontStack::new(&[&a, &b])
        .unwrap()
        .shape("abcffi", options())
        .unwrap();
    let mut svg = SvgDrawing::new(60., 25., "abcffi").unwrap();
    svg.fallback_text(&text, (5., 15.), Color::rgb(255, 0, 0))
        .unwrap();
    assert_eq!(svg.command_count(), 4);
    let output = svg.finish();
    assert!(output.contains("role=\"img\""));
    assert!(!output.contains("<text"));
    assert!(!output.contains("href"));
    #[cfg(feature = "raster")]
    {
        use ravet_surface::raster::{RasterCanvas, RasterText};
        let prepared = RasterText::from_fallback(&text, (5., 15.)).unwrap();
        let mut canvas = RasterCanvas::new(60, 25, 1).unwrap();
        canvas
            .draw_text(&prepared, Color::rgb(255, 0, 0), false)
            .unwrap();
        assert_eq!(canvas.operation_count(), 1);
        assert_eq!(prepared.missing_glyphs(), 0);
        // Exact primitive outlines: y8..15 and widths4.8,6.8,7.8,9.8 before pixel centres.
        for (x, filled) in [
            (5, true),
            (9, true),
            (10, true),
            (16, true),
            (17, true),
            (24, true),
            (25, true),
            (34, true),
            (35, false),
        ] {
            assert_eq!(canvas.pixels()[4 * (10 * 60 + x) + 3] > 0, filled, "x{x}");
        }
    }
}
#[test]
fn late_svg_capacity_restores_exact_bytes_and_command_count() {
    use ravet_surface::layout::Rect;
    let (a, b) = fonts();
    let text = TextFontStack::new(&[&a, &b])
        .unwrap()
        .shape("abcffi", options())
        .unwrap();
    let mut drawing = SvgDrawing::new(60., 25., "Original").unwrap();
    let mut expected = SvgDrawing::new(60., 25., "Original").unwrap();
    for _ in 0..4095 {
        for svg in [&mut drawing, &mut expected] {
            svg.rectangle(Rect::new(0., 0., 1., 1.).unwrap(), 0., Color::rgb(1, 2, 3))
                .unwrap();
        }
    }
    assert!(matches!(
        drawing.fallback_text(&text, (5., 15.), Color::rgb(255, 0, 0)),
        Err(TextError::Capacity)
    ));
    assert_eq!(drawing.command_count(), 4095);
    assert_eq!(drawing.finish(), expected.finish());
    #[cfg(feature = "raster")]
    {
        use ravet_surface::raster::RasterText;
        assert!(RasterText::from_fallback(&text, (65520., 15.)).is_err());
    }
}
