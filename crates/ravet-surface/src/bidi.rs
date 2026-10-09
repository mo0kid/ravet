//! Optional bounded single-line Unicode bidirectional ordering, before shaping.
use crate::text::{MAXIMUM_TEXT_BYTES, TextDirection, TextError, valid_single_line};
use std::ops::Range;
/// Visual directional runs returned after line ordering; backend work may allocate
/// before this bound is checked. Processing belongs on UI/worker threads.
pub const MAXIMUM_BIDI_RUNS: usize = 512;
/// Logical UTF8 byte range in visual run order. Feed each original slice to a
/// shaper with this direction; do not reverse its characters before shaping.
/// Levels are line-adjusted Unicode embedding levels, not font/script boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisualTextRun {
    pub range: Range<usize>,
    pub direction: TextDirection,
    pub level: u8,
}
/// Borrowed complete single-line text with resolved base/byte levels and visual
/// runs, using unicode-bidi's pinned Unicode 16.0 data. No wrapping, script/font
/// segmentation, glyph mirroring/combining reordering, caret or native UI is added.
/// The downstream shaper owns mirroring/marks within each original logical run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BidiLine<'a> {
    text: &'a str,
    base: TextDirection,
    levels: Vec<u8>,
    runs: Vec<VisualTextRun>,
}
impl<'a> BidiLine<'a> {
    /// Resolve <=4096 UTF8 bytes/<=512 visual runs. None selects first-strong
    /// base direction (LTR if no strong text); explicit direction overrides it.
    /// Controls, line/paragraph separators and noncharacters reject; directional
    /// formatting/isolate characters are allowed and retained in source ranges.
    /// Backend transient memory/time is not globally bounded; no audio callback.
    pub fn new(text: &'a str, base: Option<TextDirection>) -> Result<Self, TextError> {
        if text.len() > MAXIMUM_TEXT_BYTES {
            return Err(TextError::Capacity);
        }
        if !valid_single_line(text) {
            return Err(TextError::InvalidInput);
        }
        if text.is_empty() {
            return Ok(Self {
                text,
                base: base.unwrap_or(TextDirection::LeftToRight),
                levels: Vec::new(),
                runs: Vec::new(),
            });
        }
        let level = base.map(|direction| match direction {
            TextDirection::LeftToRight => unicode_bidi::Level::ltr(),
            TextDirection::RightToLeft => unicode_bidi::Level::rtl(),
        });
        let info = unicode_bidi::ParagraphBidiInfo::new(text, level);
        let base = if info.paragraph_level.is_rtl() {
            TextDirection::RightToLeft
        } else {
            TextDirection::LeftToRight
        };
        let (levels, ranges) = info.visual_runs(0..text.len());
        if ranges.len() > MAXIMUM_BIDI_RUNS {
            return Err(TextError::Capacity);
        }
        let mut runs = Vec::with_capacity(ranges.len());
        for range in ranges {
            if range.start >= range.end
                || range.end > text.len()
                || !text.is_char_boundary(range.start)
                || !text.is_char_boundary(range.end)
            {
                return Err(TextError::Shaping);
            }
            let level = levels[range.start];
            runs.push(VisualTextRun {
                range,
                direction: if level.is_rtl() {
                    TextDirection::RightToLeft
                } else {
                    TextDirection::LeftToRight
                },
                level: level.number(),
            });
        }
        Ok(Self {
            text,
            base,
            levels: levels.into_iter().map(|level| level.number()).collect(),
            runs,
        })
    }
    pub fn text(&self) -> &'a str {
        self.text
    }
    pub fn base_direction(&self) -> TextDirection {
        self.base
    }
    /// Embedding level per UTF8 byte; multi-byte characters repeat the same level.
    pub fn byte_levels(&self) -> &[u8] {
        &self.levels
    }
    pub fn visual_runs(&self) -> &[VisualTextRun] {
        &self.runs
    }
    pub fn run_text(&self, index: usize) -> Option<&'a str> {
        self.runs
            .get(index)
            .map(|run| &self.text[run.range.clone()])
    }
}
