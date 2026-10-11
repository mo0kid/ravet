#![cfg(feature = "text")]
use ravet_surface::{
    fallback::TextFontStack,
    fallback_paragraph::*,
    paragraph::{ParagraphOptions, TextAlignment},
    text::{TextDirection, TextError, TextFont},
};
fn fonts() -> [TextFont; 3] {
    [
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
        TextFont::from_bytes(include_bytes!("fixtures/text/NotoSansArabic.ttf"), 0).unwrap(),
    ]
}
fn style(spans: &[ParagraphScriptSpan]) -> FallbackParagraphTextOptions<'_> {
    FallbackParagraphTextOptions {
        size: 10.,
        base_direction: TextDirection::LeftToRight,
        language: None,
        features: &[],
        scripts: spans,
    }
}
fn settings(width: f64) -> ParagraphOptions {
    ParagraphOptions {
        width,
        line_height: 12.,
        alignment: TextAlignment::Left,
    }
}
fn span(text: &str) -> Vec<ParagraphScriptSpan> {
    if text.is_empty() {
        vec![]
    } else {
        vec![ParagraphScriptSpan {
            range: 0..text.len(),
            script: *b"Latn",
        }]
    }
}
#[test]
fn analytical_wrapping_exact_whitespace_alignment_and_original_clusters() {
    let f = fonts();
    let stack = TextFontStack::new(&[&f[0], &f[1]]).unwrap();
    let text = "ab ab ab";
    let spans = span(text);
    for alignment in [
        TextAlignment::Left,
        TextAlignment::Center,
        TextAlignment::Right,
    ] {
        let l = FallbackParagraphLayout::new(
            &stack,
            text,
            style(&spans),
            ParagraphOptions {
                alignment,
                ..settings(14.5)
            },
        )
        .unwrap();
        assert_eq!(
            l.lines().iter().map(|r| r.range()).collect::<Vec<_>>(),
            [0..3, 3..6, 6..8]
        );
        assert_eq!(
            l.lines().iter().map(|r| r.width()).collect::<Vec<_>>(),
            [14.5, 14.5, 12.]
        );
        assert_eq!(l.height(), 36.);
        assert_eq!(l.glyph_count(), 8);
        for (index, line) in l.lines().iter().enumerate() {
            let room = 14.5 - line.width();
            let x = match alignment {
                TextAlignment::Left => 0.,
                TextAlignment::Center => room / 2.,
                TextAlignment::Right => room,
            };
            assert_eq!(line.baseline(), (x, index as f64 * 12.));
            assert!(!line.overflows());
            for run in line.runs() {
                assert_eq!(run.context_range(), 0..text.len());
                assert!(std::ptr::eq(run.font(), &f[run.font_index()]));
                for (i, g) in run.shape().glyphs().iter().enumerate() {
                    assert_eq!(
                        run.source_cluster(i),
                        Some(run.range().start + g.cluster as usize)
                    );
                }
                assert_eq!(run.source_cluster(999), None);
            }
        }
        // Five measured candidates (including discarded overflows), only eight units probed once.
        assert_eq!(l.work().candidates, 5);
        assert_eq!(l.work().probes, 11);
    }
    let l = FallbackParagraphLayout::new(&stack, text, style(&spans), settings(14.499)).unwrap();
    assert!(l.lines()[0].overflows());
    assert_eq!(l.lines()[0].range(), 0..3);
}
#[test]
fn hard_separators_empty_trailing_and_complete_source_coverage() {
    let f = fonts();
    let stack = TextFontStack::new(&[&f[0], &f[1]]).unwrap();
    let text = "ab\r\n\na\rb\u{2028}a\u{2029}";
    let spans = span(text);
    let l = FallbackParagraphLayout::new(&stack, text, style(&spans), settings(80.)).unwrap();
    assert_eq!(
        l.lines()
            .iter()
            .map(|r| &text[r.range()])
            .collect::<Vec<_>>(),
        ["ab", "", "a", "b", "a", ""]
    );
    assert_eq!(
        l.lines()
            .iter()
            .map(|r| &text[r.separator()])
            .collect::<Vec<_>>(),
        ["\r\n", "\n", "\r", "\u{2028}", "\u{2029}", ""]
    );
    let mut end = 0;
    for line in l.lines() {
        assert_eq!(line.range().start, end);
        assert_eq!(line.separator().start, line.range().end);
        end = line.separator().end;
    }
    assert_eq!(end, text.len());
    let empty = FallbackParagraphLayout::new(&stack, "", style(&[]), settings(80.)).unwrap();
    assert_eq!(empty.lines().len(), 1);
    assert_eq!(
        empty.work(),
        ParagraphWork {
            shapes: 1,
            candidates: 1,
            probes: 0
        }
    );
    assert_eq!(empty.glyph_count(), 0);
    assert!(matches!(
        FallbackParagraphLayout::new(
            &stack,
            "",
            FallbackParagraphTextOptions {
                size: f64::NAN,
                ..style(&[])
            },
            settings(80.)
        ),
        Err(TextError::InvalidInput)
    ));
}
#[test]
fn fixed_mixed_bidi_order_and_script_piece_order_are_independent_vectors() {
    let f = fonts();
    let stack = TextFontStack::new(&[&f[0], &f[1], &f[2]]).unwrap();
    let text = "a ب c";
    let spans = [
        ParagraphScriptSpan {
            range: 0..2,
            script: *b"Latn",
        },
        ParagraphScriptSpan {
            range: 2..4,
            script: *b"Arab",
        },
        ParagraphScriptSpan {
            range: 4..6,
            script: *b"Latn",
        },
    ];
    for (direction, expected) in [
        (TextDirection::LeftToRight, vec![0..2, 2..4, 4..5, 5..6]),
        (
            TextDirection::RightToLeft,
            vec![5..6, 4..5, 2..4, 1..2, 0..1],
        ),
    ] {
        let l = FallbackParagraphLayout::new(
            &stack,
            text,
            FallbackParagraphTextOptions {
                base_direction: direction,
                ..style(&spans)
            },
            settings(100.),
        )
        .unwrap();
        assert_eq!(
            l.lines()[0]
                .runs()
                .iter()
                .map(|r| r.range())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(l.text(), text);
        assert_eq!(l.scripts(), spans);
        assert_eq!(l.missing_glyphs(), 0);
    }
    // In a single RTL range caller script pieces are reversed as whole logical pieces.
    let text = "بت";
    let spans = [
        ParagraphScriptSpan {
            range: 0..2,
            script: *b"Arab",
        },
        ParagraphScriptSpan {
            range: 2..4,
            script: *b"Latn",
        },
    ];
    let l = FallbackParagraphLayout::new(&stack, text, style(&spans), settings(100.)).unwrap();
    assert_eq!(
        l.lines()[0]
            .runs()
            .iter()
            .map(|r| (r.range(), r.script()))
            .collect::<Vec<_>>(),
        [(2..4, *b"Latn"), (0..2, *b"Arab")]
    );
}
#[test]
fn contextual_arabic_fallback_and_grapheme_missing_policy_keep_full_source() {
    let f = fonts();
    let stack = TextFontStack::new(&[&f[0], &f[1], &f[2]]).unwrap();
    let text = "تبت";
    let spans = [ParagraphScriptSpan {
        range: 0..6,
        script: *b"Arab",
    }];
    let l = FallbackParagraphLayout::new(&stack, text, style(&spans), settings(100.)).unwrap();
    assert_eq!(
        l.lines()[0]
            .runs()
            .iter()
            .map(|r| (r.range(), r.font_index()))
            .collect::<Vec<_>>(),
        [(4..6, 2), (2..4, 0), (0..2, 2)]
    );
    assert!(
        l.lines()[0]
            .runs()
            .iter()
            .all(|r| r.context_range() == (0..6))
    );
    assert_eq!(l.missing_glyphs(), 0);
    let text = "a\u{301} 👩\u{200d}💻";
    let spans = span(text);
    let l = FallbackParagraphLayout::new(
        &TextFontStack::new(&[&f[0], &f[1]]).unwrap(),
        text,
        style(&spans),
        settings(100.),
    )
    .unwrap();
    assert_eq!(l.lines()[0].runs()[0].font_index(), 1);
    assert_eq!(l.lines()[0].runs()[0].range(), 0..3);
    assert_eq!(l.missing_ranges().len(), 1);
    assert_eq!(l.missing_ranges()[0], 4..text.len());
    assert!(l.missing_glyphs() > 0);
    let hidden = "\u{200d}\u{fe0f}";
    let spans = span(hidden);
    let l = FallbackParagraphLayout::new(&stack, hidden, style(&spans), settings(100.)).unwrap();
    assert_eq!(l.glyph_count(), 0);
    assert!(l.missing_ranges().is_empty());
    assert_eq!(l.text(), hidden);
}
#[test]
fn script_spans_admit_only_whole_original_graphemes_and_complete_valid_style() {
    let f = fonts();
    let stack = TextFontStack::new(&[&f[0]]).unwrap();
    let text = "a\u{301}b";
    for spans in [
        vec![],
        vec![
            ParagraphScriptSpan {
                range: 0..1,
                script: *b"Latn",
            },
            ParagraphScriptSpan {
                range: 1..4,
                script: *b"Latn",
            },
        ],
        vec![ParagraphScriptSpan {
            range: 0..4,
            script: *b"bad!",
        }],
        vec![ParagraphScriptSpan {
            range: 1..4,
            script: *b"Latn",
        }],
        vec![ParagraphScriptSpan {
            range: 0..5,
            script: *b"Latn",
        }],
    ] {
        assert!(matches!(
            FallbackParagraphLayout::new(&stack, text, style(&spans), settings(80.)),
            Err(TextError::InvalidInput)
        ));
    }
    for bad in ["\t", "\0", "\u{fdd0}"] {
        let spans = span(bad);
        assert!(matches!(
            FallbackParagraphLayout::new(&stack, bad, style(&spans), settings(80.)),
            Err(TextError::InvalidInput)
        ));
    }
    let spans = span("a");
    for width in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(matches!(
            FallbackParagraphLayout::new(&stack, "a", style(&spans), settings(width)),
            Err(TextError::InvalidInput)
        ));
    }
    assert!(
        FallbackParagraphLayout::new(
            &stack,
            "",
            style(&[]),
            ParagraphOptions {
                line_height: f64::MAX,
                ..settings(80.)
            }
        )
        .is_ok()
    );
}
#[test]
fn exact_work_output_limits_and_immutable_face_lifetime() {
    let f = fonts();
    // Layout owns copied source/style and borrows fonts, not the temporary stack.
    let layout = {
        let stack = TextFontStack::new(&[&f[0], &f[1]]).unwrap();
        let source = String::from("ab");
        let spans = span(&source);
        FallbackParagraphLayout::new(&stack, &source, style(&spans), settings(80.)).unwrap()
    };
    assert_eq!(layout.text(), "ab");
    assert_eq!(layout.lines()[0].runs()[1].font_index(), 1);
    let stack = TextFontStack::new(&[&f[0]; 16]).unwrap();
    for (count, accepted) in [(512, true), (513, false)] {
        let text = "☃".repeat(count);
        let spans = span(&text);
        let l = FallbackParagraphLayout::new(&stack, &text, style(&spans), settings(1e6));
        if accepted {
            assert_eq!(l.unwrap().work().probes, 8192);
        } else {
            assert!(matches!(l, Err(TextError::Capacity)));
        }
    }
    let stack = TextFontStack::new(&[&f[0], &f[1]]).unwrap();
    for (count, accepted) in [(256, true), (257, false)] {
        let text = "ab".repeat(count);
        let spans = span(&text);
        let l = FallbackParagraphLayout::new(&stack, &text, style(&spans), settings(1e6));
        if accepted {
            assert_eq!(l.unwrap().lines()[0].runs().len(), 512);
        } else {
            assert!(matches!(l, Err(TextError::Capacity)));
        }
    }
    let text = "\n".repeat(256);
    let spans = span(&text);
    assert!(matches!(
        FallbackParagraphLayout::new(&stack, &text, style(&spans), settings(80.)),
        Err(TextError::Capacity)
    ));
    let text = "a".repeat(4097);
    let spans = span(&text);
    assert!(matches!(
        FallbackParagraphLayout::new(&stack, &text, style(&spans), settings(80.)),
        Err(TextError::Capacity)
    ));
    assert_eq!(layout.text(), "ab");
}

