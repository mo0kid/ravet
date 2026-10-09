#![cfg(feature = "text")]
use ravet_surface::text::{TextDirection, TextError, TextFeature, TextFont, TextOptions};
const FONT: &[u8] = include_bytes!("fixtures/text/NotoSansArabic.ttf");
fn options(direction: TextDirection) -> TextOptions<'static> {
    TextOptions {
        size: 24.,
        direction,
        script: None,
        language: None,
        features: &[],
    }
}
#[test]
fn arabic_context_visual_clusters_marks_measurement_and_scaling() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    assert!(font.units_per_em() > 0);
    let text = "سلام";
    let run = font
        .shape(text, options(TextDirection::RightToLeft))
        .unwrap();
    assert_eq!(run.missing_glyphs(), 0);
    assert!(!run.glyphs().is_empty());
    assert!(run.advance().0 > 0.);
    assert!(
        run.glyphs()
            .windows(2)
            .all(|g| g[0].cluster >= g[1].cluster)
    );
    for glyph in run.glyphs() {
        assert!(text.is_char_boundary(glyph.cluster as usize));
    }
    let isolated: Vec<_> = text
        .chars()
        .map(|c| {
            font.shape(&c.to_string(), options(TextDirection::RightToLeft))
                .unwrap()
                .glyphs()[0]
                .id
        })
        .collect();
    assert!(
        run.glyphs().len() < isolated.len()
            || run
                .glyphs()
                .iter()
                .zip(isolated)
                .any(|(glyph, id)| glyph.id != id)
    );
    let mut twice = options(TextDirection::RightToLeft);
    twice.size = 48.;
    let scaled = font.shape(text, twice).unwrap();
    assert_eq!(scaled.advance().0, run.advance().0 * 2.);
    for (a, b) in run.glyphs().iter().zip(scaled.glyphs()) {
        assert_eq!((a.id, a.cluster), (b.id, b.cluster));
        assert_eq!(
            (a.x * 2., a.y * 2., a.x_advance * 2.),
            (b.x, b.y, b.x_advance)
        );
    }
    let marks = font
        .shape("بَ", options(TextDirection::RightToLeft))
        .unwrap();
    assert_eq!(marks.missing_glyphs(), 0);
    assert!(marks.glyphs().iter().any(|g| g.x_advance == 0.));
}
#[test]
fn explicit_features_missing_glyphs_empty_runs_and_repeat_identity() {
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    let empty = font.shape("", options(TextDirection::LeftToRight)).unwrap();
    assert!(empty.glyphs().is_empty());
    assert_eq!(empty.advance(), (0., 0.));
    let normal = font
        .shape("AV", options(TextDirection::LeftToRight))
        .unwrap();
    let overrides = [TextFeature {
        tag: *b"kern",
        enabled: false,
    }];
    let mut off = options(TextDirection::LeftToRight);
    off.features = &overrides;
    let unkerned = font.shape("AV", off).unwrap();
    assert!(normal.advance().0 < unkerned.advance().0);
    let unknown = font.shape("\u{10ffff}", options(TextDirection::LeftToRight));
    assert_eq!(unknown, Err(TextError::InvalidInput));
    let missing = font
        .shape("\u{1f984}", options(TextDirection::LeftToRight))
        .unwrap();
    assert_eq!(missing.missing_glyphs(), 1);
    for _ in 0..20 {
        assert_eq!(
            font.shape("AV", options(TextDirection::LeftToRight))
                .unwrap(),
            normal
        );
    }
}
#[test]
fn bounds_invalid_settings_and_font_rejection() {
    assert!(matches!(
        TextFont::from_bytes(&[], 0),
        Err(TextError::UnsupportedFont)
    ));
    assert!(matches!(
        TextFont::from_bytes(b"invalid font", 0),
        Err(TextError::UnsupportedFont)
    ));
    assert!(TextFont::from_bytes(FONT, 1).is_err());
    assert!(matches!(
        TextFont::from_bytes(&vec![0; ravet_surface::text::MAXIMUM_FONT_BYTES + 1], 0),
        Err(TextError::Capacity)
    ));
    let font = TextFont::from_bytes(FONT, 0).unwrap();
    for size in [0., -1., 1025., f64::NAN, f64::INFINITY] {
        let mut o = options(TextDirection::LeftToRight);
        o.size = size;
        assert_eq!(font.shape("a", o), Err(TextError::InvalidInput));
    }
    for text in ["a\nb", "a\0b", "\u{fdd0}"] {
        assert_eq!(
            font.shape(text, options(TextDirection::LeftToRight)),
            Err(TextError::InvalidInput)
        );
    }
    assert_eq!(
        font.shape(&"a".repeat(4097), options(TextDirection::LeftToRight)),
        Err(TextError::Capacity)
    );
    let bad = [TextFeature {
        tag: [0; 4],
        enabled: true,
    }];
    let mut o = options(TextDirection::LeftToRight);
    o.features = &bad;
    assert_eq!(font.shape("a", o), Err(TextError::InvalidInput));
    let too_many = [TextFeature {
        tag: *b"kern",
        enabled: true,
    }; 33];
    o.features = &too_many;
    assert_eq!(font.shape("a", o), Err(TextError::Capacity));
    o = options(TextDirection::LeftToRight);
    o.language = Some("bad language");
    assert_eq!(font.shape("a", o), Err(TextError::InvalidInput));
}
