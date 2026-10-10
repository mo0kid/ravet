#![cfg(feature = "text")]
use ravet_surface::{
    caret::{CaretAffinity as Affinity, TextPosition},
    editing::{EditError as E, TextEditBuffer, TextSelection},
};
fn pos(byte_offset: usize) -> TextPosition {
    TextPosition {
        byte_offset,
        affinity: Affinity::Downstream,
    }
}
fn select(buffer: &mut TextEditBuffer, anchor: usize, focus: usize) {
    buffer
        .set_selection(
            buffer.revision(),
            TextSelection {
                anchor: pos(anchor),
                focus: pos(focus),
            },
        )
        .unwrap();
}
#[test]
fn validation_capacity_and_bad_grapheme_inputs_preserve_every_live_field() {
    assert!(matches!(TextEditBuffer::new("", 65), Err(E::Capacity)));
    assert!(matches!(
        TextEditBuffer::new(&"A".repeat(4097), 1),
        Err(E::Capacity)
    ));
    for invalid in ["a\t", "\0", "\u{ffff}"] {
        assert!(matches!(
            TextEditBuffer::new(invalid, 1),
            Err(E::InvalidInput)
        ));
    }
    let mut buffer = TextEditBuffer::new("بَ\r\nA", 3).unwrap();
    let initial = buffer.revision();
    let selection = buffer.selection();
    for range in [(0, 2), (2, 4), (5, 6), (8, 8), (4, 0)].map(|(start, end)| start..end) {
        assert_eq!(buffer.replace(initial, range, "X"), Err(E::InvalidInput));
        assert_eq!(buffer.revision(), initial);
        assert_eq!(buffer.selection(), selection);
        assert_eq!(buffer.text(), "بَ\r\nA");
        assert_eq!((buffer.undo_depth(), buffer.redo_depth()), (0, 0));
    }
    assert_eq!(buffer.replace(initial, 0..4, "\t"), Err(E::InvalidInput));
    assert_eq!(
        buffer.replace(initial, 0..0, &"A".repeat(4097)),
        Err(E::Capacity)
    );
    assert_eq!(
        buffer.set_selection(initial, TextSelection::collapsed(pos(2))),
        Err(E::InvalidInput)
    );
    assert_eq!(buffer.revision(), initial);
    let mut maximum = TextEditBuffer::new(&"A".repeat(4096), 1).unwrap();
    let original = maximum.revision();
    assert_eq!(maximum.replace(original, 0..0, "B"), Err(E::Capacity));
    assert_eq!(maximum.text().len(), 4096);
    assert_eq!(maximum.revision(), original);
    assert_eq!(maximum.replace(original, 0..1, "B"), Ok(true));
    assert_eq!(maximum.text().len(), 4096);
}
#[test]
fn combining_zwj_and_crlf_insertions_snap_forward_after_new_grapheme_joining() {
    let mut buffer = TextEditBuffer::new("a", 4).unwrap();
    assert_eq!(buffer.replace(buffer.revision(), 1..1, "\u{301}"), Ok(true));
    assert_eq!(buffer.text(), "a\u{301}");
    assert_eq!(buffer.selection(), TextSelection::collapsed(pos(3)));
    assert_eq!(buffer.delete_backward(buffer.revision()), Ok(true));
    assert_eq!(buffer.text(), "");
    let mut emoji = TextEditBuffer::new("👩👧", 4).unwrap();
    emoji.replace(emoji.revision(), 4..4, "\u{200d}").unwrap();
    assert_eq!(emoji.text(), "👩‍👧");
    assert_eq!(emoji.selection().focus.byte_offset, 11);
    assert_eq!(
        emoji.set_selection(emoji.revision(), TextSelection::collapsed(pos(7))),
        Err(E::InvalidInput)
    );
    emoji.delete_backward(emoji.revision()).unwrap();
    assert_eq!(emoji.text(), "");
    let mut crlf = TextEditBuffer::new("\r", 4).unwrap();
    crlf.replace(crlf.revision(), 1..1, "\n").unwrap();
    assert_eq!(crlf.text(), "\r\n");
    assert_eq!(crlf.selection().focus.byte_offset, 2);
    assert_eq!(
        crlf.set_selection(crlf.revision(), TextSelection::collapsed(pos(1))),
        Err(E::InvalidInput)
    );
    crlf.delete_backward(crlf.revision()).unwrap();
    assert_eq!(crlf.text(), "");
}
#[test]
fn owner_tokens_and_monotonic_revisions_invalidate_foreign_stale_and_restored_caches() {
    let mut first = TextEditBuffer::new("AV", 3).unwrap();
    let second = TextEditBuffer::new("AV", 3).unwrap();
    let original = first.revision();
    let foreign = second.revision();
    assert_ne!(original, foreign);
    assert_eq!(original.sequence(), foreign.sequence());
    assert!(!first.matches(foreign));
    assert_eq!(first.replace(foreign, 0..1, "X"), Err(E::StaleRevision));
    assert_eq!(first.undo(foreign), Err(E::StaleRevision));
    first.replace(original, 0..1, "X").unwrap();
    let edited = first.revision();
    assert_eq!(edited.sequence(), 1);
    assert!(!first.matches(original));
    assert_eq!(first.replace(original, 0..1, "Y"), Err(E::StaleRevision));
    assert_eq!(
        first.set_selection(original, TextSelection::collapsed(pos(2))),
        Err(E::StaleRevision)
    );
    first.undo(edited).unwrap();
    assert_eq!(first.text(), "AV");
    assert!(!first.matches(original));
    assert_eq!(first.revision().sequence(), 2);
    assert_eq!(first.redo(original), Err(E::StaleRevision));
}
#[test]
fn exact_selection_snapshots_restore_direction_affinity_and_nonhistorical_moves_preserve_redo() {
    let mut buffer = TextEditBuffer::new("Gain", 4).unwrap();
    let reverse = TextSelection {
        anchor: TextPosition {
            byte_offset: 4,
            affinity: Affinity::Upstream,
        },
        focus: pos(1),
    };
    buffer.set_selection(buffer.revision(), reverse).unwrap();
    buffer.replace_selection(buffer.revision(), "X").unwrap();
    assert_eq!(buffer.text(), "GX");
    assert_eq!(buffer.selection(), TextSelection::collapsed(pos(2)));
    buffer.undo(buffer.revision()).unwrap();
    assert_eq!(buffer.text(), "Gain");
    assert_eq!(buffer.selection(), reverse);
    assert_eq!(buffer.redo_depth(), 1);
    let before = buffer.revision();
    select(&mut buffer, 0, 0);
    assert_ne!(buffer.revision(), before);
    assert_eq!(buffer.redo_depth(), 1);
    buffer.redo(buffer.revision()).unwrap();
    assert_eq!(buffer.text(), "GX");
    assert_eq!(buffer.selection(), TextSelection::collapsed(pos(2)));
    buffer.undo(buffer.revision()).unwrap();
    assert_eq!(buffer.selection(), TextSelection::collapsed(pos(0)));
}
#[test]
fn bounded_history_evicts_oldest_and_branching_net_noops_are_explicit() {
    let mut buffer = TextEditBuffer::new("", 3).unwrap();
    for letter in ["a", "b", "c", "d", "e", "f"] {
        let end = buffer.text().len();
        buffer.replace(buffer.revision(), end..end, letter).unwrap();
        assert!(buffer.undo_depth() + buffer.redo_depth() <= 3);
    }
    for _ in 0..3 {
        assert_eq!(buffer.undo(buffer.revision()), Ok(true));
        assert_eq!(buffer.undo_depth() + buffer.redo_depth(), 3);
    }
    assert_eq!(buffer.text(), "abc");
    assert_eq!(buffer.undo(buffer.revision()), Ok(false));
    let before = buffer.revision();
    let selection = buffer.selection();
    assert_eq!(buffer.replace(before, 0..3, "abc"), Ok(false));
    assert_eq!(buffer.revision(), before);
    assert_eq!(buffer.selection(), selection);
    assert_eq!(buffer.redo_depth(), 3);

    assert_eq!(buffer.replace(before, 0..1, "\t"), Err(E::InvalidInput));
    assert_eq!(
        buffer.replace(before, 0..0, &"X".repeat(4097)),
        Err(E::Capacity)
    );
    assert_eq!(
        buffer.replace(before, 0..usize::MAX, "X"),
        Err(E::InvalidInput)
    );
    assert_eq!(buffer.text(), "abc");
    assert_eq!(buffer.selection(), selection);
    assert_eq!(buffer.revision(), before);
    assert_eq!((buffer.undo_depth(), buffer.redo_depth()), (0, 3));
    buffer.redo(buffer.revision()).unwrap();
    assert_eq!(buffer.text(), "abcd");
    buffer.replace(buffer.revision(), 4..4, "X").unwrap();
    assert_eq!(buffer.text(), "abcdX");
    assert_eq!(buffer.redo_depth(), 0);
    assert_eq!(buffer.redo(buffer.revision()), Ok(false));
    assert!(buffer.undo_depth() <= 3);
    let mut zero = TextEditBuffer::new("A", 0).unwrap();
    zero.replace(zero.revision(), 0..1, "B").unwrap();
    assert_eq!(zero.text(), "B");
    assert_eq!(zero.undo_depth(), 0);
    assert_eq!(zero.undo(zero.revision()), Ok(false));
}
#[test]
fn deletion_preserves_complete_graphemes_and_logical_selection_orientation() {
    let mut buffer = TextEditBuffer::new("Aبَ\r\n👩‍👧Z", 4).unwrap();
    assert_eq!(buffer.delete_backward(buffer.revision()), Ok(false));
    buffer.delete_forward(buffer.revision()).unwrap();
    assert_eq!(buffer.text(), "بَ\r\n👩‍👧Z");
    buffer.delete_forward(buffer.revision()).unwrap();
    assert_eq!(buffer.text(), "\r\n👩‍👧Z");
    buffer.delete_forward(buffer.revision()).unwrap();
    assert_eq!(buffer.text(), "👩‍👧Z");
    buffer.delete_forward(buffer.revision()).unwrap();
    assert_eq!(buffer.text(), "Z");
    select(&mut buffer, 1, 0);
    buffer.delete_backward(buffer.revision()).unwrap();
    assert_eq!(buffer.text(), "");
    let before = buffer.revision();
    assert_eq!(buffer.delete_forward(before), Ok(false));
    assert_eq!(buffer.delete_backward(before), Ok(false));
    assert_eq!(buffer.revision(), before);
}
#[test]
fn prepared_layout_identity_retains_old_source_and_invalidates_other_or_changed_documents() {
    let mut buffer = TextEditBuffer::new("first\nsecond\u{2028}third", 64).unwrap();
    let token = buffer.revision();
    let cached_source = buffer.text().to_owned();
    assert!(buffer.matches(token));
    let font = ravet_surface::text::TextFont::from_bytes(
        include_bytes!("fixtures/text/NotoSansArabic.ttf"),
        0,
    )
    .unwrap();
    let cached = buffer
        .prepare_layout(
            &font,
            ravet_surface::text::TextOptions {
                size: 24.,
                direction: ravet_surface::text::TextDirection::LeftToRight,
                script: None,
                language: None,
                features: &[],
            },
            ravet_surface::paragraph::ParagraphOptions {
                width: 300.,
                line_height: 32.,
                alignment: ravet_surface::paragraph::TextAlignment::Left,
            },
        )
        .unwrap();
    assert!(cached.matches(&buffer));
    assert_eq!(cached.revision(), token);
    assert_eq!(cached.layout().text(), cached_source);
    let other = TextEditBuffer::new(buffer.text(), 64).unwrap();
    assert!(!cached.matches(&other));
    let source_end = buffer.text().len();
    select(&mut buffer, 0, source_end);
    assert!(!buffer.matches(token));
    assert!(!cached.matches(&buffer));

    buffer.replace_selection(buffer.revision(), "A").unwrap();
    buffer.undo(buffer.revision()).unwrap();
    assert_eq!(cached.layout().text(), buffer.text());
    assert!(!cached.matches(&buffer));
    buffer.redo(buffer.revision()).unwrap();
    for index in 0..100 {
        buffer.replace(buffer.revision(), 0..1, "B").unwrap();
        buffer.replace(buffer.revision(), 0..1, "A").unwrap();
        assert_eq!(buffer.undo_depth(), 64.min(1 + 2 * (index + 1)));
    }
    assert_eq!(cached_source, "first\nsecond\u{2028}third");
    assert_eq!(buffer.undo_depth() + buffer.redo_depth(), 64);
}