#[test]
fn line_separator_keeps_open_isolate_while_paragraph_separator_resets_it() {
    let f = fonts();
    let stack = TextFontStack::new(&[&f[0], &f[1], &f[2]]).unwrap();
    // The visible second-line vectors follow the isolate level1 versus fresh base0.
    // They are fixed original byte positions, not computed using the layout backend.
    // PDI+space output retains the earlier PDI cluster13 after removal; the actual
    // independent HarfBuzz context oracle confirms local cluster4 (absolute13).
    for (separator, expected) in [
        ("\u{2028}", vec![11, 10, 9, 13, 17]),
        ("\u{2029}", vec![9, 10, 11, 13, 17]),
    ] {
        let text = format!("\u{2067}ب {separator}a ب\u{2069} c");
        let spans = span(&text);
        let l = FallbackParagraphLayout::new(&stack, &text, style(&spans), settings(100.)).unwrap();
        assert_eq!(l.lines().len(), 2);
        assert_eq!(l.lines()[1].range(), 9..18);
        let actual = l.lines()[1]
            .runs()
            .iter()
            .flat_map(|r| (0..r.shape().glyphs().len()).map(|i| r.source_cluster(i).unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{separator:?}");
        assert!(
            l.lines()[1]
                .runs()
                .iter()
                .all(|r| r.context_range() == (9..18))
        );
    }
    let text = "a\u{2067}ب ب ب\u{2069}c";
    let spans = span(text);
    let l = FallbackParagraphLayout::new(&stack, text, style(&spans), settings(12.)).unwrap();
    assert_eq!(
        l.lines().iter().map(|l| l.range()).collect::<Vec<_>>(),
        [0..7, 7..10, 10..16]
    );
    assert_eq!(
        l.lines()
            .iter()
            .map(|l| l
                .runs()
                .iter()
                .flat_map(|r| (0..r.shape().glyphs().len()).map(|i| r.source_cluster(i).unwrap()))
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [vec![0, 4, 6], vec![7, 9], vec![10, 15]]
    );
    assert!(
        l.lines()
            .iter()
            .flat_map(|l| l.runs())
            .all(|r| r.context_range() == (0..16))
    );
}
#[test]
fn matching_paragraph_svg_raster_analytic_pixels_and_late_rollback() {
    use ravet_surface::{
        drawing::{Color, SvgDrawing},
        layout::Rect,
    };
    let f = fonts();
    let spans = span("ab ab");
    let l = FallbackParagraphLayout::new(
        &TextFontStack::new(&[&f[0], &f[1]]).unwrap(),
        "ab ab",
        style(&spans),
        settings(14.5),
    )
    .unwrap();
    let mut svg = SvgDrawing::new(40., 40., "Paragraph").unwrap();
    svg.fallback_paragraph(&l, (5., 15.), Color::rgb(255, 0, 0))
        .unwrap();
    assert_eq!(svg.command_count(), 4);
    let output = svg.finish();
    assert!(!output.contains("<text"));
    assert!(!output.contains("href"));
    let mut actual = SvgDrawing::new(40., 40., "Original").unwrap();
    let mut expected = SvgDrawing::new(40., 40., "Original").unwrap();
    for _ in 0..4095 {
        for d in [&mut actual, &mut expected] {
            d.rectangle(Rect::new(0., 0., 1., 1.).unwrap(), 0., Color::rgb(1, 2, 3))
                .unwrap();
        }
    }
    assert!(matches!(
        actual.fallback_paragraph(&l, (5., 15.), Color::rgb(255, 0, 0)),
        Err(TextError::Capacity)
    ));
    assert_eq!(actual.command_count(), 4095);
    assert_eq!(actual.finish(), expected.finish());
    #[cfg(feature = "raster")]
    {
        use ravet_surface::raster::{RasterCanvas, RasterText};
        let prepared = RasterText::from_fallback_paragraph(&l, (5., 15.)).unwrap();
        drop(l);
        drop(f);
        let mut canvas = RasterCanvas::new(40, 40, 1).unwrap();
        canvas
            .draw_text(&prepared, Color::rgb(255, 0, 0), false)
            .unwrap();
        for y in [10, 22] {
            for (x, on) in [(5, true), (9, true), (10, true), (16, true), (17, false)] {
                assert_eq!(canvas.pixels()[4 * (y * 40 + x) + 3] > 0, on, "x{x}y{y}");
            }
        }
        let before = canvas.pixels().to_vec();
        let operations = canvas.operation_count();
        let commands = canvas.segment_count();
        assert!(
            canvas
                .draw_text(&prepared, Color::rgb(0, 255, 0), false)
                .is_err()
        );
        assert_eq!(canvas.pixels(), before);
        assert_eq!(canvas.operation_count(), operations);
        assert_eq!(canvas.segment_count(), commands);
    }
}

#[test]
fn selected_variations_and_adjacent_equal_script_spans_survive_rendering() {
    use ravet_surface::{
        drawing::{Color, SvgDrawing},
        text::TextVariation,
    };
    let base =
        TextFont::from_bytes(include_bytes!("fixtures/variation/RavetVariation.ttf"), 0).unwrap();
    let selected = base
        .with_variations(&[TextVariation {
            tag: *b"wght",
            value: 900.,
        }])
        .unwrap();
    let stack = TextFontStack::new(&[&selected]).unwrap();
    let spans = span("a a");
    let l = FallbackParagraphLayout::new(
        &stack,
        "a a",
        FallbackParagraphTextOptions {
            size: 100.,
            ..style(&spans)
        },
        settings(85.),
    )
    .unwrap();
    // Authored heavy a advance600, space250; first line85, second60 at size100.
    assert_eq!(
        l.lines().iter().map(|l| l.width()).collect::<Vec<_>>(),
        [85., 60.]
    );
    assert!(
        l.lines()
            .iter()
            .flat_map(|l| l.runs())
            .all(|r| std::ptr::eq(r.font(), &selected))
    );
    let mut svg = SvgDrawing::new(100., 180., "Selected heavy").unwrap();
    svg.fallback_paragraph(&l, (0., 80.), Color::rgb(255, 0, 0))
        .unwrap();
    assert_eq!(svg.command_count(), 2);
    #[cfg(feature = "raster")]
    {
        use ravet_surface::raster::{RasterCanvas, RasterText};
        let prepared = RasterText::from_fallback_paragraph(&l, (0., 80.)).unwrap();
        let mut canvas = RasterCanvas::new(100, 180, 1).unwrap();
        canvas
            .draw_text(&prepared, Color::rgb(255, 0, 0), false)
            .unwrap();
        // Heavy glyph's independent right-bound560units yields coverage through x55.
        assert!(canvas.pixels()[4 * (20 * 100 + 55) + 3] > 0);
        assert_eq!(canvas.pixels()[4 * (20 * 100 + 56) + 3], 0);
    }
    let f = fonts();
    let spans = [
        ParagraphScriptSpan {
            range: 0..1,
            script: *b"Latn",
        },
        ParagraphScriptSpan {
            range: 1..3,
            script: *b"Latn",
        },
    ];
    let l = FallbackParagraphLayout::new(
        &TextFontStack::new(&[&f[0]]).unwrap(),
        "ffi",
        style(&spans),
        settings(80.),
    )
    .unwrap();
    assert_eq!(l.glyph_count(), 1);
    assert_eq!(l.lines()[0].width(), 10.);
    assert_eq!(l.scripts(), spans);
}
#[test]
fn candidate_and_shared_shape_caps_include_discarded_measurement() {
    let f = fonts();
    let stack = TextFontStack::new(&[&f[0], &f[1]]).unwrap();
    for (count, accepted) in [(1024, true), (1025, false)] {
        let text = "a ".repeat(count);
        let spans = span(&text);
        let l = FallbackParagraphLayout::new(&stack, &text, style(&spans), settings(1e6));
        if accepted {
            let l = l.unwrap();
            assert_eq!(l.work().candidates, 1024);
            assert_eq!(l.lines().len(), 1);
            assert_eq!(l.work().probes, 2048);
        } else {
            assert!(matches!(l, Err(TextError::Capacity)));
        }
    }
    // Many alternating-font zero-width-break candidates hit the shared shape cap
    // before candidate or final-run capacity. The caller's earlier layout stays valid.
    let source = "ab";
    let spans = span(source);
    let previous =
        FallbackParagraphLayout::new(&stack, source, style(&spans), settings(80.)).unwrap();
    let text = "ab\u{200b}".repeat(150);
    let spans = span(&text);
    assert!(matches!(
        FallbackParagraphLayout::new(&stack, &text, style(&spans), settings(1e6)),
        Err(TextError::Capacity)
    ));
    let narrow =
        FallbackParagraphLayout::new(&stack, &text, style(&spans), settings(14.5)).unwrap();
    assert!(narrow.work().candidates < MAXIMUM_PARAGRAPH_CANDIDATES);
    assert_eq!(previous.text(), "ab");
    assert_eq!(previous.lines()[0].width(), 12.);
}

#[test]
fn second_selected_face_outline_failure_preserves_whole_renderer_admission() {
    use ravet_surface::{
        drawing::{Color, SvgDrawing},
        layout::Rect,
    };
    let f = fonts();
    // Keep the authored cmap/hmtx/GSUB intact but retire its glyf table identity.
    // glyx stays between gasp/head in the sorted directory: shaping remains
    // covered while the explicit glyf outline profile is unavailable.
    let mut bytes = include_bytes!("fixtures/fallback/RavetFallbackComplement.ttf").to_vec();
    let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
    let mut changed = false;
    for index in 0..count {
        let at = 12 + index * 16;
        if &bytes[at..at + 4] == b"glyf" {
            bytes[at..at + 4].copy_from_slice(b"glyx");
            changed = true;
        }
    }
    assert!(changed);
    let unavailable = TextFont::from_bytes(&bytes, 0).unwrap();
    let spans = span("ab");
    let layout = FallbackParagraphLayout::new(
        &TextFontStack::new(&[&f[0], &unavailable]).unwrap(),
        "ab",
        style(&spans),
        settings(80.),
    )
    .unwrap();
    assert_eq!(layout.missing_glyphs(), 0);
    assert_eq!(layout.lines()[0].runs().len(), 2);
    assert!(std::ptr::eq(
        layout.lines()[0].runs()[1].font(),
        &unavailable
    ));
    let mut svg = SvgDrawing::new(40., 40., "Before").unwrap();
    let mut before = SvgDrawing::new(40., 40., "Before").unwrap();
    for output in [&mut svg, &mut before] {
        output
            .rectangle(Rect::new(1., 1., 2., 2.).unwrap(), 0., Color::rgb(1, 2, 3))
            .unwrap();
    }
    assert_eq!(
        svg.fallback_paragraph(&layout, (5., 15.), Color::rgb(255, 0, 0)),
        Err(TextError::UnsupportedFont)
    );
    assert_eq!(svg.command_count(), 1);
    assert_eq!(svg.finish(), before.finish());
    #[cfg(feature = "raster")]
    {
        use ravet_surface::raster::{RasterCanvas, RasterError, RasterText};
        let healthy = FallbackParagraphLayout::new(
            &TextFontStack::new(&[&f[0], &f[1]]).unwrap(),
            "ab",
            style(&spans),
            settings(80.),
        )
        .unwrap();
        let prior = RasterText::from_fallback_paragraph(&healthy, (5., 15.)).unwrap();
        let mut canvas = RasterCanvas::new(40, 40, 2).unwrap();
        canvas
            .draw_text(&prior, Color::rgb(255, 0, 0), false)
            .unwrap();
        let pixels = canvas.pixels().to_vec();
        let commands = canvas.segment_count();
        let operations = canvas.operation_count();
        assert!(matches!(
            RasterText::from_fallback_paragraph(&layout, (5., 15.)),
            Err(RasterError::Text(TextError::UnsupportedFont))
        ));
        assert_eq!(canvas.pixels(), pixels);
        assert_eq!(canvas.segment_count(), commands);
        assert_eq!(canvas.operation_count(), operations);
        canvas
            .draw_text(&prior, Color::rgb(255, 0, 0), false)
            .unwrap();
        assert_eq!(canvas.pixels(), pixels);
    }
}

#[test]
fn trailing_x9_scalars_preserve_whole_graphemes_at_every_candidate_width() {
    let f = fonts();
    let stack = TextFontStack::new(&[&f[0], &f[1], &f[2]]).unwrap();
    for (text, rtl, script) in [
        ("ب\u{200d}", false, *b"Arab"),
        ("abc\u{200d} def", true, *b"Latn"),
        (
            "ب 🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f} ب",
            false,
            *b"Arab",
        ),
    ] {
        let spans = [ParagraphScriptSpan {
            range: 0..text.len(),
            script,
        }];
        let boundaries = unicode_segmentation::UnicodeSegmentation::grapheme_indices(text, true)
            .map(|(index, _)| index)
            .chain([text.len()])
            .collect::<Vec<_>>();
        for width in [1., 10., 14.5, 40., 1_000_000.] {
            let l = FallbackParagraphLayout::new(
                &stack,
                text,
                FallbackParagraphTextOptions {
                    base_direction: if rtl {
                        TextDirection::RightToLeft
                    } else {
                        TextDirection::LeftToRight
                    },
                    ..style(&spans)
                },
                settings(width),
            )
            .unwrap();
            for line in l.lines() {
                for run in line.runs() {
                    assert!(boundaries.contains(&run.range().start));
                    assert!(boundaries.contains(&run.range().end));
                    assert_eq!(run.context_range(), 0..text.len());
                }
            }
            assert_eq!(l.lines().last().unwrap().range().end, text.len());
        }
    }
}

#[test]
fn prepend_graphemes_use_final_retained_scalar_level_in_both_directions() {
    let f = fonts();
    let stack = TextFontStack::new(&[&f[0], &f[1], &f[2]]).unwrap();
    for (text, range, ltr_level, rtl_level) in [
        ("ب \u{0600}a\u{301} ب", 3..8, 0, 2),
        ("a \u{0600}ب\u{301} a", 2..8, 1, 1),
    ] {
        let spans = span(text);
        for rtl in [false, true] {
            for width in [1., 10., 14.5, 40., 1_000_000.] {
                let l = FallbackParagraphLayout::new(
                    &stack,
                    text,
                    FallbackParagraphTextOptions {
                        base_direction: if rtl {
                            TextDirection::RightToLeft
                        } else {
                            TextDirection::LeftToRight
                        },
                        ..style(&spans)
                    },
                    settings(width),
                )
                .unwrap();
                let run = l
                    .lines()
                    .iter()
                    .flat_map(|l| l.runs())
                    .find(|r| r.range().start <= range.start && r.range().end >= range.end)
                    .unwrap();
                assert_eq!(run.level(), if rtl { rtl_level } else { ltr_level });
                assert_eq!(run.context_range(), 0..text.len());
                assert!(
                    run.shape()
                        .glyphs()
                        .iter()
                        .enumerate()
                        .all(|(i, _)| run.source_cluster(i).unwrap() < run.range().end)
                );
                if width == 1_000_000. {
                    let units =
                        unicode_segmentation::UnicodeSegmentation::grapheme_indices(text, true)
                            .map(|(start, g)| start..start + g.len())
                            .collect::<Vec<_>>();
                    let actual = l.lines()[0]
                        .runs()
                        .iter()
                        .flat_map(|run| {
                            let mut selected = units
                                .iter()
                                .filter(|unit| {
                                    unit.start >= run.range().start && unit.end <= run.range().end
                                })
                                .cloned()
                                .collect::<Vec<_>>();
                            if run.level() % 2 == 1 {
                                selected.reverse();
                            }
                            selected
                        })
                        .collect::<Vec<_>>();
                    let expected = match (range.start, rtl) {
                        (3, false) => vec![2..3, 0..2, 3..8, 8..9, 9..11],
                        (3, true) => vec![9..11, 8..9, 3..8, 2..3, 0..2],
                        (2, false) => vec![0..1, 1..2, 2..8, 8..9, 9..10],
                        (2, true) => vec![9..10, 8..9, 2..8, 1..2, 0..1],
                        _ => unreachable!(),
                    };
                    assert_eq!(actual, expected);
                }
            }
        }
    }
}

#[test]
fn overflowing_discarded_candidate_with_trailing_joiner_still_admits_complete_text() {
    let f = fonts();
    let stack = TextFontStack::new(&[&f[0], &f[1], &f[2]]).unwrap();
    let text = "a abc\u{200d} def";
    let spans = span(text);
    let l = FallbackParagraphLayout::new(
        &stack,
        text,
        FallbackParagraphTextOptions {
            base_direction: TextDirection::RightToLeft,
            ..style(&spans)
        },
        settings(14.5),
    )
    .unwrap();
    assert_eq!(
        l.lines().iter().map(|l| l.range()).collect::<Vec<_>>(),
        [0..2, 2..9, 9..12]
    );
    assert!(l.work().candidates > l.lines().len());
}
