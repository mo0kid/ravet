#![cfg(feature = "text")]
use ravet_surface::{
    bidi::{BidiLine, MAXIMUM_BIDI_RUNS},
    text::{TextDirection, TextError, TextFont, TextOptions},
};
#[test]
fn unicode_reference_vectors_match_base_levels_and_visual_order() {
    let mut count = 0;
    for line in include_str!("fixtures/bidi/reference-vectors.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = line.split(';').collect();
        let text: String = fields[0]
            .split_whitespace()
            .map(|value| char::from_u32(u32::from_str_radix(value, 16).unwrap()).unwrap())
            .collect();
        let base = match fields[1] {
            "0" => Some(TextDirection::LeftToRight),
            "1" => Some(TextDirection::RightToLeft),
            "2" => None,
            _ => panic!("reference base"),
        };
        let bidi = BidiLine::new(&text, base).unwrap();
        assert_eq!(
            bidi.base_direction(),
            if fields[2] == "0" {
                TextDirection::LeftToRight
            } else {
                TextDirection::RightToLeft
            }
        );
        let expected_levels: Vec<u8> = fields[3]
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        let character_bytes: Vec<_> = text.char_indices().map(|(byte, _)| byte).collect();
        let actual_levels: Vec<_> = character_bytes
            .iter()
            .map(|byte| bidi.byte_levels()[*byte])
            .collect();
        assert_eq!(actual_levels, expected_levels, "{line}");
        let mut visual = Vec::new();
        for run in bidi.visual_runs() {
            let mut indices: Vec<_> = character_bytes
                .iter()
                .enumerate()
                .filter(|(_, byte)| run.range.contains(byte))
                .map(|(index, _)| index)
                .collect();
            if run.direction == TextDirection::RightToLeft {
                indices.reverse();
            }
            visual.extend(indices);
        }
        let expected_order: Vec<usize> = fields[4]
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(visual, expected_order, "{line}");
        count += 1;
    }
    assert_eq!(count, 32);
}
#[test]
fn byte_boundaries_isolates_combining_marks_auto_base_and_empty() {
    for (text, base) in [
        ("سلام", TextDirection::RightToLeft),
        ("Gain سلام 12", TextDirection::LeftToRight),
        ("---", TextDirection::LeftToRight),
        ("a\u{2067}سَلام\u{2069}b", TextDirection::LeftToRight),
    ] {
        let line = BidiLine::new(text, None).unwrap();
        assert_eq!(line.text(), text);
        assert_eq!(line.base_direction(), base);
        assert_eq!(line.byte_levels().len(), text.len());
        let mut covered = vec![false; text.len()];
        for (index, run) in line.visual_runs().iter().enumerate() {
            assert!(text.is_char_boundary(run.range.start) && text.is_char_boundary(run.range.end));
            assert_eq!(line.run_text(index).unwrap(), &text[run.range.clone()]);
            for byte in run.range.clone() {
                assert!(!covered[byte]);
                covered[byte] = true;
            }
        }
        assert!(covered.into_iter().all(|covered| covered));
        assert_eq!(line.run_text(line.visual_runs().len()), None);
        for (byte, character) in text.char_indices() {
            assert!(
                line.byte_levels()[byte..byte + character.len_utf8()]
                    .iter()
                    .all(|level| *level == line.byte_levels()[byte])
            );
        }
    }
    let forced = BidiLine::new("abc", Some(TextDirection::RightToLeft)).unwrap();
    assert_eq!(forced.base_direction(), TextDirection::RightToLeft);
    assert_eq!(
        forced.visual_runs()[0].direction,
        TextDirection::LeftToRight
    );
    let empty = BidiLine::new("", Some(TextDirection::RightToLeft)).unwrap();
    assert_eq!(empty.base_direction(), TextDirection::RightToLeft);
    assert!(empty.visual_runs().is_empty());
}
#[test]
fn line_byte_run_bounds_and_unicode_separators_reject_before_shaping() {
    assert_eq!(
        BidiLine::new(&"a".repeat(4097), None),
        Err(TextError::Capacity)
    );
    assert_eq!(
        BidiLine::new(&"aא".repeat(257), None),
        Err(TextError::Capacity)
    );
    let accepted = "aא".repeat(256);
    assert_eq!(
        BidiLine::new(&accepted, None).unwrap().visual_runs().len(),
        MAXIMUM_BIDI_RUNS
    );
    let font = TextFont::from_bytes(include_bytes!("fixtures/text/NotoSansArabic.ttf"), 0).unwrap();
    let options = TextOptions {
        size: 24.,
        direction: TextDirection::LeftToRight,
        script: None,
        language: None,
        features: &[],
    };
    for text in ["a\nb", "a\0b", "a\u{2028}b", "a\u{2029}b", "\u{fdd0}"] {
        assert_eq!(BidiLine::new(text, None), Err(TextError::InvalidInput));
        assert_eq!(font.shape(text, options), Err(TextError::InvalidInput));
    }
}
