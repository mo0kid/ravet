#![cfg(feature = "text")]
use ravet_surface::{
    bidi::BidiLine,
    drawing::{SvgDrawing, Theme},
    layout::Rect,
    paragraph::{MAXIMUM_PARAGRAPH_LINES, ParagraphLayout, ParagraphOptions, TextAlignment},
    text::{TextDirection, TextError, TextFont, TextOptions},
};
const FONT: &[u8] = include_bytes!("fixtures/text/NotoSansArabic.ttf");
fn style() -> TextOptions<'static> {
    TextOptions {
        size: 24.,
        direction: TextDirection::LeftToRight,
        script: None,
        language: None,
        features: &[],
    }
}
fn settings(width: f64) -> ParagraphOptions {
    ParagraphOptions {
        width,
        line_height: 32.,
        alignment: TextAlignment::Left,
    }
}
#[test]
fn greedy_wrapping_preserves_exact_whitespace_glyphs_clusters_and_alignment() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    let width = font.shape("Gain ", style()).unwrap().advance().0;
    let text = "Gain Gain Gain";
    for alignment in [
        TextAlignment::Left,
        TextAlignment::Center,
        TextAlignment::Right,
    ] {
        let layout = ParagraphLayout::new(
            &font,
            text,
            style(),
            ParagraphOptions {
                alignment,
                ..settings(width)
            },
        )
        .unwrap();
        assert_eq!(
            layout
                .lines()
                .iter()
                .map(|line| &text[line.range()])
                .collect::<Vec<_>>(),
            ["Gain ", "Gain ", "Gain"]
        );
        assert_eq!(layout.height(), 96.);
        assert_eq!(layout.glyph_count(), 14);
        for (index, line) in layout.lines().iter().enumerate() {
            assert!(!line.overflows());
            assert_eq!(line.separator(), line.range().end..line.range().end);
            let expected = font.shape(&text[line.range()], style()).unwrap();
            assert_eq!(line.width(), expected.advance().0);
            assert_eq!(line.runs().len(), 1);
            let room = width - line.width();
            let x = match alignment {
                TextAlignment::Left => 0.,
                TextAlignment::Center => room / 2.,
                TextAlignment::Right => room,
            };
            assert_eq!(line.baseline(), (x, index as f64 * 32.));
            assert_eq!(line.runs()[0].shape(), &expected);
            assert_eq!(line.runs()[0].x(), 0.);
            for (i, glyph) in expected.glyphs().iter().enumerate() {
                assert_eq!(
                    line.runs()[0].source_cluster(i),
                    Some(line.range().start + glyph.cluster as usize)
                );
            }
            assert_eq!(line.runs()[0].source_cluster(999), None);
        }
    }
}
#[test]
fn hard_separators_empty_lines_and_original_byte_coverage() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    let text = "Gain\r\n\nAV\rبَ\u{2028}X\u{2029}";
    let layout = ParagraphLayout::new(&font, text, style(), settings(500.)).unwrap();
    assert_eq!(
        layout
            .lines()
            .iter()
            .map(|l| &text[l.range()])
            .collect::<Vec<_>>(),
        ["Gain", "", "AV", "بَ", "X", ""]
    );
    assert_eq!(
        layout
            .lines()
            .iter()
            .map(|l| &text[l.separator()])
            .collect::<Vec<_>>(),
        ["\r\n", "\n", "\r", "\u{2028}", "\u{2029}", ""]
    );
    let mut cursor = 0;
    for line in layout.lines() {
        assert_eq!(line.range().start, cursor);
        assert_eq!(line.separator().start, line.range().end);
        cursor = line.separator().end;
    }
    assert_eq!(cursor, text.len());
    let empty = ParagraphLayout::new(&font, "", style(), settings(500.)).unwrap();
    assert_eq!(empty.lines().len(), 1);
    assert!(empty.lines()[0].runs().is_empty());
    assert_eq!(empty.height(), 32.);
}
#[test]
fn unbreakable_overflow_nonbreaking_space_marks_and_zero_width_breaks() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    for alignment in [
        TextAlignment::Left,
        TextAlignment::Center,
        TextAlignment::Right,
    ] {
        let layout = ParagraphLayout::new(
            &font,
            "Gain",
            style(),
            ParagraphOptions {
                alignment,
                ..settings(1.)
            },
        )
        .unwrap();
        assert_eq!(layout.lines().len(), 1);
        assert!(layout.lines()[0].overflows());
        assert_eq!(layout.lines()[0].range(), 0..4);
        assert_eq!(layout.lines()[0].baseline(), (0., 0.));
    }
    let width = font.shape("Gain ", style()).unwrap().advance().0;
    let text = "Gain\u{a0}Gain Gain";
    let layout = ParagraphLayout::new(&font, text, style(), settings(width)).unwrap();
    assert_eq!(&text[layout.lines()[0].range()], "Gain\u{a0}Gain ");
    assert!(layout.lines()[0].overflows());
    assert_eq!(&text[layout.lines()[1].range()], "Gain");
    let text = "A\u{301}V A\u{301}V";
    let width = font.shape("A\u{301}V ", style()).unwrap().advance().0;
    let layout = ParagraphLayout::new(&font, text, style(), settings(width)).unwrap();
    assert_eq!(layout.lines().len(), 2);
    assert_eq!(layout.lines()[0].range().end, 5);
    assert!(
        layout
            .lines()
            .iter()
            .flat_map(|l| l.runs())
            .flat_map(|r| r.shape().glyphs())
            .all(|g| g.cluster != 1)
    );
    let text = "AV\u{200b}AV";
    let width = font.shape("AV\u{200b}", style()).unwrap().advance().0;
    let layout = ParagraphLayout::new(&font, text, style(), settings(width)).unwrap();
    assert_eq!(layout.lines().len(), 2);
    assert_eq!(layout.lines()[0].range(), 0..5);
}
#[test]
fn paragraph_bidi_embedding_context_survives_soft_wrap_and_visual_order_is_retained() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    let text = "\u{202b}Gain Gain Gain\u{202c}";
    let width = font.shape("\u{202b}Gain ", style()).unwrap().advance().0;
    let layout = ParagraphLayout::new(&font, text, style(), settings(width)).unwrap();
    assert!(layout.lines().len() >= 3);
    let second = &layout.lines()[1];
    assert!(second.runs().iter().any(|r| r.level() == 2));
    let fresh = BidiLine::new(&text[second.range()], Some(TextDirection::LeftToRight)).unwrap();
    assert!(fresh.visual_runs().iter().all(|r| r.level != 2));
    let text = "Gain 12 — سلام 34 Gain 56 — سلام";
    let layout = ParagraphLayout::new(&font, text, style(), settings(240.)).unwrap();
    assert!(layout.lines().len() > 1);
    assert!(
        layout
            .lines()
            .iter()
            .flat_map(|l| l.runs())
            .any(|r| r.shape().direction() == TextDirection::RightToLeft)
    );
    for line in layout.lines() {
        let mut x = 0.;
        for run in line.runs() {
            assert_eq!(run.x(), x);
            x += run.shape().advance().0;
            for i in 0..run.shape().glyphs().len() {
                assert!(text.is_char_boundary(run.source_cluster(i).unwrap()));
            }
        }
        assert_eq!(x, line.width());
    }
}
#[test]
fn invalid_global_styles_output_limits_and_extreme_spacing_fail_completely() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    for text in ["\t", "\0", "\u{85}", "\u{fdd0}", "\u{10ffff}"] {
        assert_eq!(
            ParagraphLayout::new(&font, text, style(), settings(500.)),
            Err(TextError::InvalidInput)
        );
    }
    for width in [0., -1., f64::NAN, f64::INFINITY] {
        assert_eq!(
            ParagraphLayout::new(&font, "Gain", style(), settings(width)),
            Err(TextError::InvalidInput)
        );
    }
    for line_height in [0., -1., f64::NAN, f64::INFINITY] {
        assert_eq!(
            ParagraphLayout::new(
                &font,
                "Gain",
                style(),
                ParagraphOptions {
                    line_height,
                    ..settings(500.)
                }
            ),
            Err(TextError::InvalidInput)
        );
    }
    let mut invalid = style();
    invalid.size = f64::NAN;
    assert_eq!(
        ParagraphLayout::new(&font, "\n", invalid, settings(500.)),
        Err(TextError::InvalidInput)
    );
    assert_eq!(
        ParagraphLayout::new(&font, &"A".repeat(4097), style(), settings(500.)),
        Err(TextError::Capacity)
    );
    let exact = "\n".repeat(MAXIMUM_PARAGRAPH_LINES - 1);
    assert_eq!(
        ParagraphLayout::new(&font, &exact, style(), settings(500.))
            .unwrap()
            .lines()
            .len(),
        MAXIMUM_PARAGRAPH_LINES
    );
    assert_eq!(
        ParagraphLayout::new(
            &font,
            &"\n".repeat(MAXIMUM_PARAGRAPH_LINES),
            style(),
            settings(500.)
        ),
        Err(TextError::Capacity)
    );
    assert_eq!(
        ParagraphLayout::new(
            &font,
            "A\nA",
            style(),
            ParagraphOptions {
                line_height: f64::MAX,
                ..settings(500.)
            }
        ),
        Err(TextError::InvalidInput)
    );
    let runs = ParagraphLayout::new(&font, &"Aب".repeat(256), style(), settings(1e9)).unwrap();
    assert_eq!(runs.lines()[0].runs().len(), 512);
    assert_eq!(
        ParagraphLayout::new(&font, &"Aب".repeat(257), style(), settings(1e9)),
        Err(TextError::Capacity)
    );
    let maximum = ParagraphLayout::new(&font, &"A".repeat(4096), style(), settings(1.)).unwrap();
    assert_eq!(maximum.glyph_count(), 4096);
    assert!(maximum.lines()[0].overflows());
}
#[test]
fn outlined_paragraph_matches_cached_run_placement_and_rolls_back_late_capacity_faults() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    let text = "Gain 12 — سلام\nبَ AV";
    let mut paragraph = SvgDrawing::new(640., 240., "paragraph").unwrap();
    let layout = paragraph
        .paragraph_text(
            &font,
            text,
            style(),
            settings(250.),
            (20., 50.),
            Theme::LIGHT.foreground,
        )
        .unwrap();
    let mut manual = SvgDrawing::new(640., 240., "paragraph").unwrap();
    for line in layout.lines() {
        for run in line.runs() {
            manual
                .shaped_text(
                    &font,
                    &text[run.range()],
                    TextOptions {
                        direction: run.shape().direction(),
                        ..style()
                    },
                    (20. + line.baseline().0 + run.x(), 50. + line.baseline().1),
                    Theme::LIGHT.foreground,
                )
                .unwrap();
        }
    }
    assert_eq!(paragraph.finish(), manual.finish());
    let mut full = SvgDrawing::new(640., 240., "fault").unwrap();
    for _ in 0..4095 {
        full.rectangle(
            Rect::new(0., 0., 1., 1.).unwrap(),
            0.,
            Theme::LIGHT.background,
        )
        .unwrap();
    }
    let count = full.command_count();
    let mut backup = SvgDrawing::new(640., 240., "fault").unwrap();
    for _ in 0..4095 {
        backup
            .rectangle(
                Rect::new(0., 0., 1., 1.).unwrap(),
                0.,
                Theme::LIGHT.background,
            )
            .unwrap();
    }
    let expected = backup.finish();
    assert_eq!(
        full.paragraph_text(
            &font,
            "AV\nAV",
            style(),
            settings(250.),
            (20., 50.),
            Theme::LIGHT.foreground
        ),
        Err(TextError::Capacity)
    );
    assert_eq!(full.command_count(), count);
    assert_eq!(full.finish(), expected);
}
