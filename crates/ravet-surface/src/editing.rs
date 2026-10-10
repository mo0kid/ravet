//! Bounded source edit ownership on UI/control threads; no native input adapter.
//! Mutations require the current owner/revision token. Grapheme data is Unicode 17.0.
use crate::{
    caret::{CaretAffinity, TextPosition},
    paragraph::{ParagraphLayout, ParagraphOptions},
    text::{MAXIMUM_TEXT_BYTES, TextError, TextFont, TextOptions, valid_paragraph},
};
use std::{
    collections::VecDeque,
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};
use unicode_segmentation::UnicodeSegmentation;
/// Maximum historical snapshots across undo and redo together.
pub const MAXIMUM_EDIT_HISTORY: usize = 64;
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
/// Unforgeable process-local owner/revision identity. Not a persisted document ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EditRevision {
    owner: u64,
    sequence: u64,
}
impl EditRevision {
    /// Ordinal within this owner only. Compare complete tokens or `matches`, not
    /// this number alone, when validating document/cache identity.
    pub fn sequence(self) -> u64 {
        self.sequence
    }
}
/// Explicit original UTF8 selection endpoints, preserving direction and affinity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSelection {
    /// Fixed original-source endpoint, retaining affinity.
    pub anchor: TextPosition,
    /// Moving original-source endpoint, retaining affinity.
    pub focus: TextPosition,
}
impl TextSelection {
    /// Equal anchor/focus at one supplied source position.
    pub fn collapsed(position: TextPosition) -> Self {
        Self {
            anchor: position,
            focus: position,
        }
    }
    /// Ordered logical half-open byte range, independent of selection direction.
    pub fn range(self) -> Range<usize> {
        self.anchor.byte_offset.min(self.focus.byte_offset)
            ..self.anchor.byte_offset.max(self.focus.byte_offset)
    }
}
/// Validation and identity failures leave text, selection, history and revision intact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    /// Invalid controls, ranges or grapheme endpoints.
    InvalidInput,
    /// Complete source/replacement bytes or requested history exceed limits.
    Capacity,
    /// Expected token is stale or belongs to another buffer.
    StaleRevision,
    /// A changing operation would overflow this owner's revision sequence.
    RevisionExhausted,
    /// Process-local unique-owner allocator is exhausted; it never wraps.
    OwnerExhausted,
}
impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EditError {}
#[derive(Clone)]
struct Snapshot {
    text: String,
    selection: TextSelection,
}
/// Prepared layout retaining its exact source-owner/revision identity. Old layouts
/// remain immutable readable values, but must pass `matches` before editing/render
/// decisions use them as the current document. Font/style identity is caller-owned.
pub struct PreparedTextLayout {
    revision: EditRevision,
    layout: ParagraphLayout,
}
impl PreparedTextLayout {
    /// Complete source/selection token captured during preparation.
    pub fn revision(&self) -> EditRevision {
        self.revision
    }
    /// Immutable layout/source snapshot; readable even after it becomes stale.
    pub fn layout(&self) -> &ParagraphLayout {
        &self.layout
    }
    /// Current document owner/revision test; font/style cache policy is separate.
    pub fn matches(&self, buffer: &TextEditBuffer) -> bool {
        buffer.matches(self.revision)
    }
}
/// Owned bounded plaintext with exact snapshot undo/redo. Preparation/mutation can
/// allocate/deallocate and never belongs in audio callbacks. No Clone is exposed:
/// two independently constructed buffers always have distinct owner identities.
///
/// ```
/// use ravet_surface::editing::TextEditBuffer;
/// let mut text = TextEditBuffer::new("a", 4)?;
/// let original = text.revision();
/// text.replace(original, 1..1, "\u{301}")?;
/// assert_eq!(text.selection().focus.byte_offset, 3);
/// text.undo(text.revision())?;
/// assert_eq!(text.text(), "a");
/// assert!(!text.matches(original));
/// # Ok::<(), ravet_surface::editing::EditError>(())
/// ```
pub struct TextEditBuffer {
    owner: u64,
    sequence: u64,
    state: Snapshot,
    history_limit: usize,
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
}
impl TextEditBuffer {
    /// Retain <=4096 UTF8 bytes and <=64 snapshots. Initial selection is byte 0.
    /// LF/CRLF/CR/U+2028/U+2029 are accepted; other controls/noncharacters reject.
    /// No font coverage, shaping, native input, normalization or newline rewriting.
    pub fn new(text: &str, history_limit: usize) -> Result<Self, EditError> {
        if history_limit > MAXIMUM_EDIT_HISTORY || text.len() > MAXIMUM_TEXT_BYTES {
            return Err(EditError::Capacity);
        }
        if !valid_text(text) {
            return Err(EditError::InvalidInput);
        }
        let owner = claim_owner(&NEXT_OWNER)?;
        Ok(Self {
            owner,
            sequence: 0,
            state: Snapshot {
                text: text.to_owned(),
                selection: TextSelection::collapsed(TextPosition {
                    byte_offset: 0,
                    affinity: CaretAffinity::Downstream,
                }),
            },
            history_limit,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        })
    }
    /// Prepare one explicit-font layout while the source/revision are borrowed
    /// together. No mutation occurs on shaping/layout failure. Selection-only changes
    /// conservatively invalidate this cache token alongside source/history edits.
    pub fn prepare_layout(
        &self,
        font: &TextFont,
        text_options: TextOptions<'_>,
        paragraph_options: ParagraphOptions,
    ) -> Result<PreparedTextLayout, TextError> {
        let layout = ParagraphLayout::new(font, self.text(), text_options, paragraph_options)?;
        Ok(PreparedTextLayout {
            revision: self.revision(),
            layout,
        })
    }
    /// Complete retained original UTF8 source.
    pub fn text(&self) -> &str {
        &self.state.text
    }
    /// Directional grapheme selection in original UTF8 bytes.
    pub fn selection(&self) -> TextSelection {
        self.state.selection
    }
    /// Current unforgeable process-local owner/revision token.
    pub fn revision(&self) -> EditRevision {
        EditRevision {
            owner: self.owner,
            sequence: self.sequence,
        }
    }
    /// Compare both owner and revision before reusing prepared layout/caret caches.
    pub fn matches(&self, revision: EditRevision) -> bool {
        self.revision() == revision
    }
    /// Retained prior edit snapshots, including oldest until eviction.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }
    /// Restorable snapshots after undo; cleared by the next content change.
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }
    /// Selection changes advance revision without entering or clearing undo/redo.
    /// Exact no-ops preserve revision. Endpoints must be extended-grapheme boundaries.
    pub fn set_selection(
        &mut self,
        expected: EditRevision,
        selection: TextSelection,
    ) -> Result<bool, EditError> {
        self.check(expected)?;
        if !boundary(self.text(), selection.anchor.byte_offset)
            || !boundary(self.text(), selection.focus.byte_offset)
        {
            return Err(EditError::InvalidInput);
        }
        if selection == self.state.selection {
            return Ok(false);
        }
        let next = self.next_sequence()?;
        self.state.selection = selection;
        self.sequence = next;
        Ok(true)
    }
    /// Replace an ordered half-open grapheme range atomically. Successful content
    /// edits collapse selection after the insertion; if joining invalidates that
    /// boundary, snap forward to the next resulting grapheme boundary (or source end).
    /// Net content no-ops preserve selection/revision/history/redo exactly.
    /// One edit adds one snapshot; oldest undo evicts at capacity and branching clears
    /// redo. History 0 still edits source. Validation/capacity/token failures preserve all.
    pub fn replace(
        &mut self,
        expected: EditRevision,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<bool, EditError> {
        self.check(expected)?;
        if replacement.len() > MAXIMUM_TEXT_BYTES {
            return Err(EditError::Capacity);
        }
        if range.start > range.end
            || !boundary(self.text(), range.start)
            || !boundary(self.text(), range.end)
            || !valid_text(replacement)
        {
            return Err(EditError::InvalidInput);
        }
        let length = self.text().len() - (range.end - range.start);
        let length = length
            .checked_add(replacement.len())
            .filter(|length| *length <= MAXIMUM_TEXT_BYTES)
            .ok_or(EditError::Capacity)?;
        if &self.text()[range.clone()] == replacement {
            return Ok(false);
        }
        let next = self.next_sequence()?;
        let mut text = String::with_capacity(length);
        text.push_str(&self.text()[..range.start]);
        text.push_str(replacement);
        text.push_str(&self.text()[range.end..]);
        let insertion_end = range.start + replacement.len();
        let caret = text
            .grapheme_indices(true)
            .map(|(byte, _)| byte)
            .find(|&byte| byte >= insertion_end)
            .unwrap_or(text.len());
        let state = Snapshot {
            text,
            selection: TextSelection::collapsed(TextPosition {
                byte_offset: caret,
                affinity: CaretAffinity::Downstream,
            }),
        };
        let prior = std::mem::replace(&mut self.state, state);
        self.redo.clear();
        if self.history_limit != 0 {
            if self.undo.len() == self.history_limit {
                self.undo.pop_front();
            }
            self.undo.push_back(prior);
        }
        self.sequence = next;
        Ok(true)
    }
    /// Replace the selected logical range, preserving all net no-op/error contracts.
    pub fn replace_selection(
        &mut self,
        expected: EditRevision,
        replacement: &str,
    ) -> Result<bool, EditError> {
        self.replace(expected, self.selection().range(), replacement)
    }
    /// Delete one preceding grapheme at a collapsed selection, otherwise the selected
    /// logical range. At source start this is a complete no-op.
    pub fn delete_backward(&mut self, expected: EditRevision) -> Result<bool, EditError> {
        self.check(expected)?;
        let mut range = self.selection().range();
        if range.is_empty() {
            let end = range.end;
            range.start = self
                .text()
                .grapheme_indices(true)
                .map(|(byte, _)| byte)
                .take_while(|&byte| byte < end)
                .last()
                .unwrap_or(end);
        }
        self.replace(expected, range, "")
    }
    /// Delete one following grapheme, otherwise the selected range. At source end
    /// this is a complete no-op. CRLF/combining/ZWJ graphemes are kept atomic.
    pub fn delete_forward(&mut self, expected: EditRevision) -> Result<bool, EditError> {
        self.check(expected)?;
        let mut range = self.selection().range();
        if range.is_empty() {
            let start = range.start;
            range.end = self
                .text()
                .grapheme_indices(true)
                .map(|(byte, _)| byte)
                .find(|&byte| byte > start)
                .unwrap_or(self.text().len());
        }
        self.replace(expected, range, "")
    }
    /// Restore exact historical text/selection while monotonically advancing revision.
    /// Empty history is a no-op; restoration never reuses the historical revision.
    pub fn undo(&mut self, expected: EditRevision) -> Result<bool, EditError> {
        self.check(expected)?;
        if self.undo.is_empty() {
            return Ok(false);
        }
        let next = self.next_sequence()?;
        let prior = self.undo.pop_back().unwrap();
        self.redo
            .push_back(std::mem::replace(&mut self.state, prior));
        self.sequence = next;
        Ok(true)
    }
    /// Restore the next exact undone snapshot with a new revision. Empty redo is
    /// a no-op; successful branching edits have already cleared this history.
    pub fn redo(&mut self, expected: EditRevision) -> Result<bool, EditError> {
        self.check(expected)?;
        if self.redo.is_empty() {
            return Ok(false);
        }
        let next = self.next_sequence()?;
        let next_state = self.redo.pop_back().unwrap();
        self.undo
            .push_back(std::mem::replace(&mut self.state, next_state));
        self.sequence = next;
        Ok(true)
    }
    fn check(&self, expected: EditRevision) -> Result<(), EditError> {
        if self.matches(expected) {
            Ok(())
        } else {
            Err(EditError::StaleRevision)
        }
    }
    fn next_sequence(&self) -> Result<u64, EditError> {
        self.sequence
            .checked_add(1)
            .ok_or(EditError::RevisionExhausted)
    }
}
fn claim_owner(counter: &AtomicU64) -> Result<u64, EditError> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| EditError::OwnerExhausted)
}
fn boundary(text: &str, byte: usize) -> bool {
    byte == text.len()
        || text
            .grapheme_indices(true)
            .any(|(position, _)| position == byte)
}
fn valid_text(text: &str) -> bool {
    valid_paragraph(text)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_allocator_rejects_exhaustion_without_wrap_or_global_reset() {
        let local = AtomicU64::new(u64::MAX - 1);
        assert_eq!(claim_owner(&local), Ok(u64::MAX - 1));
        assert_eq!(local.load(Ordering::Relaxed), u64::MAX);
        assert_eq!(claim_owner(&local), Err(EditError::OwnerExhausted));
        assert_eq!(local.load(Ordering::Relaxed), u64::MAX);
    }
    #[test]
    fn exhausted_revision_preserves_complete_state() {
        let mut buffer = TextEditBuffer::new("AV", 2).unwrap();
        buffer.sequence = u64::MAX;
        let token = buffer.revision();
        let original = buffer.state.clone();
        assert_eq!(
            buffer.replace(token, 0..1, "X"),
            Err(EditError::RevisionExhausted)
        );
        assert_eq!(
            buffer.set_selection(
                token,
                TextSelection::collapsed(TextPosition {
                    byte_offset: 1,
                    affinity: CaretAffinity::Downstream
                })
            ),
            Err(EditError::RevisionExhausted)
        );
        assert_eq!(buffer.text(), original.text);
        assert_eq!(buffer.selection(), original.selection);
        assert_eq!(buffer.revision(), token);
        assert_eq!((buffer.undo_depth(), buffer.redo_depth()), (0, 0));
        assert_eq!(buffer.replace(token, 0..1, "A"), Ok(false));
        assert_eq!(buffer.set_selection(token, original.selection), Ok(false));
        assert_eq!(buffer.undo(token), Ok(false));
        let mut history = TextEditBuffer::new("A", 2).unwrap();
        history.replace(history.revision(), 0..1, "B").unwrap();
        history.sequence = u64::MAX;
        let token = history.revision();
        assert_eq!(history.undo(token), Err(EditError::RevisionExhausted));
        assert_eq!(history.text(), "B");
        assert_eq!((history.undo_depth(), history.redo_depth()), (1, 0));
        assert_eq!(history.revision(), token);
        history.sequence = u64::MAX - 1;
        history.undo(history.revision()).unwrap();
        let token = history.revision();
        assert_eq!(history.text(), "A");
        assert_eq!(history.redo(token), Err(EditError::RevisionExhausted));
        assert_eq!(history.text(), "A");
        assert_eq!((history.undo_depth(), history.redo_depth()), (0, 1));
        assert_eq!(history.revision(), token);
    }
}
