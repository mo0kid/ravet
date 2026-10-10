#![cfg(feature = "text")]
use ravet_surface::{
    caret::{CaretAffinity as Affinity, CaretMap, SelectionRect, TextPosition},
    paragraph::{ParagraphLayout, ParagraphOptions, TextAlignment},
    text::{TextDirection, TextError, TextFont, TextOptions},
};
const FONT: &[u8] = include_bytes!("fixtures/text/NotoSansArabic.ttf");
const LIGATURE: &[u8] = include_bytes!("fixtures/caret/RavetSyntheticLigature.ttf");
fn style(direction: TextDirection) -> TextOptions<'static> {
    TextOptions {
        size: 24.,
        direction,
        script: None,
        language: None,
        features: &[],
    }
}
fn options(width: f64) -> ParagraphOptions {
    ParagraphOptions {
        width,
        line_height: 32.,
        alignment: TextAlignment::Left,
    }
}
fn position(byte_offset: usize, affinity: Affinity) -> TextPosition {
    TextPosition {
        byte_offset,
        affinity,
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}")
}
#[test]
fn ltr_pen_positions_hits_and_invalid_coordinates_are_explicit() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    let layout = ParagraphLayout::new(
        &font,
        "AV",
        style(TextDirection::LeftToRight),
        options(500.),
    )
    .unwrap();
    let map = CaretMap::new(&layout).unwrap();
    assert_eq!(map.boundaries(), [0, 1, 2]);
    let shaped = font.shape("AV", style(TextDirection::LeftToRight)).unwrap();
    let advances = [0., shaped.glyphs()[0].x_advance, shaped.advance().0];
    for (byte, x) in advances.into_iter().enumerate() {
        for affinity in [Affinity::Upstream, Affinity::Downstream] {
            let c = map.caret(position(byte, affinity)).unwrap();
            assert_eq!(c.position, position(byte, affinity));
            assert_eq!(c.point, (x, 0.));
            assert_eq!(c.line, 0);
        }
    }
    assert_eq!(
        map.hit_test(-f64::MAX, -f64::MAX, Affinity::Downstream)
            .unwrap()
            .position
            .byte_offset,
        0
    );
    assert_eq!(
        map.hit_test(f64::MAX, f64::MAX, Affinity::Downstream)
            .unwrap()
            .position
            .byte_offset,
        2
    );
    assert_eq!(
        map.hit_test(advances[1] + 0.1, 0., Affinity::Downstream)
            .unwrap()
            .position
            .byte_offset,
        1
    );
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(map.hit_test(invalid, 0., Affinity::Downstream).is_none());
        assert!(map.hit_test(0., invalid, Affinity::Downstream).is_none());
    }
    assert!(
        map.caret(position(usize::MAX, Affinity::Downstream))
            .is_none()
    );

    let invisible = ParagraphLayout::new(
        &font,
        "A\u{200b}V",
        style(TextDirection::LeftToRight),
        options(500.),
    )
    .unwrap();
    let invisible_map = CaretMap::new(&invisible).unwrap();
    assert_eq!(invisible_map.boundaries(), [0, 1, 4, 5]);
    let before = invisible_map
        .caret(position(1, Affinity::Downstream))
        .unwrap();
    let after = invisible_map
        .caret(position(4, Affinity::Downstream))
        .unwrap();
    assert_eq!(before.point, after.point);
    assert_eq!(
        invisible_map
            .hit_test(after.point.0, 0., Affinity::Downstream)
            .unwrap()
            .position
            .byte_offset,
        1
    );
    let mut boxes = [SelectionRect::empty(); 1];
    assert_eq!(invisible_map.selection(1..4, &mut boxes).unwrap(), 1);
    assert_eq!(boxes[0].rectangle.width(), 0.);
}
#[test]
fn combining_arabic_uses_one_grapheme_and_rtl_edges() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    let layout =
        ParagraphLayout::new(&font, "بَ", style(TextDirection::RightToLeft), options(500.)).unwrap();
    let map = CaretMap::new(&layout).unwrap();
    assert_eq!(map.boundaries(), [0, 4]);
    assert!(map.caret(position(2, Affinity::Downstream)).is_none());
    close(
        map.caret(position(0, Affinity::Downstream))
            .unwrap()
            .point
            .0,
        layout.lines()[0].width(),
    );
    close(
        map.caret(position(4, Affinity::Upstream)).unwrap().point.0,
        0.,
    );
    let mut boxes = [SelectionRect::empty(); 1];
    assert_eq!(map.selection(0..4, &mut boxes).unwrap(), 1);
    close(boxes[0].rectangle.width(), layout.lines()[0].width());
    assert_eq!(
        map.selection(0..2, &mut boxes),
        Err(TextError::InvalidInput)
    );
}
#[test]
fn real_synthetic_ligature_interpolates_grapheme_positions_without_gdef_claim() {
    let font = TextFont::from_bytes(LIGATURE, 0).unwrap();
    let shaped = font
        .shape("ffi", style(TextDirection::LeftToRight))
        .unwrap();
    assert_eq!(shaped.glyphs().len(), 1);
    assert_eq!(shaped.advance().0, 24.);
    let layout = ParagraphLayout::new(
        &font,
        "ffi",
        style(TextDirection::LeftToRight),
        options(500.),
    )
    .unwrap();
    let map = CaretMap::new(&layout).unwrap();
    assert_eq!(map.boundaries(), [0, 1, 2, 3]);
    for byte in 0..=3 {
        close(
            map.caret(position(byte, Affinity::Downstream))
                .unwrap()
                .point
                .0,
            byte as f64 * 8.,
        );
    }
    let mut boxes = [SelectionRect::empty(); 1];
    assert_eq!(map.selection(1..2, &mut boxes).unwrap(), 1);
    assert_eq!(boxes[0].rectangle.x(), 8.);
    assert_eq!(boxes[0].rectangle.width(), 8.);
    assert_eq!(boxes[0].rectangle.y(), -16.);
    assert_eq!(boxes[0].rectangle.height(), 32.);
    assert_eq!(
        map.hit_test(12., 0., Affinity::Downstream)
            .unwrap()
            .position
            .byte_offset,
        1
    );
}
#[test]
fn wrap_and_bidi_boundaries_retain_distinct_affinities() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    let width = font
        .shape("Gain ", style(TextDirection::LeftToRight))
        .unwrap()
        .advance()
        .0;
    let layout = ParagraphLayout::new(
        &font,
        "Gain Gain",
        style(TextDirection::LeftToRight),
        options(width),
    )
    .unwrap();
    let map = CaretMap::new(&layout).unwrap();
    let up = map.caret(position(5, Affinity::Upstream)).unwrap();
    let down = map.caret(position(5, Affinity::Downstream)).unwrap();
    assert_eq!(up.line, 0);
    assert_eq!(down.line, 1);
    close(up.point.0, width);
    assert_eq!(down.point, (0., 32.));
    assert_eq!(map.hit_test(0., 16., Affinity::Downstream).unwrap().line, 1);
    assert_eq!(map.hit_test(0., 15., Affinity::Downstream).unwrap().line, 0);

    let hit = map.hit_test(width + 10., 0., Affinity::Downstream).unwrap();
    assert_eq!(hit.position, position(5, Affinity::Upstream));
    let mut boxes = [SelectionRect::empty(); 4];
    assert_eq!(map.selection(0..9, &mut boxes).unwrap(), 2);
    assert_eq!(boxes[0].line, 0);
    assert_eq!(boxes[1].line, 1);
    let text = "AV سلام";
    let mixed = ParagraphLayout::new(
        &font,
        text,
        style(TextDirection::LeftToRight),
        options(500.),
    )
    .unwrap();
    let map = CaretMap::new(&mixed).unwrap();
    let up = map.caret(position(3, Affinity::Upstream)).unwrap();
    let down = map.caret(position(3, Affinity::Downstream)).unwrap();
    assert_eq!(up.line, down.line);
    assert!(up.point.0 < down.point.0);
    assert_eq!(map.selection(0..5, &mut boxes).unwrap(), 2);
    assert!(boxes[0].rectangle.x() + boxes[0].rectangle.width() < boxes[1].rectangle.x());
}
#[test]
fn hard_separators_crlf_and_empty_lines_preserve_original_byte_boundaries() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    let layout = ParagraphLayout::new(
        &font,
        "AV\r\nAV",
        style(TextDirection::LeftToRight),
        options(500.),
    )
    .unwrap();
    let map = CaretMap::new(&layout).unwrap();
    assert_eq!(map.boundaries(), [0, 1, 2, 4, 5, 6]);
    assert!(map.caret(position(3, Affinity::Downstream)).is_none());
    assert_eq!(
        map.caret(position(2, Affinity::Downstream)).unwrap().line,
        0
    );
    assert_eq!(map.caret(position(4, Affinity::Upstream)).unwrap().line, 1);
    let mut boxes = [SelectionRect::empty(); 2];
    assert_eq!(map.selection(2..4, &mut boxes).unwrap(), 0);
    let empty = ParagraphLayout::new(
        &font,
        "",
        style(TextDirection::LeftToRight),
        ParagraphOptions {
            alignment: TextAlignment::Center,
            ..options(300.)
        },
    )
    .unwrap();
    let map = CaretMap::new(&empty).unwrap();
    assert_eq!(map.boundaries(), [0]);
    assert_eq!(
        map.caret(position(0, Affinity::Downstream)).unwrap().point,
        (150., 0.)
    );
    assert_eq!(map.selection(0..0, &mut boxes).unwrap(), 0);
}
#[test]
fn selection_preflights_capacity_and_reuses_fixed_output_without_heap() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    let layout = ParagraphLayout::new(
        &font,
        "AV سلام 12\nAV",
        style(TextDirection::LeftToRight),
        options(500.),
    )
    .unwrap();
    let map = CaretMap::new(&layout).unwrap();
    let sentinel = SelectionRect {
        line: 99,
        rectangle: ravet_surface::layout::Rect::new(7., 9., 10., 11.).unwrap(),
    };
    let mut short = [sentinel; 1];
    assert_eq!(
        map.selection(0..layout.text().len(), &mut short),
        Err(TextError::Capacity)
    );
    assert_eq!(short, [sentinel]);
    assert_eq!(
        map.selection(4..5, &mut short),
        Err(TextError::InvalidInput)
    );
    assert_eq!(short, [sentinel]);
    let mut output = [sentinel; 16];
    let counts = allocation_counter::measure(|| {
        for _ in 0..3000 {
            assert!(map.caret(position(3, Affinity::Downstream)).is_some());
            assert!(map.hit_test(25., 40., Affinity::Downstream).is_some());
            assert_eq!(
                map.selection(0..layout.text().len(), &mut output).unwrap(),
                2
            );
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(output[2], sentinel);
    let maximum = ParagraphLayout::new(
        &font,
        &"A".repeat(4096),
        style(TextDirection::LeftToRight),
        options(1.),
    )
    .unwrap();
    let map = CaretMap::new(&maximum).unwrap();
    assert_eq!(map.boundaries().len(), 4097);
    let mut one = [sentinel; 1];
    assert_eq!(map.selection(0..4096, &mut one).unwrap(), 1);
}
